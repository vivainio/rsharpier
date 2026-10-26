//! Scenarios from OptionsProviderTests: which options apply to a file

use std::path::Path;

use csharpier_rs::cli::config::OptionsProvider;
use csharpier_rs::doc::EndOfLine;
use csharpier_rs::options::{FileOptions, Formatter};
use csharpier_rs::xml::XmlWhitespaceSensitivity;

struct Dir(tempfile::TempDir);

impl Dir {
    fn new() -> Dir {
        Dir(tempfile::tempdir().unwrap())
    }
    fn file(&self, path: &str, contents: &str) {
        let p = self.0.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, contents).unwrap();
    }
    fn options(&self, file: &str) -> Option<FileOptions> {
        OptionsProvider::new(None).unwrap().options_for(&self.0.path().join(file))
    }
}

#[test]
fn default_options_without_config_for_known_extensions() {
    let d = Dir::new();
    for ext in ["cs", "csx"] {
        let o = d.options(&format!("test.{ext}")).unwrap();
        assert_eq!((o.printer.width, o.printer.indent_size, o.printer.use_tabs), (100, 4, false));
        assert_eq!(o.printer.end_of_line, EndOfLine::Auto);
    }
    for ext in ["config", "csproj", "props", "slnx", "targets", "xaml", "axaml", "xml"] {
        let o = d.options(&format!("test.{ext}")).unwrap();
        assert_eq!(o.formatter, Formatter::Xml);
        assert_eq!(o.printer.indent_size, 2);
        let expected = if ext == "xaml" || ext == "axaml" {
            XmlWhitespaceSensitivity::Ignore
        } else {
            XmlWhitespaceSensitivity::Strict
        };
        assert_eq!(o.xml_whitespace_sensitivity, expected, "{ext}");
    }
}

#[test]
fn no_options_for_unknown_extension() {
    assert!(Dir::new().options("test.bad").is_none());
}

#[test]
fn empty_config_files_give_defaults() {
    for name in [".csharpierrc", ".csharpierrc.json", ".csharpierrc.yaml"] {
        let d = Dir::new();
        d.file(name, "");
        assert_eq!(d.options("test.cs").unwrap().printer.width, 100);
    }
}

