//! Port of SyntaxPrinter/CSharpierIgnore.cs

use tree_sitter::Node;

use super::*;
use crate::doc::Doc;
use crate::trivia::{Trivia, TriviaKind};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IgnoreKind {
    Ignore,
    Start,
    End,
}

pub fn trivia_matches(trivia: &[Trivia], kind: IgnoreKind) -> bool {
    let prefix = match kind {
        IgnoreKind::Ignore => "// csharpier-ignore",
        IgnoreKind::Start => "// csharpier-ignore-start",
        IgnoreKind::End => "// csharpier-ignore-end",
    };
    trivia.iter().any(|t| {
        t.kind == TriviaKind::SingleLineComment
            && t.text.strip_prefix(prefix).map(|rest| rest.is_empty() || rest.starts_with(" -")).unwrap_or(false)
    })
}

pub fn node_matches(ctx: &Ctx, node: Node, kind: IgnoreKind) -> bool {
    trivia_matches(ctx.leading_trivia(node), kind)
}

fn is_statement(node: Node) -> bool {
    let k = node.kind();
    k == "block" || (k.ends_with("_statement") && k != "global_statement")
}

pub fn is_node_ignored(ctx: &Ctx, node: Node) -> bool {
    // handled in the method declaration and attribute list printers
    if matches!(
        node.kind(),
        "method_declaration" | "constructor_declaration" | "destructor_declaration"
            | "operator_declaration" | "conversion_operator_declaration"
    ) && child_of_kind(node, "attribute_list").is_some()
    {
        return false;
    }
    let parent_ok = node
        .parent()
        .map(|p| {
            matches!(
                p.kind(),
                "declaration_list" | "enum_member_declaration_list" | "block" | "compilation_unit" | "switch_section"
            )
        })
        .unwrap_or(false);
    parent_ok && node_matches(ctx, node, IgnoreKind::Ignore)
}

/// the text of the node including its leading and trailing trivia, like SyntaxNode.GetText()
pub fn full_text<'a>(ctx: &Ctx<'a>, node: Node) -> &'a str {
    let first = ctx.first_token_index(node);
    let last = ctx.last_token_index(node);
    let lead: usize = ctx.tokens.list[first].leading.iter().map(|t| t.text.len()).sum();
    let trail: usize = ctx.tokens.list[last].trailing.iter().map(|t| t.text.len()).sum();
    &ctx.src[ctx.tokens.list[first].start - lead..ctx.tokens.list[last].end + trail]
}

/// trim trailing whitespace + replace only existing line endings
pub fn without_formatting(code: &str, line_ending: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let bytes = code.as_bytes();
    let mut i = 0;
    let mut pending_ws_start: Option<usize> = None;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | 0x0b | 0x0c => {
                if pending_ws_start.is_none() {
                    pending_ws_start = Some(i);
                }
                i += 1;
            }
            b'\r' | b'\n' => {
                pending_ws_start = None;
                if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                out.push_str(line_ending);
                i += 1;
            }
            _ => {
                if let Some(s) = pending_ws_start.take() {
                    out.push_str(&code[s..i]);
                }
                let ch = code[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    if let Some(s) = pending_ws_start {
        out.push_str(&code[s..]);
    }
    out
}

pub fn print_node_without_formatting(ctx: &Ctx, node: Node) -> String {
    without_formatting(full_text(ctx, node), ctx.line_ending)
}

pub fn print_ignored_node(ctx: &mut Ctx, node: Node) -> Doc {
    let text = print_node_without_formatting(ctx, node).trim().to_string();
    if is_statement(node) {
        Doc::concat(vec![extra_new_lines(ctx, node), Doc::text(text)])
    } else {
        Doc::text(text)
    }
}

/// statements in a block or switch section, honouring csharpier-ignore-start/end
pub fn print_nodes_respecting_range_ignore(ctx: &mut Ctx, list: &[Node]) -> Vec<Doc> {
    let mut statements = Vec::new();
    let mut unformatted = String::new();
    let mut print_unformatted = false;
    let mut first_unformatted: Option<Node> = None;
    let mut has_unformatted = false;

    for &node in list {
        if node_matches(ctx, node, IgnoreKind::End) {
            let ignored = unformatted.trim().to_string();
            statements.push(match first_unformatted {
                Some(f) if is_statement(f) => Doc::concat(vec![extra_new_lines(ctx, f), Doc::text(ignored)]),
                _ => Doc::text(ignored),
            });
            unformatted.clear();
            has_unformatted = false;
            first_unformatted = None;
            print_unformatted = false;
        } else if node_matches(ctx, node, IgnoreKind::Start) {
            first_unformatted.get_or_insert(node);
            print_unformatted = true;
        }

        if print_unformatted {
            unformatted.push_str(&print_node_without_formatting(ctx, node));
            has_unformatted = true;
        } else {
            statements.push(super::nodes::print(ctx, node));
        }
    }

    if has_unformatted && !unformatted.is_empty() {
        let text = unformatted.trim().to_string();
        statements.push(match first_unformatted {
            Some(f) if is_statement(f) => Doc::concat(vec![extra_new_lines(ctx, f), Doc::text(text)]),
            _ => Doc::text(text),
        });
    }
    statements
}
