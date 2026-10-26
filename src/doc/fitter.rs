use std::collections::HashMap;
use std::rc::Rc;

use super::printer::{Indent, Indenter, PrintCommand, PrintMode};
use super::{Doc, GroupId, LineType};

fn trailing_whitespace_length(value: &str, existing: usize) -> usize {
    let length = value.bytes().rev().take_while(|b| *b == b' ' || *b == b'\t').count();
    if length == value.len() {
        existing + length
    } else {
        length
    }
}

pub fn fits(
    next_command: PrintCommand,
    remaining_commands: &[PrintCommand],
    mut remaining_width: isize,
    group_mode_map: &mut HashMap<GroupId, PrintMode>,
    indenter: &Indenter,
    scratch: &mut Vec<PrintCommand>,
) -> bool {
    scratch.clear();
    let mut output_length: isize = 0;
    let mut trailing_whitespace: isize = 0;
    let mut return_false_if_more_strings = false;
    scratch.push(next_command);

    // remaining_commands is a stack, so its top is the last element
    let mut x = 0;
    while x < remaining_commands.len() || !scratch.is_empty() {
        if remaining_width < 0 {
            return false;
        }

        let PrintCommand { indent, mode, doc } = match scratch.pop() {
            Some(c) => c,
            None => {
                let c = remaining_commands[remaining_commands.len() - 1 - x].clone();
                x += 1;
                c
            }
        };

        let mut push = |doc: Doc, mode: PrintMode, indent: Rc<Indent>| {
            scratch.push(PrintCommand { indent, mode, doc });
        };

        match &doc {
            Doc::Null => {}
            Doc::Str(s) => {
                // directives should not be considered when calculating if something fits
                if s.is_directive {
                    continue;
                }
                if return_false_if_more_strings {
                    return false;
                }
                output_length += s.utf16_len as isize;
                trailing_whitespace =
                    trailing_whitespace_length(&s.value, trailing_whitespace as usize) as isize;
                remaining_width -= s.printed_width as isize;
            }
            Doc::Concat(contents) => {
                for d in contents.borrow().iter().rev() {
                    push(d.clone(), mode, indent.clone());
                }
            }
            Doc::Line(line) => {
                if matches!(mode, PrintMode::Flat | PrintMode::ForceFlat) {
                    if line.is_hard_line && line.is_for_trivia {
                        return_false_if_more_strings = false;
                    } else if line.ty == LineType::Hard {
                        return true;
                    }

                    if line.ty != LineType::Soft {
                        output_length += 1;
                        trailing_whitespace += 1;
                        remaining_width -= 1;
                    }
                } else {
                    return true;
                }
            }
            Doc::Group(group) => {
                let group_mode = if group.brk.get() { PrintMode::Break } else { mode };
                // use the last option from a conditionalGroup, which should be the most expanded one
                let contents = match (&group.options, group_mode) {
                    (Some(options), PrintMode::Break) => options.last().unwrap().clone(),
                    _ => group.contents.clone(),
                };
                push(contents, group_mode, indent.clone());
                if let Some(id) = group.id {
                    group_mode_map.insert(id, group_mode);
                }
            }
            Doc::Indent(contents) => {
                push((**contents).clone(), mode, indenter.increase(&indent));
            }
            Doc::IfBreak(if_break) => {
                let if_break_mode = if_break
                    .group_id
                    .and_then(|id| group_mode_map.get(&id).copied())
                    .unwrap_or(mode);
                let contents = if if_break_mode == PrintMode::Break {
                    &if_break.break_contents
                } else {
                    &if_break.flat_contents
                };
                push(contents.clone(), mode, indent.clone());
            }
            Doc::Trim => {
                output_length -= trailing_whitespace;
                remaining_width += trailing_whitespace;
                trailing_whitespace = 0;
            }
            Doc::LeadingComment(_) | Doc::TrailingComment(_) => {
                if output_length > 0 && mode != PrintMode::ForceFlat {
                    return_false_if_more_strings = true;
                }
            }
            Doc::Region(_) => return false,
            Doc::ForceFlat(contents) => push((**contents).clone(), PrintMode::ForceFlat, indent.clone()),
            Doc::BreakParent | Doc::AlwaysFits(_) => {}
        }
    }

    remaining_width > 0
}
