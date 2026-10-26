//! Scenarios from CommandLineFormatterTests

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use csharpier_rs::cli::{self, CommandLineOptions, LogFormat, LogLevel, Logger};

const UNFORMATTED: &str = "public class ClassName { public int Field; }";
const FORMATTED: &str = "public class ClassName\n{\n    public int Field;\n}\n";

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Buffer {
    fn lines(&self) -> Vec<String> {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap().lines().map(String::from).collect()
    }
}

struct Context {
    dir: tempfile::TempDir,
}

struct Output {
    exit_code: i32,
    out: Vec<String>,
    err: Vec<String>,
}

impl Context {
    fn new() -> Context {
        Context { dir: tempfile::tempdir().unwrap() }
    }
    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }
    fn file(&self, path: &str, contents: &str) {
        let p = self.root().join(path.replace('\\', "/"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, contents).unwrap();
    }
    fn contents(&self, path: &str) -> String {
        std::fs::read_to_string(self.root().join(path)).unwrap()
    }
    fn run(&self, paths: &[&str], customize: impl FnOnce(&mut CommandLineOptions)) -> Output {
        let mut opts = CommandLineOptions {
            paths: if paths.is_empty() { vec![".".into()] } else { paths.iter().map(|s| s.to_string()).collect() },
            ..Default::default()
        };
        customize(&mut opts);
        // paths are relative to the root of the test directory
        let root = self.root();
        for p in opts.paths.iter_mut() {
            if !std::path::Path::new(p.as_str()).is_absolute() {
                *p = if p == "." { root.to_string_lossy().to_string() } else { root.join(p.as_str()).to_string_lossy().to_string() };
            }
        }
        let (out, err) = (Buffer::default(), Buffer::default());
        let logger = Logger::with_writers(opts.log_level, opts.log_format, Box::new(out.clone()), Box::new(err.clone()));
        // the supplied paths in messages are what the user typed, so run from the root
        let exit_code = in_dir(&root, || cli::run_with(opts, &logger, &mut std::io::empty()));
        Output { exit_code, out: out.lines(), err: err.lines() }
    }
}

static CWD: Mutex<()> = Mutex::new(());

fn in_dir<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
    let _guard = CWD.lock().unwrap_or_else(|e| e.into_inner());
    let old = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir).unwrap();
    let r = f();
    std::env::set_current_dir(old).unwrap();
    r
}

fn run_relative(ctx: &Context, paths: &[&str], customize: impl FnOnce(&mut CommandLineOptions)) -> Output {
    // like a user typing relative paths in the root of the test directory
    let mut opts = CommandLineOptions {
        paths: if paths.is_empty() { vec![".".into()] } else { paths.iter().map(|s| s.to_string()).collect() },
        ..Default::default()
    };
    customize(&mut opts);
    let (out, err) = (Buffer::default(), Buffer::default());
    let logger = Logger::with_writers(opts.log_level, opts.log_format, Box::new(out.clone()), Box::new(err.clone()));
    let exit_code = in_dir(&ctx.root(), || cli::run_with(opts, &logger, &mut std::io::empty()));
    Output { exit_code, out: out.lines(), err: err.lines() }
}

fn normalized(s: &str) -> String {
    s.replace('\\', "/")
}

#[test]
fn writes_failed_to_compile() {
    let c = Context::new();
    c.file("Invalid.cs", "asdfasfasdf");
    let r = run_relative(&c, &[], |_| {});
    assert_eq!(normalized(&r.err[0]), "Error ./Invalid.cs - Was not formatted due to syntax errors.");
    assert_eq!(r.exit_code, 1);
}

#[test]
fn writes_failed_to_compile_as_warning() {
    let c = Context::new();
    c.file("Invalid.cs", "asdfasfasdf");
    let r = run_relative(&c, &[], |o| o.syntax_errors_as_warnings = true);
    assert_eq!(normalized(&r.out[0]), "Warning ./Invalid.cs - Was not formatted due to syntax errors.");
    assert_eq!(r.exit_code, 0);
}