#[test]
fn json_and_yaml_options() {
    let d = Dir::new();
    d.file(".csharpierrc.json", r#"{ "printWidth": 10, "indentSize": 3, "useTabs": true }"#);
    let o = d.options("test.cs").unwrap();
    assert_eq!((o.printer.width, o.printer.indent_size, o.printer.use_tabs), (10, 3, true));

    for ext in ["yaml", "yml"] {
        let d = Dir::new();
        d.file(&format!(".csharpierrc.{ext}"), "printWidth: 20\nindentSize: 2\nuseTabs: true\nendOfLine: crlf\n");
        let o = d.options("test.cs").unwrap();
        assert_eq!((o.printer.width, o.printer.indent_size, o.printer.use_tabs), (20, 2, true));
        assert_eq!(o.printer.end_of_line, EndOfLine::Crlf);
    }
}

#[test]
fn extensionless_file_can_be_json_or_yaml() {
    for contents in ["{ \"printWidth\": 10 }", "printWidth: 10"] {
        let d = Dir::new();
        d.file(".csharpierrc", contents);
        assert_eq!(d.options("test.cs").unwrap().printer.width, 10);
    }
}

#[test]
fn configuration_is_found_in_parent_directories() {
    for (ext, contents) in [("", "printWidth: 10"), ("", "{ \"printWidth\": 10 }"), (".yml", "printWidth: 10"), (".yaml", "printWidth: 10"), (".json", "{ \"printWidth\": 10 }")] {
        let d = Dir::new();
        d.file(&format!(".csharpierrc{ext}"), contents);
        d.file("sub/deeper/placeholder.txt", "");
        assert_eq!(d.options("sub/deeper/test.cs").unwrap().printer.width, 10);
    }
}

#[test]
fn prefers_no_extension() {
    let d = Dir::new();
    d.file(".csharpierrc", "printWidth: 10");
    d.file(".csharpierrc.json", "{ \"printWidth\": 20 }");
    assert_eq!(d.options("test.cs").unwrap().printer.width, 10);
}

#[test]
fn closer_csharpierrc_wins() {
    let d = Dir::new();
    d.file(".csharpierrc", "indentSize: 1");
    d.file("sub/.csharpierrc", "indentSize: 3");
    assert_eq!(d.options("sub/test.cs").unwrap().printer.indent_size, 3);
}

#[test]
fn overrides() {
    let d = Dir::new();
    d.file(
        ".csharpierrc.yaml",
        "overrides:\n  - files: \"*.csx\"\n    indentSize: 3\n  - files: \"*.xml\"\n    formatter: xml\n    indentSize: 6\n",
    );
    assert_eq!(d.options("test.csx").map(|o| o.printer.indent_size), None, "an override without a formatter is not valid");
    assert_eq!(d.options("test.xml").unwrap().printer.indent_size, 6);
    assert_eq!(d.options("test.cs").unwrap().printer.indent_size, 4);
}

#[test]
fn override_without_indent_size_uses_the_formatter_default() {
    for (formatter, expected) in [("xml", 2), ("csharp", 4)] {
        let d = Dir::new();
        d.file(".csharpierrc.yaml", &format!("overrides:\n  - files: \"*.foo\"\n    formatter: {formatter}\n"));
        assert_eq!(d.options("test.foo").unwrap().printer.indent_size, expected);
    }
}

// ---- editorconfig ----

#[test]
fn editorconfig_basic() {
    let d = Dir::new();
    d.file(
        ".editorconfig",
        "[*.cs]\nindent_style = tab\nindent_size = 3\nmax_line_length = 120\nend_of_line = lf\n",
    );
    let o = d.options("test.cs").unwrap();
    assert!(o.printer.use_tabs);
    assert_eq!(o.printer.width, 120);
    assert_eq!(o.printer.end_of_line, EndOfLine::Lf);
}

#[test]
fn editorconfig_comments_duplicates_and_garbage() {
    let d = Dir::new();
    d.file(".editorconfig", "# comment\n; also a comment\n[*]\nindent_size = 2\nINVALID\n[*]\nindent_size = 3\nindent_size = 5\n");
    assert_eq!(d.options("test.cs").unwrap().printer.indent_size, 5);

    let d = Dir::new();
    d.file(".editorconfig", "\n[*\nindent_size==\n");
    assert_eq!(d.options("test.cs").unwrap().printer.indent_size, 4);
}

#[test]
fn editorconfig_tabs_use_tab_width() {
    for prop in ["tab_width", "indent_size"] {
        let d = Dir::new();
        d.file(".editorconfig", &format!("[*]\nindent_style = tab\n{prop} = 3\n"));
        let o = d.options("test.cs").unwrap();
        assert!(o.printer.use_tabs);
        assert_eq!(o.printer.indent_size, 3);
    }
    let d = Dir::new();
    d.file(".editorconfig", "[*]\nindent_style = tab\nindent_size = 2\ntab_width = 3\n");
    assert_eq!(d.options("test.cs").unwrap().printer.indent_size, 3);
    let d = Dir::new();
    d.file(".editorconfig", "[*]\nindent_style = tab\nindent_size = tab\ntab_width = 3\n");
    assert_eq!(d.options("test.cs").unwrap().printer.indent_size, 3);
}

#[test]
fn editorconfig_multiple_files_unset_and_root() {
    let d = Dir::new();
    d.file("sub/.editorconfig", "[*]\nindent_size = 1\n");
    d.file(".editorconfig", "[*]\nindent_size = 2\nmax_line_length = 10\n");
    let o = d.options("sub/test.cs").unwrap();
    assert_eq!((o.printer.indent_size, o.printer.width), (1, 10));

    let d = Dir::new();
    d.file("sub/.editorconfig", "[*]\nindent_size = unset\n");
    d.file(".editorconfig", "[*]\nindent_size = 2\n");
    assert_eq!(d.options("sub/test.cs").unwrap().printer.indent_size, 4);

    let d = Dir::new();
    d.file("sub/.editorconfig", "root = true\n\n[*]\nindent_size = 2\n");
    d.file(".editorconfig", "[*]\nmax_line_length = 2\n");
    assert_eq!(d.options("sub/test.cs").unwrap().printer.width, 100);
}

#[test]
fn editorconfig_globs() {
    let d = Dir::new();
    d.file(".editorconfig", "[*.{cs,csx}]\nindent_size = 3\n[sub/*.xml]\nindent_size = 5\n");
    assert_eq!(d.options("a/test.csx").unwrap().printer.indent_size, 3);
    assert_eq!(d.options("sub/test.xml").unwrap().printer.indent_size, 5);
    assert_eq!(d.options("other/test.xml").unwrap().printer.indent_size, 2);
}

#[test]
fn csharpierrc_in_any_parent_wins_over_editorconfig() {
    let d = Dir::new();
    d.file("sub/.editorconfig", "[*.cs]\nindent_size = 2\n");
    d.file(".csharpierrc", "indentSize: 1");
    assert_eq!(d.options("sub/test.cs").unwrap().printer.indent_size, 1);

    let d = Dir::new();
    d.file(".editorconfig", "[*.cs]\nindent_size = 2\n");
    d.file(".csharpierrc", "indentSize: 1");
    assert_eq!(d.options("test.cs").unwrap().printer.indent_size, 1);
}

#[test]
fn editorconfig_formatter_overrides() {
    let d = Dir::new();
    d.file(".editorconfig", "[*.foo]\ncsharpier_formatter = xml\ncsharpier_xml_whitespace_sensitivity = ignore\n");
    let o = d.options("test.foo").unwrap();
    assert_eq!(o.formatter, Formatter::Xml);
    assert_eq!(o.xml_whitespace_sensitivity, XmlWhitespaceSensitivity::Ignore);
}

#[allow(dead_code)]
fn unused(_: &Path) {}
