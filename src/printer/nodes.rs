//! Node printers, ported from SyntaxPrinter/SyntaxNodePrinters

use tree_sitter::Node;

use super::*;
use crate::doc::Doc;
use crate::trivia::{DirectiveKind, TriviaKind, TriviaListExt};
use crate::{concat, group, indent};

pub fn print(ctx: &mut Ctx, node: Node) -> Doc {
    if !node.is_named() {
        return print_token(ctx, node);
    }
    if ignore::is_node_ignored(ctx, node) {
        return ignore::print_ignored_node(ctx, node);
    }
    match node.kind() {
        "compilation_unit" => compilation_unit(ctx, node),
        "global_statement" => print(ctx, named_children(node)[0]),
        "using_directive" => using_directive(ctx, node, true),
        "file_scoped_namespace_declaration" => file_scoped_namespace(ctx, node),
        "namespace_declaration" => namespace_declaration(ctx, node),
        "class_declaration" | "struct_declaration" | "interface_declaration" => {
            type_declaration(ctx, node)
        }
        "field_declaration" | "event_field_declaration" => field_declaration(ctx, node),
        "variable_declaration" => variable_declaration(ctx, node),
        "variable_declarator" => variable_declarator(ctx, node),
        "property_declaration" => property_declaration(ctx, node),
        "method_declaration" | "constructor_declaration" | "destructor_declaration" => {
            method_declaration(ctx, node)
        }
        "parameter_list" => parameter_list(ctx, node),
        "parameter" => parameter(ctx, node),
        "attribute_list" | "global_attribute" => attribute_list(ctx, node),
        "arrow_expression_clause" => arrow_expression_clause(ctx, node),
        "block" => block(ctx, node),
        "expression_statement" => expression_statement(ctx, node),
        "return_statement" => return_statement(ctx, node),
        "local_declaration_statement" => local_declaration_statement(ctx, node),
        "if_statement" => if_statement(ctx, node),
        "binary_expression" => binary_expression(ctx, node),
        "argument_list" => exprs::argument_list(ctx, node),
        "argument" => exprs::argument(ctx, node),
        "lambda_expression" => exprs::lambda_expression(ctx, node),
        "conditional_expression" => exprs::conditional_expression(ctx, node),
        "cast_expression" => exprs::cast_expression(ctx, node),
        "object_creation_expression" => exprs::object_creation_expression(ctx, node),
        "implicit_object_creation_expression" => exprs::implicit_object_creation_expression(ctx, node),
        "array_creation_expression" => exprs::array_creation_expression(ctx, node),
        "initializer_expression" => exprs::initializer_expression(ctx, node),
        "tuple_expression" => exprs::tuple_expression(ctx, node),
        "collection_expression" => exprs::collection_expression(ctx, node),
        "is_pattern_expression" if is_binary_like(ctx, node) => binary_expression(ctx, node),
        "as_expression" => binary_expression(ctx, node),
        "is_pattern_expression" => patterns::is_pattern_expression(ctx, node),
        "declaration_pattern" => patterns::declaration_pattern(ctx, node),
        "negated_pattern" => patterns::negated_pattern(ctx, node),
        "relational_pattern" => patterns::relational_pattern(ctx, node),
        "and_pattern" | "or_pattern" => patterns::binary_pattern(ctx, node),
        "recursive_pattern" => patterns::recursive_pattern(ctx, node, true),
        "constant_pattern" => print(ctx, children(node)[0]),
        "when_clause" => patterns::when_clause(ctx, node),
        "switch_statement" => patterns::switch_statement(ctx, node),
        "switch_expression" => patterns::switch_expression(ctx, node),
        "declaration_expression" => concat![
            print(ctx, field(node, "type").unwrap()),
            " ",
            print_child(ctx, field(node, "name").unwrap())
        ],
        "tuple_element" => {
            let mut docs = vec![print(ctx, field(node, "type").unwrap())];
            if let Some(n) = field(node, "name") {
                docs.push(Doc::text(" "));
                docs.push(print_token(ctx, n));
            }
            Doc::concat(docs)
        }
        "parenthesized_expression" => {
            let kids = children(node);
            group!(
                print_token(ctx, kids[0]),
                indent!(Doc::soft_line(), print(ctx, kids[1])),
                Doc::soft_line(),
                print_token(ctx, kids[2])
            )
        }
        "delegate_declaration" => delegate_declaration(ctx, node),
        "enum_declaration" => enum_declaration(ctx, node),
        "enum_member_declaration" => enum_member_declaration(ctx, node),
        "type_parameter_constraint" => concat_children(ctx, node),
        "anonymous_object_creation_expression" => anonymous_object_creation(ctx, node),
        "fixed_statement" => fixed_statement(ctx, node),
        "while_statement" => stmts::while_statement(ctx, node),
        "do_statement" => stmts::do_statement(ctx, node),
        "for_statement" => stmts::for_statement(ctx, node),
        "foreach_statement" => stmts::foreach_statement(ctx, node),
        "using_statement" => stmts::using_statement(ctx, node),
        "lock_statement" => stmts::lock_statement(ctx, node),
        "try_statement" => stmts::try_statement(ctx, node),
        "throw_statement" => stmts::throw_statement(ctx, node),
        "yield_statement" => stmts::yield_statement(ctx, node),
        "break_statement" | "continue_statement" | "empty_statement" => stmts::simple_statement(ctx, node),
        "typeof_expression" | "default_expression" | "sizeof_expression" | "checked_expression" => {
            concat_children(ctx, node)
        }
        "assignment_expression" => assignment_expression(ctx, node),
        "member_access_expression" | "invocation_expression" | "conditional_access_expression" => {
            chain::print_member_chain(ctx, node)
        }
        "member_binding_expression" => concat_children(ctx, node),
        "postfix_unary_expression"
            if node.child(1).map(|o| o.kind() == "!").unwrap_or(false)
                && node.child(0).map(|o| o.kind() == "invocation_expression").unwrap_or(false) =>
        {
            chain::print_member_chain(ctx, node)
        }

        "qualified_name" | "alias_qualified_name" | "explicit_interface_specifier" => {
            concat_children(ctx, node)
        }
        "type_argument_list" => type_argument_list(ctx, node),
        "generic_name" => Doc::group_of(concat_children(ctx, node)),
        "ref_type" | "ref_expression" => {
            let kids = children(node);
            let mut docs = Vec::new();
            for (i, c) in kids.iter().enumerate() {
                if i + 1 < kids.len() {
                    docs.push(print_token_with_suffix(ctx, *c, " "));
                } else {
                    docs.push(print_child(ctx, *c));
                }
            }
            Doc::concat(docs)
        }
        "nullable_type" | "array_type" | "pointer_type" | "tuple_type" => concat_children(ctx, node),
        "await_expression" => {
            let kids = children(node);
            concat![print_token_with_suffix(ctx, kids[0], " "), print(ctx, kids[1])]
        }
        "postfix_unary_expression" | "prefix_unary_expression" | "range_expression"
        | "element_access_expression" => concat_children(ctx, node),
        "bracketed_argument_list" | "element_binding_expression" | "implicit_element_access" => {
            exprs::bracketed_argument_list(ctx, node)
        }
        "array_rank_specifier" => exprs::array_rank_specifier(ctx, node),
        "identifier" | "predefined_type" | "implicit_type" | "integer_literal" | "real_literal"
        | "string_literal" | "verbatim_string_literal" | "raw_string_literal"
        | "character_literal" | "boolean_literal" | "null_literal" | "this_expression"
        | "interpolated_string_expression" | "modifier" | "discard" => print_token(ctx, node),
        "throw_expression" => {
            let kids = children(node);
            concat![print_token_with_suffix(ctx, kids[0], " "), print(ctx, kids[1])]
        }
        "collection_element" | "expression_element" | "type_pattern" => print(ctx, children(node)[0]),
        "spread_element" => {
            let kids = children(node);
            group!(print_token(ctx, kids[0]), " ", print(ctx, kids[1]))
        }
        "parenthesized_pattern" => {
            let kids = children(node);
            group!(
                print_token(ctx, kids[0]),
                indent!(Doc::soft_line(), print(ctx, kids[1])),
                Doc::soft_line(),
                print_token(ctx, kids[2])
            )
        }
        "scoped_type" => {
            let kids = children(node);
            concat![print_token_with_suffix(ctx, kids[0], " "), print(ctx, kids[1])]
        }
        "with_expression" => with_expression(ctx, node),
        "anonymous_method_expression" => {
            let mut docs = Vec::new();
            let mods = modifiers(node);
            if !mods.is_empty() {
                let printed: Vec<Doc> = mods.iter().map(|m| print_token(ctx, *m)).collect();
                docs.push(group!(Doc::join(Doc::text(" "), printed), " "));
            }
            docs.push(print_token(ctx, kw(node, "delegate").unwrap()));
            if let Some(pl) = field(node, "parameters") {
                docs.push(parameter_list(ctx, pl));
            }
            docs.push(block(ctx, *children(node).last().unwrap()));
            Doc::concat(docs)
        }
        "unsafe_statement" => {
            let kids = children(node);
            concat![extra_new_lines(ctx, node), print_token(ctx, kids[0]), print(ctx, kids[1])]
        }
        "goto_statement" => {
            let kids = children(node);
            let mut docs = vec![extra_new_lines(ctx, node), print_token(ctx, kids[0])];
            for c in &kids[1..kids.len() - 1] {
                docs.push(Doc::text(" "));
                docs.push(print_child(ctx, *c));
            }
            docs.push(print_token(ctx, *kids.last().unwrap()));
            Doc::concat(docs)
        }
        "labeled_statement" => {
            let kids = children(node);
            let stmt = *kids.last().unwrap();
            concat![
                extra_new_lines(ctx, node),
                print_token(ctx, kids[0]),
                print_token(ctx, kids[1]),
                if stmt.kind() == "block" {
                    block(ctx, stmt)
                } else {
                    concat![Doc::hard_line(), print(ctx, stmt)]
                }
            ]
        }
        "stackalloc_expression" => group!(
            print_token_with_suffix(ctx, kw(node, "stackalloc").unwrap(), " "),
            print(ctx, field(node, "type").unwrap()),
            exprs::optional_initializer_with_line_pub(ctx, node)
        ),
        "implicit_stackalloc_expression" => {
            let kids = children(node);
            concat![
                print_token(ctx, kids[0]),
                print_token(ctx, kids[1]),
                print_token_with_suffix(ctx, kids[2], " "),
                print(ctx, kids[3])
            ]
        }
        "implicit_array_creation_expression" => {
            let kids = children(node);
            let open = kids[1];
            let close = kids.iter().find(|c| c.kind() == "]").copied().unwrap();
            let commas: Vec<Doc> = kids.iter().filter(|c| c.kind() == ",").map(|c| print_token(ctx, *c)).collect();
            let init = *kids.last().unwrap();
            group!(
                print_token(ctx, kids[0]),
                print_token(ctx, open),
                Doc::concat(commas),
                print_token(ctx, close),
                Doc::line(),
                exprs::initializer_expression(ctx, init)
            )
        }
        "local_function_statement" => method_declaration(ctx, node),
        _ => unhandled(ctx, node),
    }
}

