//! Ported from PreprocessorSymbolsTests

use csharpier_rs::preprocessor_symbol_sets;

fn run(code: &str, expected: &[&str]) {
    let mut actual: Vec<Vec<String>> = preprocessor_symbol_sets(code);
    let mut expected: Vec<Vec<String>> = expected
        .iter()
        .map(|s| s.split(',').map(String::from).collect())
        .collect();
    // collection equivalence does not depend on order
    for set in actual.iter_mut().chain(expected.iter_mut()) {
        set.sort();
    }
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected, "{code}");
}

#[test]
fn basic_if() {
    for symbol in ["DEBUG", "NET48", "NET48_OR_GREATER"] {
        run(&format!("#if {symbol}\n// {symbol}\n#endif\n"), &[symbol]);
    }
}

#[test]
fn and() {
    run("#if ONE && TWO\n// ONE,TWO\n#endif\n", &["ONE,TWO"]);
}

#[test]
fn or() {
    run("#if ONE || TWO\n// ONE\n#endif\n", &["ONE"]);
}

#[test]
fn basic_not_if() {
    run("#if !DEBUG\n//\n#endif\n", &[]);
}

#[test]
fn if_with_else() {
    run("#if DEBUG\n// DEBUG\n#else\n//\n#endif\n", &["DEBUG"]);
}

#[test]
fn if_with_elif() {
    run("#if ONE\n// ONE\n#elif TWO\n// TWO\n#endif\n", &["ONE", "TWO"]);
}

#[test]
fn parens() {
    run("#if (ONE || TWO) && THREE\n// ONE,THREE\n#endif\n", &["ONE,THREE"]);
}

#[test]
fn equals_true() {
    run("#if ONE == true\n// ONE\n#endif\n", &["ONE"]);
}

#[test]
fn equals_false() {
    run("#if ONE == false\n//\n#endif\n", &[]);
}

#[test]
fn not_equals_true() {
    run("#if ONE != true\n//\n#endif\n", &[]);
}

#[test]
fn not_equals_false() {
    run("#if ONE != false\n// ONE\n#endif\n", &["ONE"]);
}

#[test]
fn else_branch() {
    run("#if !ONE\n//\n#else\n// ONE\n#endif\n", &["ONE"]);
}

#[test]
fn else_if_and_else() {
    run("#if ONE\n// ONE\n#elif !TWO\n// !TWO\n#else\n// TWO\n#endif\n", &["ONE", "TWO"]);
}

#[test]
fn nested_if() {
    run("#if ONE\n#if TWO\n// ONE,TWO\n#endif\n#endif\n", &["ONE,TWO", "ONE"]);
}

#[test]
fn duplicate_if() {
    run("#if ONE\n// ONE\n#endif\n#if ONE\n// ONE\n#endif\n", &["ONE"]);
}

#[test]
fn comments() {
    run("#if ONE // comment\n// ONE\n#endif\n", &["ONE"]);
}

#[test]
fn true_literal() {
    run("#if true\n//\n#endif\n", &[]);
}

#[test]
fn and_with_not() {
    run("\n#if ONE && !TWO\n// ONE\n#endif\n", &["ONE"]);
}

#[test]
fn nothing_for_a_file_without_directives() {
    run("class ClassName\n{\n    // a comment\n    void MethodName() { }\n}\n", &[]);
}

#[test]
fn nothing_for_directives_that_are_not_if() {
    run("#nullable enable\n#region TheRegion\nclass ClassName { }\n#endregion\n", &[]);
}
