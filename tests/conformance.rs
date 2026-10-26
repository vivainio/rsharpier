//! Runs CSharpier's own formatting tests (from a checkout next to this repository) through the
//! port. Files that are known not to pass yet are listed in tests/known_failures.txt, this test
//! fails when a file that is not listed fails or when a listed file starts to pass.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use csharpier_rs::doc::PrinterOptions;
use csharpier_rs::xml::XmlWhitespaceSensitivity;

fn test_files_dir() -> Option<PathBuf> {
    let candidates = [
        std::env::var("CSHARPIER_TEST_FILES").ok().map(PathBuf::from),
        Some(PathBuf::from("../csharpier/Src/CSharpier.Tests/FormattingTests/TestFiles")),
    ];
    candidates.into_iter().flatten().find(|p| p.is_dir())
}

fn run_dir(dir: &Path, folder: &str) -> Vec<(String, bool)> {
    let is_xml = folder.starts_with("xml");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir.join(folder))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy().to_string();
            n.ends_with(".test") && !n.ends_with(".actual.test") && !n.ends_with(".expected.test")
        })
        .collect();
    files.sort();
    let mut results = Vec::new();
    for file in files {
        let name = file.file_stem().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&file).unwrap();
        let src = csharpier_rs::cli::decode(&bytes).1;
        let expected_path = file.with_extension("expected.test");
        let expected = if expected_path.exists() {
            std::fs::read_to_string(&expected_path).unwrap().trim_start_matches('\u{feff}').to_string()
        } else {
            src.clone()
        };
        let options = PrinterOptions {
            use_tabs: name.ends_with("_Tabs"),
            indent_size: if is_xml { 2 } else { 4 },
            ..Default::default()
        };
        let sensitivity = if folder.ends_with("ignore") {
            XmlWhitespaceSensitivity::Ignore
        } else {
            XmlWhitespaceSensitivity::Strict
        };
        let result = std::panic::catch_unwind(|| {
            if is_xml {
                csharpier_rs::xml::format_xml(&src, &options, sensitivity).ok()
            } else {
                csharpier_rs::format(&src, &options).ok().map(|r| r.code)
            }
        });
        let passed = matches!(result, Ok(Some(code)) if code.replace("\r\n", "\n") == expected.replace("\r\n", "\n"));
        results.push((format!("{folder}/{name}"), passed));
    }
    results
}

#[test]
fn formatting_test_files() {
    let Some(dir) = test_files_dir() else {
        eprintln!("CSharpier test files not found, skipping");
        return;
    };
    let known: BTreeSet<String> = include_str!("known_failures.txt")
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    let mut unexpected_failures = Vec::new();
    let mut unexpected_passes = Vec::new();
    let mut total = 0;
    let mut passed_count = 0;
    for folder in ["cs", "csx", "xml_strict", "xml_ignore"] {
        for (name, passed) in run_dir(&dir, folder) {
            total += 1;
            passed_count += passed as usize;
            match (passed, known.contains(&name)) {
                (false, false) => unexpected_failures.push(name),
                (true, true) => unexpected_passes.push(name),
                _ => {}
            }
        }
    }
    eprintln!("{passed_count} of {total} formatting test files pass");
    assert!(unexpected_failures.is_empty(), "these now fail: {unexpected_failures:?}");
    assert!(
        unexpected_passes.is_empty(),
        "these pass now, remove them from tests/known_failures.txt: {unexpected_passes:?}"
    );
}
