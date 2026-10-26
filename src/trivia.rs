//! Roslyn-style trivia on top of a tree-sitter parse.
//!
//! tree-sitter gives us a tree of tokens with comments as "extras" and directives as structural
//! nodes. CSharpier's printers are written against Roslyn's model where every token owns leading
//! and trailing trivia, and where inactive `#if` branches are plain text. This module rebuilds
//! that model:
//!
//! 1. `preprocess` finds directive lines and inactive (disabled) text for a symbol set.
//! 2. Those ranges are blanked out so tree-sitter only parses the active code.
//! 3. `build_tokens` walks the leaves of the tree and lexes the gaps between them (in the
//!    original text) into trivia, splitting into trailing/leading the way Roslyn does.

use std::collections::HashMap;
use tree_sitter::Node;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DirectiveKind {
    If,
    Elif,
    Else,
    EndIf,
    Region,
    EndRegion,
    PragmaWarning,
    PragmaChecksum,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TriviaKind {
    Whitespace,
    EndOfLine,
    SingleLineComment,
    SingleLineDocComment,
    MultiLineComment,
    MultiLineDocComment,
    Directive(DirectiveKind),
    DisabledText,
}

#[derive(Clone, Debug)]
pub struct Trivia {
    pub kind: TriviaKind,
    /// the full text. Like Roslyn, single line doc comments, directives and disabled text
    /// include their trailing end of line.
    pub text: String,
}

impl Trivia {
    pub fn is_comment(&self) -> bool {
        matches!(
            self.kind,
            TriviaKind::SingleLineComment
                | TriviaKind::SingleLineDocComment
                | TriviaKind::MultiLineComment
                | TriviaKind::MultiLineDocComment
        )
    }

    pub fn is_directive(&self) -> bool {
        matches!(self.kind, TriviaKind::Directive(_))
    }

    pub fn is_whitespace_or_eol(&self) -> bool {
        matches!(self.kind, TriviaKind::Whitespace | TriviaKind::EndOfLine)
    }
}

pub trait TriviaListExt {
    fn any_directive(&self) -> bool;
    fn any_comment(&self) -> bool;
    fn any_comment_or_directive(&self) -> bool;
    fn index_of_next_end_of_line(&self, start: usize) -> Option<usize>;
}

impl TriviaListExt for [Trivia] {
    fn any_directive(&self) -> bool {
        self.iter().any(|t| t.is_directive())
    }
    fn any_comment(&self) -> bool {
        self.iter().any(|t| t.is_comment())
    }
    fn any_comment_or_directive(&self) -> bool {
        self.iter().any(|t| t.is_comment() || t.is_directive())
    }
    fn index_of_next_end_of_line(&self, start: usize) -> Option<usize> {
        self.iter()
            .enumerate()
            .skip(start + 1)
            .find(|(_, t)| t.kind == TriviaKind::EndOfLine)
            .map(|(i, _)| i)
    }
}

#[derive(Debug)]
struct Directive {
    start: usize,
    /// end of the directive text, before the end of line
    text_end: usize,
    /// end including the end of line
    end: usize,
    kind: DirectiveKind,
}

#[derive(Default, Debug)]
pub struct Preprocessed {
    directives: Vec<Directive>,
    /// (start, end) of text in inactive branches
    disabled: Vec<(usize, usize)>,
    /// #if/#elif/#else/#endif in order, for discovering symbol sets
    pub events: Vec<crate::symbols::Event>,
}

/// Byte ranges that are inside strings or comments, where a leading `#` is not a directive.
pub fn protected_ranges(root: Node) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut cursor = root.walk();
    let mut descend = true;
    loop {
        let node = cursor.node();
        let kind = node.kind();
        let protected = kind == "comment"
            || kind.ends_with("string_literal")
            || kind == "interpolated_string_expression"
            || kind == "character_literal";
        if protected {
            ranges.push((node.start_byte(), node.end_byte()));
            descend = false;
        }
        if descend && cursor.goto_first_child() {
            continue;
        }
        descend = true;
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return ranges;
            }
        }
    }
}

fn directive_kind(line: &str) -> DirectiveKind {
    let rest = line.trim_start_matches('#').trim_start();
    let word: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    match word.as_str() {
        "if" => DirectiveKind::If,
        "elif" => DirectiveKind::Elif,
        "else" => DirectiveKind::Else,
        "endif" => DirectiveKind::EndIf,
        "region" => DirectiveKind::Region,
        "endregion" => DirectiveKind::EndRegion,
        "pragma" => {
            let after = rest["pragma".len()..].trim_start();
            if after.starts_with("checksum") {
                DirectiveKind::PragmaChecksum
            } else {
                DirectiveKind::PragmaWarning
            }
        }
        _ => DirectiveKind::Other,
    }
}

