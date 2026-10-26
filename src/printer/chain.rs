//! Port of SyntaxNodePrinters/InvocationExpression.cs, a member chain printer based on
//! prettier/src/language-js/print/member-chain.js

use tree_sitter::Node;

use super::nodes::*;
use super::*;
use crate::doc::Doc;
use crate::trivia::TriviaListExt;
use crate::{concat, group};

#[derive(Clone)]
enum Kind<'t> {
    Invocation,
    ElementAccess,
    /// the name is only used to find `ThenInclude`
    MemberAccess(String),
    ConditionalAccess,
    MemberBinding,
    Postfix { operand_is_invocation: bool },
    Identifier(String),
    Other(Node<'t>),
}

#[derive(Clone)]
struct Printed<'t> {
    kind: Kind<'t>,
    doc: Doc,
}

fn is_chain_postfix(node: Node) -> bool {
    node.kind() == "postfix_unary_expression"
        && children(node)
            .first()
            .map(|o| matches!(o.kind(), "invocation_expression" | "member_access_expression"))
            .unwrap_or(false)
}

pub fn print_member_chain(ctx: &mut Ctx, node: Node) -> Doc {
    let parent = node.parent();
    let mut printed: Vec<Printed> = Vec::new();
    flatten(ctx, node, &mut printed);

    let has_invocation = printed.iter().any(|p| matches!(p.kind, Kind::Invocation));
    let groups = if has_invocation {
        group_prettier_style(&printed)
    } else {
        group_on_lines(&printed)
    };

    let one_line: Vec<Doc> = groups.iter().flatten().map(|p| p.doc.clone()).collect();
    let merge = should_merge_first_two_groups(&groups, parent);
    let cutoff = if merge { 3 } else { 2 };

    let last_is_chainable = |p: &Printed| {
        !matches!(
            p.kind,
            Kind::Invocation
                | Kind::ElementAccess
                | Kind::Postfix { operand_is_invocation: true }
        )
    };
    let in_interpolated = {
        let mut p = node.parent();
        let mut found = false;
        while let Some(n) = p {
            if n.kind() == "interpolation" || n.kind() == "interpolated_string_expression" {
                found = true;
                break;
            }
            p = n.parent();
        }
        found
    };
    let force_one_line = (groups.len() <= cutoff
        && (groups
            .iter()
            .skip(if merge { 1 } else { 0 })
            .any(|g| last_is_chainable(g.last().unwrap()))
            // if the last group contains just a !, make sure it doesn't end up on a new line
            || (groups.last().unwrap().len() == 1
                && matches!(groups.last().unwrap()[0].kind, Kind::Postfix { .. }))))
        || in_interpolated
        // a multiline string in an invocation chain, conditional groups don't propagate breaks
        // so we need to avoid the conditional group
        || groups[0].iter().any(|p| match &p.kind {
            Kind::Other(n) => {
                let text = ctx.text(*n);
                (n.kind() == "raw_string_literal" && text.contains('\n'))
                    || super::strings::is_raw_interpolated_multiline(ctx, *n)
                    || (matches!(n.kind(), "string_literal" | "verbatim_string_literal")
                        && text.contains('\n'))
            }
            _ => false,
        });

    if force_one_line {
        return Doc::group(one_line);
    }

    let g0: Vec<Doc> = groups[0].iter().map(|p| p.doc.clone()).collect();
    let merged = if merge {
        let g1: Vec<Doc> = groups[1].iter().map(|p| p.doc.clone()).collect();
        let last_is_indent_group = matches!(
            &groups[1].last().unwrap().doc,
            Doc::Group(g) if matches!(g.contents, Doc::Indent(_))
        );
        Doc::indent_if(groups.len() > 2 && !last_is_indent_group, Doc::concat(g1))
    } else {
        Doc::Null
    };
    let rest = &groups[if merge { 2 } else { 1 }.min(groups.len())..];
    let expanded = concat![Doc::concat(g0), merged, print_indented_group(rest)];

    let semicolon_has_comment = parent
        .filter(|p| p.kind() == "expression_statement")
        .map(|p| {
            let semi = *children(p).last().unwrap();
            ctx.leading_trivia(semi).any_comment()
        })
        .unwrap_or(false);

    let first_is_special = groups[0].iter().any(|p| match &p.kind {
        Kind::Other(n) => {
            matches!(n.kind(), "array_creation_expression")
                || (n.kind() == "object_creation_expression"
                    && child_of_kind(*n, "initializer_expression").is_some())
        }
        _ => false,
    }) || matches!(&groups[0][0].kind, Kind::Other(n)
        if n.kind() == "parenthesized_expression"
            && named_children(*n).first().map(|c| matches!(c.kind(), "switch_expression" | "query_expression")).unwrap_or(false));

    if one_line.iter().skip(1).any(contains_break)
        || first_is_special
        || semicolon_has_comment
        || groups.len() == 1
    {
        expanded
    } else {
        Doc::conditional_group(vec![Doc::concat(one_line), expanded])
    }
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

fn flatten<'t>(ctx: &mut Ctx, expr: Node<'t>, out: &mut Vec<Printed<'t>>) {
    match expr.kind() {
        "invocation_expression" => {
            flatten(ctx, field(expr, "function").unwrap(), out);
            let args = exprs::argument_list(ctx, field(expr, "arguments").unwrap());
            out.push(Printed { kind: Kind::Invocation, doc: args });
        }
        "element_access_expression" => {
            flatten(ctx, field(expr, "expression").unwrap(), out);
            let sub = print(ctx, field(expr, "subscript").unwrap());
            out.push(Printed { kind: Kind::ElementAccess, doc: sub });
        }
        "member_access_expression" => {
            flatten(ctx, field(expr, "expression").unwrap(), out);
            let name = field(expr, "name").unwrap();
            let name_text = if name.kind() == "identifier" { ctx.text(name).to_string() } else { String::new() };
            let op = kw_any(expr, &[".", "->"]).unwrap();
            let doc = concat![print_token(ctx, op), print(ctx, name)];
            out.push(Printed { kind: Kind::MemberAccess(name_text), doc });
        }
        "conditional_access_expression" => {
            let kids = children(expr);
            flatten(ctx, kids[0], out);
            let q = print_token(ctx, kids[1]);
            out.push(Printed { kind: Kind::ConditionalAccess, doc: q });
            flatten(ctx, kids[2], out);
        }
        "member_binding_expression" => {
            let doc = concat_children_pub(ctx, expr);
            out.push(Printed { kind: Kind::MemberBinding, doc });
        }
        "postfix_unary_expression" if is_chain_postfix(expr) => {
            let kids = children(expr);
            flatten(ctx, kids[0], out);
            let op = print_token(ctx, kids[1]);
            out.push(Printed {
                kind: Kind::Postfix { operand_is_invocation: kids[0].kind() == "invocation_expression" },
                doc: op,
            });
        }
        _ => {
            let doc = print(ctx, expr);
            let kind = if expr.kind() == "identifier" {
                Kind::Identifier(ctx.text(expr).to_string())
            } else {
                Kind::Other(expr)
            };
            out.push(Printed { kind, doc });
        }
    }
}

