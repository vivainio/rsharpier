//! Document IR, a port of CSharpier.Core/DocTypes. Modelled on Prettier's doc builders.

mod fitter;
mod printer;
mod propagate_breaks;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

pub use printer::{print, PrinterOptions};

pub type GroupId = u32;

#[derive(Clone)]
pub enum Doc {
    Null,
    Str(Rc<StrDoc>),
    Concat(Rc<RefCell<Vec<Doc>>>),
    Line(Rc<LineDoc>),
    Group(Rc<GroupDoc>),
    Indent(Rc<Doc>),
    Trim,
    IfBreak(Rc<IfBreakDoc>),
    BreakParent,
    LeadingComment(Rc<CommentDoc>),
    TrailingComment(Rc<CommentDoc>),
    ForceFlat(Rc<Doc>),
    Region(Rc<RegionDoc>),
    AlwaysFits(Rc<Doc>),
}

pub struct StrDoc {
    pub value: String,
    pub is_directive: bool,
    pub printed_width: usize,
    /// length in UTF-16 code units, which is what the C# implementation measures
    pub utf16_len: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineType {
    Normal,
    Hard,
    Soft,
}

pub struct LineDoc {
    pub ty: LineType,
    pub is_literal: bool,
    pub squash: bool,
    /// true for HardLine and HardLineNoTrim, false for LiteralLine
    pub is_hard_line: bool,
    pub no_trim: bool,
    pub is_for_trivia: bool,
}

pub struct GroupDoc {
    pub contents: Doc,
    pub brk: Cell<bool>,
    pub id: Option<GroupId>,
    /// Some for a ConditionalGroup
    pub options: Option<Vec<Doc>>,
}

pub struct IfBreakDoc {
    pub break_contents: Doc,
    pub flat_contents: Doc,
    pub group_id: Option<GroupId>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommentType {
    SingleLine,
    MultiLine,
}

pub struct CommentDoc {
    pub ty: CommentType,
    pub comment: String,
}

pub struct RegionDoc {
    pub text: String,
    pub is_end: bool,
}

impl From<&str> for Doc {
    fn from(s: &str) -> Doc {
        Doc::text(s)
    }
}

impl From<String> for Doc {
    fn from(s: String) -> Doc {
        Doc::text(s)
    }
}

impl From<Vec<Doc>> for Doc {
    fn from(v: Vec<Doc>) -> Doc {
        Doc::concat(v)
    }
}

impl From<&Doc> for Doc {
    fn from(d: &Doc) -> Doc {
        d.clone()
    }
}

fn char_width(c: char) -> usize {
    // port of CharacterSizeCalculator, which works on UTF-16 code units
    let mut buf = [0u16; 2];
    c.encode_utf16(&mut buf)
        .iter()
        .map(|&c| {
            let c = c as u32;
            let wide = c >= 0x1100
                && (c <= 0x115f
                    || c == 0x2329
                    || c == 0x232a
                    || ((0x2e80..=0xa4cf).contains(&c) && c != 0x303f)
                    || (0xac00..=0xd7a3).contains(&c)
                    || (0xf900..=0xfaff).contains(&c)
                    || (0xfe10..=0xfe19).contains(&c)
                    || (0xfe30..=0xfe6f).contains(&c)
                    || (0xff00..=0xff60).contains(&c)
                    || (0xffe0..=0xffe6).contains(&c));
            if wide {
                2
            } else {
                1
            }
        })
        .sum()
}

pub fn printed_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

fn line(ty: LineType) -> Doc {
    Doc::Line(Rc::new(LineDoc {
        ty,
        is_literal: false,
        squash: false,
        is_hard_line: false,
        no_trim: false,
        is_for_trivia: false,
    }))
}

fn hard_line(squash: bool, is_for_trivia: bool, no_trim: bool) -> Doc {
    Doc::Line(Rc::new(LineDoc {
        ty: LineType::Hard,
        is_literal: false,
        squash,
        is_hard_line: true,
        no_trim,
        is_for_trivia,
    }))
}

static GROUP_COUNTER: AtomicU32 = AtomicU32::new(0);

impl Doc {
    pub fn text(s: impl Into<String>) -> Doc {
        Doc::str_doc(s.into(), false)
    }

    pub fn directive(s: impl Into<String>) -> Doc {
        Doc::str_doc(s.into(), true)
    }

    fn str_doc(value: String, is_directive: bool) -> Doc {
        Doc::Str(Rc::new(StrDoc {
            printed_width: printed_width(&value),
            utf16_len: value.encode_utf16().count(),
            value,
            is_directive,
        }))
    }

