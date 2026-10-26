//! Expression printers

use tree_sitter::Node;

use super::nodes::*;
use super::*;
use crate::doc::Doc;
use crate::trivia::TriviaListExt;
use crate::{concat, group, indent};

fn arg_expression<'t>(arg: Node<'t>) -> Node<'t> {
    *children(arg).last().unwrap()
}

fn arg_has_name_colon(arg: Node) -> bool {
    child_of_kind(arg, "name_colon").is_some() || child_of_kind(arg, ":").is_some()
}

fn is_simple_lambda(n: Node) -> bool {
    n.kind() == "lambda_expression"
        && field(n, "parameters").map(|p| p.kind() == "implicit_parameter").unwrap_or(false)
}

fn is_parenthesized_lambda(n: Node) -> bool {
    n.kind() == "lambda_expression" && !is_simple_lambda(n)
}

fn is_anonymous_function(n: Node) -> bool {
    n.kind() == "lambda_expression" || n.kind() == "anonymous_method_expression"
}

pub fn argument(ctx: &mut Ctx, node: Node) -> Doc {
    let modifiers = argument_modifiers(ctx, node);
    let expr = print(ctx, arg_expression(node));
    if matches!(modifiers, Doc::Null) {
        expr
    } else {
        concat![modifiers, expr]
    }
}

fn argument_modifiers(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let mut docs = Vec::new();
    for c in &kids[..kids.len() - 1] {
        match c.kind() {
            ":" => docs.push(print_token_with_suffix(ctx, *c, " ")),
            "identifier" => docs.push(print_token(ctx, *c)),
            "name_colon" => {
                let nc = children(*c);
                docs.push(print(ctx, nc[0]));
                docs.push(print_token_with_suffix(ctx, nc[1], " "));
            }
            _ => docs.push(print_token_with_suffix(ctx, *c, " ")),
        }
    }
    Doc::concat(docs)
}

/// the first argument list in a method chain is indented
fn should_indent_argument_list(ctx: &Ctx, node: Node) -> bool {
    let Some(invocation) = node.parent() else { return false };
    if invocation.kind() != "invocation_expression" {
        return false;
    }
    let Some(function) = field(invocation, "function") else { return false };
    let function_ok = match function.kind() {
        "identifier" | "generic_name" => true,
        "member_access_expression" => {
            let e = field(function, "expression").unwrap();
            matches!(e.kind(), "this" | "this_expression" | "predefined_type")
                || (e.kind() == "identifier" && ctx.text(e).chars().count() <= 4)
        }
        _ => false,
    };
    if !function_ok {
        return false;
    }
    let Some(parent) = invocation.parent() else { return false };
    let is_invocation = |n: Option<Node>| n.map(|n| n.kind() == "invocation_expression").unwrap_or(false);
    is_invocation(parent.parent())
        || (parent.kind() == "postfix_unary_expression" && is_invocation(parent.parent().and_then(|p| p.parent())))
}

pub fn argument_list(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let indent_it = should_indent_argument_list(ctx, node);
    let inner = argument_list_like(ctx, node, kids[0], *kids.last().unwrap());
    Doc::group_of(Doc::indent_if(indent_it, inner))
}

fn has_comment_or_directive_between(ctx: &Ctx, start: usize, end: usize) -> bool {
    let first = ctx.tokens.first_at_or_after(start);
    for (i, t) in ctx.tokens.list.iter().enumerate().skip(first) {
        if t.start > end {
            break;
        }
        if i != first && t.leading.any_comment_or_directive() {
            return true;
        }
        if t.start < end && t.trailing.any_comment_or_directive() {
            return true;
        }
    }
    false
}

