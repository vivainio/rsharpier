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

// ---- the rest of DocPrinterTests ------------------------------------------------------------

fn s(v: &str) -> Doc {
    Doc::text(v)
}

#[test]
fn if_break_prints_break_contents_when_group_id_breaks() {
    let id = Doc::next_group_id();
    check(
        concat![
            "1",
            Doc::group_with_id(id, vec![hl()]),
            Doc::if_break("break".into(), "flat".into(), Some(id))
        ],
        "1\nbreak",
    );
}

#[test]
fn if_break_propagates_breaks_from_break_contents() {
    check(
        group!("1", Doc::line(), Doc::if_break(concat!["break", hl()], "flat".into(), None)),
        "1\nbreak",
    );
}

#[test]
fn if_break_propagates_breaks_from_flat_contents() {
    check(
        group!("1", Doc::line(), Doc::if_break("break".into(), concat!["flat", hl()], None)),
        "1\nbreak",
    );
}

#[test]
fn indent_if_break_prints_indented_when_breaks() {
    let id = Doc::next_group_id();
    check(
        concat![
            Doc::group_with_id(id, vec![Doc::indent(vec![hl()])]),
            Doc::indent_if_break("indent".into(), id)
        ],
        "\n    indent",
    );
}

#[test]
fn indent_if_break_not_indented_when_flat() {
    let id = Doc::next_group_id();
    check(
        concat![
            Doc::group_with_id(id, vec![Doc::indent(vec![Doc::soft_line()])]),
            Doc::indent_if_break("indent".into(), id)
        ],
        "indent",
    );
}

#[test]
fn trim_trims_current_line() {
    for indent in [" ", "   "] {
        check(concat![format!("1{indent}"), Doc::Trim], "1");
    }
}

#[test]
fn trim_does_not_trim_non_whitespace() {
    for value in ["1", ""] {
        check(concat![value, Doc::Trim], value);
    }
}

#[test]
fn trim_trims_indentation() {
    check(
        concat!["{", indent!(hl(), "1", hl(), Doc::Trim, "#if"), hl(), "}"],
        "{\n    1\n#if\n}",
    );
}

#[test]
fn trim_does_not_affect_fits() {
    check_width(group!("test    ", Doc::Trim, Doc::line(), "test"), "test test", 10);
}

#[test]
fn trailing_comment_does_not_get_extra_space() {
    check(
        concat![
            "x",
            Doc::trailing_comment("// comment", csharpier_rs::doc::CommentType::SingleLine),
            " ",
            "y"
        ],
        "x // comment\ny",
    );
}

#[test]
fn trailing_comment_does_not_get_extra_space_from_line() {
    check(
        group!(
            "x",
            Doc::trailing_comment("// comment", csharpier_rs::doc::CommentType::SingleLine),
            Doc::line(),
            "y"
        ),
        "x // comment\ny",
    );
}

#[test]
fn trailing_comment_does_affect_trim() {
    check(
        concat![
            Doc::trailing_comment("// trailing", csharpier_rs::doc::CommentType::SingleLine),
            hl(),
            indent!(Doc::hard_line_if_no_previous_line_for_trivia(), Doc::Trim, "    #endregion")
        ],
        " // trailing\n    #endregion",
    );
}

#[test]
fn hard_line_if_no_previous_line_inserts_a_line_if_there_is_not_one() {
    check(concat!["1", Doc::hard_line_if_no_previous_line(), "2"], "1\n2");
}

#[test]
fn hard_line_if_no_previous_line_does_not_insert_if_there_is_one() {
    check(concat!["1", hl(), Doc::hard_line_if_no_previous_line(), "2"], "1\n2");
}

#[test]
fn hard_line_if_no_previous_line_does_not_blow_up() {
    check(concat![Doc::hard_line_if_no_previous_line(), "1"], "\n1");
}

#[test]
fn hard_line_if_no_previous_line_not_after_indented_hard_line() {
    // a Concat that is not flattened
    let doc = Doc::Concat(std::rc::Rc::new(std::cell::RefCell::new(vec![indent!(
        "1",
        hl(),
        Doc::hard_line_if_no_previous_line(),
        "2"
    )])));
    check(doc, "1\n    2");
}

#[test]
fn hard_line_for_trivia_should_not_break() {
    check(group!(Doc::hard_line_for_trivia(), "1", Doc::line(), "2"), "\n1 2");
}

#[test]
fn hard_line_for_trivia_with_hard_line_should_break() {
    check(
        group!(Doc::hard_line_for_trivia(), "1", hl(), "2", Doc::line(), "3"),
        "\n1\n2\n3",
    );
}