    pub fn hard_line() -> Doc {
        hard_line(false, false, false)
    }
    pub fn hard_line_no_trim() -> Doc {
        hard_line(false, false, true)
    }
    pub fn hard_line_for_trivia() -> Doc {
        hard_line(false, true, false)
    }
    pub fn hard_line_if_no_previous_line() -> Doc {
        hard_line(true, false, false)
    }
    pub fn hard_line_if_no_previous_line_for_trivia() -> Doc {
        hard_line(true, true, false)
    }
    pub fn literal_line() -> Doc {
        Doc::Line(Rc::new(LineDoc {
            ty: LineType::Hard,
            is_literal: true,
            squash: false,
            is_hard_line: false,
            no_trim: false,
            is_for_trivia: false,
        }))
    }
    pub fn line() -> Doc {
        line(LineType::Normal)
    }
    pub fn soft_line() -> Doc {
        line(LineType::Soft)
    }
    pub fn break_parent() -> Doc {
        Doc::BreakParent
    }

    pub fn leading_comment(comment: impl Into<String>, ty: CommentType) -> Doc {
        Doc::LeadingComment(Rc::new(CommentDoc {
            ty,
            comment: comment.into(),
        }))
    }

    pub fn trailing_comment(comment: impl Into<String>, ty: CommentType) -> Doc {
        Doc::TrailingComment(Rc::new(CommentDoc {
            ty,
            comment: comment.into(),
        }))
    }

    pub fn concat(mut contents: Vec<Doc>) -> Doc {
        match contents.len() {
            0 => Doc::Null,
            1 => contents.pop().unwrap(),
            _ => Doc::Concat(Rc::new(RefCell::new(contents))),
        }
    }

    pub fn join(separator: Doc, docs: impl IntoIterator<Item = Doc>) -> Doc {
        let mut out = Vec::new();
        for (i, d) in docs.into_iter().enumerate() {
            if i != 0 {
                out.push(separator.clone());
            }
            out.push(d);
        }
        Doc::concat(out)
    }

    pub fn force_flat(contents: Vec<Doc>) -> Doc {
        Doc::ForceFlat(Rc::new(Doc::concat(contents)))
    }

    pub fn group(contents: Vec<Doc>) -> Doc {
        Doc::group_of(Doc::concat(contents))
    }

    pub fn group_of(contents: Doc) -> Doc {
        Doc::Group(Rc::new(GroupDoc {
            contents,
            brk: Cell::new(false),
            id: None,
            options: None,
        }))
    }

    pub fn group_with_id(id: GroupId, contents: Vec<Doc>) -> Doc {
        Doc::Group(Rc::new(GroupDoc {
            contents: Doc::concat(contents),
            brk: Cell::new(false),
            id: Some(id),
            options: None,
        }))
    }

    /// ids only have to be unique within a single print so an incrementing counter is enough
    pub fn next_group_id() -> GroupId {
        GROUP_COUNTER.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn conditional_group(options: Vec<Doc>) -> Doc {
        assert!(!options.is_empty(), "Options was an empty array");
        Doc::Group(Rc::new(GroupDoc {
            contents: options[0].clone(),
            brk: Cell::new(false),
            id: None,
            options: Some(options),
        }))
    }

    pub fn indent(contents: Vec<Doc>) -> Doc {
        Doc::Indent(Rc::new(Doc::concat(contents)))
    }

    pub fn indent_of(contents: Doc) -> Doc {
        Doc::Indent(Rc::new(contents))
    }

    pub fn indent_if(condition: bool, contents: Doc) -> Doc {
        if condition {
            Doc::indent_of(contents)
        } else {
            contents
        }
    }

    pub fn if_break(break_contents: Doc, flat_contents: Doc, group_id: Option<GroupId>) -> Doc {
        Doc::IfBreak(Rc::new(IfBreakDoc {
            break_contents,
            flat_contents,
            group_id,
        }))
    }

    pub fn indent_if_break(contents: Doc, group_id: GroupId) -> Doc {
        Doc::if_break(Doc::indent_of(contents.clone()), contents, Some(group_id))
    }

    pub fn always_fits(contents: Doc) -> Doc {
        Doc::AlwaysFits(Rc::new(contents))
    }

    pub fn begin_region(text: impl Into<String>) -> Doc {
        Doc::Region(Rc::new(RegionDoc {
            text: text.into(),
            is_end: false,
        }))
    }

    pub fn end_region(text: impl Into<String>) -> Doc {
        Doc::Region(Rc::new(RegionDoc {
            text: text.into(),
            is_end: true,
        }))
    }
}

/// Builds a `Doc::Concat` from anything convertible to a `Doc`.
#[macro_export]
macro_rules! concat {
    ($($d:expr),* $(,)?) => { $crate::doc::Doc::concat(vec![$($crate::doc::Doc::from($d)),*]) };
}

/// Builds a `Doc::Group` from anything convertible to a `Doc`.
#[macro_export]
macro_rules! group {
    ($($d:expr),* $(,)?) => { $crate::doc::Doc::group(vec![$($crate::doc::Doc::from($d)),*]) };
}

/// Builds a `Doc::Indent` from anything convertible to a `Doc`.
#[macro_export]
macro_rules! indent {
    ($($d:expr),* $(,)?) => { $crate::doc::Doc::indent(vec![$($crate::doc::Doc::from($d)),*]) };
}