/// Strips a trailing `// comment` from a directive's condition.
fn condition_of(line: &str, keyword: &str) -> String {
    let rest = line.trim_start_matches('#').trim_start()[keyword.len()..].trim();
    match rest.find("//") {
        Some(i) => rest[..i].trim().to_string(),
        None => rest.to_string(),
    }
}

pub fn preprocess(src: &str, protected: &[(usize, usize)], symbols: &[String]) -> Preprocessed {
    struct Frame {
        parent_active: bool,
        taken: bool,
        active: bool,
    }

    let mut result = Preprocessed::default();
    let mut defined: Vec<String> = symbols.to_vec();
    let mut frames: Vec<Frame> = Vec::new();
    let mut disabled_start: Option<usize> = None;

    let mut offset = 0;
    while offset < src.len() {
        let line_start = offset;
        let line_end_incl = match src[offset..].find('\n') {
            Some(i) => offset + i + 1,
            None => src.len(),
        };
        offset = line_end_incl;
        let line_full = &src[line_start..line_end_incl];
        let line = line_full.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            continue;
        }
        let hash_offset = line_start + (line.len() - trimmed.len());
        if protected.iter().any(|&(s, e)| s < hash_offset && hash_offset < e) {
            continue;
        }

        if let Some(ds) = disabled_start.take() {
            if ds < line_start {
                result.disabled.push((ds, line_start));
            }
        }

        let kind = directive_kind(trimmed);
        let active_now = |frames: &Vec<Frame>| frames.last().map(|f| f.active).unwrap_or(true);
        match kind {
            DirectiveKind::If => {
                let cond = condition_of(trimmed, "if");
                result.events.push(crate::symbols::Event::If(cond.clone()));
                let parent_active = active_now(&frames);
                let active = parent_active && eval_condition(&cond, &defined);
                frames.push(Frame {
                    parent_active,
                    taken: active || !parent_active,
                    active,
                });
            }
            DirectiveKind::Elif => {
                let cond = condition_of(trimmed, "elif");
                result.events.push(crate::symbols::Event::Elif(cond.clone()));
                if let Some(f) = frames.last_mut() {
                    if f.parent_active && !f.taken {
                        let c = eval_condition(&cond, &defined);
                        f.active = c;
                        f.taken |= c;
                    } else {
                        f.active = false;
                    }
                }
            }
            DirectiveKind::Else => {
                result.events.push(crate::symbols::Event::Else);
                if let Some(f) = frames.last_mut() {
                    f.active = f.parent_active && !f.taken;
                    f.taken = true;
                }
            }
            DirectiveKind::EndIf => {
                result.events.push(crate::symbols::Event::EndIf);
                frames.pop();
            }
            DirectiveKind::Other => {
                if active_now(&frames) {
                    let rest = trimmed.trim_start_matches('#').trim_start();
                    if let Some(sym) = rest.strip_prefix("define") {
                        if let Some(s) = sym.split_whitespace().next() {
                            defined.push(s.to_string());
                        }
                    } else if let Some(sym) = rest.strip_prefix("undef") {
                        if let Some(s) = sym.split_whitespace().next() {
                            defined.retain(|d| d != s);
                        }
                    }
                }
            }
            _ => {}
        }

        result.directives.push(Directive {
            start: hash_offset,
            text_end: line_start + line.trim_end().len(),
            end: line_end_incl,
            kind,
        });
        if !frames.last().map(|f| f.active).unwrap_or(true) {
            disabled_start = Some(line_end_incl);
        }
    }
    if let Some(ds) = disabled_start {
        if ds < src.len() {
            result.disabled.push((ds, src.len()));
        }
    }
    result
}

impl Preprocessed {
    /// The source with directives and disabled text blanked out, keeping all offsets intact.
    pub fn mask(&self, src: &str) -> String {
        let mut bytes = src.as_bytes().to_vec();
        let mut blank = |s: usize, e: usize| {
            for b in &mut bytes[s..e] {
                if *b != b'\n' && *b != b'\r' {
                    *b = b' ';
                }
            }
        };
        for d in &self.directives {
            blank(d.start, d.end);
        }
        for &(s, e) in &self.disabled {
            blank(s, e);
        }
        String::from_utf8(bytes).expect("masking only replaces whole lines")
    }
}

// ---- condition evaluation -------------------------------------------------------------------