pub(super) fn unhandled(ctx: &mut Ctx, node: Node) -> Doc {
    ctx.unhandled.push(node.kind().to_string());
    Doc::text(ctx.text(node))
}

/// Prints every child in order with no spacing, good enough for tight syntax like `a.b<c>[d]`.
pub(super) fn concat_children(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = Vec::new();
    for child in children(node) {
        docs.push(print_child(ctx, child));
    }
    Doc::concat(docs)
}

/// tokens are printed as tokens, everything else dispatches
pub(super) fn print_child(ctx: &mut Ctx, child: Node) -> Doc {
    if !child.is_named() || child.child_count() == 0 {
        print_token(ctx, child)
    } else {
        print(ctx, child)
    }
}

pub(super) fn kw<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    children(node).into_iter().find(|c| c.kind() == kind && !c.is_named())
}

// ---- separated lists ------------------------------------------------------------------------

/// items and separators between the delimiters (first and last child) of a list node
pub(super) fn list_parts<'t>(node: Node<'t>, has_closer: bool) -> (Vec<Node<'t>>, Vec<Node<'t>>) {
    let all = children(node);
    let end = if has_closer { all.len().saturating_sub(1) } else { all.len() };
    let mut items = Vec::new();
    let mut seps = Vec::new();
    for c in &all[1.min(all.len())..end] {
        if c.kind() == "," {
            seps.push(*c);
        } else {
            items.push(*c);
        }
    }
    (items, seps)
}

pub(super) fn trailing_comma(ctx: &Ctx, closing: Node, skip_if_break: bool) -> Doc {
    let comma = Doc::text(",");
    if ctx.leading_trivia(closing).any_directive() {
        Doc::Null
    } else if skip_if_break {
        comma
    } else {
        Doc::if_break(comma, Doc::Null, None)
    }
}

pub(super) fn separated_list(
    ctx: &mut Ctx,
    items: &[Node],
    seps: &[Node],
    print_item: fn(&mut Ctx, Node) -> Doc,
    after_separator: Doc,
    starting_index: usize,
) -> Doc {
    separated_list_with_closing(ctx, items, seps, print_item, after_separator, starting_index, None)
}

/// Port of SeparatedSyntaxList.Print, `closing` is only set when a trailing comma is wanted
pub(super) fn separated_list_with_closing(
    ctx: &mut Ctx,
    items: &[Node],
    seps: &[Node],
    print_item: fn(&mut Ctx, Node) -> Doc,
    after_separator: Doc,
    starting_index: usize,
    closing: Option<Node>,
) -> Doc {
    let mut docs = Vec::new();
    for x in starting_index..items.len() {
        let last_has_no_separator = x >= seps.len();
        let mut has_trailing_comment = false;

        // we want a trailing comma, but we need to get it printed in place before a trailing
        // comment, so shove it in the context for token printing to pick up
        if last_has_no_separator {
            if let Some(closing) = closing {
                let idx = ctx.last_token_index(items[x]);
                has_trailing_comment = ctx.tokens.list[idx].trailing.any_comment();
                if has_trailing_comment {
                    ctx.state.trailing_comma = Some((idx, trailing_comma(ctx, closing, true)));
                }
            }
        }

        docs.push(print_item(ctx, items[x]));

        if last_has_no_separator {
            if let Some(closing) = closing {
                if !has_trailing_comment {
                    docs.push(trailing_comma(ctx, closing, false));
                }
            }
            continue;
        }

        let is_trailing_separator = x == items.len() - 1;
        if is_trailing_separator {
            let sep_idx = ctx.token_index(seps[x]);
            // when the trailing separator has trailing comments, we have to print it normally to
            // prevent it from collapsing. When the closing token has a directive, we can't assume
            // the comma should be added/removed so just print it normally
            if ctx.tokens.list[sep_idx].trailing.any_comment()
                || closing.map(|c| ctx.leading_trivia(c).any_directive()).unwrap_or(false)
            {
                docs.push(print_token(ctx, seps[x]));
            } else if let Some(closing) = closing {
                docs.push(trailing_comma(ctx, closing, false));
            } else {
                docs.push(Doc::if_break(print_token(ctx, seps[x]), Doc::Null, None));
            }
        } else {
            docs.push(print_token(ctx, seps[x]));
            docs.push(after_separator.clone());
        }
    }
    Doc::concat(docs)
}

// ---- compilation unit & namespaces ----------------------------------------------------------

struct NsItems<'t> {
    externs: Vec<Node<'t>>,
    usings: Vec<Node<'t>>,
    attributes: Vec<Node<'t>>,
    members: Vec<Node<'t>>,
}

fn partition<'t>(nodes: Vec<Node<'t>>) -> NsItems<'t> {
    let mut items = NsItems {
        externs: vec![],
        usings: vec![],
        attributes: vec![],
        members: vec![],
    };
    for n in nodes {
        match n.kind() {
            "extern_alias_directive" => items.externs.push(n),
            "using_directive" => items.usings.push(n),
            "global_attribute" => items.attributes.push(n),
            _ => items.members.push(n),
        }
    }
    items
}

fn ns_items<'t>(node: Node<'t>) -> NsItems<'t> {
    match node.kind() {
        "compilation_unit" => {
            // members after a file scoped namespace belong to it
            let mut nodes = Vec::new();
            for c in children(node) {
                nodes.push(c);
                if c.kind() == "file_scoped_namespace_declaration" {
                    break;
                }
            }
            partition(nodes)
        }
        "file_scoped_namespace_declaration" => {
            let parent = node.parent().unwrap();
            let after: Vec<Node> = children(parent)
                .into_iter()
                .filter(|c| c.start_byte() > node.start_byte())
                .collect();
            partition(after)
        }
        _ => {
            let body = field(node, "body").unwrap();
            let all = children(body);
            let inner = all[1..all.len() - 1].to_vec();
            partition(inner)
        }
    }
}

pub(super) fn namespace_like(ctx: &mut Ctx, node: Node, docs: &mut Vec<Doc>) {
    let items = ns_items(node);
    let is_cu = node.kind() == "compilation_unit";

    if !items.externs.is_empty() {
        let printed: Vec<Doc> = items
            .externs
            .iter()
            .enumerate()
            .map(|(i, e)| extern_alias(ctx, *e, i != 0))
            .collect();
        docs.push(Doc::join(Doc::hard_line(), printed));
    }

    if !items.usings.is_empty() {
        if !items.externs.is_empty() {
            docs.push(Doc::hard_line());
        }
        docs.push(print_usings_with_sorting(ctx, &items.usings, !items.externs.is_empty()));
    }

    let mut is_cu_with_attributes = false;
    if is_cu && !items.attributes.is_empty() {
        is_cu_with_attributes = true;
        if !items.externs.is_empty() || !items.usings.is_empty() {
            let first = items.attributes[0];
            docs.push(if ctx.leading_trivia(first).any_directive() {
                extra_new_lines(ctx, first)
            } else {
                Doc::hard_line()
            });
        }
        docs.push(Doc::hard_line());
        let printed: Vec<Doc> = items.attributes.iter().map(|a| attribute_list(ctx, *a)).collect();
        docs.push(Doc::join(Doc::hard_line(), printed));
        docs.push(Doc::hard_line());
    }

    if items.members.is_empty() {
        return;
    }

    if !items.usings.is_empty() || !items.externs.is_empty() {
        let first = items.members[0];
        let directive_trivia: Vec<_> = ctx
            .leading_trivia(first)
            .iter()
            .filter(|t| t.is_directive())
            .collect();
        let all_end_if = directive_trivia
            .iter()
            .all(|t| t.kind == TriviaKind::Directive(DirectiveKind::EndIf));
        if !directive_trivia.is_empty() {
            if (!(is_cu && !items.attributes.is_empty()) && all_end_if) || !all_end_if {
                docs.push(extra_new_lines(ctx, first));
            }
        } else if !(is_cu && !items.attributes.is_empty()) {
            docs.push(Doc::hard_line());
        }
    }

    let skip_first = items.usings.is_empty() && items.externs.is_empty() && !is_cu_with_attributes;
    docs.extend(members::print(ctx, node.kind(), node, &items.members, skip_first));
}

