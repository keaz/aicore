# Regex Runtime (`std.regex`)

This document defines the `std.regex` API contract implemented for `[DT-T1] std regex engine integration`.

## Module surface

```aic
module std.regex;

enum RegexError {
    InvalidPattern,
    InvalidInput,
    NoMatch,
    UnsupportedFeature,
    TooComplex,
    Internal,
}

struct Regex {
    pattern: String,
    flags: Int,
}
```

Public APIs:

- `compile(pattern: String) -> Result[Regex, RegexError]`
- `compile_with_flags(pattern: String, flags: Int) -> Result[Regex, RegexError]`
- `is_match(regex: Regex, text: String) -> Result[Bool, RegexError]`
- `find(regex: Regex, text: String) -> Result[String, RegexError]`
- `replace(regex: Regex, text: String, replacement: String) -> Result[String, RegexError]`

## Flags

`flags` is a deterministic bitmask:

- `no_flags() -> Int`: `0`
- `flag_case_insensitive() -> Int`: `1`
- `flag_multiline() -> Int`: `2`
- `flag_dot_matches_newline() -> Int`: `4`

Behavior notes:

- Unknown bits produce `Err(InvalidInput)`.
- `flag_multiline() + flag_dot_matches_newline()` is currently unsupported and returns `Err(UnsupportedFeature)`.
- Matching uses POSIX ERE semantics in the runtime.

## Error model

- `InvalidPattern`: regex compile failure (`regcomp` style syntax errors).
- `InvalidInput`: invalid flags or invalid ABI string input.
- `NoMatch`: `find` has no match.
- `UnsupportedFeature`: unsupported flag combinations or unsupported host runtime.
- `TooComplex`: runtime allocation/engine complexity boundary.
- `Internal`: unexpected runtime failure.

## Runtime behavior

- `compile*` validates pattern + flags and returns a `Regex` value if successful.
- `is_match` returns `Ok(true|false)`; no-match is not an error.
- `find` returns the first matched substring or `Err(NoMatch)`.
- `replace` replaces only the first match; if no match, original text is returned.

## Example

See `/Users/kasunranasinghe/Projects/Rust/aicore/examples/data/log_parse_regex.aic`.
