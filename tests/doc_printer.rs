//! Ported from CSharpier.Tests/DocPrinterTests.cs
use csharpier_rs::doc::{print, Doc, PrinterOptions};
use csharpier_rs::{concat, group, indent};

fn print_doc(doc: &Doc, width: usize, trim_initial_lines: bool) -> String {
    let options = PrinterOptions {
        width,
        trim_initial_lines,
        ..Default::default()
    };
    print(doc, &options, "\n").trim_end_matches(['\r', '\n']).to_string()
}

fn check(doc: Doc, expected: &str) {
    assert_eq!(print_doc(&doc, 100, false), expected);
}

fn check_width(doc: Doc, expected: &str, width: usize) {
    assert_eq!(print_doc(&doc, width, false), expected);
}

fn hl() -> Doc {
    Doc::hard_line()
}

#[test]
fn lines_allowed() {
    check(concat![hl(), "1"], "\n1");
}

#[test]
fn basic_concat() {
    check(concat!["1", "2", "3"], "123");
}

#[test]
fn concat_with_line() {
    check(concat!["1", Doc::line(), "3"], "1\n3");
}

#[test]
fn group_with_line() {
    check(group!(concat!["1", Doc::line(), "3"]), "1 3");
}

#[test]
fn group_with_hardline() {
    check(group!(concat!["1", hl(), "3"]), "1\n3");
}

#[test]
fn group_with_line_and_break_parent() {
    check(
        group!(concat!["1", Doc::line(), "2", Doc::line(), "3", Doc::break_parent()]),
        "1\n2\n3",
    );
}

#[test]
fn indent_with_break_parent() {
    check(
        concat![
            "0",
            group!(indent!(concat![
                Doc::soft_line(),
                "1",
                Doc::line(),
                "2",
                Doc::line(),
                "3",
                Doc::break_parent()
            ]))
        ],
        "0\n    1\n    2\n    3",
    );
}

#[test]
fn large_group_concat_with_line() {
    let t = "LongTextLongTextLongTextLongText";
    check_width(
        group!(concat![t, Doc::line(), t, Doc::line(), t]),
        &format!("{t}\n{t}\n{t}"),
        80,
    );
}

#[test]
fn two_indents_with_hardline() {
    check(
        concat!["0", concat![indent!(concat![hl(), "1"]), hl(), indent!(concat![hl(), "2"])]],
        "0\n    1\n\n    2",
    );
}

#[test]
fn lines_removed_from_beginning() {
    assert_eq!(print_doc(&concat![hl(), "1"], 100, true), "1");
}

#[test]
fn literal_lines_removed_from_beginning() {
    check(concat![Doc::literal_line(), "1"], "1");
}

#[test]
fn force_flat_prevents_breaking() {
    check(Doc::force_flat(vec!["1".into(), hl(), "2".into()]), "1 2");
}

#[test]
fn force_flat_does_not_affect_literal_lines() {
    check(Doc::force_flat(vec!["1".into(), Doc::literal_line(), "2".into()]), "1\n2");
}

#[test]
fn force_flat_with_literal_string_still_breaks_group() {
    check(
        group!(
            Doc::force_flat(vec!["1".into(), Doc::literal_line(), "2".into()]),
            Doc::line(),
            "3"
        ),
        "1\n2\n3",
    );
}

#[test]
fn literal_line_trims_space() {
    check(
        concat!["{", indent!(hl(), "indent", Doc::literal_line()), "}"],
        "{\n    indent\n}",
    );
}

#[test]
fn literal_line_does_not_trim_output_or_indent_next_line() {
    check(
        concat!["(", indent!(hl(), "1", " ", Doc::literal_line(), "2", hl(), "3"), ")"],
        "(\n    1 \n2\n    3)",
    );
}

#[test]
fn if_break_flat() {
    check(group!(Doc::if_break("break".into(), "flat".into(), None)), "flat");
}

#[test]
fn if_break_break() {
    check(group!(hl(), Doc::if_break("break".into(), "flat".into(), None)), "\nbreak");
}

#[test]
fn if_break_when_group_does_not_fit() {
    check_width(
        group!("another", Doc::line(), Doc::if_break("break".into(), "flat".into(), None)),
        "another\nbreak",
        10,
    );
}

#[test]
fn if_break_flat_when_group_id_does_not_break() {
    let id = Doc::next_group_id();
    check(
        concat![
            Doc::group_with_id(id, vec!["1".into()]),
            Doc::if_break("break".into(), "flat".into(), Some(id))
        ],
        "1flat",
    );
}

#[test]
#[should_panic(expected = "ifBreak before the group")]
fn if_break_targeting_unprinted_group_panics() {
    let id = Doc::next_group_id();
    check(
        concat![
            Doc::if_break("break".into(), "flat".into(), Some(id)),
            Doc::group_with_id(id, vec!["1".into()])
        ],
        "flat1",
    );
}

#[test]
fn conditional_group_picks_first_that_fits() {
    let doc = Doc::conditional_group(vec![
        concat!["aaaaaaaaaa", "bbbbbbbbbb"],
        concat!["short"],
        concat!["last"],
    ]);
    check_width(doc, "short", 8);
}
