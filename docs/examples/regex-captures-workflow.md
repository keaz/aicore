# Regex Captures Workflow (Data/Text)

This workflow demonstrates using `std.regex.captures` and `std.regex.find_all` to extract structured groups from text.

## APIs Used

- `compile(pattern) -> Result[Regex, RegexError]`
- `captures(regex, text) -> Result[Option[RegexMatch], RegexError]`
- `find_all(regex, text) -> Result[Vec[RegexMatch], RegexError]`

## Recommended Pattern

1. Compile once with `compile` or `compile_with_flags`.
2. Use `captures` when you need only the first match.
3. Use `find_all` for complete non-overlapping extraction in source order.
4. Treat `Ok(None())` / empty vectors as stable no-match outcomes.

## Runnable Example

- `examples/data/regex_capture_groups.aic`

Run:

```bash
cargo run --quiet --bin aic -- run examples/data/regex_capture_groups.aic
```

Expected final line: `42`.