pub(super) fn compilation_unit(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = Vec::new();
    namespace_like(ctx, node, &mut docs);

    let eof = ctx.tokens.eof();
    let final_trivia = print_leading_trivia_with_new_lines(ctx, &eof.leading);
    if !matches!(final_trivia, Doc::Null) {
        // really ugly code to prevent a comment at the end of a file from continually inserting new blank lines
        let mut handled = false;
        if let (Doc::Concat(list), Some(Doc::Concat(previous))) = (&final_trivia, docs.last()) {
            let previous = previous.borrow();
            let n = previous.len();
            if list.borrow().len() > 1
                && n > 1
                && is_hard_line(&previous[n - 1])
                && is_hard_line(&previous[n - 2])
            {
                let mut list = list.borrow_mut();
                while matches!(list.first(), Some(d) if is_hard_line_for_trivia(d)) {
                    list.remove(0);
                }
                handled = true;
            }
        }
        if handled {
            docs.push(final_trivia);
        } else {
            // even though we include the initial new lines above, a literalLine from directives trims the hardline, so add an extra one here
            docs.push(Doc::hard_line_if_no_previous_line());
            docs.push(final_trivia);
        }
    }
    docs.push(Doc::hard_line_if_no_previous_line());
    Doc::concat(docs)
}

pub(super) fn file_scoped_namespace(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = vec![
        print_attribute_lists(ctx, node),
        print_modifiers(ctx, node),
        print_token(ctx, kw(node, "namespace").unwrap()),
        Doc::text(" "),
        print(ctx, field(node, "name").unwrap()),
        print_token(ctx, kw(node, ";").unwrap()),
        Doc::hard_line(),
        Doc::hard_line(),
    ];
    namespace_like(ctx, node, &mut docs);
    Doc::concat(docs)
}

pub(super) fn namespace_declaration(ctx: &mut Ctx, node: Node) -> Doc {
    let items = ns_items(node);
    let has_content = !items.members.is_empty() || !items.usings.is_empty() || !items.externs.is_empty();
    let mut inner = Vec::new();
    if has_content {
        inner.push(Doc::hard_line());
        namespace_like(ctx, node, &mut inner);
    } else {
        inner.push(Doc::text(" "));
    }
    let inner = remove_initial_double_hard_line_list(inner);

    let body = field(node, "body").unwrap();
    let body_children = children(body);
    let open = body_children[0];
    let close = *body_children.last().unwrap();

    concat![
        print_attribute_lists(ctx, node),
        print_modifiers(ctx, node),
        print_token(ctx, kw(node, "namespace").unwrap()),
        " ",
        print(ctx, field(node, "name").unwrap()),
        group!(
            Doc::line(),
            print_token(ctx, open),
            Doc::indent(inner),
            if has_content { Doc::hard_line() } else { Doc::Null },
            print_token(ctx, close),
            print_token_opt(ctx, kw(node, ";"))
        )
    ]
}

// ---- usings ---------------------------------------------------------------------------------

pub(super) fn extern_alias(ctx: &mut Ctx, node: Node, print_extra_lines: bool) -> Doc {
    let mut docs = vec![if print_extra_lines {
        extra_new_lines(ctx, node)
    } else {
        Doc::Null
    }];
    let kids = children(node);
    for (i, c) in kids.iter().enumerate() {
        docs.push(print_token(ctx, *c));
        if i + 2 < kids.len() {
            docs.push(Doc::text(" "));
        }
    }
    Doc::concat(docs)
}

pub(super) fn using_directive(ctx: &mut Ctx, node: Node, print_extra_lines: bool) -> Doc {
    let mut docs = vec![if print_extra_lines {
        extra_new_lines(ctx, node)
    } else {
        Doc::Null
    }];
    let mut skip_leading = true;
    for c in children(node) {
        let doc = match c.kind() {
            "global" | "using" | "unsafe" | "static" => {
                let d = print_token_with(
                    ctx,
                    c,
                    Doc::text(" "),
                    TokenOptions {
                        skip_leading,
                        ..Default::default()
                    },
                );
                d
            }
            "=" => concat![" ", print_token(ctx, c), " "],
            ";" => print_token(ctx, c),
            _ => print_child(ctx, c),
        };
        skip_leading = true;
        docs.push(doc);
    }
    Doc::concat(docs)
}

pub(super) fn using_group(ctx: &Ctx, node: Node) -> usize {
    let kids = children(node);
    let has = |k: &str| kids.iter().any(|c| c.kind() == k && !c.is_named());
    let has_alias = kids.iter().any(|c| c.kind() == "=");
    let target = kids.iter().rev().find(|c| c.is_named());
    let is_system = target.map(|t| is_system_name(ctx, *t)).unwrap_or(false);
    if has("global") {
        if has_alias {
            2
        } else if is_system {
            0
        } else {
            1
        }
    } else if has("static") {
        if is_system {
            6
        } else {
            7
        }
    } else if has_alias {
        8
    } else if target.map(|t| t.kind() == "alias_qualified_name").unwrap_or(false) {
        4
    } else if is_system {
        3
    } else {
        5
    }
}

pub(super) fn is_system_name(ctx: &Ctx, mut node: Node) -> bool {
    while node.kind() == "qualified_name" {
        node = field(node, "qualifier").unwrap();
    }
    node.kind() == "identifier" && ctx.text(node) == "System"
}

pub(super) fn using_sort_key(ctx: &Ctx, node: Node) -> String {
    let kids = children(node);
    let alias = kids.iter().position(|c| c.kind() == "=");
    let text = match alias {
        Some(i) => ctx.text(kids[i - 1]),
        None => ctx.text(*kids.iter().rev().find(|c| c.is_named()).unwrap()),
    };
    // InvariantCultureIgnoreCase, approximated
    text.trim().to_lowercase()
}

pub(super) fn print_usings_with_sorting(ctx: &mut Ctx, usings: &[Node], print_extra_lines: bool) -> Doc {
    let mut docs = Vec::new();
    // the leading trivia of the first using is printed as is, everything after an #if is moved
    // along with that using
    let first_trivia = ctx.leading_trivia(usings[0]).to_vec();
    docs.push(print_leading_trivia_list(ctx, &first_trivia));

    let mut groups: Vec<Vec<(Node, Doc)>> = (0..9).map(|_| Vec::new()).collect();
    let mut is_first = true;
    for &u in usings {
        let trivia = if is_first {
            first_trivia_placeholder()
        } else {
            let t = ctx.leading_trivia(u).to_vec();
            print_leading_trivia_list(ctx, &t)
        };
        is_first = false;
        groups[using_group(ctx, u)].push((u, trivia));
    }
    for g in groups.iter_mut() {
        g.sort_by_key(|(n, _)| using_sort_key(ctx, *n));
    }

    let mut first = true;
    for g in groups {
        for (u, trivia) in g {
            if !first {
                docs.push(Doc::hard_line());
            }
            first = false;
            if !matches!(trivia, Doc::Null) {
                docs.push(trivia);
            }
            docs.push(using_directive(ctx, u, print_extra_lines));
        }
    }
    Doc::concat(docs)
}

pub(super) fn first_trivia_placeholder() -> Doc {
    Doc::Null
}

// ---- modifiers, attributes ------------------------------------------------------------------

pub(super) fn modifier_order(text: &str) -> usize {
    match text {
        "public" => 0,
        "private" => 1,
        "protected" => 2,
        "internal" => 3,
        "file" => 4,
        "static" => 5,
        "extern" => 6,
        "new" => 7,
        "virtual" => 8,
        "closed" => 9,
        "abstract" => 10,
        "sealed" => 11,
        "override" => 12,
        "readonly" => 13,
        "unsafe" => 14,
        "required" => 15,
        "volatile" => 16,
        "async" => 17,
        _ => usize::MAX,
    }
}

pub(super) fn print_modifiers(ctx: &mut Ctx, node: Node) -> Doc {
    let mods = modifiers(node);
    if mods.is_empty() {
        return Doc::Null;
    }
    let mut order: Vec<usize> = (0..mods.len()).collect();

    let can_reorder = mods.len() > 1 && {
        let tokens = ctx.tokens;
        let first_ok = !tokens.list[ctx.token_index(mods[0])].leading.any_directive();
        first_ok
            && mods[1..]
                .iter()
                .all(|m| !tokens.list[ctx.token_index(*m)].leading.any_comment_or_directive())
    };
    if can_reorder {
        let keys: Vec<usize> = mods.iter().map(|m| modifier_order(ctx.text(*m))).collect();
        order.sort_by_key(|&i| keys[i]);
        if order.iter().enumerate().any(|(pos, &i)| pos != i) {
            ctx.state.reordered_modifiers = true;
        }
    }

    let leading_from = ctx.token_index(mods[0]);
    let docs: Vec<Doc> = order
        .iter()
        .enumerate()
        .map(|(pos, &i)| {
            if order == (0..mods.len()).collect::<Vec<_>>() {
                print_token(ctx, mods[i])
            } else {
                print_token_with(
                    ctx,
                    mods[i],
                    Doc::Null,
                    TokenOptions {
                        skip_leading: pos != 0,
                        leading_from: Some(leading_from),
                        ..Default::default()
                    },
                )
            }
        })
        .collect();
    group!(Doc::join(Doc::text(" "), docs), " ")
}