fn eval_condition(cond: &str, symbols: &[String]) -> bool {
    struct P<'a> {
        chars: Vec<char>,
        pos: usize,
        symbols: &'a [String],
    }
    impl P<'_> {
        fn ws(&mut self) {
            while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
                self.pos += 1;
            }
        }
        fn eat(&mut self, s: &str) -> bool {
            self.ws();
            let n = s.chars().count();
            if self.chars[self.pos..].iter().take(n).collect::<String>() == s {
                self.pos += n;
                true
            } else {
                false
            }
        }
        fn or(&mut self) -> bool {
            let mut v = self.and();
            while self.eat("||") {
                let r = self.and();
                v = v || r;
            }
            v
        }
        fn and(&mut self) -> bool {
            let mut v = self.eq();
            while self.eat("&&") {
                let r = self.eq();
                v = v && r;
            }
            v
        }
        fn eq(&mut self) -> bool {
            let mut v = self.unary();
            loop {
                if self.eat("==") {
                    let r = self.unary();
                    v = v == r;
                } else if self.eat("!=") {
                    let r = self.unary();
                    v = v != r;
                } else {
                    return v;
                }
            }
        }
        fn unary(&mut self) -> bool {
            self.ws();
            if self.pos < self.chars.len() && self.chars[self.pos] == '!' && !self.eat("!=") {
                self.pos += 1;
                return !self.unary();
            }
            if self.eat("(") {
                let v = self.or();
                self.eat(")");
                return v;
            }
            let start = self.pos;
            while self.pos < self.chars.len()
                && (self.chars[self.pos].is_alphanumeric() || self.chars[self.pos] == '_')
            {
                self.pos += 1;
            }
            let word: String = self.chars[start..self.pos].iter().collect();
            match word.as_str() {
                "true" => true,
                "false" => false,
                "" => {
                    self.pos += 1;
                    false
                }
                w => self.symbols.iter().any(|s| s == w),
            }
        }
    }
    let mut p = P {
        chars: cond.chars().collect(),
        pos: 0,
        symbols,
    };
    p.or()
}

// ---- tokens ---------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub leading: Vec<Trivia>,
    pub trailing: Vec<Trivia>,
}

#[derive(Debug, Default)]
pub struct Tokens {
    pub list: Vec<Token>,
    by_start: HashMap<usize, usize>,
}

impl Tokens {
    pub fn index_at(&self, start: usize) -> Option<usize> {
        self.by_start.get(&start).copied()
    }

    /// index of the first token starting at or after `byte`
    pub fn first_at_or_after(&self, byte: usize) -> usize {
        self.list.partition_point(|t| t.start < byte)
    }

    /// index of the last token ending at or before `byte`
    pub fn last_ending_at_or_before(&self, byte: usize) -> Option<usize> {
        self.list.partition_point(|t| t.end <= byte).checked_sub(1)
    }

    pub fn eof(&self) -> &Token {
        self.list.last().expect("tokens always has an EOF token")
    }
}

/// Node kinds that are printed as a single token even though tree-sitter splits them up.
fn is_atomic(kind: &str) -> bool {
    matches!(
        kind,
        "string_literal"
            | "verbatim_string_literal"
            | "raw_string_literal"
            | "character_literal"
            | "interpolated_string_expression"
    )
}

pub fn build_tokens(src: &str, root: Node, pre: &Preprocessed) -> Tokens {
    // leaf ranges
    let mut leaves: Vec<(usize, usize)> = Vec::new();
    let mut cursor = root.walk();
    let mut descend = true;
    'walk: loop {
        let node = cursor.node();
        if node.kind() != "comment" {
            if is_atomic(node.kind()) || node.child_count() == 0 {
                if node.end_byte() > node.start_byte() {
                    leaves.push((node.start_byte(), node.end_byte()));
                }
                descend = false;
            }
        } else {
            descend = false;
        }
        if descend && cursor.goto_first_child() {
            continue;
        }
        descend = true;
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                break 'walk;
            }
        }
    }
    leaves.push((src.len(), src.len())); // EOF

    let directive_at: HashMap<usize, &Directive> =
        pre.directives.iter().map(|d| (d.start, d)).collect();
    let disabled_at: HashMap<usize, usize> = pre.disabled.iter().copied().collect();

    let mut tokens = Tokens::default();
    let mut prev_end = 0;
    // trailing trivia of the previous token, which is lexed from the same gap as the leading
    // trivia of the next token
    for (i, &(start, end)) in leaves.iter().enumerate() {
        let gap = lex_gap(src, prev_end, start, i == 0, &directive_at, &disabled_at);
        let (trailing, leading) = split_trailing(gap);
        if let Some(prev) = tokens.list.last_mut() {
            prev.trailing = trailing;
        } else {
            // before the first token there is no trailing trivia
            let mut all = trailing;
            all.extend(leading.clone());
            let leading = all;
            tokens.by_start.insert(start, 0);
            tokens.list.push(Token {
                start,
                end,
                leading,
                trailing: Vec::new(),
            });
            prev_end = end;
            continue;
        }
        tokens.by_start.insert(start, tokens.list.len());
        tokens.list.push(Token {
            start,
            end,
            leading,
            trailing: Vec::new(),
        });
        prev_end = end;
    }
    tokens
}