/// `list_node` has children open, arguments/commas, close
pub fn argument_list_like(ctx: &mut Ctx, list_node: Node, open: Node, close: Node) -> Doc {
    let (items, seps) = list_parts_pub(list_node);
    let args;
    let single = if items.len() == 1 { Some(items[0]) } else { None };
    let single_expr = single.map(arg_expression);

    if let Some((arg, expr)) = single.zip(single_expr).filter(|(_, e)| {
        is_simple_lambda(*e) && !ctx.leading_trivia(*e).any_comment()
    }) {
        let group_id = Doc::next_group_id();
        let head = Doc::group_with_id(
            group_id,
            vec![Doc::indent(vec![
                Doc::soft_line(),
                argument_modifiers(ctx, arg),
                lambda_head(ctx, expr),
            ])],
        );
        let body = lambda_body(ctx, expr);
        let body_kind = field(expr, "body").unwrap().kind();
        args = concat![
            head,
            Doc::if_break(Doc::indent(vec![Doc::group_of(body.clone())]), body, Some(group_id)),
            if matches!(body_kind, "block" | "object_creation_expression" | "anonymous_object_creation_expression") {
                Doc::if_break(Doc::soft_line(), Doc::Null, Some(group_id))
            } else {
                Doc::soft_line()
            }
        ];
    } else if let Some((arg, expr)) = single.zip(single_expr).filter(|(_, e)| {
        is_parenthesized_lambda(*e)
            && field(*e, "parameters").map(|p| children(p).len() == 2).unwrap_or(false)
    }) {
        let group_id = Doc::next_group_id();
        let has_block = field(expr, "body").unwrap().kind() == "block";
        args = concat![
            Doc::group_with_id(
                group_id,
                vec![Doc::indent(vec![
                    Doc::soft_line(),
                    argument_modifiers(ctx, arg),
                    lambda_head(ctx, expr),
                ])]
            ),
            Doc::indent_if_break(lambda_body(ctx, expr), group_id),
            if has_block {
                Doc::if_break(Doc::soft_line(), Doc::Null, Some(group_id))
            } else {
                Doc::soft_line()
            }
        ];
    } else if single_expr.map(|e| e.kind() == "collection_expression").unwrap_or(false)
        && !arg_has_name_colon(single.unwrap())
    {
        args = separated_list(ctx, &items, &seps, argument, Doc::line(), 0);
    } else if items.len() > 1
        && is_anonymous_function_lambda(arg_expression(*items.last().unwrap()))
        && !items[..items.len() - 1]
            .iter()
            .any(|a| is_anonymous_function(arg_expression(*a)))
        && {
            let last_lambda = arg_expression(*items.last().unwrap());
            let arrow = kw(last_lambda, "=>").unwrap();
            !has_comment_or_directive_between(ctx, open.start_byte(), arrow.start_byte())
        }
    {
        args = argument_list_with_trailing_lambda(ctx, &items, &seps);
    } else if !items.is_empty() {
        args = concat![
            Doc::indent(vec![
                Doc::soft_line(),
                separated_list(ctx, &items, &seps, argument, Doc::line(), 0)
            ]),
            Doc::soft_line()
        ];
    } else {
        args = Doc::Null;
    }

    concat![print_token(ctx, open), args, print_token(ctx, close)]
}

fn is_anonymous_function_lambda(n: Node) -> bool {
    n.kind() == "lambda_expression"
}

fn contains_break(doc: &Doc) -> bool {
    match doc {
        Doc::Group(g) => g.brk.get() || contains_break(&g.contents),
        Doc::Line(l) => l.ty == crate::doc::LineType::Hard,
        Doc::BreakParent => true,
        Doc::Indent(c) | Doc::ForceFlat(c) => contains_break(c),
        Doc::Concat(c) => c.borrow().iter().any(contains_break),
        Doc::IfBreak(i) => contains_break(&i.flat_contents),
        _ => false,
    }
}

