//! Statement printers

use tree_sitter::Node;

use super::nodes::*;
use super::*;
use crate::doc::Doc;
use crate::{concat, group, indent};

fn body_of_loop(ctx: &mut Ctx, node: Node, body: Node) -> Doc {
    if body.kind() == node.kind() {
        group!(Doc::hard_line(), print(ctx, body))
    } else {
        optional_braces(ctx, body)
    }
}


pub fn while_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let kw_while = kw(node, "while").unwrap();
    let open = kw(node, "(").unwrap();
    let close = kw(node, ")").unwrap();
    let condition = field(node, "condition").unwrap();
    let body = field(node, "body").unwrap();
    concat![
        extra_new_lines(ctx, node),
        group!(
            print_token(ctx, kw_while),
            " ",
            print_token(ctx, open),
            group!(indent!(Doc::soft_line(), print(ctx, condition)), Doc::soft_line()),
            print_token(ctx, close)
        ),
        body_of_loop(ctx, node, body)
    ]
}

pub fn do_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let body = field(node, "body").unwrap();
    let is_block = body.kind() == "block";
    let do_kw = kw(node, "do").unwrap();
    let while_kw = kw(node, "while").unwrap();
    let open = kw(node, "(").unwrap();
    let close = kw(node, ")").unwrap();
    let condition = field(node, "condition").unwrap();
    concat![
        extra_new_lines(ctx, node),
        print_token_with_suffix(ctx, do_kw, if is_block { Doc::Null } else { Doc::text(" ") }),
        print(ctx, body),
        if is_block { Doc::text(" ") } else { Doc::hard_line() },
        print_token_with_suffix(ctx, while_kw, " "),
        print_token(ctx, open),
        group!(indent!(Doc::soft_line(), print(ctx, condition)), Doc::soft_line()),
        print_token(ctx, close),
        print_token(ctx, kw(node, ";").unwrap())
    ]
}

pub fn for_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let for_kw = kids[0];
    let open = kids[1];
    let semis: Vec<Node> = kids.iter().copied().filter(|c| c.kind() == ";").collect();
    let close = kids.iter().copied().find(|c| c.kind() == ")").unwrap();
    let body = field(node, "body").unwrap();

    // everything between ( and the first ; is the declaration or initializers
    let first_semi = semis[0].start_byte();
    let second_semi = semis[1].start_byte();
    let initializers: Vec<Node> = kids[2..]
        .iter()
        .copied()
        .filter(|c| c.end_byte() <= first_semi)
        .collect();
    let condition = kids
        .iter()
        .copied()
        .find(|c| c.start_byte() > first_semi && c.end_byte() <= second_semi);
    let updates: Vec<Node> = kids
        .iter()
        .copied()
        .filter(|c| c.start_byte() > second_semi && c.end_byte() <= close.start_byte())
        .collect();

    let init_doc = if initializers.len() == 1 && initializers[0].kind() == "variable_declaration" {
        variable_declaration(ctx, initializers[0])
    } else {
        let items: Vec<Node> = initializers.iter().copied().filter(|c| c.kind() != ",").collect();
        let seps: Vec<Node> = initializers.iter().copied().filter(|c| c.kind() == ",").collect();
        separated_list(ctx, &items, &seps, print, Doc::text(" "), 0)
    };
    let items: Vec<Node> = updates.iter().copied().filter(|c| c.kind() != ",").collect();
    let seps: Vec<Node> = updates.iter().copied().filter(|c| c.kind() == ",").collect();

    let first_part = group!(init_doc, print_token(ctx, semis[0]));
    let condition_doc = match condition {
        Some(c) => concat![Doc::line(), print(ctx, c)],
        None => Doc::line(),
    };
    let update_doc = group!(indent!(separated_list(ctx, &items, &seps, print, Doc::line(), 0)));
    concat![
        extra_new_lines(ctx, node),
        group!(
            print_token(ctx, for_kw),
            " ",
            print_token(ctx, open),
            group!(
                indent!(
                    Doc::soft_line(),
                    first_part,
                    condition_doc,
                    print_token(ctx, semis[1]),
                    Doc::line(),
                    update_doc
                ),
                Doc::soft_line()
            ),
            print_token(ctx, close)
        ),
        body_of_loop(ctx, node, body)
    ]
}

pub fn foreach_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let await_kw = kw(node, "await");
    let foreach_kw = kw(node, "foreach").unwrap();
    let open = kw(node, "(").unwrap();
    let in_kw = kw(node, "in").unwrap();
    let close = kw(node, ")").unwrap();
    let body = field(node, "body").unwrap();
    let left = print_child_pub(ctx, field(node, "left").unwrap());
    let variable = match field(node, "type") {
        Some(t) => concat![print(ctx, t), " ", left],
        None => left,
    };
    let right = field(node, "right").unwrap();
    concat![
        extra_new_lines(ctx, node),
        group!(
            print_token_opt(ctx, await_kw),
            if await_kw.is_some() { Doc::text(" ") } else { Doc::Null },
            print_token(ctx, foreach_kw),
            " ",
            print_token(ctx, open),
            group!(
                indent!(
                    Doc::soft_line(),
                    variable,
                    " ",
                    print_token(ctx, in_kw),
                    " ",
                    print(ctx, right)
                ),
                Doc::soft_line()
            ),
            print_token(ctx, close)
        ),
        body_of_loop(ctx, node, body)
    ]
}

