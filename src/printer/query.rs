//! Query expressions

use tree_sitter::Node;

use super::nodes::*;
use super::*;
use crate::doc::Doc;
use crate::{concat, group, indent};

pub fn query_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let from = query_clause(ctx, kids[0]);
    concat![from, Doc::line(), query_body(ctx, &kids[1..])]
}

fn is_select_or_group(n: Node) -> bool {
    matches!(n.kind(), "select_clause" | "group_clause")
}

/// `rest` are the clauses after the first from, tree-sitter keeps a continuation flat
fn query_body(ctx: &mut Ctx, rest: &[Node]) -> Doc {
    let terminal = rest.iter().position(|n| is_select_or_group(*n)).unwrap_or(rest.len() - 1);
    let clauses = &rest[..terminal];
    let mut docs = Vec::new();
    let printed: Vec<Doc> = clauses.iter().map(|c| query_clause(ctx, *c)).collect();
    docs.push(Doc::join(Doc::line(), printed));
    if !clauses.is_empty() {
        docs.push(Doc::line());
    }
    docs.push(query_clause(ctx, rest[terminal]));
    let after = &rest[terminal + 1..];
    if let Some(into) = after.first().filter(|n| n.kind() == "into") {
        // continuation: into identifier body
        docs.push(Doc::text(" "));
        docs.push(print_token_with_suffix(ctx, *into, " "));
        docs.push(print_token_with_suffix(ctx, after[1], Doc::line()));
        docs.push(query_body(ctx, &after[2..]));
    }
    Doc::concat(docs)
}

fn query_clause(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    match node.kind() {
        "from_clause" => {
            // from [type] name in expression
            let mut docs = Vec::new();
            for c in &kids {
                if c.is_named() && c.kind() != "identifier" || matches!(c.kind(), "from" | "in") || c.kind() == "identifier" {
                    let is_last = c.id() == kids.last().unwrap().id();
                    if is_last {
                        docs.push(print(ctx, *c));
                    } else if c.is_named() && c.kind() != "identifier" {
                        // the optional type
                        docs.push(concat![print(ctx, *c), " "]);
                    } else {
                        docs.push(print_token_with_suffix(ctx, *c, " "));
                    }
                }
            }
            Doc::concat(docs)
        }
        "select_clause" => concat![
            extra_new_lines(ctx, node),
            print_token_with_suffix(ctx, kids[0], " "),
            print(ctx, kids[1])
        ],
        "where_clause" => concat![
            extra_new_lines(ctx, node),
            group!(print_token(ctx, kids[0]), indent!(Doc::line(), print(ctx, kids[1])))
        ],
        "let_clause" => concat![
            extra_new_lines(ctx, node),
            print_token_with_suffix(ctx, kids[0], " "),
            print_token_with_suffix(ctx, kids[1], " "),
            print_token_with_suffix(ctx, kids[2], " "),
            print(ctx, kids[3])
        ],
        "group_clause" => concat![
            extra_new_lines(ctx, node),
            print_token_with_suffix(ctx, kids[0], " "),
            print(ctx, kids[1]),
            " ",
            print_token_with_suffix(ctx, kids[2], " "),
            print(ctx, kids[3])
        ],
        "order_by_clause" => {
            let mut docs = vec![extra_new_lines(ctx, node), print_token(ctx, kids[0])];
            // orderings: expression [ascending|descending] separated by commas
            let mut i = 1;
            while i < kids.len() {
                let c = kids[i];
                match c.kind() {
                    "," => docs.push(print_token(ctx, c)),
                    "ascending" | "descending" => {
                        docs.push(Doc::text(" "));
                        docs.push(print_token(ctx, c));
                    }
                    _ => {
                        docs.push(Doc::text(" "));
                        docs.push(print(ctx, c));
                    }
                }
                i += 1;
            }
            Doc::concat(docs)
        }
        "join_clause" => {
            // join [type] id in inExpr on left equals right [into id]
            let mut idx = 1;
            let mut head = vec![print_token_with_suffix(ctx, kids[0], " ")];
            if kids[idx].is_named() && kids[idx].kind() != "identifier" || (kids[idx].kind() == "identifier" && kids[idx + 1].kind() == "identifier") {
                head.push(concat![print(ctx, kids[idx]), " "]);
                idx += 1;
            }
            head.push(print_token_with_suffix(ctx, kids[idx], " "));
            head.push(print_token_with_suffix(ctx, kids[idx + 1], " "));
            head.push(print(ctx, kids[idx + 2]));
            let on = kids[idx + 3];
            let left = kids[idx + 4];
            let equals = kids[idx + 5];
            let right = kids[idx + 6];
            let mut inner = vec![
                Doc::line(),
                print_token_with_suffix(ctx, on, " "),
                print(ctx, left),
                Doc::text(" "),
                print_token_with_suffix(ctx, equals, " "),
                print(ctx, right),
            ];
            if let Some(into) = kids.get(idx + 7) {
                let k = children(*into);
                inner.push(Doc::line());
                inner.push(print_token_with_suffix(ctx, k[0], " "));
                inner.push(print_token(ctx, k[1]));
            }
            head.push(Doc::indent(inner));
            concat![extra_new_lines(ctx, node), Doc::group(head)]
        }
        "from_clause_nested" => Doc::Null,
        _ => {
            ctx.unhandled.push(node.kind().to_string());
            Doc::text(ctx.text(node))
        }
    }
}