/// Roslyn: trailing trivia runs up to and including the first end of line.
fn split_trailing(gap: Vec<Trivia>) -> (Vec<Trivia>, Vec<Trivia>) {
    let mut trailing = Vec::new();
    let mut iter = gap.into_iter();
    for t in iter.by_ref() {
        let eol = t.kind == TriviaKind::EndOfLine;
        let ends_line = eol
            || matches!(
                t.kind,
                TriviaKind::SingleLineDocComment | TriviaKind::Directive(_) | TriviaKind::DisabledText
            );
        if ends_line && !eol {
            // these always start on a fresh line, so they belong to the next token
            let mut rest = vec![t];
            rest.extend(iter);
            return (trailing, rest);
        }
        trailing.push(t);
        if eol {
            break;
        }
    }
    (trailing, iter.collect())
}

fn lex_gap(
    src: &str,
    start: usize,
    end: usize,
    file_start: bool,
    directive_at: &HashMap<usize, &Directive>,
    disabled_at: &HashMap<usize, usize>,
) -> Vec<Trivia> {
    let mut out: Vec<Trivia> = Vec::new();
    let bytes = src.as_bytes();
    let mut p = start;
    let mut at_line_start = file_start;
    while p < end {
        if at_line_start {
            if let Some(&dend) = disabled_at.get(&p) {
                out.push(Trivia {
                    kind: TriviaKind::DisabledText,
                    text: src[p..dend].to_string(),
                });
                p = dend;
                continue;
            }
        }
        if let Some(d) = directive_at.get(&p) {
            out.push(Trivia {
                kind: TriviaKind::Directive(d.kind),
                text: src[d.start..d.text_end].to_string() + &src[d.text_end..d.end],
            });
            p = d.end;
            at_line_start = true;
            continue;
        }
        let c = bytes[p];
        match c {
            b'\r' | b'\n' => {
                let len = if c == b'\r' && bytes.get(p + 1) == Some(&b'\n') { 2 } else { 1 };
                out.push(Trivia {
                    kind: TriviaKind::EndOfLine,
                    text: src[p..p + len].to_string(),
                });
                p += len;
                at_line_start = true;
            }
            b' ' | b'\t' | 0x0b | 0x0c => {
                let s = p;
                while p < end && matches!(bytes[p], b' ' | b'\t' | 0x0b | 0x0c) {
                    p += 1;
                }
                out.push(Trivia {
                    kind: TriviaKind::Whitespace,
                    text: src[s..p].to_string(),
                });
            }
            b'/' if bytes.get(p + 1) == Some(&b'/') => {
                let is_doc = at_line_start
                    && bytes.get(p + 2) == Some(&b'/')
                    && bytes.get(p + 3) != Some(&b'/');
                if is_doc {
                    // consecutive /// lines are a single trivia that includes the end of lines
                    let s = p;
                    loop {
                        // consume to end of line, including the line ending
                        while p < end && bytes[p] != b'\n' && bytes[p] != b'\r' {
                            p += 1;
                        }
                        if p < end {
                            if bytes[p] == b'\r' && bytes.get(p + 1) == Some(&b'\n') {
                                p += 1;
                            }
                            p += 1;
                        }
                        // does another /// line follow?
                        let mut q = p;
                        while q < end && matches!(bytes[q], b' ' | b'\t') {
                            q += 1;
                        }
                        if q + 3 <= end
                            && bytes[q..].starts_with(b"///")
                            && bytes.get(q + 3) != Some(&b'/')
                        {
                            p = q;
                            continue;
                        }
                        break;
                    }
                    out.push(Trivia {
                        kind: TriviaKind::SingleLineDocComment,
                        text: src[s..p].to_string(),
                    });
                    at_line_start = true;
                } else {
                    let s = p;
                    while p < end && bytes[p] != b'\n' && bytes[p] != b'\r' {
                        p += 1;
                    }
                    out.push(Trivia {
                        kind: TriviaKind::SingleLineComment,
                        text: src[s..p].to_string(),
                    });
                    at_line_start = false;
                }
            }
            b'/' if bytes.get(p + 1) == Some(&b'*') => {
                let s = p;
                let close = src[p + 2..].find("*/").map(|i| p + 2 + i + 2).unwrap_or(end);
                p = close.min(end);
                let text = &src[s..p];
                let is_doc = text.starts_with("/**") && text != "/**/";
                out.push(Trivia {
                    kind: if is_doc {
                        TriviaKind::MultiLineDocComment
                    } else {
                        TriviaKind::MultiLineComment
                    },
                    text: text.to_string(),
                });
                at_line_start = false;
            }
            _ => {
                // BOM or an unexpected character; skip it so we make progress
                let ch = src[p..].chars().next().unwrap();
                p += ch.len_utf8();
            }
        }
    }
    out
}