fn argument_list_with_trailing_lambda(ctx: &mut Ctx, items: &[Node], seps: &[Node]) -> Doc {
    let chop = concat![
        Doc::indent(vec![
            Doc::soft_line(),
            separated_list(ctx, items, seps, argument, Doc::line(), 0)
        ]),
        Doc::soft_line()
    ];
    let last = *items.last().unwrap();
    let lambda = arg_expression(last);
    let body = field(lambda, "body").unwrap();
    let (body_is_braced, lambda_head_doc, lambda_body_doc);
    if is_simple_lambda(lambda) {
        body_is_braced = matches!(
            body.kind(),
            "block" | "object_creation_expression" | "anonymous_object_creation_expression"
        );
        lambda_head_doc = lambda_head(ctx, lambda);
        lambda_body_doc = lambda_body(ctx, lambda);
    } else {
        body_is_braced = body.kind() == "block";
        lambda_head_doc = lambda_head(ctx, lambda);
        lambda_body_doc = if body_is_braced {
            lambda_body(ctx, lambda)
        } else {
            Doc::indent(vec![Doc::line(), print(ctx, body)])
        };
    }

    let mut flat_parts = Vec::new();
    for x in 0..items.len() - 1 {
        flat_parts.push(argument(ctx, items[x]));
        flat_parts.push(print_token(ctx, seps[x]));
        flat_parts.push(Doc::text(" "));
    }
    flat_parts.push(argument_modifiers(ctx, last));
    flat_parts.push(lambda_head_doc);

    if flat_parts.iter().any(contains_break) {
        return chop;
    }

    let wrap = concat![
        Doc::force_flat(flat_parts),
        lambda_body_doc,
        if body_is_braced { Doc::Null } else { Doc::soft_line() }
    ];
    Doc::conditional_group(vec![wrap.clone(), wrap, chop])
}

// ---- lambdas --------------------------------------------------------------------------------

fn lambda_head(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = Vec::new();
    docs.push(print_attribute_lists_pub(ctx, node));
    docs.push(print_modifiers_pub(ctx, node));
    if let Some(ret) = field(node, "returns") {
        docs.push(concat![print(ctx, ret), " "]);
    }
    let params = field(node, "parameters").unwrap();
    docs.push(match params.kind() {
        "parameter_list" => parameter_list(ctx, params),
        _ => print_child_pub(ctx, params),
    });
    docs.push(Doc::text(" "));
    docs.push(print_token(ctx, kw(node, "=>").unwrap()));
    Doc::concat(docs)
}

fn lambda_body(ctx: &mut Ctx, node: Node) -> Doc {
    let body = field(node, "body").unwrap();
    if is_simple_lambda(node) {
        return match body.kind() {
            "block" => block(ctx, body),
            "object_creation_expression" | "anonymous_object_creation_expression" => {
                group!(" ", print(ctx, body))
            }
            _ => Doc::indent(vec![Doc::line(), print(ctx, body)]),
        };
    }
    if body.kind() == "block" {
        let statements = children(body).len() > 2;
        return concat![
            if statements { Doc::hard_line() } else { Doc::text(" ") },
            block(ctx, body)
        ];
    }
    let body_doc = print(ctx, body);
    let params_empty = field(node, "parameters").map(|p| children(p).len() == 2).unwrap_or(false);
    let in_single_arg_list = node
        .parent()
        .and_then(|p| p.parent())
        .map(|gp| gp.kind() == "argument_list" && children(gp).len() == 3)
        .unwrap_or(false);
    if params_empty && in_single_arg_list {
        return Doc::if_break(
            Doc::indent(vec![Doc::line(), body_doc.clone()]),
            Doc::group_of(Doc::indent(vec![Doc::line(), body_doc])),
            None,
        );
    }
    Doc::group_of(Doc::indent(vec![Doc::line(), body_doc]))
}

pub fn lambda_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let head = lambda_head(ctx, node);
    let body = lambda_body(ctx, node);
    if is_simple_lambda(node) {
        group!(head, body)
    } else {
        concat![head, body]
    }
}

// ---- conditional, cast ----------------------------------------------------------------------

pub fn conditional_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let when_true = field(node, "consequence").unwrap();
    let when_false = field(node, "alternative").unwrap();
    if when_true.kind() != "conditional_expression" && when_false.kind() == "conditional_expression" {
        conditional_chained(ctx, node)
    } else {
        conditional_regular(ctx, node)
    }
}