pub(super) fn print_attribute_lists(ctx: &mut Ctx, node: Node) -> Doc {
    let lists = children_of_kind(node, "attribute_list");
    if lists.is_empty() {
        return Doc::Null;
    }
    let separator = if matches!(node.kind(), "type_parameter" | "parameter" | "accessor_declaration")
    {
        Doc::line()
    } else {
        Doc::hard_line()
    };
    let printed: Vec<Doc> = lists.iter().map(|l| attribute_list(ctx, *l)).collect();
    let joined = Doc::join(separator.clone(), printed);
    if matches!(node.kind(), "parameter" | "type_parameter") {
        joined
    } else {
        concat![joined, separator]
    }
}

pub(super) fn attribute_list(ctx: &mut Ctx, node: Node) -> Doc {
    if node.parent().map(|p| matches!(p.kind(), "method_declaration" | "constructor_declaration" | "destructor_declaration" | "operator_declaration" | "conversion_operator_declaration")).unwrap_or(false)
        && ignore::node_matches(ctx, node, ignore::IgnoreKind::Ignore)
    {
        return Doc::text(ignore::print_node_without_formatting(ctx, node).trim().to_string());
    }
    let kids = children(node);
    let open = kids[0];
    let close = *kids.last().unwrap();
    let mut docs = Vec::new();
    if node.kind() == "global_attribute" {
        let first = children(node.parent().unwrap())
            .into_iter()
            .find(|c| c.kind() == "global_attribute");
        if first.map(|f| f.id()) != Some(node.id()) {
            docs.push(extra_new_lines(ctx, node));
        }
    }
    docs.push(print_token(ctx, open));
    let mut rest: Vec<Node> = kids[1..kids.len() - 1].to_vec();
    // the target is either a node, or (for assembly and module) bare keyword tokens
    while let Some(t) = rest.first().copied() {
        if t.kind() == "attribute" {
            break;
        }
        rest.remove(0);
        let parts = if t.kind() == "attribute_target_specifier" { children(t) } else { vec![t] };
        for c in parts {
            if c.kind() == ":" {
                docs.push(print_token_with_suffix(ctx, c, " "));
            } else {
                docs.push(print_token(ctx, c));
            }
        }
    }
    let attrs: Vec<Node> = rest.iter().copied().filter(|c| c.kind() != ",").collect();
    let seps: Vec<Node> = rest.iter().copied().filter(|c| c.kind() == ",").collect();
    let printed = separated_list(ctx, &attrs, &seps, attribute, Doc::line(), 0);
    docs.push(if attrs.len() > 1 {
        Doc::indent(vec![Doc::soft_line(), printed])
    } else {
        printed
    });
    if attrs.len() > 1 {
        docs.push(Doc::soft_line());
    }
    docs.push(print_token(ctx, close));
    Doc::group(docs)
}

pub(super) fn attribute(ctx: &mut Ctx, node: Node) -> Doc {
    let name_node = field(node, "name").unwrap_or_else(|| named_children(node)[0]);
    let name = print(ctx, name_node);
    let Some(args) = child_of_kind(node, "attribute_argument_list") else {
        return name;
    };
    let kids = children(args);
    let (items, seps) = list_parts(args, true);
    let single_collection = items.len() == 1
        && children(items[0]).len() == 1
        && children(items[0])[0].kind() == "collection_expression";
    let printed = separated_list(ctx, &items, &seps, attribute_argument, Doc::line(), 0);
    group!(
        name,
        print_token(ctx, kids[0]),
        Doc::indent_if(
            !single_collection,
            concat![
                if single_collection { Doc::Null } else { Doc::soft_line() },
                printed
            ]
        ),
        if single_collection { Doc::Null } else { Doc::soft_line() },
        print_token(ctx, *kids.last().unwrap())
    )
}

pub(super) fn attribute_argument(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = Vec::new();
    for c in children(node) {
        match c.kind() {
            "=" => docs.push(concat![" ", print_token(ctx, c), " "]),
            ":" => docs.push(print_token_with_suffix(ctx, c, " ")),
            _ => docs.push(print_child(ctx, c)),
        }
    }
    Doc::concat(docs)
}

// ---- declarations ---------------------------------------------------------------------------

pub(super) fn type_declaration(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = vec![print_attribute_lists(ctx, node), print_modifiers(ctx, node)];

    let keyword = kids_anon(node, &["class", "struct", "interface"]).unwrap();
    docs.push(print_token_with_suffix(ctx, keyword, " "));
    docs.push(print_token(ctx, field(node, "name").unwrap()));

    if let Some(tpl) = child_of_kind(node, "type_parameter_list") {
        docs.push(type_parameter_list(ctx, tpl));
    }
    if let Some(pl) = child_of_kind(node, "parameter_list") {
        docs.push(parameter_list(ctx, pl));
    }
    if let Some(bl) = child_of_kind(node, "base_list") {
        docs.push(base_list(ctx, bl));
    }
    let clauses = children_of_kind(node, "type_parameter_constraints_clause");
    if !clauses.is_empty() {
        docs.push(constraint_clauses(ctx, &clauses));
    }

    let body = field(node, "body");
    if let Some(body) = body {
        let kids = children(body);
        let open = kids[0];
        let close = *kids.last().unwrap();
        let members: Vec<Node> = kids[1..kids.len() - 1].to_vec();
        if !members.is_empty() {
            let content = Doc::indent(members::print(ctx, node.kind(), node, &members, false));
            let content = remove_initial_double_hard_line(content);
            docs.push(Doc::hard_line());
            docs.push(print_token(ctx, open));
            docs.push(content);
            docs.push(Doc::hard_line());
            docs.push(print_token(ctx, close));
        } else {
            let separator = if super::has_comments(&ctx.tokens.list[ctx.token_index(close)].leading)
            {
                Doc::line()
            } else {
                Doc::text(" ")
            };
            docs.push(separator.clone());
            docs.push(print_token(ctx, open));
            docs.push(separator);
            docs.push(print_token(ctx, close));
        }
    }
    docs.push(print_token_opt(ctx, kw(node, ";")));
    Doc::concat(docs)
}

pub(super) fn kids_anon<'t>(node: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    children(node)
        .into_iter()
        .find(|c| !c.is_named() && kinds.contains(&c.kind()))
}

pub(super) fn constraint_clauses(ctx: &mut Ctx, clauses: &[Node]) -> Doc {
    let printed: Vec<Doc> = clauses.iter().map(|c| constraint_clause(ctx, *c)).collect();
    group!(indent!(Doc::hard_line(), Doc::join(Doc::hard_line(), printed)))
}

pub(super) fn constraint_clause(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let where_kw = kids[0];
    let name = kids[1];
    let colon = kids[2];
    let constraints = &kids[3..];
    let items: Vec<Node> = constraints.iter().copied().filter(|c| c.kind() != ",").collect();
    let seps: Vec<Node> = constraints.iter().copied().filter(|c| c.kind() == ",").collect();
    group!(
        print_token_with_suffix(ctx, where_kw, " "),
        print(ctx, name),
        " ",
        print_token_with_suffix(ctx, colon, " "),
        Doc::indent(vec![separated_list(ctx, &items, &seps, print, Doc::line(), 0)])
    )
}

pub(super) fn type_parameter_list(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let (items, seps) = list_parts(node, true);
    if items.is_empty() {
        return Doc::Null;
    }
    let should_break_more = items.len() > 1 || child_of_kind(items[0], "attribute_list").is_some();
    group!(
        print_token(ctx, kids[0]),
        indent!(
            if should_break_more { Doc::soft_line() } else { Doc::Null },
            separated_list(ctx, &items, &seps, type_parameter, Doc::line(), 0)
        ),
        if should_break_more { Doc::soft_line() } else { Doc::Null },
        print_token(ctx, *kids.last().unwrap())
    )
}

pub(super) fn type_parameter(ctx: &mut Ctx, node: Node) -> Doc {
    let has_attribute = child_of_kind(node, "attribute_list").is_some();
    let group_id = Doc::next_group_id();
    let mut docs = vec![print_attribute_lists(ctx, node)];
    if has_attribute {
        docs.push(Doc::indent_if_break(Doc::line(), group_id));
    }
    for c in children(node) {
        match c.kind() {
            "attribute_list" => {}
            "in" | "out" => docs.push(print_token_with_suffix(ctx, c, " ")),
            _ => docs.push(print_child(ctx, c)),
        }
    }
    if has_attribute {
        Doc::group_with_id(group_id, docs)
    } else {
        Doc::concat(docs)
    }
}

pub(super) fn type_argument_list(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let (items, seps) = list_parts(node, true);
    let has_generic = items.iter().any(|i| i.kind() == "generic_name");
    let separator = if items.first().map(|i| i.kind()) != Some("omitted_type_argument")
        && (items.len() > 1 || has_generic)
    {
        Doc::soft_line()
    } else {
        Doc::Null
    };
    let is_sep = !matches!(separator, Doc::Null);
    let after = if items.first().map(|i| i.kind()) == Some("omitted_type_argument") {
        Doc::Null
    } else {
        Doc::line()
    };
    concat![
        print_token(ctx, kids[0]),
        Doc::indent_if(
            is_sep,
            concat![separator.clone(), separated_list(ctx, &items, &seps, print, after, 0)]
        ),
        separator,
        print_token(ctx, *kids.last().unwrap())
    ]
}

pub(super) fn field_declaration(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = vec![print_attribute_lists(ctx, node), print_modifiers(ctx, node)];
    if let Some(e) = kw(node, "event") {
        docs.push(print_token_with_suffix(ctx, e, " "));
    }
    docs.push(variable_declaration(ctx, child_of_kind(node, "variable_declaration").unwrap()));
    docs.push(print_token(ctx, kw(node, ";").unwrap()));
    Doc::concat(docs)
}