#[test]
fn writes_failed_to_compile_for_subdirectory_and_nested_directories() {
    let c = Context::new();
    c.file("Subdirectory/Invalid.cs", "asdfasfasdf");
    let r = run_relative(&c, &["Subdirectory"], |_| {});
    assert_eq!(normalized(&r.err[0]), "Error ./Subdirectory/Invalid.cs - Was not formatted due to syntax errors.");

    let c = Context::new();
    c.file("Directory/Invalid.cs", "asdfasfasdf");
    let r = run_relative(&c, &[], |_| {});
    assert_eq!(normalized(&r.err[0]), "Error ./Directory/Invalid.cs - Was not formatted due to syntax errors.");
}

#[test]
fn writes_unsupported_for_explicit_files_only() {
    let c = Context::new();
    c.file("Unsupported.js", "asdfasfasdf");
    let r = run_relative(&c, &["Unsupported.js"], |_| {});
    assert_eq!(normalized(&r.out[0]), "Warning ./Unsupported.js - Is an unsupported file type.");

    // not when formatting a directory
    let r = run_relative(&c, &[], |_| {});
    assert!(r.out.iter().all(|l| !l.contains("unsupported")), "{:?}", r.out);

    // not when the file is also ignored
    c.file(".csharpierignore", "Unsupported.js");
    let r = run_relative(&c, &["Unsupported.js"], |_| {});
    assert!(r.out.iter().all(|l| !l.contains("unsupported")), "{:?}", r.out);
}

#[test]
fn writes_formatted_files_and_supports_skip_write() {
    let c = Context::new();
    c.file("Unformatted.cs", UNFORMATTED);
    run_relative(&c, &["Unformatted.cs"], |_| {});
    assert_eq!(c.contents("Unformatted.cs"), FORMATTED);

    let c = Context::new();
    c.file("Unformatted.cs", UNFORMATTED);
    run_relative(&c, &[], |o| o.skip_write = true);
    assert_eq!(c.contents("Unformatted.cs"), UNFORMATTED);
}

#[test]
fn check_reports_unformatted_files() {
    let c = Context::new();
    c.file("Unformatted.cs", UNFORMATTED);
    let r = run_relative(&c, &[], |o| o.check = true);
    assert_eq!(r.exit_code, 1);
    assert_eq!(c.contents("Unformatted.cs"), UNFORMATTED);
    assert!(normalized(&r.err[0]).starts_with("Error ./Unformatted.cs - Was not formatted."), "{:?}", r.err);

    let r = run_relative(&c, &[], |o| {
        o.check = true;
        o.log_format = LogFormat::MsBuild;
    });
    assert!(normalized(&r.err[0]).starts_with("./Unformatted.cs: error: Was not formatted."), "{:?}", r.err);

    let r = run_relative(&c, &[], |o| {
        o.check = true;
        o.unformatted_as_warnings = true;
    });
    assert_eq!(r.exit_code, 0);
}

#[test]
fn check_passes_formatted_files() {
    let c = Context::new();
    c.file("Formatted.cs", FORMATTED);
    let r = run_relative(&c, &[], |o| o.check = true);
    assert_eq!(r.exit_code, 0);
    assert!(r.err.is_empty());
}

#[test]
fn ignores_special_folders() {
    for path in ["Src/node_modules/File.cs", "node_modules/File.cs", "node_modules/Folder/File.cs", "Src/obj/File.cs", "obj/File.cs", "obj/Folder/File.cs", "bin/File.cs"] {
        let c = Context::new();
        c.file(path, UNFORMATTED);
        let r = run_relative(&c, &[], |o| o.check = true);
        assert_eq!(r.exit_code, 0, "{path}");
    }
}

#[test]
fn skips_generated_files() {
    for name in ["TemporaryGeneratedFile_Tester.cs", "TestFile.designer.cs", "TestFile.generated.cs", "TestFile.g.cs", "TestFile.g.i.cs"] {
        let c = Context::new();
        c.file(name, UNFORMATTED);
        run_relative(&c, &[], |_| {});
        assert_eq!(c.contents(name), UNFORMATTED, "{name}");

        run_relative(&c, &[], |o| o.include_generated = true);
        assert_eq!(c.contents(name), FORMATTED, "{name}");
    }
}

#[test]
fn skips_auto_generated_comment_files() {
    for comment in ["// <autogenerated />", "// <auto-generated/>", "/* <auto-generated> */"] {
        let c = Context::new();
        let code = format!("{comment}\npublic class ClassName {{ public int Field; }}");
        c.file("File.cs", &code);
        run_relative(&c, &[], |_| {});
        assert_eq!(c.contents("File.cs"), code, "{comment}");
    }
}

