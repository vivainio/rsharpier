use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::fitter;
use super::propagate_breaks;
use super::{CommentDoc, CommentType, Doc, GroupDoc, GroupId, LineDoc, LineType, StrDoc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EndOfLine {
    /// use the line ending of the first line of the source
    #[default]
    Auto,
    Lf,
    Crlf,
}

#[derive(Clone, Debug)]
pub struct PrinterOptions {
    pub end_of_line: EndOfLine,
    pub width: usize,
    pub indent_size: usize,
    pub use_tabs: bool,
    pub trim_initial_lines: bool,
    pub include_generated: bool,
}

impl Default for PrinterOptions {
    fn default() -> Self {
        PrinterOptions {
            end_of_line: EndOfLine::Auto,
            width: 100,
            indent_size: 4,
            use_tabs: false,
            trim_initial_lines: true,
            include_generated: false,
        }
    }
}

#[derive(Default)]
pub struct Indent {
    pub value: String,
    pub length: usize,
    increased: RefCell<Option<Rc<Indent>>>,
}

pub struct Indenter {
    use_tabs: bool,
    indent_size: usize,
}

impl Indenter {
    fn new(options: &PrinterOptions) -> Self {
        Indenter {
            use_tabs: options.use_tabs,
            indent_size: options.indent_size,
        }
    }

    pub fn increase(&self, indent: &Rc<Indent>) -> Rc<Indent> {
        indent
            .increased
            .borrow_mut()
            .get_or_insert_with(|| {
                Rc::new(Indent {
                    value: if self.use_tabs {
                        format!("{}\t", indent.value)
                    } else {
                        format!("{}{}", indent.value, " ".repeat(self.indent_size))
                    },
                    length: indent.length + self.indent_size,
                    increased: RefCell::new(None),
                })
            })
            .clone()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PrintMode {
    Flat,
    Break,
    ForceFlat,
}

#[derive(Clone)]
pub struct PrintCommand {
    pub indent: Rc<Indent>,
    pub mode: PrintMode,
    pub doc: Doc,
}

struct DocPrinter<'a> {
    remaining: Vec<PrintCommand>,
    group_modes: HashMap<GroupId, PrintMode>,
    current_width: usize,
    output: String,
    should_remeasure: bool,
    new_line_next_string_value: bool,
    skip_next_new_line: bool,
    end_of_line: &'a str,
    options: &'a PrinterOptions,
    indenter: Indenter,
    region_indents: Vec<Rc<Indent>>,
    scratch: Vec<PrintCommand>,
}

pub fn print(document: &Doc, options: &PrinterOptions, end_of_line: &str) -> String {
    propagate_breaks::run_on(document);

    let mut printer = DocPrinter {
        remaining: vec![PrintCommand {
            indent: Rc::new(Indent::default()),
            mode: PrintMode::Break,
            doc: document.clone(),
        }],
        group_modes: HashMap::new(),
        current_width: 0,
        output: String::new(),
        should_remeasure: false,
        new_line_next_string_value: false,
        skip_next_new_line: false,
        end_of_line,
        options,
        indenter: Indenter::new(options),
        region_indents: Vec::new(),
        scratch: Vec::new(),
    };
    printer.run()
}

/// Mirrors .NET's StringReader.ReadLine: splits on \r, \n and \r\n, no trailing empty line.
fn read_lines(s: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' | b'\n' => {
                lines.push(&s[start..i]);
                if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < s.len() {
        lines.push(&s[start..]);
    }
    lines
}

fn leading_indentation(line: &str, indent_size: usize) -> usize {
    let mut result = 0;
    for c in line.chars() {
        match c {
            ' ' => result += 1,
            '\t' => result += indent_size,
            _ => break,
        }
    }
    result
}

impl DocPrinter<'_> {
    fn run(&mut self) -> String {
        while let Some(cmd) = self.remaining.pop() {
            self.process(cmd);
        }

        let trimmed = self.output.trim_end_matches(['\r', '\n']).len();
        self.output.truncate(trimmed);
        self.output.push_str(self.end_of_line);

        let mut result = std::mem::take(&mut self.output);
        if self.options.trim_initial_lines {
            result = result.trim_start_matches(['\n', '\r']).to_string();
        }
        result
    }

    fn trim_trailing_whitespace(&mut self) -> usize {
        let trimmed = self
            .output
            .bytes()
            .rev()
            .take_while(|b| *b == b' ' || *b == b'\t')
            .count();
        self.output.truncate(self.output.len() - trimmed);
        trimmed
    }

    fn ends_with_new_line_and_whitespace(&self) -> bool {
        for b in self.output.bytes().rev() {
            match b {
                b' ' | b'\t' => continue,
                b'\n' => return true,
                _ => return false,
            }
        }
        false
    }

    fn push(&mut self, doc: Doc, mode: PrintMode, indent: Rc<Indent>) {
        self.remaining.push(PrintCommand { indent, mode, doc });
    }

    fn process(&mut self, cmd: PrintCommand) {
        let PrintCommand { indent, mode, doc } = cmd;
        match &doc {
            Doc::Null => {}
            Doc::Str(s) => self.process_string(s, &indent),
            Doc::Concat(contents) => {
                for d in contents.borrow().iter().rev() {
                    self.push(d.clone(), mode, indent.clone());
                }
            }
            Doc::Line(line) => self.process_line(line, mode, &indent),
            Doc::Group(group) => self.process_group(group, mode, &indent),
            Doc::Indent(contents) => {
                let increased = self.indenter.increase(&indent);
                self.push((**contents).clone(), mode, increased);
            }
            Doc::Trim => {
                self.current_width -= self.trim_trailing_whitespace();
                self.new_line_next_string_value = false;
            }
            Doc::IfBreak(if_break) => {
                let group_mode = match if_break.group_id {
                    Some(id) => *self
                        .group_modes
                        .get(&id)
                        .expect("You cannot use an ifBreak before the group it targets."),
                    None => mode,
                };
                let contents = if group_mode == PrintMode::Break {
                    &if_break.break_contents
                } else {
                    &if_break.flat_contents
                };
                self.push(contents.clone(), mode, indent);
            }
            Doc::BreakParent => {}
            Doc::LeadingComment(comment) => {
                self.trim_trailing_whitespace();
                if (!self.output.is_empty() && !self.output.ends_with('\n'))
                    || self.new_line_next_string_value
                {
                    self.output.push_str(self.end_of_line);
                }
                self.append_comment(comment, &indent);
                self.current_width = indent.length;
                self.new_line_next_string_value = false;
                self.skip_next_new_line = false;
            }
            Doc::TrailingComment(comment) => {
                self.trim_trailing_whitespace();
                self.output.push(' ');
                self.output.push_str(&comment.comment);
                self.current_width = indent.length;
                if mode != PrintMode::ForceFlat {
                    self.new_line_next_string_value = true;
                    self.skip_next_new_line = true;
                }
            }
            Doc::ForceFlat(contents) => {
                self.push((**contents).clone(), PrintMode::ForceFlat, indent);
            }
            Doc::Region(region) => {
                if region.is_end {
                    // when regions are combined with ignored ranges, the start region ends up
                    // printing inside the unformatted nodes, so there may be no matching start
                    let region_indent = self.region_indents.pop().unwrap_or_else(|| indent.clone());
                    self.output.push_str(&region_indent.value);
                } else {
                    self.output.push_str(&indent.value);
                    self.region_indents.push(indent.clone());
                }
                self.output.push_str(&region.text);
            }
            Doc::AlwaysFits(contents) => self.push((**contents).clone(), mode, indent),
        }
    }

    fn append_comment(&mut self, comment: &CommentDoc, indent: &Rc<Indent>) {
        let indent_size = self.options.indent_size;
        let lines = read_lines(&comment.comment);
        let mut spaces_to_add_or_remove: isize = 0;
        if comment.ty == CommentType::MultiLine {
            if let Some(first) = lines.first() {
                // maintain the formatting inside of a multiline comment by changing the
                // indentation of every line by the amount the first line changes
                let first_len = leading_indentation(first, indent_size) as isize;
                let current = leading_indentation(&indent.value, indent_size) as isize;
                spaces_to_add_or_remove = current - first_len;
            }
        }

        for (i, line) in lines.iter().enumerate() {
            if comment.ty == CommentType::SingleLine {
                self.output.push_str(&indent.value);
            } else {
                let mut spaces =
                    leading_indentation(line, indent_size) as isize + spaces_to_add_or_remove;
                if self.options.use_tabs {
                    let indent_length = leading_indentation(&indent.value, indent_size) as isize;
                    if spaces >= indent_length {
                        self.output.push_str(&indent.value);
                        spaces -= indent_length;
                    }
                    while spaces > 0 && spaces >= indent_size as isize {
                        self.output.push('\t');
                        spaces -= indent_size as isize;
                    }
                }
                if spaces > 0 {
                    self.output.push_str(&" ".repeat(spaces as usize));
                }
            }
            self.output.push_str(line.trim());
            if i + 1 == lines.len() {
                return;
            }
            self.output.push_str(self.end_of_line);
        }
    }

    fn process_string(&mut self, s: &StrDoc, indent: &Rc<Indent>) {
        if s.value.is_empty() {
            return;
        }

        // ensures we don't print extra spaces after a trailing comment
        if self.new_line_next_string_value && self.skip_next_new_line && s.value == " " {
            return;
        }

        if self.new_line_next_string_value {
            self.trim_trailing_whitespace();
            self.output.push_str(self.end_of_line);
            self.output.push_str(&indent.value);
            self.current_width = indent.length;
            self.new_line_next_string_value = false;
        }

        self.output.push_str(&s.value);
        self.current_width += s.printed_width;
    }

    fn process_line(&mut self, line: &LineDoc, mode: PrintMode, indent: &Rc<Indent>) {
        if matches!(mode, PrintMode::Flat | PrintMode::ForceFlat) {
            match line.ty {
                LineType::Soft => return,
                LineType::Normal => {
                    self.output.push(' ');
                    self.current_width += 1;
                    return;
                }
                // this line was forced into the output even in flat mode, so the next group
                // must remeasure because the previous measurement was incomplete
                LineType::Hard => self.should_remeasure = true,
            }
        }

        if line.squash && !self.output.is_empty() && self.ends_with_new_line_and_whitespace() {
            return;
        }

        if line.is_literal {
            if !self.output.is_empty() {
                self.output.push_str(self.end_of_line);
                self.current_width = 0;
            }
        } else {
            if !self.skip_next_new_line || !self.new_line_next_string_value {
                if !line.no_trim {
                    self.trim_trailing_whitespace();
                }
                self.output.push_str(self.end_of_line);
                self.output.push_str(&indent.value);
                self.current_width = indent.length;
            }
            if self.skip_next_new_line {
                self.skip_next_new_line = false;
            }
        }
    }

    fn fits(&mut self, command: PrintCommand) -> bool {
        let remaining_width = self.options.width as isize - self.current_width as isize;
        fitter::fits(
            command,
            &self.remaining,
            remaining_width,
            &mut self.group_modes,
            &self.indenter,
            &mut self.scratch,
        )
    }

    fn process_group(&mut self, group: &GroupDoc, mode: PrintMode, indent: &Rc<Indent>) {
        if matches!(mode, PrintMode::Flat | PrintMode::ForceFlat) && !self.should_remeasure {
            let m = if group.brk.get() { PrintMode::Break } else { mode };
            self.push(group.contents.clone(), m, indent.clone());
        } else {
            self.should_remeasure = false;
            let mut possible = PrintCommand {
                indent: indent.clone(),
                mode: PrintMode::Flat,
                doc: group.contents.clone(),
            };

            if !group.brk.get() && self.fits(possible.clone()) {
                self.remaining.push(possible);
            } else if let Some(options) = &group.options {
                if group.brk.get() {
                    self.push(options.last().unwrap().clone(), PrintMode::Break, indent.clone());
                } else {
                    let mut found = false;
                    for option in options.iter().skip(1) {
                        possible = PrintCommand {
                            indent: indent.clone(),
                            mode,
                            doc: option.clone(),
                        };
                        if !self.fits(possible.clone()) {
                            continue;
                        }
                        self.remaining.push(possible.clone());
                        found = true;
                        break;
                    }
                    if !found {
                        self.remaining.push(possible);
                    }
                }
            } else {
                self.push(group.contents.clone(), PrintMode::Break, indent.clone());
            }
        }

        if let Some(id) = group.id {
            let mode = self.remaining.last().unwrap().mode;
            self.group_modes.insert(id, mode);
        }
    }
}
