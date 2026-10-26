use tree_sitter::Parser;

use crate::doc::{self, PrinterOptions};
use crate::printer::{self, Ctx, State};
use crate::trivia;

#[derive(Debug)]
pub struct FormatResult {
    pub code: String,
    /// node kinds that were printed verbatim because there is no printer yet
    pub unhandled: Vec<String>,
}

#[derive(Debug)]
pub enum FormatError {
    Syntax(String),
    /// the syntax tree is nested too deeply to print
    TooDeep,
    /// the formatted code does not contain the same tokens as the original, which means a printer
    /// dropped or invented code. Mirrors CSharpier's SyntaxNodeComparer check.
    Validation(String),
}

/// The preprocessed source and the tree of its masked version, which is what gets printed.
struct Analysis {
    pre: trivia::Preprocessed,
    masked: String,
    tree: tree_sitter::Tree,
}

/// `src_tree` is the tree of `src` itself. When masking changes nothing it is reused instead of parsing again.
fn analyze(src: &str, src_tree: &tree_sitter::Tree, symbols: &[String]) -> Analysis {
    let protected = trivia::protected_ranges(src_tree.root_node());
    let pre = trivia::preprocess(src, &protected, symbols);
    let masked = pre.mask(src);
    let tree = if masked == src { src_tree.clone() } else { parse(&masked) };
    Analysis { pre, masked, tree }
}

fn parse(src: &str) -> tree_sitter::Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .expect("loading C# grammar");
    parser.parse(src, None).expect("parse failed")
}

fn first_error(src: &str, node: tree_sitter::Node) -> Option<String> {
    if node.is_error() || node.is_missing() {
        let p = node.start_position();
        return Some(format!(
            "{} at {}:{}: {:?}",
            if node.is_missing() { "missing " } else { "error" },
            p.row + 1,
            p.column + 1,
            node.kind()
        ));
    }
    if !node.has_error() {
        return None;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(e) = first_error(src, child) {
            return Some(e);
        }
    }
    None
}

/// Port of PrinterOptions.GetLineEnding
pub fn get_line_ending(code: &str, options: &PrinterOptions) -> &'static str {
    match options.end_of_line {
        doc::EndOfLine::Lf => "\n",
        doc::EndOfLine::Crlf => "\r\n",
        doc::EndOfLine::Auto => line_ending(code),
    }
}

