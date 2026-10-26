//! File type detection and the options that apply to one file.

use crate::doc::{EndOfLine, PrinterOptions};
use crate::xml::XmlWhitespaceSensitivity;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Formatter {
    Unknown,
    CSharp,
    CSharpScript,
    Xml,
}

impl Formatter {
    pub fn from_path(path: &str) -> Formatter {
        let Some(ext) = std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()) else {
            return Formatter::Unknown;
        };
        match ext.as_str() {
            "cs" => Formatter::CSharp,
            "csx" => Formatter::CSharpScript,
            "config" | "csproj" | "props" | "slnx" | "targets" | "axaml" | "xaml" | "xml" => Formatter::Xml,
            _ => Formatter::Unknown,
        }
    }

    pub fn parse(name: &str) -> Option<Formatter> {
        match name.to_lowercase().as_str() {
            "unknown" => Some(Formatter::Unknown),
            "csharp" => Some(Formatter::CSharp),
            "csharpscript" => Some(Formatter::CSharpScript),
            "xml" => Some(Formatter::Xml),
            _ => None,
        }
    }
}

pub fn xml_whitespace_sensitivity_for_path(path: &str) -> XmlWhitespaceSensitivity {
    match std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()) {
        Some(e) if e == "xaml" || e == "axaml" => XmlWhitespaceSensitivity::Ignore,
        _ => XmlWhitespaceSensitivity::Strict,
    }
}

#[derive(Clone, Debug)]
pub struct FileOptions {
    pub formatter: Formatter,
    pub printer: PrinterOptions,
    pub xml_whitespace_sensitivity: XmlWhitespaceSensitivity,
}

impl FileOptions {
    pub fn new(formatter: Formatter, xml_whitespace_sensitivity: XmlWhitespaceSensitivity) -> FileOptions {
        let mut printer = PrinterOptions::default();
        if formatter == Formatter::Xml {
            printer.indent_size = 2;
        }
        FileOptions {
            formatter,
            printer,
            xml_whitespace_sensitivity,
        }
    }

    pub fn default_for_path(path: &str) -> Option<FileOptions> {
        let formatter = Formatter::from_path(path);
        (formatter != Formatter::Unknown)
            .then(|| FileOptions::new(formatter, xml_whitespace_sensitivity_for_path(path)))
    }

    pub fn set_end_of_line(&mut self, e: EndOfLine) {
        self.printer.end_of_line = e;
    }
}

pub enum FormatOutcome {
    Formatted(String),
    /// the file could not be parsed, nothing was changed
    SyntaxError(String),
    Warning(String),
    Failure(String),
}

/// Formats the contents of a file of the given type
pub fn format_contents(contents: &str, options: &FileOptions, include_generated: bool, validate: bool) -> FormatOutcome {
    let mut printer = options.printer.clone();
    printer.include_generated = include_generated;
    match options.formatter {
        Formatter::CSharp | Formatter::CSharpScript => {
            let r = std::panic::catch_unwind(|| crate::format::format_with(contents, &printer, validate));
            match r {
                Ok(Ok(result)) => FormatOutcome::Formatted(result.code),
                Ok(Err(crate::FormatError::Syntax(e))) => FormatOutcome::SyntaxError(e),
                Ok(Err(crate::FormatError::TooDeep)) => FormatOutcome::Failure("We can't handle this deep of recursion yet.".into()),
                Ok(Err(crate::FormatError::Validation(e))) => FormatOutcome::Failure(format!("Failed formatting validation.\n{e}")),
                Err(_) => FormatOutcome::Failure("Threw exception while formatting.".into()),
            }
        }
        Formatter::Xml => match crate::xml::format_xml(contents, &printer, options.xml_whitespace_sensitivity) {
            Ok(code) => FormatOutcome::Formatted(code),
            Err(_) => FormatOutcome::Warning("Appeared to be invalid xml so was not formatted.".into()),
        },
        Formatter::Unknown => FormatOutcome::Warning("Is an unsupported file type.".into()),
    }
}
