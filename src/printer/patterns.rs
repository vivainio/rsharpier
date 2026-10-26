//! Patterns, switch statements and switch expressions

use tree_sitter::Node;

use super::nodes::*;
use super::*;
use crate::doc::Doc;
use crate::trivia::TriviaListExt;
use crate::{concat, group, indent};

fn is_pattern_kind(kind: &str) -> bool {
    kind.ends_with("_pattern")
}

pub fn is_pattern_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");
    let expression = field(node, "expression").unwrap();
    let is_kw = kw(node, "is").unwrap();
    let pattern = field(node, "pattern").unwrap();

    if !matches!(
        parent_kind,
        "if_statement" | "parenthesized_expression" | "equals_value_clause" | "variable_declarator"
    ) {
        return group!(
            print(ctx, expression),
            indent!(Doc::line(), print_token(ctx, is_kw), " ", print(ctx, pattern))
        );
    }
    if pattern.kind() != "recursive_pattern" {
        return group!(
            print(ctx, expression),
            Doc::line(),
            print_token(ctx, is_kw),
            " ",
            print(ctx, pattern)
        );
    }
    match field(pattern, "type") {
        None => group!(
            print(ctx, expression),
            " ",
            print_token(ctx, is_kw),
            Doc::line(),
            recursive_pattern(ctx, pattern, true)
        ),
        Some(ty) => group!(
            group!(
                print(ctx, expression),
                Doc::line(),
                print_token(ctx, is_kw),
                " ",
                print(ctx, ty)
            ),
            recursive_pattern(ctx, pattern, false)
        ),
    }
}

pub fn declaration_pattern(ctx: &mut Ctx, node: Node) -> Doc {
    concat![
        print(ctx, field(node, "type").unwrap()),
        " ",
        print_child_pub(ctx, field(node, "name").unwrap())
    ]
}

pub fn negated_pattern(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    concat![print_token_with_suffix(ctx, kids[0], " "), print(ctx, kids[1])]
}

pub fn relational_pattern(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    concat![print_token_with_suffix(ctx, kids[0], " "), print(ctx, kids[1])]
}

pub fn binary_pattern(ctx: &mut Ctx, node: Node) -> Doc {
    let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");
    let left = field(node, "left").unwrap();
    let op = field(node, "operator").unwrap();
    let right = field(node, "right").unwrap();
    Doc::indent_if(
        matches!(parent_kind, "subpattern" | "is_pattern_expression"),
        concat![
            print(ctx, left),
            Doc::line(),
            print_token_with_suffix(ctx, op, " "),
            print(ctx, right)
        ],
    )
}

fn subpattern_positional(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    if kids.len() == 3 {
        let pattern = kids[2];
        let suffix = if pattern.kind() == "recursive_pattern" && field(pattern, "type").is_none() {
            Doc::line()
        } else {
            Doc::text(" ")
        };
        return concat![print(ctx, kids[0]), print_token_with_suffix(ctx, kids[1], suffix), print(ctx, pattern)];
    }
    Doc::concat(kids.iter().map(|c| print(ctx, *c)).collect())
}

fn subpattern_property(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let mut docs = Vec::new();
    // [expression ":"] pattern
    if kids.len() == 3 {
        docs.push(print(ctx, kids[0]));
        let pattern = kids[2];
        let colon_suffix = if pattern.kind() == "recursive_pattern" && field(pattern, "type").is_none() {
            Doc::line()
        } else {
            Doc::text(" ")
        };
        docs.push(print_token_with_suffix(ctx, kids[1], colon_suffix));
        docs.push(print(ctx, pattern));
    } else {
        docs.push(print(ctx, kids[0]));
    }
    Doc::group(docs)
}