fn kw_any<'t>(node: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    children(node).into_iter().find(|c| !c.is_named() && kinds.contains(&c.kind()))
}

fn group_on_lines<'a, 't>(printed: &'a [Printed<'t>]) -> Vec<Vec<&'a Printed<'t>>> {
    // a?.b.c!.d  =>  a / ?.b / .c! / .d
    let mut groups: Vec<Vec<&Printed>> = vec![vec![&printed[0]]];
    for index in 1..printed.len() {
        let kind = &printed[index].kind;
        if matches!(kind, Kind::ConditionalAccess) {
            groups.push(vec![]);
        } else if matches!(kind, Kind::MemberAccess(_) | Kind::MemberBinding | Kind::Identifier(_))
            && !matches!(printed[index - 1].kind, Kind::ConditionalAccess)
        {
            groups.push(vec![]);
        }
        groups.last_mut().unwrap().push(&printed[index]);
    }
    groups
}

fn group_prettier_style<'a, 't>(printed: &'a [Printed<'t>]) -> Vec<Vec<&'a Printed<'t>>> {
    // a().b.c().d().e  =>  [a()] [.b .c()] [.d()] [.e]
    let mut groups: Vec<Vec<&Printed>> = Vec::new();
    let mut current = vec![&printed[0]];
    let mut index = 1;
    while index < printed.len() && matches!(printed[index].kind, Kind::Invocation) {
        current.push(&printed[index]);
        index += 1;
    }

    if !matches!(printed[0].kind, Kind::Invocation | Kind::Postfix { .. })
        && index < printed.len()
        && matches!(printed[index].kind, Kind::ElementAccess | Kind::Postfix { .. })
    {
        current.push(&printed[index]);
        index += 1;
    }

    groups.push(current);
    current = vec![];

    let mut has_seen_node_that_requires_break = false;
    while index < printed.len() {
        if has_seen_node_that_requires_break
            && matches!(printed[index].kind, Kind::MemberAccess(_) | Kind::ConditionalAccess)
        {
            groups.push(std::mem::take(&mut current));
            has_seen_node_that_requires_break = false;
        }
        if matches!(printed[index].kind, Kind::Invocation | Kind::ElementAccess) {
            has_seen_node_that_requires_break = true;
        }
        current.push(&printed[index]);
        index += 1;
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

fn print_indented_group(groups: &[Vec<&Printed>]) -> Doc {
    if groups.is_empty() {
        return Doc::Null;
    }
    fn possible(groups: &[Vec<&Printed>], index: &mut usize) -> Doc {
        if *index >= groups.len() {
            return Doc::Null;
        }
        let next = &groups[*index];
        if !matches!(&next[0].kind, Kind::MemberAccess(n) if n == "ThenInclude") {
            return Doc::Null;
        }
        *index += 1;
        let docs: Vec<Doc> = next.iter().map(|p| p.doc.clone()).collect();
        Doc::indent(vec![Doc::hard_line(), Doc::group(docs), possible(groups, index)])
    }

    let mut result = Vec::new();
    let mut index = 0;
    while index < groups.len() {
        let mut i = index;
        let pc = possible(groups, &mut i);
        if !matches!(pc, Doc::Null) {
            result.push(pc);
            index = i;
        } else {
            result.push(Doc::hard_line());
            let docs: Vec<Doc> = groups[index].iter().map(|p| p.doc.clone()).collect();
            result.push(Doc::group(docs));
            index += 1;
        }
    }
    Doc::indent_of(Doc::group(result))
}

fn should_merge_first_two_groups(groups: &[Vec<&Printed>], parent: Option<Node>) -> bool {
    if groups.len() < 2 || groups[0].len() != 1 {
        return false;
    }
    let first_ok = match &groups[0][0].kind {
        Kind::Identifier(t) => t.chars().count() <= 4,
        Kind::Other(n) => matches!(n.kind(), "this_expression" | "this" | "predefined_type" | "base_expression" | "base"),
        _ => false,
    };
    if !first_ok {
        return false;
    }
    let parent_ok = parent
        .map(|p| {
            matches!(p.kind(), "argument" | "binary_expression" | "expression_statement")
                || (p.kind() == "lambda_expression"
                    && field(p, "parameters").map(|x| x.kind() == "implicit_parameter").unwrap_or(false))
        })
        .unwrap_or(false);
    groups[1].len() == 1
        || parent_ok
        || matches!(
            groups[1][1].kind,
            Kind::Invocation | Kind::ElementAccess | Kind::Postfix { .. }
        )
}