fn formatted_files(r: &Output) -> String {
    r.out[0].split(" in ").next().unwrap().to_string()
}

#[test]
fn file_in_ignore_skips_formatting() {
    for (file, ignore) in [
        ("File.cs", "File.cs"),
        ("Debug/Logs/File.cs", "Logs/"),
        ("Debug/Logs/File.cs", "Debug/Logs/File.cs"),
        ("Src/App_Data/Uploads/f45e11a8.cs", "Uploads/"),
        ("Dir/File.cs", "*.cs"),
        ("Dir/File.cs", "Dir/**/*.cs"),
    ] {
        let c = Context::new();
        c.file(file, UNFORMATTED);
        c.file(".csharpierignore", ignore);
        let r = run_relative(&c, &[], |_| {});
        assert_eq!(formatted_files(&r), "Formatted 0 files", "{file} {ignore}");
    }
}

#[test]
fn multiple_files_use_their_own_ignores() {
    let c = Context::new();
    c.file("Subfolder/1/File1.cs", UNFORMATTED);
    c.file("Subfolder/2/File2.cs", UNFORMATTED);
    c.file(".csharpierignore", "Subfolder/**/*.cs");
    let r = run_relative(&c, &["Subfolder/1/File1.cs", "Subfolder/2/File2.cs"], |_| {});
    assert_eq!(formatted_files(&r), "Formatted 0 files");

    let c = Context::new();
    c.file("SubFolder/1/File1.cs", UNFORMATTED);
    c.file("SubFolder/2/File2.cs", UNFORMATTED);
    c.file("SubFolder/1/.csharpierignore", "File1.cs");
    c.file("SubFolder/2/.csharpierignore", "File2.cs");
    let r = run_relative(&c, &["SubFolder/1/File1.cs", "SubFolder/2/File2.cs"], |_| {});
    assert_eq!(formatted_files(&r), "Formatted 0 files");
}

#[test]
fn ignore_deals_with_periods() {
    let c = Context::new();
    c.file("Directory.WithPeriod/File1.cs", UNFORMATTED);
    c.file("Directory.WithPeriod/.csharpierignore", "File1.cs");
    let r = run_relative(&c, &["Directory.WithPeriod"], |_| {});
    assert_eq!(formatted_files(&r), "Formatted 0 files");
}

#[test]
fn csharpier_ignore_and_git_ignore_precedence() {
    for (git, csharpier, ignored) in [("File.cs", "!File.cs", false), ("", "File.cs", true), ("!File.cs", "File.cs", true), ("File.cs", "", true)] {
        for sub in [false, true] {
            let c = Context::new();
            c.file(if sub { "Sub/File.cs" } else { "File.cs" }, UNFORMATTED);
            c.file(".csharpierignore", csharpier);
            c.file(if sub { "Sub/.gitignore" } else { ".gitignore" }, git);
            let r = run_relative(&c, &[], |_| {});
            assert_eq!(formatted_files(&r), if ignored { "Formatted 0 files" } else { "Formatted 1 files" }, "{git} {csharpier} {sub}");
        }
    }
}

#[test]
fn two_git_ignores() {
    for (root, sub, ignored) in [("File.cs", "!File.cs", false), ("", "File.cs", true), ("!File.cs", "File.cs", true), ("File.cs", "", true)] {
        let c = Context::new();
        c.file("Sub/File.cs", UNFORMATTED);
        c.file(".gitignore", root);
        c.file("Sub/.gitignore", sub);
        let r = run_relative(&c, &[], |_| {});
        assert_eq!(formatted_files(&r), if ignored { "Formatted 0 files" } else { "Formatted 1 files" }, "{root} {sub}");
    }
}

#[test]
fn gitignore_negation_patterns_are_evaluated_last() {
    let c = Context::new();
    c.file("Folder1/Folder2/IgnoreMe/File.cs", UNFORMATTED);
    c.file(".gitignore", "Folder1/*\n!Folder2/\nIgnoreMe/");
    let r = run_relative(&c, &[], |_| {});
    assert_eq!(formatted_files(&r), "Formatted 0 files");
}