pub fn using_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let await_kw = kw(node, "await");
    let using_kw = kw(node, "using").unwrap();
    let open = kw(node, "(").unwrap();
    let close = kw(node, ")").unwrap();
    let body = field(node, "body").unwrap();
    let inner: Vec<Node> = kids
        .iter()
        .copied()
        .filter(|c| c.start_byte() > open.start_byte() && c.end_byte() <= close.start_byte() && c.id() != close.id())
        .collect();
    let inner_docs: Vec<Doc> = inner.iter().map(|c| print_child_pub(ctx, *c)).collect();
    let mut docs = vec![extra_new_lines(ctx, node)];
    docs.push(group!(
        print_token_opt(ctx, await_kw),
        if await_kw.is_some() { Doc::text(" ") } else { Doc::Null },
        print_token(ctx, using_kw),
        " ",
        print_token(ctx, open),
        group!(indent!(Doc::soft_line(), Doc::concat(inner_docs)), Doc::soft_line()),
        print_token(ctx, close),
        Doc::if_break(Doc::Null, Doc::soft_line(), None)
    ));
    match body.kind() {
        "using_statement" => {
            docs.push(Doc::hard_line());
            docs.push(print(ctx, body));
        }
        "block" => docs.push(block(ctx, body)),
        _ => docs.push(Doc::indent(vec![Doc::hard_line(), print(ctx, body)])),
    }
    Doc::concat(docs)
}

pub fn lock_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let statement = *kids.last().unwrap();
    let printed = print(ctx, statement);
    concat![
        extra_new_lines(ctx, node),
        print_token_with_suffix(ctx, kids[0], " "),
        print_token(ctx, kids[1]),
        print(ctx, kids[2]),
        print_token(ctx, kids[3]),
        if statement.kind() == "block" { printed } else { Doc::indent(vec![Doc::hard_line(), printed]) }
    ]
}

pub fn try_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = vec![extra_new_lines(ctx, node)];
    docs.push(print_attribute_lists_pub(ctx, node));
    docs.push(print_token(ctx, kw(node, "try").unwrap()));
    docs.push(block(ctx, field(node, "body").unwrap()));
    let catches = children_of_kind(node, "catch_clause");
    if !catches.is_empty() {
        docs.push(Doc::hard_line());
    }
    let printed: Vec<Doc> = catches.iter().map(|c| catch_clause(ctx, *c)).collect();
    docs.push(Doc::join(Doc::hard_line(), printed));
    if let Some(f) = child_of_kind(node, "finally_clause") {
        docs.push(Doc::hard_line());
        let kids = children(f);
        docs.push(concat![print_token(ctx, kids[0]), print(ctx, kids[1])]);
    }
    Doc::concat(docs)
}

fn catch_clause(ctx: &mut Ctx, node: Node) -> Doc {
    let catch_kw = kw(node, "catch").unwrap();
    let mut inner = Vec::new();
    if let Some(decl) = child_of_kind(node, "catch_declaration") {
        let kids = children(decl);
        let mut d = vec![Doc::text(" "), print_token(ctx, kids[0]), print(ctx, field(decl, "type").unwrap())];
        if let Some(name) = field(decl, "name") {
            d.push(Doc::text(" "));
            d.push(print_token(ctx, name));
        }
        d.push(print_token(ctx, *kids.last().unwrap()));
        inner.push(Doc::concat(d));
    }
    if let Some(filter) = child_of_kind(node, "catch_filter_clause") {
        let kids = children(filter);
        inner.push(Doc::indent(vec![
            Doc::line(),
            print_token_with_suffix(ctx, kids[0], " "),
            print_token(ctx, kids[1]),
            group!(Doc::indent(vec![print(ctx, kids[2])]), Doc::soft_line()),
            print_token(ctx, kids[3]),
        ]));
    }
    concat![
        print_token(ctx, catch_kw),
        Doc::group(inner),
        block(ctx, field(node, "body").unwrap())
    ]
}

pub fn throw_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let expression = if kids.len() > 2 {
        concat![" ", print(ctx, kids[1])]
    } else {
        Doc::Null
    };
    concat![
        extra_new_lines(ctx, node),
        print_token(ctx, kids[0]),
        expression,
        print_token(ctx, *kids.last().unwrap())
    ]
}

pub fn yield_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let expression = if kids.len() > 3 {
        concat![" ", print(ctx, kids[2])]
    } else {
        Doc::Null
    };
    concat![
        extra_new_lines(ctx, node),
        print_token_with_suffix(ctx, kids[0], " "),
        print_token(ctx, kids[1]),
        expression,
        print_token(ctx, *kids.last().unwrap())
    ]
}

/// break, continue, empty statements and the like: just their tokens, with extra new lines
pub fn simple_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = vec![extra_new_lines(ctx, node)];
    let kids = children(node);
    for (i, c) in kids.iter().enumerate() {
        let is_last = i + 1 == kids.len();
        if !is_last && c.kind() != ";" && kids.len() > 2 {
            docs.push(print_token_with_suffix(ctx, *c, " "));
        } else if c.is_named() && c.child_count() > 0 {
            docs.push(print(ctx, *c));
        } else {
            docs.push(print_token(ctx, *c));
        }
    }
    Doc::concat(docs)
}