fn conditional_regular(ctx: &mut Ctx, node: Node) -> Doc {
    let when_true = field(node, "consequence").unwrap();
    let when_false = field(node, "alternative").unwrap();
    let condition = field(node, "condition").unwrap();
    let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");
    let t = if when_true.kind() == "conditional_expression" {
        conditional_regular(ctx, when_true)
    } else {
        print(ctx, when_true)
    };
    let f = if when_false.kind() == "conditional_expression" {
        conditional_regular(ctx, when_false)
    } else {
        print(ctx, when_false)
    };
    let inner = vec![
        Doc::line(),
        print_token_with_suffix(ctx, kw(node, "?").unwrap(), " "),
        Doc::concat(vec![t]),
        Doc::line(),
        print_token_with_suffix(ctx, kw(node, ":").unwrap(), " "),
        Doc::concat(vec![f]),
    ];
    let condition_doc = if parent_kind == "return_statement"
        && matches!(condition.kind(), "binary_expression" | "is_pattern_expression")
    {
        Doc::indent(vec![Doc::group_of(concat![
            Doc::if_break(Doc::soft_line(), Doc::Null, None),
            print(ctx, condition)
        ])])
    } else {
        print(ctx, condition)
    };
    let outer = vec![
        if parent_kind == "conditional_expression" { Doc::break_parent() } else { Doc::Null },
        condition_doc,
        Doc::indent(inner),
    ];
    if parent_kind == "conditional_expression" {
        Doc::concat(outer)
    } else {
        Doc::group(outer)
    }
}

fn conditional_chained(ctx: &mut Ctx, node: Node) -> Doc {
    let when_true = field(node, "consequence").unwrap();
    let when_false = field(node, "alternative").unwrap();
    let parent = node.parent();
    let parent_kind = parent.map(|p| p.kind()).unwrap_or("");
    let t = if when_true.kind() == "conditional_expression" {
        conditional_chained(ctx, when_true)
    } else {
        print(ctx, when_true)
    };
    let f = if when_false.kind() == "conditional_expression" {
        conditional_chained(ctx, when_false)
    } else {
        print(ctx, when_false)
    };
    let contents = group!(
        if parent_kind == "conditional_expression" { Doc::break_parent() } else { Doc::Null },
        group!(
            print(ctx, field(node, "condition").unwrap()),
            indent!(
                Doc::line(),
                print_token_with_suffix(ctx, kw(node, "?").unwrap(), " "),
                Doc::indent_if(when_true.kind() == "conditional_expression", t)
            )
        ),
        Doc::line(),
        group!(print_token_with_suffix(ctx, kw(node, ":").unwrap(), " "), f)
    );
    let in_multi_arg_list = parent_kind == "argument"
        && parent
            .and_then(|p| p.parent())
            .map(|l| l.kind() == "argument_list" && children_of_kind(l, "argument").len() > 1)
            .unwrap_or(false);
    if parent_kind == "return_statement" || in_multi_arg_list {
        Doc::indent_of(contents)
    } else {
        contents
    }
}

pub fn cast_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let open = kw(node, "(").unwrap();
    let close = kw(node, ")").unwrap();
    let ty = field(node, "type").unwrap();
    let value = field(node, "value").unwrap();
    let simple = value.kind() == "parenthesized_expression"
        || (matches!(value.kind(), "identifier")
            || (value.kind() == "invocation_expression"
                && field(value, "function").map(|f| f.kind() == "identifier").unwrap_or(false)))
            && ty.kind() != "generic_name";
    if simple {
        concat![print_token(ctx, open), print(ctx, ty), print_token(ctx, close), print(ctx, value)]
    } else {
        group!(
            print_token(ctx, open),
            print(ctx, ty),
            print_token(ctx, close),
            indent!(Doc::soft_line(), print(ctx, value))
        )
    }
}

// ---- object creation & initializers ---------------------------------------------------------

fn break_parent_if_nested(ctx: &Ctx, node: Node, doc: Doc) -> Doc {
    let _ = ctx;
    let initializer = child_of_kind(node, "initializer_expression")
        .or_else(|| field(node, "initializer"));
    let parent_init = match node.parent() {
        Some(p) if p.kind() == "initializer_expression" => Some(p),
        _ => node
            .parent()
            .and_then(|p| p.parent())
            .filter(|gp| gp.kind() == "initializer_expression"),
    };
    match (parent_init, initializer) {
        (Some(parent), Some(init))
            if expression_count(init) > 1 || expression_count(parent) > 1 =>
        {
            concat![doc, Doc::break_parent()]
        }
        _ => doc,
    }
}

