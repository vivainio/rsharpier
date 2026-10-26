//! Raw string literals and interpolated strings

use tree_sitter::Node;

use super::nodes::*;
use super::*;
use crate::doc::Doc;
use crate::{concat, group};

fn split_lines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\r' || b[i] == b'\n' {
            out.push(&s[start..i]);
            if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            start = i + 1;
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
}

/// Token.ShouldIndentRawString(argument)
fn should_indent_raw_string(node: Node) -> bool {
    let mut p = node.parent();
    while let Some(n) = p {
        if n.kind() == "argument" {
            let expr = *children(n).last().unwrap();
            return expr.kind() == "lambda_expression";
        }
        p = n.parent();
    }
    true
}

fn dedent(line: &str, indent: &str) -> String {
    // removes up to the indentation of the closing delimiter, whitespace only lines may be shorter
    let mut chars = line.chars();
    let mut removed = 0;
    let want = indent.chars().count();
    let mut rest = line;
    while removed < want {
        match chars.next() {
            Some(c) if c == ' ' || c == '\t' => {
                removed += 1;
                rest = chars.as_str();
            }
            _ => break,
        }
    }
    rest.to_string()
}

/// a multi line raw string literal token: """ ... """
pub fn raw_string_literal_doc(node: Node, text: &str) -> Doc {
    let lines_including_quotes = split_lines(text);
    let last = *lines_including_quotes.last().unwrap();
    let last_line_is_indented = last.starts_with([' ', '\t']);
    let indent: String = last.chars().take_while(|c| *c == ' ' || *c == '\t').collect();

    let mut contents = vec![
        Doc::text(lines_including_quotes[0]),
        if last_line_is_indented { Doc::hard_line_no_trim() } else { Doc::literal_line() },
    ];
    let value_lines: Vec<String> = lines_including_quotes[1..lines_including_quotes.len() - 1]
        .iter()
        .map(|l| dedent(l, &indent))
        .collect();
    for line in &value_lines {
        contents.push(Doc::text(line.clone()));
        contents.push(if last_line_is_indented {
            if line.is_empty() { Doc::hard_line() } else { Doc::hard_line_no_trim() }
        } else {
            Doc::literal_line()
        });
    }
    contents.push(Doc::text(last.trim_start()));
    Doc::indent_if(should_indent_raw_string(node), Doc::concat(contents))
}

pub fn is_raw_interpolated_multiline(ctx: &Ctx, node: Node) -> bool {
    if node.kind() != "interpolated_string_expression" {
        return false;
    }
    let kids = children(node);
    let Some(quote) = kids.iter().find(|c| c.kind() == "interpolation_quote") else {
        return false;
    };
    let after = &ctx.src[quote.end_byte()..node.end_byte()];
    after.starts_with('\n') || after.starts_with("\r\n")
}

pub fn interpolated_string_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    // if any of the expressions in the interpolation contain a newline then don't force this flat
    if kids
        .iter()
        .any(|k| k.kind() == "interpolation" && ctx.text(*k).contains('\n'))
    {
        let leading = print_leading_trivia_list(ctx, &ctx.leading_trivia(node).to_vec());
        return concat![leading, Doc::text(ctx.text(node))];
    }
    if is_raw_interpolated_multiline(ctx, node) {
        return raw_interpolated(ctx, node, &kids);
    }

    let verbatim = ctx.text(kids[0]).contains('@');
    let mut docs = Vec::new();
    // the start token is `$"` and is made of two tokens in tree-sitter
    let mut i = 0;
    docs.push(print_token_without_leading_trivia(ctx, kids[0]));
    i += 1;
    docs.push(print_token(ctx, kids[1]));
    i += 1;
    while i + 1 < kids.len() {
        docs.push(content_doc(ctx, kids[i], verbatim));
        i += 1;
    }
    let end = *kids.last().unwrap();
    docs.push(print_token_with(
        ctx,
        end,
        Doc::Null,
        TokenOptions { skip_trailing: true, ..Default::default() },
    ));

    // pull out the trivia so it doesn't get forced flat
    let first_leading = print_leading_trivia(ctx, kids[0]);
    let end_idx = ctx.token_index(end);
    let trailing = print_trailing_trivia(&ctx.tokens.list[end_idx].trailing);
    concat![first_leading, Doc::force_flat(docs), trailing]
}