pub(super) fn variable_declaration(ctx: &mut Ctx, node: Node) -> Doc {
    let ty = field(node, "type").unwrap();
    let declarators = children_of_kind(node, "variable_declarator");
    if declarators.len() > 1 {
        let seps = children_of_kind(node, ",");
        let after = if node.parent().map(|p| p.kind()) == Some("for_statement") {
            Doc::line()
        } else {
            Doc::hard_line()
        };
        return concat![
            print(ctx, ty),
            " ",
            Doc::indent(vec![separated_list(ctx, &declarators, &seps, print, after, 0)])
        ];
    }

    let variable = declarators[0];
    let kids = children(variable);
    let name = field(variable, "name").unwrap_or(kids[0]);
    let mut left_docs = vec![print(ctx, ty), Doc::text(" "), print_token(ctx, name)];
    for c in &kids {
        if c.kind() == "bracketed_argument_list" {
            left_docs.push(print(ctx, *c));
        }
    }
    let left = Doc::concat(left_docs);
    match kids.iter().position(|c| c.kind() == "=") {
        None => Doc::group_of(left),
        Some(eq) => {
            let value = kids[eq + 1];
            let op = print_token(ctx, kids[eq]);
            right_hand_side(ctx, variable, concat![left, " "], op, value)
        }
    }
}

pub(super) fn variable_declarator(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = Vec::new();
    let kids = children(node);
    for (i, c) in kids.iter().enumerate() {
        if c.kind() == "=" {
            docs.push(equals_value_clause(ctx, node, *c, kids[i + 1]));
            break;
        }
        docs.push(print_child(ctx, *c));
    }
    Doc::concat(docs)
}

pub(super) fn equals_value_clause(ctx: &mut Ctx, parent: Node, equals: Node, value: Node) -> Doc {
    let mut separator = Doc::line();
    let vk = value.kind();
    if parent.kind() == "property_declaration" && vk != "anonymous_object_creation_expression" {
        // keeping line
    } else if vk == "query_expression" {
        separator = Doc::Null;
    } else if matches!(
        vk,
        "anonymous_object_creation_expression"
            | "anonymous_method_expression"
            | "conditional_expression"
            | "object_creation_expression"
            | "initializer_expression"
            | "lambda_expression"
            | "invocation_expression"
            | "switch_expression"
    ) {
        separator = Doc::text(" ");
    }
    let is_line = matches!(separator, Doc::Line(_));
    let mut result = group!(
        " ",
        print_token_with_suffix(ctx, equals, separator),
        print(ctx, value)
    );
    if is_line && vk != "collection_expression" {
        result = Doc::indent_of(result);
    }
    result
}

pub(super) fn property_declaration(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = vec![
        print_attribute_lists(ctx, node),
        print_modifiers(ctx, node),
        print(ctx, field(node, "type").unwrap()),
        Doc::text(" "),
    ];
    if let Some(spec) = child_of_kind(node, "explicit_interface_specifier") {
        docs.push(concat_children(ctx, spec));
    }
    docs.push(print_token(ctx, field(node, "name").unwrap()));
    if let Some(accessors) = field(node, "accessors") {
        docs.push(accessor_list(ctx, accessors));
    } else if let Some(arrow) = child_of_kind(node, "arrow_expression_clause") {
        docs.push(arrow_expression_clause(ctx, arrow));
    }
    let kids = children(node);
    if let Some(eq) = kids.iter().position(|c| c.kind() == "=") {
        docs.push(equals_value_clause(ctx, node, kids[eq], kids[eq + 1]));
    }
    docs.push(print_token_opt(ctx, kw(node, ";")));
    Doc::group(vec![Doc::concat(docs)])
}

pub(super) fn accessor_list(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let accessors = &kids[1..kids.len() - 1];
    let complex = accessors.iter().any(|a| {
        field(*a, "body").is_some()
            || child_of_kind(*a, "arrow_expression_clause").is_some()
            || !modifiers(*a).is_empty()
            || child_of_kind(*a, "attribute_list").is_some()
    });
    let separator = if complex { Doc::line() } else { Doc::text(" ") };
    let printed: Vec<Doc> = accessors
        .iter()
        .map(|a| accessor_declaration(ctx, *a, &separator))
        .collect();
    group!(
        separator.clone(),
        print_token(ctx, kids[0]),
        Doc::indent(printed),
        separator,
        print_token(ctx, *kids.last().unwrap())
    )
}

pub(super) fn accessor_declaration(ctx: &mut Ctx, node: Node, separator: &Doc) -> Doc {
    let body = field(node, "body");
    let arrow = child_of_kind(node, "arrow_expression_clause");
    let attr_lists = children_of_kind(node, "attribute_list");
    let first_attr_has_args = attr_lists
        .first()
        .and_then(|l| child_of_kind(*l, "attribute"))
        .map(|a| child_of_kind(a, "attribute_argument_list").is_some())
        .unwrap_or(false);
    let mut docs = Vec::new();
    docs.push(
        if attr_lists.len() > 1 || body.is_some() || arrow.is_some() || first_attr_has_args {
            Doc::hard_line()
        } else {
            separator.clone()
        },
    );
    docs.push(print_attribute_lists(ctx, node));
    docs.push(print_modifiers(ctx, node));
    docs.push(print_token(ctx, field(node, "name").unwrap()));
    if let Some(b) = body {
        docs.push(print(ctx, b));
    } else if let Some(a) = arrow {
        docs.push(arrow_expression_clause(ctx, a));
    }
    docs.push(print_token_opt(ctx, kw(node, ";")));
    Doc::concat(docs)
}

pub(super) fn arrow_expression_clause(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    group!(indent!(
        " ",
        print_token_with_suffix(ctx, kids[0], Doc::line()),
        print(ctx, kids[1])
    ))
}

pub(super) fn method_declaration(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = Vec::new();
    let mut decl = Vec::new();

    if node.kind() == "local_function_statement" {
        docs.push(extra_new_lines(ctx, node));
    }
    if !children_of_kind(node, "attribute_list").is_empty() {
        docs.push(print_attribute_lists(ctx, node));

        // csharpier-ignore after the attributes prints the rest of the method as it is
        let mods = modifiers(node);
        let ret = field(node, "returns").or_else(|| field(node, "type"));
        let anchor = if let Some(m) = mods.first() {
            Some(*m)
        } else {
            ret
        };
        if let Some(a) = anchor {
            if ignore::node_matches(ctx, a, ignore::IgnoreKind::Ignore) {
                let attrs = children_of_kind(node, "attribute_list");
                let full = ignore::full_text(ctx, node);
                let first_tok = ctx.first_token_index(attrs[0]);
                let attr_start = ctx.tokens.list[first_tok].start
                    - ctx.tokens.list[first_tok].leading.iter().map(|t| t.text.len()).sum::<usize>();
                let anchor_tok = ctx.first_token_index(a);
                let anchor_start = ctx.tokens.list[anchor_tok].start
                    - ctx.tokens.list[anchor_tok].leading.iter().map(|t| t.text.len()).sum::<usize>();
                let _ = full;
                let end = ignore::full_text(ctx, node).len() + attr_start;
                let text = ctx.src[anchor_start..end].trim().to_string();
                docs.push(Doc::text(ignore::without_formatting(&text, ctx.line_ending)));
                return Doc::group(docs);
            }
        }
    }
    if !modifiers(node).is_empty() {
        decl.push(print_modifiers(ctx, node));
    }
    if let Some(ret) = field(node, "returns").or_else(|| field(node, "type")) {
        decl.push(print(ctx, ret));
        decl.push(Doc::text(" "));
    }
    if node.kind() == "destructor_declaration" {
        decl.push(print_token(ctx, kw(node, "~").unwrap()));
    }
    if let Some(spec) = child_of_kind(node, "explicit_interface_specifier") {
        decl.push(concat_children(ctx, spec));
    }
    if let Some(name) = field(node, "name") {
        decl.push(print_token(ctx, name));
    }
    if let Some(tpl) = field(node, "type_parameters") {
        decl.push(type_parameter_list(ctx, tpl));
    }
    if let Some(pl) = field(node, "parameters") {
        if children(pl).len() > 2 {
            decl.push(parameter_list(ctx, pl));
        } else {
            decl.push(concat_children(ctx, pl));
        }
        decl.push(Doc::if_break(Doc::Null, Doc::soft_line(), None));
    }
    if let Some(init) = child_of_kind(node, "constructor_initializer") {
        let kids = children(init);
        let colon = print_token_with_suffix(ctx, kids[0], " ");
        let this_or_base = print_token(ctx, kids[1]);
        let args = Doc::group_of(exprs::argument_list(ctx, kids[2]));
        decl.push(group!(
            indent!(Doc::hard_line()),
            indent!(colon),
            this_or_base,
            Doc::indent_of(args)
        ));
    }
    docs.push(Doc::group(decl));

    let clauses = children_of_kind(node, "type_parameter_constraints_clause");
    if !clauses.is_empty() {
        docs.push(constraint_clauses(ctx, &clauses));
    }
    if let Some(body) = field(node, "body") {
        docs.push(print(ctx, body));
    } else if let Some(arrow) = child_of_kind(node, "arrow_expression_clause") {
        docs.push(arrow_expression_clause(ctx, arrow));
    }
    docs.push(print_token_opt(ctx, kw(node, ";")));
    Doc::group(docs)
}

pub(super) fn parameter_list(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let (items, seps) = list_parts(node, true);
    group!(
        print_token(ctx, kids[0]),
        if items.is_empty() {
            Doc::Null
        } else {
            concat![
                indent!(
                    Doc::soft_line(),
                    separated_list(ctx, &items, &seps, parameter, Doc::line(), 0)
                ),
                Doc::soft_line()
            ]
        },
        print_token(ctx, *kids.last().unwrap())
    )
}