#[test]
fn hard_line_for_trivia_should_break_outer_group() {
    check(
        group!("1", Doc::line(), "2", group!(Doc::hard_line_for_trivia(), "3")),
        "1\n2\n3",
    );
}

#[test]
fn hard_line_for_trivia_twice_should_not_break() {
    check(
        group!(Doc::hard_line_for_trivia(), Doc::hard_line_for_trivia(), "1", Doc::line(), "2"),
        "\n\n1 2",
    );
}

#[test]
fn hard_line_if_no_previous_line_for_trivia_should_not_break() {
    check(
        group!(Doc::hard_line_if_no_previous_line_for_trivia(), "1", Doc::line(), "2"),
        "\n1 2",
    );
}

#[test]
fn hard_line_for_trivia_should_break_when_does_not_fit() {
    check_width(
        group!(Doc::hard_line_for_trivia(), "someValue", Doc::line(), "someValue"),
        "\nsomeValue\nsomeValue",
        10,
    );
}

#[test]
fn hard_line_for_trivia_not_break_with_multiple_comments() {
    use csharpier_rs::doc::CommentType::SingleLine;
    check(
        group!(
            Doc::leading_comment("// shouldn't break next line", SingleLine),
            Doc::hard_line_for_trivia(),
            Doc::leading_comment("// shouldn't break next line", SingleLine),
            Doc::hard_line_for_trivia(),
            "true",
            Doc::line(),
            "|| false"
        ),
        "// shouldn't break next line\n// shouldn't break next line\ntrue || false",
    );
}

#[test]
fn directive_does_not_affect_first_in_group() {
    check(
        group!(Doc::directive("#pragma"), Doc::hard_line_for_trivia(), "1", Doc::line(), "2"),
        "#pragma\n1 2",
    );
}

#[test]
fn conditional_group_prints_basic_group() {
    check(Doc::conditional_group(vec![s("short")]), "short");
}

#[test]
fn conditional_group_prints_group_that_fits() {
    check_width(
        Doc::conditional_group(vec![s("tooLong tooLong"), concat!["tooLong", hl(), "tooLong"]]),
        "tooLong\ntooLong",
        10,
    );
}

#[test]
fn conditional_group_prints_as_group() {
    check_width(Doc::conditional_group(vec![concat!["1", Doc::line(), "2"]]), "1 2", 10);
}

#[test]
fn conditional_group_does_propagate_breaks_within_options() {
    check_width(
        Doc::conditional_group(vec![s("ThisContentIsTooLong"), group!("1", Doc::line(), "2", hl(), "3")]),
        "1\n2\n3",
        10,
    );
}

#[test]
fn conditional_group_does_not_propagate_breaks_to_parent() {
    check_width(
        group!(Doc::conditional_group(vec![
            concat!["1", Doc::line(), "2"],
            concat!["11", hl(), "22"]
        ])),
        "1 2",
        10,
    );
}

#[test]
fn conditional_group_prints_alternative_if_initial_group_does_not_fit() {
    check_width(
        Doc::conditional_group(vec![
            concat![
                group!("(", Doc::soft_line(), "1111111", ")"),
                group!("(", Doc::soft_line(), "1111111", ")")
            ],
            concat!["(1111111)", hl(), "(1111111)"],
        ]),
        "(1111111)\n(1111111)",
        10,
    );
}

#[test]
fn conditional_group_prints_initial_group_if_it_fits() {
    check_width(
        Doc::conditional_group(vec![
            concat![
                group!("(", Doc::soft_line(), "1", ")"),
                group!("(", Doc::soft_line(), "1", ")")
            ],
            concat!["(1)", hl(), "(1)"],
        ]),
        "(1)(1)",
        10,
    );
}

#[test]
fn print_includes_single_new_line_at_end_of_file() {
    for instances in 0..3 {
        for eol in ["\n", "\r\n"] {
            let doc = Doc::text(format!("1{}", eol.repeat(instances)));
            let result = print(&doc, &PrinterOptions::default(), eol);
            assert_eq!(result, format!("1{eol}"));
        }
    }
}

#[test]
fn conditional_group_does_not_propagate_breaks() {
    // this seems odd, but this is how conditional groups work
    check(
        group!(
            Doc::conditional_group(vec![concat!["1", hl(), "2"], concat!["1", hl(), "2"]]),
            Doc::soft_line(),
            "3"
        ),
        "1\n23",
    );
}