fn expression_count(init: Node) -> usize {
    children(init)
        .iter()
        .filter(|c| !matches!(c.kind(), "{" | "}" | ","))
        .count()
}

fn optional_initializer_with_line(ctx: &mut Ctx, node: Node) -> Doc {
    match field(node, "initializer").or_else(|| child_of_kind(node, "initializer_expression")) {
        Some(init) => concat![Doc::line(), initializer_expression(ctx, init)],
        None => Doc::Null,
    }
}

pub fn object_creation_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let new_kw = kw(node, "new").unwrap();
    let ty = field(node, "type").unwrap();
    let args = field(node, "arguments").map(|a| {
        let kids = children(a);
        Doc::group_of(argument_list_like(ctx, a, kids[0], *kids.last().unwrap()))
    });
    let doc = group!(
        print_token_with_suffix(ctx, new_kw, " "),
        print(ctx, ty),
        args.unwrap_or(Doc::Null),
        optional_initializer_with_line(ctx, node)
    );
    break_parent_if_nested(ctx, node, doc)
}

pub fn implicit_object_creation_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let new_kw = kw(node, "new").unwrap();
    let args = child_of_kind(node, "argument_list").unwrap();
    let doc = group!(
        print_token(ctx, new_kw),
        argument_list(ctx, args),
        optional_initializer_with_line(ctx, node)
    );
    break_parent_if_nested(ctx, node, doc)
}

pub fn array_creation_expression(ctx: &mut Ctx, node: Node) -> Doc {
    group!(
        print_token_with_suffix(ctx, kw(node, "new").unwrap(), " "),
        print(ctx, field(node, "type").unwrap()),
        optional_initializer_with_line(ctx, node)
    )
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum InitKind {
    Object,
    Collection,
    Array,
    Complex,
    With,
}

fn init_kind(node: Node) -> InitKind {
    let first_is_assignment = children(node)
        .iter()
        .find(|c| !matches!(c.kind(), "{" | "}" | ","))
        .map(|c| c.kind() == "assignment_expression")
        .unwrap_or(false);
    match node.parent().map(|p| p.kind()).unwrap_or("") {
        "object_creation_expression" | "implicit_object_creation_expression" | "assignment_expression" => {
            if first_is_assignment { InitKind::Object } else { InitKind::Collection }
        }
        "with_expression" => InitKind::With,
        "initializer_expression" => {
            if init_kind(node.parent().unwrap()) == InitKind::Collection {
                InitKind::Complex
            } else {
                InitKind::Array
            }
        }
        _ => InitKind::Array,
    }
}

pub fn initializer_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let open = kids[0];
    let close = *kids.last().unwrap();
    let items: Vec<Node> = kids[1..kids.len() - 1].iter().copied().filter(|c| c.kind() != ",").collect();
    let seps: Vec<Node> = kids[1..kids.len() - 1].iter().copied().filter(|c| c.kind() == ",").collect();
    let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");
    let kind = init_kind(node);

    let separator = if parent_kind == "assignment_expression"
        || (parent_kind == "variable_declarator")
        || (parent_kind == "equals_value_clause")
    {
        Doc::line()
    } else {
        Doc::Null
    };

    let first_kind = items.first().map(|c| c.kind());
    let always_break = (items.len() >= 3
        && (kind == InitKind::Object
            || (kind == InitKind::Collection && first_kind == Some("initializer_expression"))))
        || (kind == InitKind::Array && first_kind == Some("initializer_expression"));

    let line = if always_break { Doc::hard_line() } else { Doc::line() };
    let closing = if kind != InitKind::Complex { Some(close) } else { None };
    let inner = if items.is_empty() {
        Doc::text(" ")
    } else {
        concat![
            Doc::indent(vec![
                line.clone(),
                separated_list_with_closing(ctx, &items, &seps, print, line.clone(), 0, closing)
            ]),
            line
        ]
    };
    let result = concat![separator, print_token(ctx, open), inner, print_token(ctx, close)];

    let no_group = matches!(
        parent_kind,
        "object_creation_expression"
            | "array_creation_expression"
            | "stackalloc_array_creation_expression"
            | "implicit_array_creation_expression"
            | "implicit_object_creation_expression"
    ) || kind == InitKind::With;
    if no_group {
        result
    } else {
        Doc::group_of(result)
    }
}