pub(super) fn parameter(ctx: &mut Ctx, node: Node) -> Doc {
    let has_attribute = child_of_kind(node, "attribute_list").is_some();
    let mut docs = Vec::new();
    if has_attribute {
        docs.push(print_attribute_lists(ctx, node));
        let attr_count = children_of_kind(node, "attribute_list").len();
        let in_empty_list = node.parent().map(|p| children(p).len() == 3).unwrap_or(false);
        if attr_count < 2 && (ctx.leading_trivia(node).any_comment() || in_empty_list) {
            docs.push(Doc::text(" "));
        } else {
            docs.push(Doc::indent_of(Doc::line()));
        }
    }
    let mods = modifiers(node);
    if !mods.is_empty() {
        let printed: Vec<Doc> = mods.iter().map(|m| print_token(ctx, *m)).collect();
        docs.push(group!(Doc::join(Doc::text(" "), printed), " "));
    }
    let name = field(node, "name");
    if let Some(ty) = field(node, "type") {
        docs.push(print(ctx, ty));
        if name.is_some() {
            docs.push(Doc::text(" "));
        }
    }
    docs.push(print_token_opt(ctx, name));
    let kids = children(node);
    if let Some(eq) = kids.iter().position(|c| c.kind() == "=") {
        docs.push(equals_value_clause(ctx, node, kids[eq], kids[eq + 1]));
    }
    if has_attribute {
        Doc::group(docs)
    } else {
        Doc::concat(docs)
    }
}

// ---- statements -----------------------------------------------------------------------------

pub(super) fn block(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let open = kids[0];
    let close = *kids.last().unwrap();
    let statements: Vec<Node> = kids[1..kids.len() - 1].to_vec();
    let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");

    if statements.is_empty()
        && parent_kind == "method_declaration"
        && !super::has_comments(&ctx.tokens.list[ctx.token_index(close)].leading)
        && !super::has_comments(&ctx.tokens.list[ctx.token_index(close)].trailing)
    {
        return concat![" ", print_token(ctx, open), " ", print_token(ctx, close)];
    }

    let statement_separator = if parent_kind == "accessor_declaration" && statements.len() <= 1 {
        Doc::line()
    } else {
        Doc::hard_line()
    };

    let mut inner = Doc::Null;
    if !statements.is_empty() {
        let printed: Vec<Doc> = ignore::print_nodes_respecting_range_ignore(ctx, &statements);
        inner = Doc::indent(vec![
            statement_separator.clone(),
            Doc::join(statement_separator.clone(), printed),
        ]);
        inner = remove_initial_double_hard_line(inner);
    }

    let result = group!(
        if matches!(parent_kind, "block" | "global_statement")
            || (parent_kind == "lambda_expression"
                && field(node.parent().unwrap(), "parameters")
                    .map(|p| p.kind() != "implicit_parameter")
                    .unwrap_or(false))
        {
            Doc::Null
        } else {
            Doc::line()
        },
        print_token(ctx, open),
        if statements.is_empty() {
            Doc::text(" ")
        } else {
            concat![inner, statement_separator]
        },
        print_token(ctx, close)
    );

    if parent_kind == "block" {
        concat![extra_new_lines(ctx, node), result]
    } else {
        result
    }
}

pub(super) fn optional_braces(ctx: &mut Ctx, node: Node) -> Doc {
    if node.kind() == "block" {
        block(ctx, node)
    } else {
        remove_initial_double_hard_line(Doc::indent(vec![Doc::hard_line(), print(ctx, node)]))
    }
}

pub(super) fn expression_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let semicolon = *kids.last().unwrap();
    let semicolon_leading = print_leading_trivia(ctx, semicolon);
    group!(
        extra_new_lines(ctx, node),
        print(ctx, kids[0]),
        semicolon_leading,
        print_token_without_leading_trivia(ctx, semicolon)
    )
}

pub(super) fn return_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let expr = kids.get(1).filter(|k| k.kind() != ";");
    group!(
        extra_new_lines(ctx, node),
        print_token_with_suffix(ctx, kids[0], if expr.is_some() { " " } else { "" }),
        match expr {
            Some(e) if matches!(e.kind(), "binary_expression" | "query_expression") =>
                Doc::indent_of(print(ctx, *e)),
            Some(e) => print(ctx, *e),
            None => Doc::Null,
        },
        print_token(ctx, *kids.last().unwrap())
    )
}

pub(super) fn local_declaration_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = vec![extra_new_lines(ctx, node)];
    for c in children(node) {
        match c.kind() {
            "await" | "using" => docs.push(print_token_with_suffix(ctx, c, " ")),
            "modifier" => docs.push(group!(print_token(ctx, c), " ")),
            "variable_declaration" => docs.push(variable_declaration(ctx, c)),
            _ => docs.push(print_token(ctx, c)),
        }
    }
    Doc::concat(docs)
}

pub(super) fn if_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = Vec::new();
    let in_else = node.parent().map(|p| p.kind()) == Some("if_statement")
        && field(node.parent().unwrap(), "alternative").map(|a| a.id()) == Some(node.id());
    if !in_else {
        docs.push(extra_new_lines(ctx, node));
    }
    let kids = children(node);
    let if_kw = kids[0];
    let open = kids[1];
    let condition = field(node, "condition").unwrap();
    let close = kids.iter().find(|c| c.kind() == ")").copied().unwrap();
    let consequence = field(node, "consequence").unwrap();

    docs.push(print_token(ctx, if_kw));
    docs.push(Doc::text(" "));
    docs.push(group!(
        print_token(ctx, open),
        indent!(Doc::if_break(Doc::soft_line(), Doc::Null, None), print(ctx, condition)),
        Doc::if_break(Doc::soft_line(), Doc::Null, None)
    ));
    docs.push(print_token(ctx, close));
    docs.push(optional_braces(ctx, consequence));

    if let Some(else_kw) = kw(node, "else") {
        let alternative = field(node, "alternative").unwrap();
        docs.push(Doc::hard_line());
        docs.push(print_token(ctx, else_kw));
        if alternative.kind() == "if_statement" {
            docs.push(concat![" ", if_statement(ctx, alternative)]);
        } else {
            docs.push(optional_braces(ctx, alternative));
        }
    }
    Doc::concat(docs)
}

// ---- expressions ----------------------------------------------------------------------------

/// APPROXIMATION: the real thing is a port of Prettier's member chain, still to do.
pub(super) fn member_chain_simple(ctx: &mut Ctx, node: Node) -> Doc {
    concat_children(ctx, node)
}

pub(super) fn assignment_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let left = field(node, "left").unwrap();
    let op = field(node, "operator").unwrap();
    let right = field(node, "right").unwrap();
    let left_doc = concat![print(ctx, left), " "];
    let op_doc = print_token(ctx, op);
    right_hand_side(ctx, node, left_doc, op_doc, right)
}