#[test]
fn gitignore_outside_git_is_not_used() {
    let c = Context::new();
    c.file("Sub/File.cs", UNFORMATTED);
    c.file(".gitignore", "*.cs");
    c.file("Sub/.git/test.txt", "");
    let r = run_relative(&c, &["Sub"], |_| {});
    assert_eq!(formatted_files(&r), "Formatted 1 files");

    // a worktree has a .git file
    let c = Context::new();
    c.file("Sub/File.cs", UNFORMATTED);
    c.file(".gitignore", "*.cs");
    c.file("Sub/.git", "gitdir: ../.git/worktrees/foo");
    let r = run_relative(&c, &["Sub"], |_| {});
    assert_eq!(formatted_files(&r), "Formatted 1 files");
}

#[test]
fn gitignore_can_unignore_all_directories() {
    let c = Context::new();
    c.file("IgnoreTest/IncludedSourceFolder/Test/File.cs", UNFORMATTED);
    c.file(".gitignore", "*\n!*/\n!/IgnoreTest/IncludedSourceFolder/**/*.cs");
    let r = run_relative(&c, &[], |_| {});
    assert_eq!(formatted_files(&r), "Formatted 1 files");
}

#[test]
fn write_stdout_only_writes_the_file() {
    let c = Context::new();
    c.file("file1.cs", UNFORMATTED);
    // stdout is the process stdout here, so just make sure nothing was written to the file
    let r = run_relative(&c, &[], |o| o.write_stdout = true);
    assert_eq!(r.exit_code, 0);
    assert_eq!(c.contents("file1.cs"), UNFORMATTED);
}

#[test]
fn compilation_errors_do_not_lose_code() {
    let c = Context::new();
    let code = "public class ClassName\n{\n    public void M() { var x = ; }\n}\n";
    c.file("Broken.cs", code);
    run_relative(&c, &[], |_| {});
    assert_eq!(c.contents("Broken.cs"), code);
}

#[test]
fn formats_xml_and_csx_files() {
    let c = Context::new();
    c.file("a.xml", "<a><b>x</b></a>");
    c.file("script.csx", "var   x = 1;");
    run_relative(&c, &[], |_| {});
    assert_eq!(c.contents("a.xml"), "<a>\n  <b>x</b>\n</a>\n");
    assert_eq!(c.contents("script.csx"), "var x = 1;\n");
}

#[test]
fn config_path_is_supported() {
    let c = Context::new();
    c.file("File.cs", UNFORMATTED);
    c.file("config/.csharpierrc.json", "{ \"indentSize\": 2 }");
    let cfg = c.root().join("config/.csharpierrc.json");
    run_relative(&c, &[], |o| o.config_path = Some(cfg.to_string_lossy().to_string()));
    assert_eq!(c.contents("File.cs"), "public class ClassName\n{\n  public int Field;\n}\n");
}

#[test]
fn empty_config_files_log_a_warning() {
    let c = Context::new();
    c.file("File.cs", UNFORMATTED);
    c.file(".csharpierrc", "");
    let r = run_relative(&c, &[], |_| {});
    assert_eq!(r.exit_code, 0);
    assert!(r.out.iter().any(|l| l.starts_with("Warning The configuration file at") && l.ends_with("was empty.")), "{:?}", r.out);
    let _ = LogLevel::Debug;
}

#[test]
fn explicit_files_resolve_options_from_their_own_directories() {
    let c = Context::new();
    c.file("a/File.cs", UNFORMATTED);
    c.file("b/File.cs", UNFORMATTED);
    c.file("a/.csharpierrc", "indentSize: 2");
    c.file("b/.csharpierrc", "indentSize: 8");
    run_relative(&c, &["a/File.cs", "b/File.cs"], |_| {});
    assert!(c.contents("a/File.cs").contains("\n  public int Field;"));
    assert!(c.contents("b/File.cs").contains("\n        public int Field;"));
}

#[test]
fn extensionless_file_with_editorconfig() {
    let c = Context::new();
    c.file("Subdirectory/File.cs", UNFORMATTED);
    c.file(".editorconfig", "[*.cs]\nindent_size = 3\n");
    run_relative(&c, &[], |_| {});
    assert!(c.contents("Subdirectory/File.cs").contains("\n   public int Field;"));
}

#[allow(dead_code)]
fn unused(c: &Context) -> Output {
    c.run(&[], |_| {})
}