fn content_doc(ctx: &mut Ctx, node: Node, verbatim: bool) -> Doc {
    match node.kind() {
        "interpolation" => interpolation(ctx, node),
        _ => {
            let text = ctx.text(node);
            if verbatim && text.contains('\n') {
                let lines: Vec<Doc> = text.replace('\r', "").split('\n').map(Doc::text).collect();
                Doc::join(Doc::literal_line(), lines)
            } else {
                print_token(ctx, node)
            }
        }
    }
}

pub fn interpolation(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let mut docs = Vec::new();
    for c in &kids {
        match c.kind() {
            "interpolation_brace" => docs.push(print_token(ctx, *c)),
            "interpolation_alignment_clause" => {
                let k = children(*c);
                docs.push(print_token_with_suffix(ctx, k[0], " "));
                docs.push(print(ctx, k[1]));
            }
            "interpolation_format_clause" => docs.push(print_token(ctx, *c)),
            _ => docs.push(print(ctx, *c)),
        }
    }
    Doc::concat(docs)
}

fn raw_interpolated(ctx: &mut Ctx, node: Node, kids: &[Node]) -> Doc {
    let end = *kids.last().unwrap();
    let quote_idx = kids.iter().position(|c| c.kind() == "interpolation_quote").unwrap();
    let inner = &kids[quote_idx + 1..kids.len() - 1];

    // the indentation of the closing delimiter is the last line of the last text
    let last_text = inner.last().filter(|n| n.kind() != "interpolation").map(|n| ctx.text(*n));
    let closing_line = last_text.and_then(|t| t.rsplit('\n').next()).unwrap_or("");
    let end_delimiter_is_indented = closing_line.starts_with([' ', '\t']);
    let indent: String = closing_line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();

    let start = concat![print_token(ctx, kids[0]), print_token(ctx, kids[quote_idx])];
    let mut contents = vec![
        start,
        if end_delimiter_is_indented { Doc::hard_line_no_trim() } else { Doc::literal_line() },
    ];

    let mut at_line_start = true;
    let mut first_text = true;
    let mut last_is_empty_string = false;
    for (n_idx, c) in inner.iter().enumerate() {
        if c.kind() == "interpolation" {
            contents.push(interpolation(ctx, *c));
            at_line_start = false;
            last_is_empty_string = false;
            continue;
        }
        let mut text = ctx.text(*c).replace('\r', "");
        if first_text {
            first_text = false;
            // the newline directly after the opening delimiter is not content
            if let Some(rest) = text.strip_prefix('\n') {
                text = rest.to_string();
            }
        }
        let is_last_text = n_idx + 1 == inner.len();
        let mut value = String::new();
        for (li, line) in text.split('\n').enumerate() {
            if li > 0 {
                value.push('\n');
                at_line_start = true;
            }
            if at_line_start {
                value.push_str(&dedent(line, &indent));
            } else {
                value.push_str(line);
            }
            if !line.is_empty() || li > 0 {
                at_line_start = false;
            }
            if li > 0 && line.is_empty() {
                at_line_start = true;
            }
        }
        if is_last_text {
            // the closing delimiter line is not content
            if let Some(pos) = value.rfind('\n') {
                value.truncate(pos + 1);
            }
        }
        if value.is_empty() {
            continue;
        }
        let lines: Vec<&str> = value.split('\n').collect();
        for (index, line) in lines.iter().enumerate() {
            contents.push(Doc::text(*line));
            last_is_empty_string = line.is_empty();
            if index == lines.len() - 1 {
                continue;
            }
            contents.push(if end_delimiter_is_indented {
                if line.is_empty() { Doc::hard_line() } else { Doc::hard_line_no_trim() }
            } else {
                Doc::literal_line()
            });
        }
    }
    contents.push(if end_delimiter_is_indented {
        if last_is_empty_string { Doc::hard_line() } else { Doc::hard_line_no_trim() }
    } else {
        Doc::literal_line()
    });
    contents.push(print_token_with_end_delimiter(ctx, end));
    Doc::indent_if(should_indent_raw_string(node), Doc::concat(contents))
}

fn print_token_with_end_delimiter(ctx: &mut Ctx, end: Node) -> Doc {
    print_token(ctx, end)
}

#[allow(dead_code)]
fn unused(_: Doc) -> Doc {
    group!(Doc::Null)
}
