//! Port of SyntaxPrinter/MembersWithForcedLines.cs (without csharpier-ignore support yet)

use tree_sitter::Node;

use super::{child_of_kind, extra_new_lines, ignore, modifiers, nodes, print_token, Ctx};
use crate::doc::Doc;
use crate::trivia::{DirectiveKind, TriviaKind, TriviaListExt};

fn has_modifier(ctx: &Ctx, node: Node, text: &str) -> bool {
    modifiers(node).iter().any(|m| ctx.text(*m) == text)
}

pub fn print(
    ctx: &mut Ctx,
    parent_kind: &str,
    parent: Node,
    members: &[Node],
    skip_first_hard_line: bool,
) -> Vec<Doc> {
    print_impl(ctx, parent_kind, parent, members, skip_first_hard_line, None)
}

/// enum members are a separated list, the separator (or a trailing comma) is printed after each
pub fn print_enum(ctx: &mut Ctx, parent: Node, members: &[Node], seps: &[Node], close: Node) -> Vec<Doc> {
    print_impl(ctx, "enum_declaration", parent, members, false, Some((seps, close)))
}

fn print_impl(
    ctx: &mut Ctx,
    parent_kind: &str,
    parent: Node,
    members: &[Node],
    skip_first_hard_line: bool,
    enum_info: Option<(&[Node], Node)>,
) -> Vec<Doc> {
    let mut result = Vec::with_capacity(members.len() * 3);
    if !skip_first_hard_line {
        result.push(Doc::hard_line());
    }

    let mut last_member_forced_blank_line = false;
    let mut unformatted_code = String::new();
    let mut print_unformatted = false;
    for (member_index, &member) in members.iter().enumerate() {
        let mut skip_adding_line_because_ignore_ended = false;
        let leading_trivia = ctx.leading_trivia(member);

        if ignore::trivia_matches(leading_trivia, ignore::IgnoreKind::End) {
            skip_adding_line_because_ignore_ended = true;
            result.push(Doc::text(unformatted_code.trim().to_string()));
            unformatted_code.clear();
            print_unformatted = false;
        } else if ignore::trivia_matches(leading_trivia, ignore::IgnoreKind::Start) {
            if !print_unformatted && member_index > 0 {
                result.push(Doc::hard_line());
                result.push(extra_new_lines(ctx, member));
            }
            print_unformatted = true;
        }

        if print_unformatted {
            unformatted_code.push_str(&ignore::print_node_without_formatting(ctx, member));
            continue;
        }

        let mut blank_line_is_forced = (member.kind() == "method_declaration"
            && parent_kind != "interface_declaration")
            || matches!(
                member.kind(),
                "class_declaration"
                    | "constructor_declaration"
                    | "conversion_operator_declaration"
                    | "destructor_declaration"
                    | "enum_declaration"
                    | "file_scoped_namespace_declaration"
                    | "interface_declaration"
                    | "namespace_declaration"
                    | "operator_declaration"
                    | "record_declaration"
                    | "record_struct_declaration"
                    | "struct_declaration"
            );

        if member.kind() == "method_declaration"
            && parent_kind == "class_declaration"
            && has_modifier(ctx, parent, "abstract")
            && has_modifier(ctx, member, "abstract")
        {
            blank_line_is_forced = false;
        }

        let get_separator = |ctx: &mut Ctx| -> Doc {
            let Some((seps, close)) = enum_info else { return Doc::Null };
            if member_index < seps.len() {
                return print_token(ctx, seps[member_index]);
            }
            let idx = ctx.last_token_index(member);
            if ctx.tokens.list[idx].trailing.any_comment() {
                ctx.state.trailing_comma = Some((idx, nodes::trailing_comma(ctx, close, true)));
                Doc::Null
            } else {
                nodes::trailing_comma(ctx, close, false)
            }
        };

        if member_index == 0 {
            last_member_forced_blank_line = blank_line_is_forced;
            result.push(nodes::print(ctx, member));
            let sep = get_separator(ctx);
            if !matches!(sep, Doc::Null) {
                result.push(sep);
            }
            continue;
        }

        let mut add_blank_line = blank_line_is_forced || last_member_forced_blank_line;

        let mut contains_comment_or_new_line = false;
        let mut print_extra_new_lines = false;
        let mut contains_end_if_or_region = false;
        let mut contains_if = false;
        let mut contains_elif = false;
        let mut contains_else = false;
        let mut contains_end_of_line = false;
        let mut line_follows_end_if = false;

        for (i, t) in leading_trivia.iter().enumerate() {
            let k = t.kind;
            if k == TriviaKind::EndOfLine || t.is_comment() {
                contains_comment_or_new_line = true;
            } else if matches!(
                k,
                TriviaKind::Directive(
                    DirectiveKind::PragmaWarning
                        | DirectiveKind::PragmaChecksum
                        | DirectiveKind::If
                        | DirectiveKind::EndRegion
                )
            ) {
                print_extra_new_lines = true;
            } else if matches!(
                k,
                TriviaKind::Directive(DirectiveKind::EndIf | DirectiveKind::EndRegion)
            ) {
                contains_end_if_or_region = true;
            }

            if k == TriviaKind::Directive(DirectiveKind::EndIf) {
                line_follows_end_if = leading_trivia.index_of_next_end_of_line(i).is_some();
            }

            match k {
                TriviaKind::Directive(DirectiveKind::If) => contains_if = true,
                TriviaKind::Directive(DirectiveKind::Elif) => contains_elif = true,
                TriviaKind::Directive(DirectiveKind::Else) => contains_else = true,
                TriviaKind::EndOfLine => contains_end_of_line = true,
                _ => {}
            }
        }

        if !add_blank_line {
            add_blank_line = child_of_kind(member, "attribute_list").is_some()
                || contains_comment_or_new_line;
        }

        if print_extra_new_lines {
            result.push(extra_new_lines(ctx, member));
        } else if add_blank_line && !contains_end_if_or_region && !skip_adding_line_because_ignore_ended {
            result.push(Doc::hard_line());
        }

        // this handles inserting a new line after directives but before comments on members.
        // The directives are printed by Token, so we can't directly print them here
        if add_blank_line
            && ((!contains_end_if_or_region && contains_if && !contains_end_of_line)
                || (contains_end_if_or_region
                    && !contains_if
                    && !contains_elif
                    && !contains_else
                    && !line_follows_end_if
                    && !print_extra_new_lines))
        {
            ctx.state.next_trivia_needs_line = true;
        }

        // this has a side effect that fixes the trailing comma + trailing comment issue so it has
        // to be called before printing the member
        let separator = get_separator(ctx);
        result.push(Doc::hard_line());
        result.push(nodes::print(ctx, member));
        if !matches!(separator, Doc::Null) {
            result.push(separator);
        }

        last_member_forced_blank_line = blank_line_is_forced;
    }

    if !unformatted_code.is_empty() {
        result.push(Doc::text(unformatted_code.trim().to_string()));
    }

    result
}