pub fn tuple_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    Doc::group_of(argument_list_like(ctx, node, kids[0], *kids.last().unwrap()))
}

pub fn collection_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let open = kids[0];
    let close = *kids.last().unwrap();
    let items: Vec<Node> = kids[1..kids.len() - 1].iter().copied().filter(|c| c.kind() != ",").collect();
    let seps: Vec<Node> = kids[1..kids.len() - 1].iter().copied().filter(|c| c.kind() == ",").collect();
    let parent = node.parent();
    let parent_kind = parent.map(|p| p.kind()).unwrap_or("");
    let gp_kind = parent.and_then(|p| p.parent()).map(|p| p.kind()).unwrap_or("");

    let no_separator = match parent_kind {
        "argument" => parent.map(|p| !arg_has_name_colon(p)).unwrap_or(false),
        "attribute_argument" | "arrow_expression_clause" | "cast_expression" | "expression_element"
        | "initializer_expression" | "lambda_expression" => true,
        "assignment_expression" => {
            !matches!(gp_kind, "object_creation_expression" | "initializer_expression" | "expression_statement")
        }
        "variable_declarator" => false,
        "equals_value_clause" => gp_kind != "variable_declarator",
        _ => false,
    };
    let separator = if no_separator { Doc::Null } else { Doc::if_break(Doc::line(), Doc::Null, None) };
    let always_break = items
        .first()
        .map(|i| {
            let k = children(*i);
            i.kind() == "collection_expression"
                || (i.kind() == "expression_element" && k.len() == 1 && k[0].kind() == "collection_expression")
        })
        .unwrap_or(false);

    let any = !items.is_empty();
    let close_leading = ctx.leading_trivia(close);
    let close_doc = if close_leading.any_comment_or_directive() {
        let leading = print_leading_trivia(ctx, close);
        concat![Doc::indent_of(leading), Doc::hard_line()]
    } else {
        Doc::Null
    };
    let list = if any {
        Doc::indent(vec![
            if always_break { Doc::hard_line() } else { Doc::if_break(Doc::line(), Doc::Null, None) },
            separated_list_with_closing(
                ctx,
                &items,
                &seps,
                print,
                if always_break { Doc::hard_line() } else { Doc::line() },
                0,
                Some(close),
            ),
        ])
    } else {
        Doc::Null
    };
    let result = concat![
        separator,
        print_token(ctx, open),
        list,
        if any {
            if always_break { Doc::hard_line() } else { Doc::if_break(Doc::line(), Doc::Null, None) }
        } else {
            Doc::Null
        },
        close_doc,
        print_token_without_leading_trivia(ctx, close)
    ];
    Doc::group_of(result)
}

/// `[a, b]` as used by element access, element binding and implicit element access
pub fn bracketed_argument_list(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let (items, seps) = list_parts_pub(node);
    group!(
        print_token(ctx, kids[0]),
        indent!(Doc::soft_line(), separated_list(ctx, &items, &seps, print, Doc::line(), 0)),
        Doc::soft_line(),
        print_token(ctx, *kids.last().unwrap())
    )
}

pub fn array_rank_specifier(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let (items, seps) = list_parts_pub(node);
    if items.is_empty() {
        let mut docs = Vec::new();
        for c in &kids {
            docs.push(print_token(ctx, *c));
        }
        return Doc::concat(docs);
    }
    group!(
        print_token(ctx, kids[0]),
        concat![
            indent!(Doc::soft_line(), separated_list(ctx, &items, &seps, print, Doc::line(), 0)),
            Doc::soft_line()
        ],
        print_token(ctx, *kids.last().unwrap())
    )
}