pub fn recursive_pattern(ctx: &mut Ctx, node: Node, include_type: bool) -> Doc {
    let mut result = Vec::new();
    let ty = field(node, "type");
    if let (Some(t), true) = (ty, include_type) {
        result.push(print(ctx, t));
    }
    if let Some(pos) = child_of_kind(node, "positional_pattern_clause") {
        let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");
        let gp_kind = node.parent().and_then(|p| p.parent()).map(|p| p.kind()).unwrap_or("");
        let no_soft = matches!(parent_kind, "switch_expression_arm" | "is_pattern_expression" | "switch_section")
            || (matches!(parent_kind, "and_pattern" | "or_pattern")
                && matches!(gp_kind, "switch_expression_arm" | "switch_section"));
        let kids = children(pos);
        let (items, seps) = split_commas(&kids[1..kids.len() - 1]);
        result.push(if no_soft { Doc::Null } else { Doc::soft_line() });
        result.push(group!(
            print_token(ctx, kids[0]),
            indent!(
                Doc::soft_line(),
                separated_list(ctx, &items, &seps, subpattern_positional, Doc::line(), 0)
            ),
            Doc::soft_line(),
            print_token(ctx, *kids.last().unwrap())
        ));
    }
    if let Some(prop) = child_of_kind(node, "property_pattern_clause") {
        let kids = children(prop);
        let (items, seps) = split_commas(&kids[1..kids.len() - 1]);
        let open = kids[0];
        let has_trivia = ctx.leading_trivia(open).any_comment_or_directive();
        result.push(group!(
            if ty.is_some() && !has_trivia { Doc::line() } else { Doc::Null },
            print_token(ctx, open),
            indent!(
                if items.is_empty() { Doc::Null } else { Doc::line() },
                separated_list(ctx, &items, &seps, subpattern_property, Doc::line(), 0)
            ),
            Doc::line(),
            print_token(ctx, *kids.last().unwrap())
        ));
    }
    if let Some(name) = field(node, "name") {
        result.push(Doc::text(" "));
        result.push(print_child_pub(ctx, name));
    }
    Doc::concat(result)
}

pub fn split_commas<'t>(nodes: &[Node<'t>]) -> (Vec<Node<'t>>, Vec<Node<'t>>) {
    (
        nodes.iter().copied().filter(|c| c.kind() != ",").collect(),
        nodes.iter().copied().filter(|c| c.kind() == ",").collect(),
    )
}

pub fn when_clause(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let parent = node.parent().unwrap();
    let after_discard = parent.kind() == "switch_expression_arm"
        && named_children(parent).first().map(|c| c.kind() == "discard").unwrap_or(false);
    group!(Doc::indent(vec![
        if after_discard { Doc::text(" ") } else { Doc::line() },
        print_token_with_suffix(ctx, kids[0], " "),
        print(ctx, kids[1])
    ]))
}

// ---- switch ---------------------------------------------------------------------------------

pub fn switch_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let body = field(node, "body").unwrap();
    let bkids = children(body);
    let open_brace = bkids[0];
    let close_brace = *bkids.last().unwrap();
    let sections_nodes: Vec<Node> = bkids[1..bkids.len() - 1].to_vec();

    let sections = if sections_nodes.is_empty() {
        Doc::text(" ")
    } else {
        let printed: Vec<Doc> = sections_nodes.iter().map(|s| switch_section(ctx, *s)).collect();
        concat![
            Doc::indent(vec![concat![Doc::hard_line(), Doc::join(Doc::hard_line(), printed)]]),
            Doc::hard_line()
        ]
    };
    let sections = remove_initial_double_hard_line(sections);
    let id = Doc::next_group_id();
    let open = kw(node, "(");
    let close = kw(node, ")");
    let value = field(node, "value").unwrap();
    concat![
        extra_new_lines(ctx, node),
        group!(
            print_token(ctx, kw(node, "switch").unwrap()),
            " ",
            print_token_opt(ctx, open),
            Doc::group_with_id(id, vec![Doc::indent(vec![Doc::soft_line(), print(ctx, value)]), Doc::soft_line()]),
            print_token_opt(ctx, close),
            if sections_nodes.is_empty() { Doc::text(" ") } else { Doc::line() },
            print_token(ctx, open_brace),
            sections,
            print_token(ctx, close_brace)
        )
    ]
}