pub fn line_ending(code: &str) -> &'static str {
    match code.find('\n') {
        Some(i) if i > 0 && code.as_bytes()[i - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

/// All token texts of `src` as parsed with `symbols`, sorted so that reordering usings and
/// modifiers is not reported as a difference.
fn sorted_token_texts(src: &str, a: &Analysis) -> Result<Vec<String>, FormatError> {
    if let Some(e) = first_error(&a.masked, a.tree.root_node()) {
        return Err(FormatError::Syntax(e));
    }
    let tokens = trivia::build_tokens(src, a.tree.root_node(), &a.pre);
    let mut texts: Vec<String> = tokens
        .list
        .iter()
        .filter(|t| t.end > t.start)
        .map(|t| {
            let text = &src[t.start..t.end];
            if text.contains('\n') {
                // multi line strings are re-indented, so only compare their content
                text.replace('\r', "").split('\n').map(|l| l.trim()).collect::<Vec<_>>().join("\n")
            } else {
                text.to_string()
            }
        })
        // trailing commas are added and removed by the formatter
        .filter(|t| t != ",")
        .collect();
    texts.sort();
    Ok(texts)
}

fn validate(original: &str, before: &Analysis, formatted: &str, symbols: &[String]) -> Result<(), FormatError> {
    let before = sorted_token_texts(original, before)?;
    let formatted_tree = parse(formatted);
    let after = match sorted_token_texts(formatted, &analyze(formatted, &formatted_tree, symbols)) {
        Ok(a) => a,
        Err(FormatError::Syntax(e)) => {
            return Err(FormatError::Validation(format!("formatted code does not parse: {e}")))
        }
        Err(e) => return Err(e),
    };
    if before == after {
        return Ok(());
    }
    // report what is missing or extra
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < before.len() || j < after.len() {
        if j >= after.len() || (i < before.len() && before[i] < after[j]) {
            missing.push(before[i].clone());
            i += 1;
        } else if i >= before.len() || after[j] < before[i] {
            extra.push(after[j].clone());
            j += 1;
        } else {
            i += 1;
            j += 1;
        }
    }
    missing.truncate(8);
    extra.truncate(8);
    Err(FormatError::Validation(format!(
        "tokens changed. missing: {missing:?} extra: {extra:?}"
    )))
}

/// Formats with one set of preprocessor symbols.
pub fn format_with_symbols(
    src: &str,
    options: &PrinterOptions,
    symbols: &[String],
) -> Result<FormatResult, FormatError> {
    let src_tree = parse(src);
    let analysis = analyze(src, &src_tree, symbols);
    format_with_symbols_validated(src, options, symbols, true, analysis)
}

fn format_with_symbols_validated(
    src: &str,
    options: &PrinterOptions,
    symbols: &[String],
    validate_output: bool,
    analysis: Analysis,
) -> Result<FormatResult, FormatError> {
    let root = analysis.tree.root_node();
    if let Some(e) = first_error(&analysis.masked, root) {
        return Err(FormatError::Syntax(e));
    }

    let tokens = trivia::build_tokens(src, root, &analysis.pre);
    let mut ctx = Ctx {
        src,
        tokens: &tokens,
        state: State::default(),
        unhandled: Vec::new(),
        line_ending: get_line_ending(src, options),
    };
    let printed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let doc = printer::print_root(&mut ctx, root);
        (doc, std::mem::take(&mut ctx.unhandled))
    }));
    let (doc, unhandled) = match printed {
        Ok(v) => v,
        Err(payload) if payload.is::<printer::InTooDeep>() => return Err(FormatError::TooDeep),
        Err(payload) => std::panic::resume_unwind(payload),
    };
    if std::env::var_os("CSHARPIER_DUMP_DOC").is_some() {
        eprintln!("{}", doc.dump());
    }
    let code = doc::print(&doc, options, get_line_ending(src, options));
    if validate_output && std::env::var_os("CSHARPIER_NO_VALIDATE").is_none() {
        validate(src, &analysis, &code, symbols)?;
    }
    Ok(FormatResult {
        code,
        unhandled,
    })
}

/// Formats the code, then formats the result again for every set of preprocessor symbols needed
/// to activate all `#if` branches, so code in every branch ends up formatted.
pub fn format(src: &str, options: &PrinterOptions) -> Result<FormatResult, FormatError> {
    format_with(src, options, true)
}

pub fn format_with(src: &str, options: &PrinterOptions, validate_output: bool) -> Result<FormatResult, FormatError> {
    let src_tree = parse(src);
    let analysis = analyze(src, &src_tree, &[]);
    if !options.include_generated && begins_with_auto_generated_comment(src, &analysis) {
        return Ok(FormatResult {
            code: src.to_string(),
            unhandled: vec![],
        });
    }
    let sets = crate::symbols::get_sets(&analysis.pre.events);
    let mut result = format_with_symbols_validated(src, options, &[], validate_output, analysis)?;

    for set in sets {
        let tree = parse(&result.code);
        let analysis = analyze(&result.code, &tree, &set);
        let next = format_with_symbols_validated(&result.code, options, &set, validate_output, analysis)?;
        result.code = next.code;
        result.unhandled.extend(next.unhandled);
    }
    Ok(result)
}

/// Port of GeneratedCodeUtilities.BeginsWithAutoGeneratedComment
fn begins_with_auto_generated_comment(src: &str, a: &Analysis) -> bool {
    let tokens = trivia::build_tokens(src, a.tree.root_node(), &a.pre);
    tokens.list[0].leading.iter().any(|t| {
        matches!(
            t.kind,
            trivia::TriviaKind::SingleLineComment | trivia::TriviaKind::MultiLineComment
        ) && (t.text.contains("<autogenerated") || t.text.contains("<auto-generated"))
    })
}

/// The sets of preprocessor symbols needed to make every `#if` branch active (PreprocessorSymbols.GetSets)
pub fn preprocessor_symbol_sets(src: &str) -> Vec<Vec<String>> {
    let tree = parse(src);
    let protected = trivia::protected_ranges(tree.root_node());
    let pre = trivia::preprocess(src, &protected, &[]);
    crate::symbols::get_sets(&pre.events)
}
