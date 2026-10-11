# `aic fmt` Guide

Related docs:
- [Agent-First aic Command Playbook](../aic-command-playbook.md)
- [`docs/syntax.md` canonical formatting contract](../../syntax.md#canonical-formatting-contract)
- [`src/formatter.rs`](../../../src/formatter.rs)

## What it does

`aic fmt <file>` rewrites a source file in canonical layout. `aic fmt <file> --check`
only compares: it exits `0` when the file is already canonical and `1` (with
`format check failed: <file>` on stderr) when it is not. The file is never written in
`--check` mode.

The formatter re-prints the program from the parsed IR, so layout (indentation, line
breaks, spacing) is always canonical. Everything other than whitespace that the
programmer wrote is kept:

- every `//` comment, including `///` doc-style comments
- typed holes (`_` in type positions)
- `pub` / `pub(crate)` visibility, `async`/`unsafe` methods, struct field defaults
- receiver spelling (`self`, `&self`, `&mut self`, `self: Self`)
- literal spelling (`0xFF`, `1.5f32`, `"\u{301}"`, `$"..."` / `f"..."` templates), also in attribute arguments
- sugar the parser lowers: `for` loops, `else if`, `let (a, _) = ...`

## Usage

```bash
aic fmt src/main.aic
aic fmt src/main.aic --check
```

## Comment placement

The lexer keeps comments as trivia with source spans, and the formatter re-emits them
at fixed slots:

| Source position | Output |
|---|---|
| On its own line before an item, statement, block tail, struct field, enum variant, trait/impl method, match arm, attribute, `module`, or `import` | Own line before that element, at its indentation |
| After code on the same line (`x + 1 // why`) | Same line, after the element |
| After a `{` on the same line | Same line, after the `{` |
| After the last element of a block, struct, enum, trait, impl, or match | Own line before the closing `}` |
| After the last item of the file | End of the file |
| Inside a part of an element that the canonical layout prints inline (between call arguments, in a parameter list, in an `if` condition, between two block arguments) | Own line directly above that element |

Additional rules:

- A blank line between two elements or comments is kept; runs of blank lines collapse to one.
- `import` lines are sorted; comments before or after an import move with it.
- A trait or trait impl with no methods prints as `trait Name;` / `impl Name[T];`
  unless it contains a comment, in which case the braces are kept.

Trailing whitespace inside a comment is removed. The comment text is otherwise unchanged.

## Canonicalizations

These rewrites change spelling but not meaning, and are applied on every run:

- `UInt` prints as `USize`, `Float` as `Float64`.
- `where` clauses move into the generic parameter list: `fn f[T](x: T) -> T where T: Eq`
  becomes `fn f[T: Eq](x: T) -> T`.
- `effects { ... }` and `capabilities { ... }` lists are sorted and de-duplicated.
- Redundant parentheses are removed; required ones are added.
- A `module` declaration is followed by one blank line.

## Safety check

Before `aic fmt` writes anything, it re-parses its own output and compares it with
the input:

1. the output must parse without errors;
2. the output must contain exactly the same comment texts;
3. every number, string, and char literal must keep its exact spelling, in order;
4. the parsed program must be identical apart from source positions and the
   canonicalizations above.

If any check fails, `aic fmt` exits with the internal-error code, prints
`format failed for <file>: formatter self-check failed: ...; file left unchanged`, and
does not touch the file. Such a failure is a formatter bug; report it with the input file.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Formatted (or already canonical with `--check`) |
| `1` | Input has parse errors (diagnostics on stdout), or `--check` found a difference |
| `3` | Formatter self-check failed; nothing written |

## Programmatic use

- `aicore::formatter::format_source(source, file)` is the `aic fmt` pipeline: parse,
  format with comments, and verify. It returns `FormatError::Diagnostics` for parse
  errors and `FormatError::Verification` when the safety check fails.
- `aicore::formatter::format_program(&ir)` prints an IR program that has no source
  text (generated code). It cannot print comments, and a tuple `let` loses trailing
  `_` positions because the IR does not record them.
- `aicore::lexer::lex_with_comments(source, file)` returns tokens plus `Comment { text, span }`
  trivia; `lex` returns tokens only.

The LSP `textDocument/formatting` request uses `format_source`, so editor formatting
keeps comments too, and returns no edits when the document does not parse or the
safety check fails.

## Validation

```bash
cargo test --locked --lib -- formatter::
cargo test --locked --lib -- lexer::
cargo test --locked --test formatter_tests
cargo test --locked --test golden_tests
```

`tests/formatter_tests.rs` formats every checked-in `.aic` file that parses and fails
if a comment changes, the safety check trips, or a second run changes the output.