/// (left, operator, right) of anything Roslyn models as a BinaryExpression
pub(super) fn binary_parts<'t>(ctx: &Ctx, node: Node<'t>) -> Option<(Node<'t>, Node<'t>, Node<'t>)> {
    match node.kind() {
        "binary_expression" | "as_expression" => Some((
            field(node, "left")?,
            field(node, "operator")?,
            field(node, "right")?,
        )),
        "is_pattern_expression" => {
            let pattern = field(node, "pattern")?;
            if pattern.kind() != "constant_pattern" {
                return None;
            }
            let inner = children(pattern)[0];
            let _ = ctx;
            if matches!(
                inner.kind(),
                "identifier" | "qualified_name" | "generic_name" | "predefined_type"
                    | "nullable_type" | "array_type" | "alias_qualified_name"
            ) {
                Some((field(node, "expression")?, kw(node, "is")?, inner))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(super) fn is_binary_like(ctx: &Ctx, node: Node) -> bool {
    binary_parts(ctx, node).is_some()
}

pub(super) fn binary_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let docs = print_binary(ctx, node);
    let parent_kind = node.parent().map(|p| p.kind()).unwrap_or("");
    if parent_kind == "if_statement" {
        return Doc::concat(docs);
    }
    let should_not_indent = matches!(
        parent_kind,
        "arrow_expression_clause"
            | "assignment_expression"
            | "catch_filter_clause"
            | "checked_expression"
            | "do_statement"
            | "equals_value_clause"
            | "variable_declarator"
            | "parenthesized_expression"
            | "return_statement"
            | "switch_expression"
            | "switch_statement"
            | "where_clause"
            | "while_statement"
    ) || (parent_kind == "lambda_expression")
        || (parent_kind == "conditional_expression"
            && field(node.parent().unwrap(), "consequence").map(|n| n.id()) != Some(node.id())
            && field(node.parent().unwrap(), "alternative").map(|n| n.id()) != Some(node.id()));
    if should_not_indent {
        Doc::group(docs)
    } else {
        let mut it = docs.into_iter();
        let first = it.next().unwrap();
        Doc::group(vec![first, Doc::indent(it.collect())])
    }
}

fn precedence_of_op(op: &str) -> i32 {
    match op {
        "??" => 1,
        "||" => 2,
        "&&" => 3,
        "|" => 4,
        "^" => 5,
        "&" => 6,
        "!=" => 7,
        "==" => 8,
        "<" | "<=" | ">" | ">=" | "is" | "as" => 9,
        "<<" | ">>" | ">>>" => 10,
        "-" | "+" => 11,
        "*" | "/" | "%" => 12,
        _ => -1,
    }
}

fn precedence(ctx: &Ctx, node: Node) -> i32 {
    match binary_parts(ctx, node) {
        Some((_, op, _)) => precedence_of_op(ctx.text(op)),
        None => -1,
    }
}

fn print_binary(ctx: &mut Ctx, node: Node) -> Vec<Doc> {
    let Some((left, op, right)) = binary_parts(ctx, node) else {
        return vec![Doc::group_of(print(ctx, node))];
    };
    let op_text = ctx.text(op).to_string();
    let parent = node.parent().unwrap();

    // This group ensures that something like == 0 does not end up on its own line
    let parent_op_text = binary_parts(ctx, parent).map(|(_, o, _)| ctx.text(o).to_string());
    let should_group = (op_text != "??" || should_group_null_coalescing(ctx, node))
        && parent_op_text.as_deref() != Some(op_text.as_str())
        && precedence(ctx, node) != precedence(ctx, parent)
        && !is_binary_like(ctx, left)
        && !is_binary_like(ctx, right)
        && left.kind() != "is_pattern_expression";

    // nested ?? have the next level on the right side, everything else has it on the left
    let binary_on_the_right = op_text == "??";
    let mut docs = Vec::new();
    if binary_on_the_right {
        docs.push(print(ctx, left));
        docs.push(Doc::line());
        docs.push(print_token(ctx, op));
        docs.push(Doc::text(" "));
    }
    let possible = if binary_on_the_right { right } else { left };
    // Put all operators with the same precedence level in the same group
    let flatten = binary_parts(ctx, possible)
        .map(|(_, child_op, _)| precedence_of_op(&op_text) == precedence_of_op(ctx.text(child_op)))
        .unwrap_or(false);
    if flatten {
        docs.extend(print_binary(ctx, possible));
    } else {
        docs.push(print(ctx, possible));
    }

    if binary_on_the_right {
        return if should_group {
            let mut it = docs.into_iter();
            let first = it.next().unwrap();
            vec![first, Doc::group(it.collect())]
        } else {
            docs
        };
    }

    let right_doc = concat![Doc::line(), print_token(ctx, op), " ", print(ctx, right)];
    docs.push(if should_group { Doc::group_of(right_doc) } else { right_doc });
    docs
}

fn any_descendant(node: Node, skip: Option<usize>, pred: &dyn Fn(Node) -> bool) -> bool {
    if Some(node.id()) == skip {
        return false;
    }
    if pred(node) {
        return true;
    }
    children(node).into_iter().any(|c| any_descendant(c, skip, pred))
}

fn leftmost(node: Node) -> Node {
    match node.kind() {
        "member_access_expression" => leftmost(field(node, "expression").unwrap()),
        "invocation_expression" => leftmost(field(node, "function").unwrap()),
        "conditional_access_expression" => leftmost(children(node)[0]),
        "await_expression" => leftmost(children(node)[1]),
        _ => node,
    }
}

fn has_invocation_excluding_leftmost_parenthesized(expr: Node) -> bool {
    let l = leftmost(expr);
    let excluded = if l.kind() == "parenthesized_expression" { Some(l.id()) } else { None };
    any_descendant(expr, excluded, &|n| {
        matches!(n.kind(), "invocation_expression" | "conditional_access_expression")
    })
}

fn is_terminal_invocation(expr: Node) -> bool {
    match expr.kind() {
        "invocation_expression" => {
            !has_invocation_excluding_leftmost_parenthesized(field(expr, "function").unwrap())
        }
        "conditional_access_expression" => {
            let kids = children(expr);
            let when_not_null = *kids.last().unwrap();
            let rightmost = rightmost_expression(when_not_null);
            rightmost.kind() == "invocation_expression"
                && !any_descendant(when_not_null, None, &|n| {
                    n.kind() == "invocation_expression" && n.id() != rightmost.id()
                })
                && !has_invocation_excluding_leftmost_parenthesized(kids[0])
        }
        "await_expression" => is_terminal_invocation(children(expr)[1]),
        _ => false,
    }
}

fn rightmost_expression(expr: Node) -> Node {
    match expr.kind() {
        "conditional_access_expression" => rightmost_expression(*children(expr).last().unwrap()),
        "member_access_expression" => rightmost_expression(field(expr, "expression").unwrap()),
        _ => expr,
    }
}

fn should_group_null_coalescing(ctx: &Ctx, node: Node) -> bool {
    let Some((left, op, _)) = binary_parts(ctx, node) else { return false };
    if ctx.text(op) != "??" {
        return false;
    }
    is_terminal_invocation(left)
        || left.kind() == "parenthesized_expression"
        || (left.kind() == "cast_expression"
            && field(left, "value").map(|v| v.kind() == "parenthesized_expression").unwrap_or(false))
        || left.kind() == "switch_expression"
}

// ---- right hand side ------------------------------------------------------------------------

#[derive(PartialEq, Eq, Debug)]
enum Layout {
    BasicConcatWithoutLine,
    BasicConcatWithSpace,
    BreakAfterOperator,
    Chain,
    ChainTail,
    Fluid,
}

pub(super) fn right_hand_side(ctx: &mut Ctx, left_node: Node, left: Doc, operator: Doc, right: Node) -> Doc {
    let layout = determine_layout(ctx, left_node, right);
    match layout {
        Layout::BasicConcatWithoutLine => concat![left, operator, print(ctx, right)],
        Layout::BasicConcatWithSpace => concat![left, operator, " ", print(ctx, right)],
        Layout::BreakAfterOperator => group!(
            Doc::group_of(left),
            operator,
            Doc::group_of(Doc::indent(vec![Doc::line(), print(ctx, right)]))
        ),
        Layout::Chain => concat![Doc::group_of(left), operator, Doc::line(), print(ctx, right)],
        Layout::ChainTail => concat![
            Doc::group_of(left),
            operator,
            Doc::indent(vec![Doc::line(), print(ctx, right)])
        ],
        Layout::Fluid => {
            let id = Doc::next_group_id();
            group!(
                Doc::group_of(left),
                operator,
                Doc::group_with_id(id, vec![Doc::indent(vec![Doc::line()])]),
                Doc::indent_if_break(print(ctx, right), id)
            )
        }
    }
}

fn determine_layout(ctx: &Ctx, left_node: Node, right: Node) -> Layout {
    if ctx.leading_trivia(right).any_comment() {
        return Layout::BreakAfterOperator;
    }
    let is_tail = right.kind() != "assignment_expression";
    let parent_kind = left_node.parent().map(|p| p.kind()).unwrap_or("");
    let should_use_chain = left_node.kind() == "assignment_expression"
        && matches!(parent_kind, "assignment_expression" | "equals_value_clause")
        && (!is_tail
            || !matches!(
                left_node.parent().and_then(|p| p.parent()).map(|p| p.kind()).unwrap_or(""),
                "expression_statement" | "variable_declarator" | "arrow_expression_clause"
            ));
    if should_use_chain {
        return if !is_tail { Layout::Chain } else { Layout::ChainTail };
    }
    if !is_tail
        && field(right, "right").map(|r| r.kind() == "assignment_expression").unwrap_or(false)
    {
        return Layout::BreakAfterOperator;
    }
    let rk = right.kind();
    if rk == "object_creation_expression"
        && field(right, "type").map(|t| t.kind() == "generic_name").unwrap_or(false)
        && field(right, "arguments").map(|a| children(a).len() == 2).unwrap_or(true)
        && field(right, "initializer").is_none()
    {
        return Layout::BreakAfterOperator;
    }
    let condition_kind = field(right, "condition").map(|c| c.kind()).unwrap_or("");
    let is_conditional = rk == "conditional_expression";
    if is_conditional
        && (matches!(condition_kind, "binary_expression" | "parenthesized_expression")
            || (field(right, "alternative").map(|a| a.kind() == "conditional_expression").unwrap_or(false)
                && field(right, "consequence").map(|a| a.kind() != "conditional_expression").unwrap_or(true)))
    {
        return Layout::BreakAfterOperator;
    }
    match rk {
        "collection_expression" | "raw_string_literal" => Layout::BasicConcatWithSpace,
        "initializer_expression" => Layout::BasicConcatWithoutLine,
        "binary_expression" | "interpolated_string_expression" | "is_pattern_expression"
        | "string_literal" | "verbatim_string_literal" | "integer_literal" | "real_literal"
        | "character_literal" | "boolean_literal" | "null_literal" | "query_expression" => {
            Layout::BreakAfterOperator
        }
        "cast_expression" if field(right, "type").map(|t| t.kind() == "generic_name").unwrap_or(false) => {
            Layout::BreakAfterOperator
        }
        "stackalloc_expression" if child_of_kind(right, "initializer_expression").is_none() => {
            Layout::BreakAfterOperator
        }
        _ => Layout::Fluid,
    }
}

// ---- doc utilities --------------------------------------------------------------------------

pub(super) fn remove_initial_double_hard_line_list(docs: Vec<Doc>) -> Vec<Doc> {
    let mut remove_next = false;
    remove_initial_in_list(&docs, &mut remove_next);
    docs
}

pub(super) fn remove_initial_in_list(docs: &[Doc], remove_next: &mut bool) {
    // returns after the first non-hardline doc, like the C# version
    let _ = (docs, remove_next);
}

/// Removes the second of two leading hard lines, which are produced when the first member
/// already starts with a blank line.
pub fn remove_initial_double_hard_line(doc: Doc) -> Doc {
    let mut remove_next = false;
    remove_initial(&doc, &mut remove_next)
}

pub(super) fn remove_initial(doc: &Doc, remove_next: &mut bool) -> Doc {
    match doc {
        Doc::Indent(contents) => {
            if is_hard_line(contents) {
                if *remove_next {
                    return Doc::indent_of(Doc::Null);
                }
                *remove_next = true;
                return doc.clone();
            }
            Doc::indent_of(remove_initial(contents, remove_next))
        }
        Doc::Group(g) if g.options.is_none() => {
            if is_hard_line(&g.contents) {
                if *remove_next {
                    return Doc::group_of(Doc::Null);
                }
                *remove_next = true;
                return doc.clone();
            }
            let inner = remove_initial(&g.contents, remove_next);
            Doc::Group(std::rc::Rc::new(crate::doc::GroupDoc {
                contents: inner,
                brk: g.brk.clone(),
                id: g.id,
                options: None,
            }))
        }
        Doc::Concat(list) => {
            let mut list = list.borrow_mut();
            let mut x = 0;
            while x < list.len() {
                if is_hard_line(&list[x]) {
                    if *remove_next {
                        list[x] = Doc::Null;
                        return doc.clone();
                    }
                    *remove_next = true;
                } else {
                    let replaced = remove_initial(&list[x].clone(), remove_next);
                    list[x] = replaced;
                    return doc.clone();
                }
                x += 1;
            }
            doc.clone()
        }
        _ => doc.clone(),
    }
}

pub(super) fn print_child_pub(ctx: &mut Ctx, child: Node) -> Doc {
    print_child(ctx, child)
}

pub(super) fn print_attribute_lists_pub(ctx: &mut Ctx, node: Node) -> Doc {
    print_attribute_lists(ctx, node)
}

pub(super) fn list_parts_pub<'t>(node: Node<'t>) -> (Vec<Node<'t>>, Vec<Node<'t>>) {
    list_parts(node, true)
}

pub(super) fn print_modifiers_pub(ctx: &mut Ctx, node: Node) -> Doc {
    print_modifiers(ctx, node)
}

fn delegate_declaration(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = vec![
        print_attribute_lists(ctx, node),
        print_modifiers(ctx, node),
        print_token_with_suffix(ctx, kw(node, "delegate").unwrap(), " "),
        print(ctx, field(node, "type").unwrap()),
        Doc::text(" "),
        print_token(ctx, field(node, "name").unwrap()),
    ];
    if let Some(tpl) = child_of_kind(node, "type_parameter_list") {
        docs.push(type_parameter_list(ctx, tpl));
    }
    docs.push(parameter_list(ctx, field(node, "parameters").unwrap()));
    let clauses = children_of_kind(node, "type_parameter_constraints_clause");
    if !clauses.is_empty() {
        docs.push(constraint_clauses(ctx, &clauses));
    }
    docs.push(print_token(ctx, kw(node, ";").unwrap()));
    Doc::concat(docs)
}

fn enum_member_declaration(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = vec![print_attribute_lists(ctx, node), print_modifiers(ctx, node)];
    docs.push(print_token(ctx, field(node, "name").unwrap()));
    let kids = children(node);
    if let Some(eq) = kids.iter().position(|c| c.kind() == "=") {
        docs.push(equals_value_clause(ctx, node, kids[eq], kids[eq + 1]));
    }
    Doc::concat(docs)
}

fn enum_declaration(ctx: &mut Ctx, node: Node) -> Doc {
    let mut docs = vec![print_attribute_lists(ctx, node), print_modifiers(ctx, node)];
    docs.push(print_token_with_suffix(ctx, kw(node, "enum").unwrap(), " "));
    docs.push(print_token(ctx, field(node, "name").unwrap()));
    if let Some(bl) = child_of_kind(node, "base_list") {
        docs.push(base_list(ctx, bl));
    }
    let body = field(node, "body").unwrap();
    let kids = children(body);
    let open = kids[0];
    let close = *kids.last().unwrap();
    let inner = &kids[1..kids.len() - 1];
    let members: Vec<Node> = inner.iter().copied().filter(|c| c.kind() != ",").collect();
    let seps: Vec<Node> = inner.iter().copied().filter(|c| c.kind() == ",").collect();
    if !members.is_empty() {
        let content = Doc::indent(members::print_enum(ctx, node, &members, &seps, close));
        let content = remove_initial_double_hard_line(content);
        docs.push(Doc::hard_line());
        docs.push(print_token(ctx, open));
        docs.push(content);
        docs.push(Doc::hard_line());
        docs.push(print_token(ctx, close));
    } else {
        let separator = if super::has_comments(&ctx.tokens.list[ctx.token_index(close)].leading) {
            Doc::line()
        } else {
            Doc::text(" ")
        };
        docs.push(separator.clone());
        docs.push(print_token(ctx, open));
        docs.push(separator);
        docs.push(print_token(ctx, close));
    }
    docs.push(print_token_opt(ctx, kw(node, ";")));
    Doc::concat(docs)
}

pub(super) fn base_list(ctx: &mut Ctx, bl: Node) -> Doc {
    let kids = children(bl);
    let (items, seps) = list_parts(bl, false);
    let colon = kids[0];
    let rest = if items.len() > 1 {
        Doc::indent(vec![
            print_token(ctx, seps[0]),
            Doc::line(),
            separated_list(ctx, &items, &seps, print, Doc::line(), 1),
        ])
    } else {
        Doc::Null
    };
    let base_list_doc = concat![print_token(ctx, colon), " ", print(ctx, items[0]), rest];
    group!(indent!(Doc::line(), base_list_doc))
}

fn anonymous_object_creation(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let new_kw = kids[0];
    let open = kids[1];
    let close = *kids.last().unwrap();
    let inner = &kids[2..kids.len() - 1];
    // items are the value expressions, names are found from the preceding `name =`
    let mut items = Vec::new();
    let mut seps = Vec::new();
    let mut i = 0;
    while i < inner.len() {
        if inner[i].kind() == "," {
            seps.push(inner[i]);
        } else if inner.get(i + 1).map(|c| c.kind()) == Some("=") {
            items.push(inner[i + 2]);
            i += 2;
        } else {
            items.push(inner[i]);
        }
        i += 1;
    }
    let always_break = items.len() >= 3;
    let list = if items.is_empty() {
        Doc::Null
    } else {
        Doc::indent(vec![
            if always_break { Doc::hard_line() } else { Doc::line() },
            separated_list_with_closing(ctx, &items, &seps, anonymous_member, Doc::line(), 0, Some(close)),
        ])
    };
    group!(
        print_token_with_suffix(ctx, new_kw, Doc::line()),
        print_token(ctx, open),
        list,
        Doc::line(),
        print_token(ctx, close)
    )
}

fn anonymous_member(ctx: &mut Ctx, expr: Node) -> Doc {
    let parent = expr.parent().unwrap();
    let siblings = children(parent);
    let pos = siblings.iter().position(|c| c.id() == expr.id()).unwrap();
    let mut docs = Vec::new();
    let first_node = if pos >= 2 && siblings[pos - 1].kind() == "=" { siblings[pos - 2] } else { expr };
    let first_member_start = siblings
        .iter()
        .find(|c| c.kind() != "new" && c.kind() != "{")
        .map(|c| c.start_byte());
    if Some(first_node.start_byte()) != first_member_start {
        docs.push(extra_new_lines(ctx, first_node));
    }
    if pos >= 2 && siblings[pos - 1].kind() == "=" {
        docs.push(print_token_with_suffix(ctx, siblings[pos - 2], " "));
        docs.push(print_token_with_suffix(ctx, siblings[pos - 1], " "));
    }
    docs.push(print(ctx, expr));
    Doc::concat(docs)
}

fn fixed_statement(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let statement = *kids.last().unwrap();
    let open = kw(node, "(").unwrap();
    let close = kw(node, ")").unwrap();
    let decl = child_of_kind(node, "variable_declaration").unwrap();
    concat![
        extra_new_lines(ctx, node),
        group!(
            print_token(ctx, kw(node, "fixed").unwrap()),
            " ",
            print_token(ctx, open),
            group!(indent!(Doc::soft_line(), variable_declaration(ctx, decl)), Doc::soft_line()),
            print_token(ctx, close),
            Doc::if_break(Doc::Null, Doc::soft_line(), None)
        ),
        if statement.kind() == "block" {
            block(ctx, statement)
        } else {
            Doc::indent_if(
                statement.kind() != "fixed_statement",
                concat![Doc::hard_line(), print(ctx, statement)],
            )
        }
    ]
}

pub(super) fn concat_children_pub(ctx: &mut Ctx, node: Node) -> Doc {
    concat_children(ctx, node)
}

fn with_expression(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let expr = kids[0];
    let with_kw = kw(node, "with").unwrap();
    let open = kw(node, "{").unwrap();
    let close = *kids.last().unwrap();
    let items: Vec<Node> = kids.iter().copied().filter(|c| c.kind() == "with_initializer").collect();
    let seps: Vec<Node> = kids.iter().copied().filter(|c| c.kind() == ",").collect();
    let inner = if items.is_empty() {
        Doc::text(" ")
    } else {
        concat![
            Doc::indent(vec![
                Doc::line(),
                separated_list_with_closing(ctx, &items, &seps, with_initializer, Doc::line(), 0, Some(close))
            ]),
            Doc::line()
        ]
    };
    concat![
        print(ctx, expr),
        " ",
        print_token_with_suffix(ctx, with_kw, Doc::line()),
        print_token(ctx, open),
        inner,
        print_token(ctx, close)
    ]
}

fn with_initializer(ctx: &mut Ctx, node: Node) -> Doc {
    let kids = children(node);
    let left = concat![print(ctx, kids[0]), " "];
    let op = print_token(ctx, kids[1]);
    right_hand_side(ctx, node, left, op, kids[2])
}
