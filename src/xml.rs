//! Port of CSharpier.Core/Xml: a hand written reader that keeps all whitespace information and
//! the printers that turn it into a doc.

use std::collections::HashMap;

use crate::doc::{self, Doc, GroupId, PrinterOptions};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum XmlWhitespaceSensitivity {
    Strict,
    Ignore,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NodeType {
    Document,
    Element,
    Text,
    Comment,
    CData,
    ProcessingInstruction,
    DocumentType,
    Whitespace,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum IgnoreType {
    None,
    Ignore,
    IgnoreStart,
    IgnoreEnd,
}

struct RawAttribute {
    name: String,
    value: String,
}

struct RawNode {
    parent: Option<usize>,
    previous: Option<usize>,
    next: Option<usize>,
    name: String,
    node_type: NodeType,
    is_empty: bool,
    attributes: Vec<RawAttribute>,
    nodes: Vec<usize>,
    value: String,
    sensitivity: XmlWhitespaceSensitivity,
    is_whitespace_preserved: bool,
    has_leading_whitespace: bool,
    start: usize,
    end: usize,
    ignore: IgnoreType,
}

impl RawNode {
    fn new(node_type: NodeType, sensitivity: XmlWhitespaceSensitivity) -> RawNode {
        RawNode {
            parent: None,
            previous: None,
            next: None,
            name: String::new(),
            node_type,
            is_empty: false,
            attributes: Vec::new(),
            nodes: Vec::new(),
            value: String::new(),
            sensitivity,
            is_whitespace_preserved: false,
            has_leading_whitespace: false,
            start: 0,
            end: 0,
            ignore: IgnoreType::None,
        }
    }

    fn is_text_like(&self) -> bool {
        matches!(self.node_type, NodeType::Text | NodeType::Comment | NodeType::CData)
    }
}

#[derive(Debug)]
pub struct XmlError(pub String);

struct Tree {
    nodes: Vec<RawNode>,
}

impl Tree {
    fn get(&self, id: usize) -> &RawNode {
        &self.nodes[id]
    }

    fn last_descendant(&self, id: usize) -> usize {
        let n = self.get(id);
        if n.node_type == NodeType::Element {
            n.nodes.last().copied().unwrap_or(id)
        } else {
            id
        }
    }
}

// ---- reader ---------------------------------------------------------------------------------

struct Reader<'a> {
    xml: &'a str,
    line_ending: &'a str,
    default_sensitivity: XmlWhitespaceSensitivity,
    current_sensitivity: XmlWhitespaceSensitivity,
    is_whitespace_preserved: bool,
    has_leading_whitespace: bool,
    position: usize,
    stack: Vec<usize>,
    tree: Tree,
}

fn ignore_type(comment: &str) -> IgnoreType {
    if !comment.starts_with(" csharpier-") {
        return IgnoreType::None;
    }
    let matches = |prefix: &str| {
        comment
            .strip_prefix(prefix)
            .map(|rest| rest.is_empty() || rest.starts_with("- "))
            .unwrap_or(false)
    };
    if matches(" csharpier-ignore ") {
        IgnoreType::Ignore
    } else if matches(" csharpier-ignore-start ") {
        IgnoreType::IgnoreStart
    } else if matches(" csharpier-ignore-end ") {
        IgnoreType::IgnoreEnd
    } else {
        IgnoreType::None
    }
}

impl<'a> Reader<'a> {
    fn cur(&self) -> Option<char> {
        self.xml[self.position..].chars().next()
    }

    fn byte(&self, pos: usize) -> Option<u8> {
        self.xml.as_bytes().get(pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.cur() {
            if !c.is_whitespace() {
                break;
            }
            self.position += c.len_utf8();
        }
    }

    fn add_node(&mut self, mut node: RawNode) -> usize {
        let id = self.tree.nodes.len();
        if let Some(&parent) = self.stack.last() {
            node.parent = Some(parent);
            self.tree.nodes.push(node);
            self.tree.nodes[parent].nodes.push(id);
        } else {
            self.tree.nodes.push(node);
        }
        id
    }

    fn with_line_endings(&self, s: &str) -> String {
        s.replace('\n', self.line_ending)
    }

    fn parse(mut self) -> Result<(Tree, String), XmlError> {
        let root = RawNode::new(NodeType::Document, self.current_sensitivity);
        self.tree.nodes.push(root);
        self.stack.push(0);

        while self.position < self.xml.len() {
            let mut new_lines = 0;
            let before_ws = self.position;
            while let Some(c) = self.cur() {
                if !c.is_whitespace() {
                    break;
                }
                if c == '\n' {
                    new_lines += 1;
                }
                self.position += c.len_utf8();
            }
            self.has_leading_whitespace = self.position != before_ws;

            let top_is_element = self.tree.get(*self.stack.last().unwrap()).node_type == NodeType::Element;
            if self.is_whitespace_preserved
                && self.has_leading_whitespace
                && top_is_element
                && self.byte(self.position) == Some(b'<')
            {
                // ParseText picks up whitespace that runs into text, this covers the rest of it
                let mut n = RawNode::new(NodeType::Text, self.current_sensitivity);
                n.value = self.with_line_endings(&self.xml[before_ws..self.position]);
                n.has_leading_whitespace = self.has_leading_whitespace;
                n.is_whitespace_preserved = true;
                self.add_node(n);
            } else if new_lines > 1 {
                // all whitespace with more than one newline becomes a single marker that the
                // printer turns into an empty line
                let mut n = RawNode::new(NodeType::Whitespace, self.current_sensitivity);
                n.value = "\n".into();
                n.has_leading_whitespace = self.has_leading_whitespace;
                self.add_node(n);
            }

            self.skip_whitespace();
            if self.position >= self.xml.len() {
                break;
            }

            if self.byte(self.position) == Some(b'<') {
                self.parse_tag()?;
            } else {
                if self.tree.get(0).nodes.is_empty() {
                    return Err(XmlError("There do not appear to be any root nodes".into()));
                }
                self.parse_text();
            }
        }
        Ok((self.tree, self.xml.to_string()))
    }

    fn starts_with_at(&self, pos: usize, s: &str) -> bool {
        self.xml[pos..].starts_with(s)
    }

    fn parse_tag(&mut self) -> Result<(), XmlError> {
        if self.position + 1 >= self.xml.len() {
            return Ok(());
        }
        match self.byte(self.position + 1) {
            Some(b'!') => {
                if self.position + 4 < self.xml.len() && self.starts_with_at(self.position, "<!--") {
                    self.parse_comment();
                } else if self.position + 9 < self.xml.len() && self.starts_with_at(self.position, "<![CDATA[") {
                    self.parse_cdata();
                } else if self.position + 9 < self.xml.len() && self.starts_with_at(self.position, "<!DOCTYPE") {
                    self.parse_doctype();
                }
            }
            Some(b'?') => self.parse_processing_instruction(),
            Some(b'/') => self.parse_end_element(),
            _ => self.parse_start_element()?,
        }
        Ok(())
    }

    fn read_until(&mut self, terminator: &str) -> String {
        let idx = self.xml[self.position..].find(terminator).map(|i| i + self.position);
        let content_end = match idx {
            Some(i) => i,
            None => self
                .position
                .max((self.xml.len() + 1).saturating_sub(terminator.len())),
        };
        let content_end = content_end.min(self.xml.len());
        let content = self.with_line_endings(&self.xml[self.position..content_end]);
        self.position = match idx {
            Some(i) => i + terminator.len(),
            None => content_end,
        };
        content
    }

    fn parse_comment(&mut self) {
        self.position += 4;
        let content = self.read_until("-->");
        let mut n = RawNode::new(NodeType::Comment, self.current_sensitivity);
        n.value = format!("<!--{content}-->");
        n.has_leading_whitespace = self.has_leading_whitespace;
        n.ignore = ignore_type(&content);
        self.add_node(n);
    }

    fn parse_cdata(&mut self) {
        self.position += 9;
        let content = self.read_until("]]>");
        let mut n = RawNode::new(NodeType::CData, self.current_sensitivity);
        n.value = format!("<![CDATA[{content}]]>");
        n.has_leading_whitespace = self.has_leading_whitespace;
        self.add_node(n);
    }

    fn read_name(&mut self) -> String {
        let start = self.position;
        while let Some(c) = self.cur() {
            if c.is_alphanumeric() || matches!(c, '_' | ':' | '-' | '.') {
                self.position += c.len_utf8();
            } else {
                break;
            }
        }
        self.xml[start..self.position].to_string()
    }

    fn parse_processing_instruction(&mut self) {
        self.position += 2;
        let name = self.read_name();
        self.skip_whitespace();
        let content = self.read_until("?>");
        let mut n = RawNode::new(NodeType::ProcessingInstruction, self.current_sensitivity);
        n.value = format!("<?{name} {content}?>");
        n.name = name;
        n.has_leading_whitespace = self.has_leading_whitespace;
        self.add_node(n);
    }

    fn skip_to_char(&mut self, target: char) {
        while let Some(c) = self.cur() {
            if c == target {
                break;
            }
            self.position += c.len_utf8();
        }
        if self.position < self.xml.len() {
            self.position += target.len_utf8();
        }
    }

    fn parse_end_element(&mut self) {
        self.position += 2;
        self.read_name();
        self.skip_to_char('>');
        let Some(element) = self.stack.pop() else { return };
        if element == 0 {
            self.stack.push(0);
            return;
        }
        self.tree.nodes[element].end = self.position;
        if let Some(&parent) = self.stack.last() {
            self.current_sensitivity = self.tree.get(parent).sensitivity;
            self.is_whitespace_preserved = self.tree.get(parent).is_whitespace_preserved;
        }
        // no leading or trailing newline markers in an element's children
        let mut nodes = std::mem::take(&mut self.tree.nodes[element].nodes);
        if nodes.last().map(|&n| self.tree.get(n).node_type == NodeType::Whitespace).unwrap_or(false) {
            nodes.pop();
        }
        if nodes.first().map(|&n| self.tree.get(n).node_type == NodeType::Whitespace).unwrap_or(false) {
            nodes.remove(0);
        }
        for x in 0..nodes.len() {
            if x > 0 {
                self.tree.nodes[nodes[x - 1]].next = Some(nodes[x]);
                self.tree.nodes[nodes[x]].previous = Some(nodes[x - 1]);
            }
        }
        self.tree.nodes[element].nodes = nodes;
    }

    fn parse_start_element(&mut self) -> Result<(), XmlError> {
        let mut original = self.position;
        while original > 0 && matches!(self.xml.as_bytes()[original - 1], b' ' | b'\n' | b'\r') {
            original -= 1;
        }
        self.position += 1;
        let name = self.read_name();
        let attributes = self.parse_attributes()?;

        let mut is_empty = false;
        if self.byte(self.position) == Some(b'/') {
            is_empty = true;
            self.position += 1;
        }
        self.skip_to_char('>');

        let mut sensitivity = self.current_sensitivity;
        let mut preserved = self.is_whitespace_preserved;
        if let Some(space) = attributes.iter().find(|a| a.name == "xml:space") {
            preserved = space.value == "preserve";
            sensitivity = if preserved {
                XmlWhitespaceSensitivity::Strict
            } else {
                self.default_sensitivity
            };
        }

        let mut n = RawNode::new(NodeType::Element, sensitivity);
        n.name = name;
        n.is_empty = is_empty;
        n.attributes = attributes;
        n.has_leading_whitespace = self.has_leading_whitespace;
        n.is_whitespace_preserved = preserved;
        n.start = original;
        n.end = self.position;
        let id = self.add_node(n);
        if !is_empty {
            self.stack.push(id);
            self.current_sensitivity = sensitivity;
            self.is_whitespace_preserved = preserved;
        }
        Ok(())
    }

    fn parse_text(&mut self) {
        // all whitespace was skipped in the main read, so go backwards to the real start
        while self.position > 0 && self.xml.as_bytes()[self.position - 1] != b'>' {
            self.position -= 1;
            while !self.xml.is_char_boundary(self.position) {
                self.position -= 1;
            }
        }
        let start = self.position;
        while self.position < self.xml.len() && self.xml.as_bytes()[self.position] != b'<' {
            self.position += 1;
        }
        let text = self.with_line_endings(&self.xml[start..self.position]);
        if text.is_empty() {
            return;
        }
        let mut n = RawNode::new(NodeType::Text, self.current_sensitivity);
        n.value = text;
        n.has_leading_whitespace = self.has_leading_whitespace;
        n.is_whitespace_preserved = self.is_whitespace_preserved;
        self.add_node(n);
    }

    fn parse_attributes(&mut self) -> Result<Vec<RawAttribute>, XmlError> {
        let mut attributes = Vec::new();
        while self.position < self.xml.len() {
            self.skip_whitespace();
            match self.byte(self.position) {
                None | Some(b'>') | Some(b'/') => break,
                _ => {}
            }
            let name = self.read_name();
            self.skip_whitespace();
            if self.byte(self.position) == Some(b'=') {
                self.position += 1;
                self.skip_whitespace();
                let value = self.read_quoted_value();
                attributes.push(RawAttribute {
                    name,
                    value: value.replace('"', "&quot;"),
                });
            } else {
                return Err(XmlError("Cannot find next attribute".into()));
            }
        }
        Ok(attributes)
    }

    fn read_quoted_value(&mut self) -> String {
        let Some(quote) = self.cur().filter(|c| *c == '"' || *c == '\'') else {
            return String::new();
        };
        self.position += 1;
        let start = self.position;
        while let Some(c) = self.cur() {
            if c == quote {
                break;
            }
            self.position += c.len_utf8();
        }
        let value = self.with_line_endings(&self.xml[start..self.position]);
        if self.position < self.xml.len() {
            self.position += 1;
        }
        value
    }

    fn parse_doctype(&mut self) {
        self.position += 9;
        let mut content = String::from("<!DOCTYPE");
        let mut brackets = 0;
        let mut quote: Option<char> = None;
        while let Some(ch) = self.cur() {
            match quote {
                None => {
                    if ch == '"' || ch == '\'' {
                        quote = Some(ch);
                    } else if ch == '[' {
                        brackets += 1;
                    } else if ch == ']' {
                        brackets -= 1;
                    } else if ch == '>' && brackets == 0 {
                        content.push(ch);
                        self.position += 1;
                        break;
                    }
                }
                Some(q) if ch == q => quote = None,
                _ => {}
            }
            if ch == '\n' {
                content.push_str(self.line_ending);
            } else {
                content.push(ch);
            }
            self.position += ch.len_utf8();
        }
        let mut n = RawNode::new(NodeType::DocumentType, self.current_sensitivity);
        n.value = content;
        self.add_node(n);
    }
}

// ---- printers -------------------------------------------------------------------------------

struct Printer<'a> {
    tree: &'a Tree,
    normalized: &'a str,
    line_ending: &'a str,
}

fn is_hard_line(d: &Doc) -> bool {
    matches!(d, Doc::Line(l) if l.is_hard_line)
}

impl Printer<'_> {
    fn node(&self, id: usize) -> &RawNode {
        self.tree.get(id)
    }

    fn print(&self, id: usize) -> Doc {
        let n = self.node(id);
        match n.node_type {
            NodeType::Document => {
                let mut result = Vec::new();
                for &c in &n.nodes {
                    result.push(self.print(c));
                    result.push(if self.node(c).node_type == NodeType::Whitespace {
                        Doc::Null
                    } else {
                        Doc::hard_line()
                    });
                }
                result.push(Doc::hard_line());
                Doc::concat(result)
            }
            NodeType::DocumentType => Doc::text(n.value.clone()),
            NodeType::Element => self.print_element(id),
            NodeType::Text => {
                let prefix = self.opening_tag_prefix(id);
                let text = self.text_value(id);
                let suffix = self.closing_tag_suffix(id);
                Doc::concat(vec![prefix, text, suffix])
            }
            NodeType::Comment | NodeType::ProcessingInstruction | NodeType::CData => {
                if n.parent.is_none() {
                    Doc::concat(vec![Doc::text(n.value.clone()), Doc::hard_line()])
                } else {
                    Doc::text(n.value.clone())
                }
            }
            NodeType::Whitespace => Doc::hard_line(),
        }
    }

    fn text_value(&self, id: usize) -> Doc {
        let n = self.node(id);
        let mut text = n.value.clone();
        if n.sensitivity == XmlWhitespaceSensitivity::Ignore {
            if n.previous.is_none() {
                text = text.trim_start().to_string();
            }
            if n.next.is_none() {
                text = text.trim_end().to_string();
            }
            if n.parent.map(|p| self.node(p).nodes.len() == 1).unwrap_or(false) {
                let chars: Vec<char> = text.chars().collect();
                if chars.len() > 2 {
                    let inner: String = chars[1..chars.len() - 1].iter().collect();
                    let mut collapsed = String::new();
                    let mut in_ws = false;
                    for c in inner.chars() {
                        if c.is_whitespace() {
                            if !in_ws {
                                collapsed.push(' ');
                            }
                            in_ws = true;
                        } else {
                            collapsed.push(c);
                            in_ws = false;
                        }
                    }
                    text = format!("{}{}{}", chars[0], collapsed, chars[chars.len() - 1]);
                }
            }
        }
        if text.is_empty() {
            return Doc::Null;
        }
        if n.parent.and_then(|p| self.node(p).nodes.first().copied()) == Some(id) {
            if text.starts_with('\r') {
                text.remove(0);
            }
            if text.starts_with('\n') {
                text.remove(0);
            }
        }
        if n.is_whitespace_preserved && text.contains('\n') {
            // the doc printer tracks the column by string length, so newlines have to be real
            // line docs or everything after them is measured as if it were on the same line
            let mut result = Vec::new();
            for (x, line) in text.split(self.line_ending).enumerate() {
                if x > 0 {
                    result.push(Doc::literal_line());
                }
                if !line.is_empty() {
                    result.push(Doc::text(line));
                }
            }
            return Doc::concat(result);
        }
        Doc::text(text)
    }

    // ---- element ----

    fn print_element(&self, id: usize) -> Doc {
        let n = self.node(id);
        if n.previous.map(|p| self.node(p).ignore == IgnoreType::Ignore).unwrap_or(false) {
            return Doc::text(self.normalized[n.start..n.end].replace('\n', self.line_ending));
        }
        let attr_group_id: GroupId = Doc::next_group_id();

        let element_content = if n.is_empty || n.nodes.is_empty() {
            Doc::Null
        } else {
            let children_doc = Doc::indent(vec![self.line_before_children(id), self.print_children(id)]);
            Doc::concat(vec![
                if self.force_break_content(id) { Doc::break_parent() } else { Doc::text("") },
                children_doc,
                self.line_after_children(id),
            ])
        };

        Doc::group(vec![
            Doc::group_with_id(attr_group_id, vec![self.opening_tag(id)]),
            element_content,
            self.closing_tag(id),
        ])
    }

    fn line_before_children(&self, id: usize) -> Doc {
        let n = self.node(id);
        let first = n.nodes.first().map(|&c| self.node(c));
        if n.sensitivity == XmlWhitespaceSensitivity::Strict {
            if let Some(f) = first {
                if f.node_type == NodeType::Text && (f.value.starts_with('\n') || f.value.starts_with('\r')) {
                    return Doc::literal_line();
                }
            }
        }
        if n.is_whitespace_preserved {
            return Doc::Null;
        }
        if n.attributes.is_empty()
            && n.nodes.len() == 1
            && self.node(n.nodes[0]).node_type == NodeType::Text
            && n.sensitivity == XmlWhitespaceSensitivity::Strict
        {
            return Doc::Null;
        }
        if n.nodes.len() > 1
            && n.nodes.iter().any(|&c| {
                let c = self.node(c);
                c.node_type == NodeType::Text && c.value.contains('\n')
            })
        {
            return Doc::hard_line();
        }
        Doc::soft_line()
    }

    fn line_after_children(&self, id: usize) -> Doc {
        let n = self.node(id);
        if let Some(&last) = n.nodes.last() {
            if self.parent_closing_tag_start_with_content(last) {
                return Doc::Null;
            }
        }
        if n.is_whitespace_preserved {
            return Doc::Null;
        }
        if n.attributes.is_empty()
            && n.nodes.len() == 1
            && self.node(n.nodes[0]).node_type == NodeType::Text
            && n.sensitivity == XmlWhitespaceSensitivity::Strict
        {
            return Doc::Null;
        }
        if n.sensitivity == XmlWhitespaceSensitivity::Strict {
            if let Some(&last) = n.nodes.last() {
                let l = self.node(last);
                if l.node_type == NodeType::Text {
                    let trimmed = l.value.trim_end_matches(' ');
                    if matches!(trimmed.chars().last(), Some('\r') | Some('\n')) {
                        return Doc::Null;
                    }
                }
            }
        }
        Doc::soft_line()
    }

    fn force_break_content(&self, id: usize) -> bool {
        let n = self.node(id);
        if n.nodes.len() != 1 {
            return false;
        }
        let c = self.node(n.nodes[0]);
        c.node_type == NodeType::CData || c.node_type != NodeType::Text
    }

    // ---- children ----

    fn print_children(&self, id: usize) -> Doc {
        let n = self.node(id);
        let mut result = Vec::new();
        let has_multiple = n.nodes.len() > 1;
        let mut previous_group_id: Option<GroupId> = None;
        let mut print_ignored = false;
        for &cid in &n.nodes {
            let child = self.node(cid);
            if child.ignore == IgnoreType::IgnoreEnd {
                print_ignored = false;
            }
            if print_ignored {
                result.push(Doc::text(self.normalized[child.start..child.end].replace('\n', self.line_ending)));
                continue;
            }
            if child.node_type == NodeType::Whitespace {
                let next_is_text = child.next.map(|x| self.node(x).node_type == NodeType::Text).unwrap_or(false);
                if !next_is_text {
                    result.push(Doc::hard_line());
                }
                continue;
            }

            let mut prev_part: Option<Doc> = None;
            let mut leading_part: Option<Doc> = None;
            let mut trailing_part: Option<Doc> = None;
            let mut next_part: Option<Doc> = None;

            let prev_between = match child.previous {
                Some(p) => self.between_line(p, cid),
                None => Doc::Null,
            };
            let next_between = match child.next {
                Some(nx) => self.between_line(cid, nx),
                None => Doc::Null,
            };

            if !matches!(prev_between, Doc::Null) {
                if is_hard_line(&prev_between) {
                    prev_part = Some(Doc::hard_line());
                } else if child.previous.map(|p| self.node(p).node_type == NodeType::Text).unwrap_or(false) {
                    leading_part = Some(prev_between);
                } else {
                    leading_part = Some(if has_multiple {
                        Doc::if_break(Doc::Null, Doc::soft_line(), previous_group_id)
                    } else {
                        prev_between
                    });
                }
            }
            if !matches!(next_between, Doc::Null) {
                if is_hard_line(&next_between) {
                    if child.next.map(|x| self.node(x).node_type == NodeType::Text).unwrap_or(false) {
                        next_part = Some(Doc::hard_line());
                    }
                } else {
                    trailing_part = Some(next_between);
                }
            }

            if let Some(p) = prev_part {
                result.push(p);
            }
            let gid = Doc::next_group_id();
            previous_group_id = Some(gid);
            result.push(Doc::group(vec![
                leading_part.unwrap_or(Doc::Null),
                Doc::group_with_id(gid, vec![self.print(cid), trailing_part.unwrap_or(Doc::Null)]),
            ]));
            if let Some(p) = next_part {
                result.push(p);
            }
            if child.ignore == IgnoreType::IgnoreStart {
                print_ignored = true;
            }
        }
        Doc::concat(result)
    }

    fn between_line(&self, prev: usize, next: usize) -> Doc {
        use NodeType::*;
        let p = self.node(prev);
        let n = self.node(next);
        let text_or_cdata = |t: NodeType| matches!(t, Text | CData);
        let null = !n.has_leading_whitespace
            || (p.node_type == Whitespace && n.node_type == Text)
            || (text_or_cdata(p.node_type) && text_or_cdata(n.node_type))
            || (text_or_cdata(p.node_type) && n.node_type == Comment)
            || (p.node_type == Comment && text_or_cdata(n.node_type))
            || (text_or_cdata(p.node_type) && n.node_type == Element)
            || (p.node_type == Element && text_or_cdata(n.node_type))
            || p.ignore == IgnoreType::Ignore;
        if null {
            Doc::Null
        } else {
            Doc::hard_line()
        }
    }

    // ---- tags ----

    fn opening_tag(&self, id: usize) -> Doc {
        let n = self.node(id);
        Doc::concat(vec![
            self.opening_tag_start(id),
            self.attributes(id),
            if n.is_empty { Doc::Null } else { self.opening_tag_end(id) },
        ])
    }

    fn opening_tag_start(&self, id: usize) -> Doc {
        let n = self.node(id);
        if n.previous.map(|p| self.needs_to_borrow_next_opening_tag_start_marker(p)).unwrap_or(false) {
            Doc::Null
        } else {
            Doc::concat(vec![self.opening_tag_prefix(id), Doc::text(format!("<{}", n.name))])
        }
    }

    fn opening_tag_end(&self, id: usize) -> Doc {
        let n = self.node(id);
        match n.nodes.first() {
            Some(&first) if self.needs_to_borrow_parent_opening_tag_end_marker(first) => Doc::Null,
            _ => Doc::text(">"),
        }
    }

    fn opening_tag_prefix(&self, id: usize) -> Doc {
        if self.needs_to_borrow_parent_opening_tag_end_marker(id) {
            Doc::text(">")
        } else {
            Doc::text("")
        }
    }

    fn closing_tag(&self, id: usize) -> Doc {
        let n = self.node(id);
        let last_with_content = n
            .nodes
            .last()
            .map(|&l| self.parent_closing_tag_start_with_content(l))
            .unwrap_or(false);
        Doc::concat(vec![
            if n.is_empty { Doc::Null } else { self.closing_tag_start(id) },
            if last_with_content {
                Doc::Null
            } else {
                Doc::concat(vec![self.closing_tag_end_marker(id), self.closing_tag_suffix(id)])
            },
        ])
    }

    fn closing_tag_start(&self, id: usize) -> Doc {
        let n = self.node(id);
        match n.nodes.last() {
            Some(&l) if self.parent_closing_tag_start_with_content(l) => Doc::Null,
            _ => self.closing_tag_start_marker(id),
        }
    }

    fn closing_tag_start_marker(&self, id: usize) -> Doc {
        Doc::text(format!("</{}", self.node(id).name))
    }

    fn closing_tag_end_marker(&self, id: usize) -> Doc {
        Doc::text(if self.node(id).is_empty { "/>" } else { ">" })
    }

    fn closing_tag_suffix(&self, id: usize) -> Doc {
        let n = self.node(id);
        if self.parent_closing_tag_start_with_content(id) {
            let p = n.parent.unwrap();
            Doc::concat(vec![self.closing_tag_start_marker(p), self.closing_tag_end_marker(p)])
        } else if self.needs_to_borrow_next_opening_tag_start_marker(id) {
            Doc::text(format!("<{}", self.node(n.next.unwrap()).name))
        } else {
            Doc::Null
        }
    }

    fn needs_to_borrow_next_opening_tag_start_marker(&self, id: usize) -> bool {
        let n = self.node(id);
        n.next.map(|x| !self.node(x).is_text_like()).unwrap_or(false)
            && n.is_text_like()
            && n.node_type == NodeType::Text
    }

    fn parent_closing_tag_start_with_content(&self, id: usize) -> bool {
        let n = self.node(id);
        if !(n.sensitivity == XmlWhitespaceSensitivity::Strict && n.next.is_none() && n.is_text_like()) {
            return false;
        }
        let last = self.node(self.tree.last_descendant(id));
        if last.node_type != NodeType::Text {
            return false;
        }
        let last_char = last.value.chars().last();
        !matches!(last_char, Some(' ') | Some('\r') | Some('\n'))
            || !last.value.contains('\n')
            || n.parent
                .map(|p| self.node(p).nodes.iter().any(|&c| !self.node(c).is_text_like()))
                .unwrap_or(false)
    }

    fn needs_to_borrow_parent_opening_tag_end_marker(&self, id: usize) -> bool {
        let n = self.node(id);
        n.sensitivity == XmlWhitespaceSensitivity::Strict
            && n.previous.is_none()
            && n.node_type == NodeType::Text
            && !matches!(n.value.chars().next(), Some('\r') | Some('\n'))
    }

    fn attributes(&self, id: usize) -> Doc {
        let n = self.node(id);
        if n.attributes.is_empty() {
            return if n.is_empty { Doc::text(" ") } else { Doc::Null };
        }
        let printed: Vec<Doc> = n
            .attributes
            .iter()
            .map(|a| Doc::text(format!("{}=\"{}\"", a.name, a.value)))
            .collect();
        let do_not_break = n.attributes.len() == 1
            && !n.attributes[0].value.contains('\n')
            && (n.nodes.iter().any(|&c| self.node(c).node_type == NodeType::Element) || n.is_empty);

        let mut parts = vec![Doc::indent(vec![
            if do_not_break { Doc::text(" ") } else { Doc::line() },
            Doc::join(Doc::line(), printed),
        ])];
        if n.nodes.first().map(|&f| self.needs_to_borrow_parent_opening_tag_end_marker(f)).unwrap_or(false)
            || do_not_break
        {
            parts.push(if n.is_empty { Doc::text(" ") } else { Doc::text("") });
        } else {
            parts.push(if n.is_empty { Doc::line() } else { Doc::soft_line() });
        }
        Doc::concat(parts)
    }
}

