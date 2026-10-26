//! Port of SyntaxPrinter/Token.cs

use tree_sitter::Node;

use super::{extra_new_lines_for_trivia, Ctx};
use crate::doc::{CommentType, Doc};
use crate::trivia::{Trivia, TriviaKind, TriviaListExt};

#[derive(Default, Clone, Copy)]
pub struct TokenOptions {
    pub skip_leading: bool,
    pub skip_trailing: bool,
    /// print the leading trivia of another token instead, used when reordering modifiers
    pub leading_from: Option<usize>,
}

pub fn is_hard_line_for_trivia(doc: &Doc) -> bool {
    matches!(doc, Doc::Line(l) if l.is_hard_line && l.is_for_trivia && !l.squash)
}

pub fn is_hard_line(doc: &Doc) -> bool {
    matches!(doc, Doc::Line(l) if l.is_hard_line)
}

fn is_only_whitespace(trivia: &[Trivia]) -> bool {
    trivia.iter().all(|t| t.is_whitespace_or_eol())
}

impl Ctx<'_> {
    pub fn token_index(&self, node: Node) -> usize {
        self.tokens
            .index_at(node.start_byte())
            .unwrap_or_else(|| panic!("no token for {} at {}", node.kind(), node.start_byte()))
    }
}

/// `node` must be a leaf (or atomic) node, like an identifier or an anonymous token.
pub fn print_token(ctx: &mut Ctx, node: Node) -> Doc {
    print_token_with(ctx, node, Doc::Null, TokenOptions::default())
}

pub fn print_token_opt(ctx: &mut Ctx, node: Option<Node>) -> Doc {
    match node {
        Some(n) => print_token(ctx, n),
        None => Doc::Null,
    }
}

pub fn print_token_with_suffix(ctx: &mut Ctx, node: Node, suffix: impl Into<Doc>) -> Doc {
    print_token_with(ctx, node, suffix.into(), TokenOptions::default())
}

pub fn print_token_without_leading_trivia(ctx: &mut Ctx, node: Node) -> Doc {
    print_token_with(
        ctx,
        node,
        Doc::Null,
        TokenOptions {
            skip_leading: true,
            ..Default::default()
        },
    )
}

pub fn print_token_with(ctx: &mut Ctx, node: Node, suffix: Doc, opts: TokenOptions) -> Doc {
    let index = ctx.token_index(node);
    let tokens = ctx.tokens;
    let token = &tokens.list[index];

    let leading = if opts.skip_leading {
        Doc::Null
    } else {
        print_leading_trivia_of_token(ctx, node, opts.leading_from.unwrap_or(index))
    };
    let trailing = if opts.skip_trailing {
        Doc::Null
    } else {
        print_trailing_trivia(&token.trailing)
    };

    let text = &ctx.src[token.start..token.end];
    let is_verbatim = node.kind() == "verbatim_string_literal"
        || (node.kind() == "interpolated_string_expression" && text.starts_with("$@"));

    let token_doc = if is_verbatim && text.contains('\n') {
        let lines: Vec<Doc> = text
            .replace('\r', "")
            .split('\n')
            .map(Doc::text)
            .collect();
        Doc::join(Doc::literal_line(), lines)
    } else {
        Doc::text(text)
    };

    if matches!(suffix, Doc::Null)
        && matches!(leading, Doc::Null)
        && matches!(trailing, Doc::Null)
    {
        return token_doc;
    }

    let mut docs = Vec::new();
    if !matches!(leading, Doc::Null) {
        docs.push(leading);
    }
    docs.push(token_doc);
    if !matches!(trailing, Doc::Null) {
        if matches!(&ctx.state.trailing_comma, Some((i, _)) if *i == index) {
            let (_, comma) = ctx.state.trailing_comma.take().unwrap();
            docs.push(comma);
            ctx.state.moved_trailing_trivia = true;
        }
        docs.push(trailing);
    }
    if !matches!(suffix, Doc::Null) {
        docs.push(suffix);
    }
    Doc::concat(docs)
}

