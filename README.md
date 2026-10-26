# rsharpier

A Rust port of [CSharpier](https://github.com/belav/csharpier), the opinionated C# formatter.
It parses with [tree-sitter](https://tree-sitter.github.io/) (a patched copy of
[tree-sitter-c-sharp](https://github.com/tree-sitter/tree-sitter-c-sharp)) instead of Roslyn and
produces the same output as CSharpier for the formatting tests of the .NET version.

```
rsharpier format [paths...]      # format files and directories
rsharpier check [paths...]       # exit with 1 if something is not formatted
echo 'class  A{}' | rsharpier format
rsharpier format --write-stdout --stdin-path src/File.cs < src/File.cs
rsharpier pipe-files             # editor integration, files piped through stdin
rsharpier server --server-port N # editor integration, POST /format
```

Supported: `.cs`, `.csx` and the XML flavours (`.csproj`, `.props`, `.targets`, `.config`, `.slnx`,
`.xaml`, `.axaml`, `.xml`). Options come from `.csharpierrc` / `.csharpierrc.json` / `.yaml` /
`.yml` (with `overrides`) or `.editorconfig`; `.csharpierignore` and `.gitignore` files are used
the same way the .NET version uses them.

## Status

The ported formatting tests of CSharpier, which live in a checkout of the .NET repository next to
this one (`../csharpier`, or set `CSHARPIER_TEST_FILES`), all pass:

```
cargo test --release
cargo run --release --example conformance [dir] [-v] [name]   # per file results and diffs
```

| | |
|---|---|
| `FormattingTests/TestFiles/cs` | 208 / 208 |
| `csx`, `xml_strict`, `xml_ignore` | all |
| `DocPrinterTests` | all 55 |
| `PreprocessorSymbolsTests`, `LineEndingTests`, `CSharpFormatterTests`, `DeepRecursionTests` | ported |
| `OptionsProviderTests`, `CommandLineFormatterTests`, ignore file tests | the main scenarios ported (`tests/options_provider.rs`, `tests/cli.rs`) |

Against real code, `cargo run --release --example selfcheck <dir>` formats every `.cs` file below a
directory (`IDEM=1` formats the output a second time). On CSharpier's own sources 370 of 380
files come out byte for byte the same (the rest were formatted with an older version of the
.NET formatter). On more than 25,000 files from other repositories (about 4.5 million lines)
formatting never panicked, kept all tokens, and was idempotent for all but two files.

## How it works

| path | ported from | notes |
|---|---|---|
| `src/doc/` | `DocTypes`, `DocPrinter` | the doc IR, `PropagateBreaks`, fits logic and printer |
| `src/trivia.rs` | (new) | rebuilds Roslyn's token + leading/trailing trivia model on top of tree-sitter |
| `src/symbols.rs` | `BooleanExpressionParser`, `PreprocessorSymbols` | the symbol sets that activate every `#if` branch |
| `src/format.rs` | `CSharpFormatter` | pipeline, one pass per symbol set, validation |
| `src/printer/` | `SyntaxPrinter/*` | all node printers, member chains, usings, ignore comments, raw strings |
| `src/xml.rs` | `Xml/*` | the XML reader and printers |
| `src/cli/` | `CSharpier.Cli` | commands, configuration, ignore files, server |
| `grammar/` | | the patched tree-sitter grammar |

### Trivia

Roslyn attaches comments, directives and disabled text to tokens, and CSharpier's printers are
written against that. tree-sitter gives comments as free floating "extras" and parses all `#if`
branches as code. `trivia.rs`

1. finds directive lines (ignoring `#` inside strings and comments),
2. evaluates `#if/#elif/#else` for a symbol set, so inactive branches become *disabled text*,
3. blanks directives and disabled text with spaces (offsets stay stable) and parses that,
4. takes the leaves of the tree as tokens, lexes the gaps between them in the original text into
   trivia and splits them into trailing (up to the first newline) and leading, like Roslyn.

`format()` then formats its own output once for every symbol set from `symbols::get_sets`, which is
what the C# version does so code in every branch gets formatted.

### Validation

`format.rs::validate` compares the multiset of tokens before and after formatting (commas are
excluded, the formatter adds and removes them; multi line strings only by their content). It is
the counterpart of `SyntaxNodeComparer` and catches printers that drop or invent code.

### The grammar

tree-sitter-c-sharp 0.23.5 mis-parses or rejects several things CSharpier needs, so the crate is
vendored in `grammar/tree-sitter-c-sharp`. `grammar/patch_grammar.py` applies the fixes to the
pristine `grammar.js` and regenerates the parser (it needs the `tree-sitter` CLI):

```
python3 grammar/patch_grammar.py <path to upstream grammar.js> <tree-sitter binary>
```

Fixes: `x is null || y` (the `||` ended up in the pattern), `a ?? b?.c`, `a.b is X or Y` and
member access on patterns, list patterns starting with `..`, `params` parameters, `and`/`or`/`not`
as identifiers, null conditional assignment, unions, `extension` blocks, `allows ref struct`,
`closed`, labeled `break`/`continue`, destructors with modifiers, `checked(...)` and plain
names/member accesses as statements, `extern alias` in namespaces and generic names in
`new List<(int a, int b)>`.

## Known differences and gaps

* Sorting of usings uses a lowercase comparison instead of `InvariantCultureIgnoreCase`.
* There is no formatting cache (`--no-cache`/`--use-cache` are accepted and ignored) and no
  CSharpier.MsBuild version check (`--no-msbuild-check` is accepted).
* Files in an encoding other than UTF-8/UTF-16 with a byte order mark are read as UTF-8.
* Syntax errors are reported by tree-sitter positions and not with Roslyn's messages.
* Code that tree-sitter cannot parse (preview syntax that is not in the grammar yet) is reported
  as a syntax error and left alone.
