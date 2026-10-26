//! The library API, like CSharpier.Core's CSharpFormatter and XmlFormatter

use crate::doc::{EndOfLine, PrinterOptions};
use crate::xml::XmlWhitespaceSensitivity;
use crate::FormatError;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum IndentStyle {
    #[default]
    Spaces,
    Tabs,
}

#[derive(Clone, Debug)]
pub struct CodeFormatterOptions {
    pub width: usize,
    pub indent_size: usize,
    pub indent_style: IndentStyle,
    pub end_of_line: EndOfLine,
    pub include_generated: bool,
    pub xml_whitespace_sensitivity: XmlWhitespaceSensitivity,
}

impl Default for CodeFormatterOptions {
    fn default() -> Self {
        CodeFormatterOptions {
            width: 100,
            indent_size: 4,
            indent_style: IndentStyle::Spaces,
            end_of_line: EndOfLine::Auto,
            include_generated: false,
            xml_whitespace_sensitivity: XmlWhitespaceSensitivity::Strict,
        }
    }
}

impl CodeFormatterOptions {
    fn printer_options(&self) -> PrinterOptions {
        PrinterOptions {
            width: self.width,
            indent_size: self.indent_size,
            use_tabs: self.indent_style == IndentStyle::Tabs,
            end_of_line: self.end_of_line,
            include_generated: self.include_generated,
            ..Default::default()
        }
    }
}

#[derive(Debug, Default)]
pub struct CodeFormatterResult {
    pub code: String,
    pub error_diagnostics: Vec<String>,
    pub failure_message: String,
    pub warning_message: String,
}

pub struct CSharpFormatter;

impl CSharpFormatter {
    pub fn format(code: &str, options: &CodeFormatterOptions) -> CodeFormatterResult {
        match crate::format(code, &options.printer_options()) {
            Ok(r) => CodeFormatterResult { code: r.code, ..Default::default() },
            Err(FormatError::Syntax(e)) => CodeFormatterResult {
                code: code.to_string(),
                error_diagnostics: vec![e],
                ..Default::default()
            },
            Err(FormatError::TooDeep) => CodeFormatterResult {
                code: code.to_string(),
                failure_message: "We can't handle this deep of recursion yet.".into(),
                ..Default::default()
            },
            Err(FormatError::Validation(e)) => CodeFormatterResult {
                code: code.to_string(),
                failure_message: e,
                ..Default::default()
            },
        }
    }
}

pub struct XmlFormatter;

impl XmlFormatter {
    pub fn format(xml: &str, options: &CodeFormatterOptions) -> CodeFormatterResult {
        let mut printer = options.printer_options();
        if options.indent_size == 4 {
            printer.indent_size = 2;
        }
        match crate::xml::format_xml(xml, &printer, options.xml_whitespace_sensitivity) {
            Ok(code) => CodeFormatterResult { code, ..Default::default() },
            Err(e) => CodeFormatterResult {
                code: xml.to_string(),
                error_diagnostics: vec![e.0],
                warning_message: "Appeared to be invalid xml so was not formatted.".into(),
                ..Default::default()
            },
        }
    }
}