pub fn print_trailing_trivia(trailing: &[Trivia]) -> Doc {
    if !trailing.iter().any(|t| {
        matches!(
            t.kind,
            TriviaKind::SingleLineComment | TriviaKind::MultiLineComment
        )
    }) {
        return Doc::Null;
    }
    let mut docs = Vec::new();
    for t in trailing {
        match t.kind {
            TriviaKind::SingleLineComment => {
                docs.push(Doc::trailing_comment(t.text.clone(), CommentType::SingleLine))
            }
            TriviaKind::MultiLineComment => {
                docs.push(Doc::text(" "));
                docs.push(Doc::trailing_comment(t.text.clone(), CommentType::MultiLine));
            }
            _ => {}
        }
    }
    Doc::concat(docs)
}

fn print_leading_trivia_of_token(ctx: &mut Ctx, node: Node, index: usize) -> Doc {
    let leading = &ctx.tokens.list[index].leading;
    let is_closing_brace = node.kind() == "}"
        || (node.parent().map(|p| p.kind()) == Some("collection_expression")
            && node.kind() == "]");

    let mut printed = private_print_leading_trivia(ctx, leading, false, is_closing_brace);

    let has_directive = leading.any_directive();
    if has_directive {
        // the leading trivia "always fits" for purposes of deciding when to break lines, so a
        // method call after a false #if directive doesn't break when it actually fits
        printed = Doc::always_fits(printed);
    }

    if node.kind() != "}" {
        return printed;
    }

    let mut extra_new_lines = Doc::Null;
    if has_directive || leading.any_comment() {
        extra_new_lines = extra_new_lines_for_trivia(leading);
    }

    if !matches!(printed, Doc::Null) || !matches!(extra_new_lines, Doc::Null) {
        let has_printed = !matches!(printed, Doc::Null);
        Doc::concat(vec![
            extra_new_lines,
            Doc::indent_if(has_printed, printed),
            Doc::hard_line(),
        ])
    } else {
        printed
    }
}

pub fn print_leading_trivia(ctx: &mut Ctx, node: Node) -> Doc {
    let index = ctx.token_index(node);
    print_leading_trivia_of_token(ctx, node, index)
}

pub fn print_leading_trivia_list(ctx: &mut Ctx, trivia: &[Trivia]) -> Doc {
    private_print_leading_trivia(ctx, trivia, false, false)
}

pub fn print_leading_trivia_with_new_lines(ctx: &mut Ctx, trivia: &[Trivia]) -> Doc {
    private_print_leading_trivia(ctx, trivia, true, false)
}

