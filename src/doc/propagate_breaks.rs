use std::collections::HashSet;

use super::{Doc, GroupDoc, LineType};
use std::rc::Rc;

enum Item {
    Doc(Doc),
    Exit(Doc),
}

struct State {
    group_stack: Vec<Rc<GroupDoc>>,
    force_flat: i32,
    can_skip_break: bool,
    string_count: usize,
    string_count_at_last_hard_line: usize,
    visited: HashSet<*const GroupDoc>,
}

impl State {
    fn break_parent_group(&self) {
        if let Some(parent) = self.group_stack.last() {
            if parent.options.is_none() {
                parent.brk.set(true);
            }
        }
    }

    fn on_enter(&mut self, doc: &Doc) -> bool {
        match doc {
            Doc::Str(s) if !s.is_directive => {
                self.can_skip_break = false;
                if !s.value.is_empty() {
                    self.string_count += 1;
                }
            }
            Doc::Group(g) => {
                self.can_skip_break = true;
                self.group_stack.push(g.clone());
                if !self.visited.insert(Rc::as_ptr(g)) {
                    return false;
                }
            }
            Doc::BreakParent | Doc::Line(_) => {
                let (is_break_parent, is_hard_line, is_literal, for_trivia) = match doc {
                    Doc::BreakParent => (true, false, false, false),
                    Doc::Line(l) => (
                        l.ty == LineType::Hard,
                        l.is_hard_line,
                        l.is_literal,
                        l.is_for_trivia,
                    ),
                    _ => unreachable!(),
                };
                if is_break_parent && (self.force_flat == 0 || is_literal) {
                    let at_start = self.string_count == self.string_count_at_last_hard_line;
                    if is_hard_line && for_trivia && (at_start || self.can_skip_break) {
                        if !at_start && self.group_stack.len() > 1 {
                            let n = self.group_stack.len();
                            self.group_stack[n - 2].brk.set(true);
                        }
                    } else {
                        self.break_parent_group();
                    }
                    if is_hard_line {
                        self.string_count_at_last_hard_line = self.string_count;
                    }
                }
            }
            Doc::ForceFlat(_) => self.force_flat += 1,
            _ => {}
        }
        true
    }

    fn on_exit(&mut self, doc: &Doc) {
        match doc {
            Doc::ForceFlat(_) => self.force_flat -= 1,
            Doc::Group(_) => {
                self.can_skip_break = false;
                let group = self.group_stack.pop().unwrap();
                if group.brk.get() {
                    self.break_parent_group();
                }
            }
            _ => {}
        }
    }
}

pub fn run_on(document: &Doc) {
    let mut state = State {
        group_stack: Vec::new(),
        force_flat: 0,
        can_skip_break: false,
        string_count: 0,
        string_count_at_last_hard_line: 0,
        visited: HashSet::new(),
    };
    let mut stack = vec![Item::Doc(document.clone())];
    while let Some(item) = stack.pop() {
        let doc = match item {
            Item::Exit(doc) => {
                state.on_exit(&doc);
                continue;
            }
            Item::Doc(doc) => doc,
        };

        if !state.on_enter(&doc) {
            state.on_exit(&doc);
            continue;
        }

        if matches!(doc, Doc::ForceFlat(_) | Doc::Group(_)) {
            stack.push(Item::Exit(doc.clone()));
        }

        match &doc {
            Doc::Concat(contents) => {
                let mut contents = contents.borrow_mut();
                for x in (0..contents.len()).rev() {
                    if state.force_flat > 0 {
                        if let Doc::Line(l) = &contents[x] {
                            if !l.is_literal {
                                contents[x] = if l.ty == LineType::Soft {
                                    Doc::text("")
                                } else {
                                    Doc::text(" ")
                                };
                            }
                        }
                    }
                    stack.push(Item::Doc(contents[x].clone()));
                }
            }
            Doc::IfBreak(i) => {
                stack.push(Item::Doc(i.flat_contents.clone()));
                stack.push(Item::Doc(i.break_contents.clone()));
            }
            Doc::Group(g) => match &g.options {
                Some(options) => {
                    for o in options {
                        stack.push(Item::Doc(o.clone()));
                    }
                }
                None => stack.push(Item::Doc(g.contents.clone())),
            },
            Doc::Indent(c) | Doc::ForceFlat(c) => stack.push(Item::Doc((**c).clone())),
            _ => {}
        }
    }
}
