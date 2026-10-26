//! Runs CSharpier's own formatting test files through the Rust port.
//!
//! usage: cargo run --release --example conformance [dir] [-v] [name-filter]
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use csharpier_rs::doc::PrinterOptions;

fn main() {
    let mut dir = PathBuf::from("../csharpier/Src/CSharpier.Tests/FormattingTests/TestFiles/cs");
    let mut verbose = false;
    let mut filter: Option<String> = None;
    for a in std::env::args().skip(1) {
        if a == "-v" {
            verbose = true;
        } else if Path::new(&a).is_dir() {
            dir = PathBuf::from(a);
        } else {
            filter = Some(a);
        }
    }

    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy().to_string();
            n.ends_with(".test") && !n.ends_with(".actual.test") && !n.ends_with(".expected.test")
        })
        .collect();
    files.sort();

    let (mut pass, mut pass_verbatim, mut fail, mut syntax, mut unhandled_files) = (0, 0, 0, 0, 0);
    let mut unhandled_kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    let mut verbatim_files: Vec<String> = Vec::new();

    for file in &files {
        let name = file.file_stem().unwrap().to_string_lossy().to_string();
        if let Some(f) = &filter {
            if !name.contains(f.as_str()) {
                continue;
            }
        }
        let bytes = std::fs::read(file).unwrap();
        let src = String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string();
        let expected_path = file.with_extension("expected.test");
        let expected = if expected_path.exists() {
            std::fs::read_to_string(&expected_path).unwrap().trim_start_matches('\u{feff}').to_string()
        } else {
            src.clone()
        };
        let options = PrinterOptions {
            width: 100,
            use_tabs: name.ends_with("_Tabs"),
            ..Default::default()
        };
        let result = std::panic::catch_unwind(|| csharpier_rs::format(&src, &options));
        match result {
            Err(_) => {
                fail += 1;
                failures.push((name, "panic".to_string()));
            }
            Ok(Err(e)) => {
                syntax += 1;
                failures.push((name, format!("{e:?}")));
            }
            Ok(Ok(r)) => {
                let normalize = |s: &str| s.replace("\r\n", "\n");
                if normalize(&r.code) == normalize(&expected) {
                    if r.unhandled.is_empty() {
                        pass += 1;
                    } else {
                        // only passes because unhandled nodes were copied through unchanged
                        pass_verbatim += 1;
                        for k in r.unhandled.iter().collect::<std::collections::BTreeSet<_>>() {
                            *unhandled_kinds.entry(k.clone()).or_default() += 1;
                        }
                        verbatim_files.push(name.clone());
                    }
                } else {
                    if !r.unhandled.is_empty() {
                        unhandled_files += 1;
                        for k in r.unhandled.iter().collect::<std::collections::BTreeSet<_>>() {
                            *unhandled_kinds.entry(k.clone()).or_default() += 1;
                        }
                    }
                    fail += 1;
                    std::fs::create_dir_all("target/conformance").unwrap();
                    std::fs::write(format!("target/conformance/{name}.expected"), &expected).unwrap();
                    std::fs::write(format!("target/conformance/{name}.actual"), &r.code).unwrap();
                    if verbose {
                        println!("=== {name}\n--- expected\n{expected}--- actual\n{}", r.code);
                    }
                    failures.push((
                        name,
                        if r.unhandled.is_empty() {
                            "output differs".to_string()
                        } else {
                            "unhandled nodes".to_string()
                        },
                    ));
                }
            }
        }
    }

    println!(
        "pass {pass}  pass-with-verbatim-fallback {pass_verbatim}  fail {fail} (unhandled nodes: {unhandled_files})  parse errors {syntax}  total {}",
        pass + pass_verbatim + fail + syntax
    );
    let mut kinds: Vec<_> = unhandled_kinds.into_iter().collect();
    kinds.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!("\nmost common unhandled node kinds (files affected):");
    for (k, n) in kinds.iter().take(25) {
        println!("  {n:4} {k}");
    }
    if !verbatim_files.is_empty() {
        println!("\npassing only through verbatim fallback: {}", verbatim_files.join(", "));
    }
    if !verbose {
        println!("\nfailures:");
        for (n, why) in failures.iter().take(400) {
            println!("  {n}: {why}");
        }
    }
}
