# csharpier-rs

Spike: a Rust port of [CSharpier](https://github.com/belav/csharpier) that parses with
[tree-sitter-c-sharp](https://crates.io/crates/tree-sitter-c-sharp) instead of Roslyn.
The reference implementation is expected at `../csharpier` and its formatting tests are used
as the conformance suite.

## Layout

| path | ported from | notes |
|---|---|---|
| `src/doc/` | `CSharpier.Core/DocTypes`, `DocPrinter` | Doc IR, `PropagateBreaks`, fits logic and printer. Complete. 22 tests ported from `DocPrinterTests.cs` in `tests/doc_printer.rs` |
| `src/trivia.rs` | (new) | rebuilds Roslyn's token + leading/trailing trivia model on top of tree-sitter, see below |
| `src/symbols.rs` | `BooleanExpressionParser`, `PreprocessorSymbols` | finds the symbol sets needed to activate every `#if` branch |
| `src/format.rs` | `CSharpFormatter` | pipeline, one pass per symbol set, token multiset validation |
| `src/printer/` | `SyntaxPrinter/*` | token/trivia printing, members, statements, expressions, patterns, member chains |
| `examples/conformance.rs` | `BaseTest` | runs `Src/CSharpier.Tests/FormattingTests/TestFiles/cs/*.test` |

### How trivia works

Roslyn attaches comments, directives and disabled text to tokens, and CSharpier's printers are
written against that. tree-sitter instead gives comments as free floating "extras" and parses all
`#if` branches as code. `trivia.rs`:

1. finds directive lines (ignoring `#` inside strings/comments, using the tree-sitter ranges),
2. evaluates `#if/#elif/#else` for a symbol set, so inactive branches become *disabled text*,
3. blanks directives and disabled text with spaces (offsets stay stable) and parses that,
4. takes the leaves of the tree as tokens, lexes the gaps between them in the original text into
   trivia and splits them into trailing (up to the first newline) and leading, like Roslyn.

`format()` then re-formats its own output once per symbol set from `symbols::get_sets`, which is
exactly what the C# version does.

### Safety net

`format.rs::validate` compares the multiset of token texts before and after formatting
(commas excluded, they are added/removed). It caught several printers that silently dropped
code. CSharpier proper uses `SyntaxNodeComparer` for this.

## Conformance (cs tests, 208 files)

```
cargo run --release --example conformance            # summary, writes target/conformance/*.{expected,actual}
cargo run --release --example conformance -- -v Name # print diff for matching tests
```

Last run: **101 pass**, 30 more only pass because unhandled nodes are copied through verbatim,
62 differ, 15 fail to parse.

## Known gaps

tree-sitter-c-sharp 0.23.5 does not parse (roughly one parse error file in 14):
`allows ref struct`, `checked(x)`/`unchecked(x)` statements, C# 14 `extension` blocks, the
`closed` modifier, list patterns with property patterns, `x.Prop!;` as a statement,
destructors with leading modifiers, expression statements like `Foo;` (valid syntax,
CS0201 semantically), UCS-2 files.

Not ported yet: `csharpier-ignore` (and range ignore), raw string literals, interpolated string
holes (printed verbatim as one token), query expressions, local functions, anonymous methods,
`goto`/labels, `checked`/`unsafe` statements, type patterns / list patterns / var patterns,
`with`, `stackalloc`, extern alias + using directive edge cases around `#if`, comparing usings
with `InvariantCultureIgnoreCase` (lowercase compare is used), `TrailingComma` for some nodes, xml
formatting, CLI features (editorconfig, ignore files, server).
