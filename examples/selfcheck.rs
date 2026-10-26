//! Formats every .cs file under a directory that is already formatted with CSharpier and reports
//! files whose output differs from the input.
//!
//! usage: cargo run --release --example selfcheck <dir> [max-diffs]
use std::path::{Path, PathBuf};

use csharpier_rs::doc::PrinterOptions;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if p.is_dir() {
            if matches!(name.as_str(), "bin" | "obj" | "node_modules" | ".git" | "TestFiles" | "target") {
                continue;
            }
            walk(&p, out);
        } else if name.ends_with(".cs") {
            out.push(p);
        }
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("directory");
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    files.sort();
    let (mut same, mut diff, mut errors, mut panics) = (0, 0, 0, 0);
    let mut lines_total = 0;
    let start = std::time::Instant::now();
    std::fs::create_dir_all("target/selfcheck").unwrap();
    let mut report = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let bytes = std::fs::read(f).unwrap();
        let Ok(src) = String::from_utf8(bytes) else { continue };
        let src = src.trim_start_matches('\u{feff}').to_string();
        lines_total += src.lines().count();
        let options = PrinterOptions::default();
        let t0 = std::time::Instant::now();
        if std::env::var_os("VERBOSE").is_some() {
            eprintln!("START {}", f.display());
        }
        let r = std::panic::catch_unwind(|| csharpier_rs::format(&src, &options));
        if t0.elapsed().as_millis() > 300 {
            eprintln!("SLOW {:?} {} ({} bytes)", t0.elapsed(), f.display(), src.len());
        }
        match r {
            Err(_) => {
                panics += 1;
                report.push(format!("PANIC {}", f.display()));
            }
            Ok(Err(e)) => {
                errors += 1;
                report.push(format!("ERROR {} {:?}", f.display(), e).chars().take(300).collect());
            }
            Ok(Ok(res)) if std::env::var_os("IDEM").is_some() => {
                // formatting the output again must not change it
                match std::panic::catch_unwind(|| csharpier_rs::format(&res.code, &options)).unwrap_or_else(|_| Err(csharpier_rs::FormatError::Syntax("panic on second format".into()))) {
                    Ok(again) if again.code == res.code => same += 1,
                    Ok(again) => {
                        diff += 1;
                        std::fs::write(format!("target/selfcheck/{i}.expected"), &res.code).unwrap();
                        std::fs::write(format!("target/selfcheck/{i}.actual"), &again.code).unwrap();
                        report.push(format!("NOTIDEMPOTENT {i} {}", f.display()));
                    }
                    Err(e) => {
                        errors += 1;
                        report.push(format!("ERROR2 {} {:?}", f.display(), e).chars().take(300).collect());
                    }
                }
            }
            Ok(Ok(res)) => {
                if res.code.replace("\r\n", "\n") == src.replace("\r\n", "\n") {
                    same += 1;
                } else {
                    diff += 1;
                    std::fs::write(format!("target/selfcheck/{i}.expected"), &src).unwrap();
                    std::fs::write(format!("target/selfcheck/{i}.actual"), &res.code).unwrap();
                    report.push(format!("DIFF  {i} {}", f.display()));
                }
            }
        }
    }
    for r in &report {
        println!("{r}");
    }
    println!(
        "\nfiles {}  same {same}  different {diff}  errors {errors}  panics {panics}  lines {lines_total}  {:?}",
        files.len(),
        start.elapsed()
    );
}