fn validate_well_formed(xml: &str) -> Result<(), XmlError> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().check_end_names = true;
    let mut depth = 0i32;
    let mut seen_root = false;
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(quick_xml::events::Event::Start(_)) => {
                depth += 1;
                seen_root = true;
            }
            Ok(quick_xml::events::Event::End(_)) => depth -= 1,
            Ok(quick_xml::events::Event::Empty(_)) => seen_root = true,
            Ok(_) => {}
            Err(e) => return Err(XmlError(e.to_string())),
        }
    }
    if depth != 0 || !seen_root {
        return Err(XmlError("document is not well formed".into()));
    }
    Ok(())
}

pub fn format_xml(
    xml: &str,
    options: &PrinterOptions,
    sensitivity: XmlWhitespaceSensitivity,
) -> Result<String, XmlError> {
    validate_well_formed(xml)?;
    let line_ending = crate::format::get_line_ending(xml, options);
    let normalized = if xml.contains('\r') {
        xml.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        xml.to_string()
    };
    let reader = Reader {
        xml: &normalized,
        line_ending,
        default_sensitivity: sensitivity,
        current_sensitivity: sensitivity,
        is_whitespace_preserved: false,
        has_leading_whitespace: false,
        position: 0,
        stack: Vec::new(),
        tree: Tree { nodes: Vec::new() },
    };
    let (tree, normalized_xml) = reader.parse()?;
    let printer = Printer {
        tree: &tree,
        normalized: &normalized_xml,
        line_ending,
    };
    let doc = printer.print(0);
    let _ = HashMap::<u8, u8>::new();
    Ok(doc::print(&doc, options, line_ending))
}