fn switch_label(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    if kids[0].kind() == "default" {
        return concat![
            extra_new_lines(ctx, node),
            print_token(ctx, kids[0]),
            print_token(ctx, kids.iter().find(|c| c.kind() == ":").copied().unwrap())
        ];
    }
    let colon = kids.iter().find(|c| c.kind() == ":").copied().unwrap();
    let pattern = kids[1];
    let when = child_of_kind(node, "when_clause");
    if pattern.kind() == "constant_pattern" && when.is_none() {
        let value = children(pattern)[0];
        return concat![
            extra_new_lines(ctx, node),
            print_token_with_suffix(ctx, kids[0], " "),
            Doc::group_of(print(ctx, value)),
            print_token(ctx, colon)
        ];
    }
    group!(
        extra_new_lines(ctx, node),
        print_token_with_suffix(ctx, kids[0], " "),
        print(ctx, pattern),
        match when {
            Some(w) => when_clause(ctx, w),
            None => Doc::Null,
        },
        print_token(ctx, colon)
    )
}

fn switch_section(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let colon_pos = kids.iter().position(|c| c.kind() == ":").unwrap();
    let statements: Vec<Node> = kids[colon_pos + 1..].to_vec();
    let mut docs = vec![switch_label(ctx, node)];
    if statements.len() == 1 && statements[0].kind() == "block" {
        docs.push(block(ctx, statements[0]));
    } else if !statements.is_empty() {
        let printed: Vec<Doc> = ignore::print_nodes_respecting_range_ignore(ctx, &statements);
        docs.push(Doc::indent(vec![
            if statements[0].kind() == "block" { Doc::Null } else { Doc::hard_line() },
            Doc::join(Doc::hard_line(), printed),
        ]));
    }
    Doc::concat(docs)
}

pub fn switch_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let governing = kids[0];
    let switch_kw = kw(node, "switch").unwrap();
    let open = kw(node, "{").unwrap();
    let close = *kids.last().unwrap();
    let inner: Vec<Node> = kids
        .iter()
        .copied()
        .filter(|c| c.start_byte() > open.start_byte() && c.id() != close.id())
        .collect();
    let (arms, seps) = split_commas(&inner);
    let sections = group!(
        indent!(
            Doc::hard_line(),
            separated_list_with_closing(ctx, &arms, &seps, switch_arm, Doc::hard_line(), 0, Some(close))
        ),
        Doc::hard_line()
    );
    let sections = remove_initial_double_hard_line(sections);
    concat![
        print(ctx, governing),
        " ",
        print_token(ctx, switch_kw),
        Doc::hard_line(),
        print_token(ctx, open),
        sections,
        print_token(ctx, close)
    ]
}

fn switch_arm(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let arrow = kw(node, "=>").unwrap();
    let arrow_idx = kids.iter().position(|c| c.id() == arrow.id()).unwrap();
    let expression = kids[arrow_idx + 1];
    let pattern = kids[0];
    let when = child_of_kind(node, "when_clause");
    let arrow_has_comment = ctx.leading_trivia(arrow).any_comment();
    let group_id2 = Doc::next_group_id();
    let inner = if arrow_has_comment {
        Doc::indent(vec![concat![
            Doc::line(),
            print_token_with_suffix(ctx, arrow, " "),
            print(ctx, expression)
        ]])
    } else {
        concat![
            " ",
            print_token(ctx, arrow),
            Doc::group_with_id(group_id2, vec![Doc::indent(vec![Doc::line()])]),
            Doc::indent_if_break(print(ctx, expression), group_id2)
        ]
    };
    let group_id1 = Doc::next_group_id();
    concat![
        extra_new_lines(ctx, node),
        group!(
            Doc::group_with_id(
                group_id1,
                vec![concat![
                    print(ctx, pattern),
                    match when {
                        Some(w) => when_clause(ctx, w),
                        None => Doc::Null,
                    }
                ]]
            ),
            inner
        )
    ]
}

pub fn is_pattern(kind: &str) -> bool {
    is_pattern_kind(kind)
}