fn private_print_leading_trivia(
    ctx: &mut Ctx,
    leading: &[Trivia],
    include_initial_new_lines: bool,
    skip_last_hardline: bool,
) -> Doc {
    if leading.is_empty() {
        return Doc::Null;
    }

    if !include_initial_new_lines
        && !ctx.state.next_trivia_needs_line
        && is_only_whitespace(leading)
    {
        return Doc::Null;
    }

    let mut docs: Vec<Doc> = Vec::new();

    // we don't print any new lines until we run into a comment or directive
    // the extra_new_lines method takes care of printing the initial new lines for a given node
    let mut print_new_lines = include_initial_new_lines;

    let mut x = 0;
    while x < leading.len() {
        let trivia = &leading[x];
        let kind = trivia.kind;

        if print_new_lines && kind == TriviaKind::EndOfLine {
            if docs.last().map(is_hard_line_for_trivia).unwrap_or(false) {
                print_new_lines = false;
            }

            let n = docs.len();
            let after_single_line_comment = n > 1
                && is_hard_line_for_trivia(&docs[n - 1])
                && matches!(&docs[n - 2], Doc::LeadingComment(c) if c.ty == CommentType::SingleLine);
            if !after_single_line_comment {
                docs.push(Doc::hard_line_for_trivia());
            }
        }
        if !matches!(kind, TriviaKind::EndOfLine | TriviaKind::Whitespace) {
            print_new_lines = true;
        }

        let add_leading_comment = |docs: &mut Vec<Doc>, ty: CommentType| {
            let mut comment = trivia.text.trim_end_matches(['\n', '\r']).to_string();
            if ty == CommentType::MultiLine && x > 0 && leading[x - 1].kind == TriviaKind::Whitespace {
                comment = format!("{}{}", leading[x - 1].text, comment);
            }
            docs.push(Doc::leading_comment(comment, ty));
        };

        match kind {
            TriviaKind::SingleLineComment | TriviaKind::SingleLineDocComment => {
                add_leading_comment(&mut docs, CommentType::SingleLine);
                docs.push(if kind == TriviaKind::SingleLineDocComment {
                    Doc::hard_line_for_trivia()
                } else {
                    Doc::Null
                });
            }
            TriviaKind::MultiLineComment | TriviaKind::MultiLineDocComment => {
                add_leading_comment(&mut docs, CommentType::MultiLine);
            }
            TriviaKind::DisabledText => {
                docs.push(Doc::Trim);
                docs.push(Doc::text(trivia.text.clone()));
            }
            TriviaKind::Directive(dk)
                if matches!(
                    dk,
                    crate::trivia::DirectiveKind::Region | crate::trivia::DirectiveKind::EndRegion
                ) =>
            {
                let text = trivia.text.trim_end_matches(['\n', '\r']).to_string();
                docs.push(Doc::hard_line_if_no_previous_line());
                docs.push(Doc::Trim);
                docs.push(if dk == crate::trivia::DirectiveKind::Region {
                    Doc::begin_region(text)
                } else {
                    Doc::end_region(text)
                });
                docs.push(Doc::hard_line());
            }
            TriviaKind::Directive(dk) => {
                let text = trivia.text.trim_end_matches(['\n', '\r']).to_string();
                // adding two of these to ensure we get a new line when a directive follows a trailing comment
                docs.push(Doc::hard_line_if_no_previous_line_for_trivia());
                docs.push(Doc::hard_line_if_no_previous_line_for_trivia());
                docs.push(Doc::Trim);
                docs.push(Doc::directive(text));
                docs.push(Doc::hard_line_for_trivia());

                // keep one line after an #endif if there is at least one
                if dk == crate::trivia::DirectiveKind::EndIf {
                    if let Some(line_index) = leading.index_of_next_end_of_line(x) {
                        x = line_index;
                        docs.push(Doc::hard_line_for_trivia());
                    }
                    print_new_lines = false;
                }
            }
            _ => {}
        }
        x += 1;
    }

    while skip_last_hardline
        && docs
            .last()
            .map(|d| is_hard_line(d) || matches!(d, Doc::Null))
            .unwrap_or(false)
    {
        docs.pop();
    }

    if ctx.state.next_trivia_needs_line {
        if leading
            .iter()
            .any(|t| t.kind == TriviaKind::Directive(crate::trivia::DirectiveKind::If))
        {
            docs.insert(0, Doc::hard_line_for_trivia());
        } else {
            let mut index = docs.len() as isize - 1;
            while index >= 0
                && (is_hard_line(&docs[index as usize])
                    || matches!(docs[index as usize], Doc::LeadingComment(_) | Doc::Null))
            {
                index -= 1;
            }
            // handles an edge case where we get here but already added the line, it relates to
            // the fact that single line comments include new line directives
            let i = (index + 1) as usize;
            let already = i + 1 < docs.len() && is_hard_line(&docs[i]) && is_hard_line(&docs[i + 1]);
            if !already {
                docs.insert(i, Doc::hard_line_for_trivia());
            }
        }
        ctx.state.next_trivia_needs_line = false;
    }

    if docs.is_empty() {
        Doc::Null
    } else {
        Doc::concat(docs)
    }
}

pub fn has_comments(trivia: &[Trivia]) -> bool {
    trivia.iter().any(|t| !t.is_whitespace_or_eol())
}
