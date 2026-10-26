use std::io::Read;

use csharpier_rs::doc::PrinterOptions;

fn main() {
    let path = std::env::args().nth(1);
    let src = match &path {
        Some(p) if p != "-" => std::fs::read_to_string(p).expect("reading file"),
        _ => {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s).unwrap();
            s
        }
    };
    match csharpier_rs::format(&src, &PrinterOptions::default()) {
        Ok(result) => {
            print!("{}", result.code);
            if !result.unhandled.is_empty() {
                eprintln!("unhandled nodes: {:?}", result.unhandled);
            }
        }
        Err(e) => {
            eprintln!("{e:?}");
            std::process::exit(1);
        }
    }
}
