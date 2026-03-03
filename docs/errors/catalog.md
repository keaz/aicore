# Diagnostic Code Catalog

This catalog covers all registered diagnostics from /Users/kasunranasinghe/Projects/Rust/aicore/src/diagnostic_codes.rs (251 codes).

Each row includes a concise description plus compile-intent trigger/fix snippets aligned with AIC syntax.

## Runtime IO error context chaining

Runtime IO context chains are modeled by `std.error_context` and `std.io` helper APIs, not diagnostic codes.

- `ErrorContext[E]` stores `error`, last `context`, and flattened `chain`.
- `from_fs_error_with_context`, `from_net_error_with_context`, `from_proc_error_with_context`, and `from_env_error_with_context` preserve the source cause (for example `fs.NotFound`) and mapped IO cause (for example `io.EndOfInput`) in the flattened chain.
- `error_chain(...)` returns the flattened chain string.
- `io_error(...)` / `error_value(...)` recover the mapped typed error without changing existing `Result[..., IoError]` APIs.

## Secure networking error contract (module-level)

`aic explain` covers compiler/runtime diagnostics (`E...`). Module-level secure-networking failures are standardized separately:

- Contract file: `/Users/kasunranasinghe/Projects/Rust/aicore/docs/errors/secure-networking-error-contract.v1.json`
- Deterministic replay file: `/Users/kasunranasinghe/Projects/Rust/aicore/docs/security-ops/postgres-tls-scram-replay.v1.json`
- AIC mapping APIs: `std.secure_errors` (`buffer_error_info`, `crypto_error_info`, `tls_error_info`, `pool_error_info`)
- Compatibility rules:
  - existing `code` values are immutable
  - existing `category` and `retryable` flags are immutable
  - new codes are additive-only
  - agent tooling should branch on `code` first, then `category` / `retryable`

| Code | Description | Trigger example | Fix example |
|---|---|---|---|
| `E0001` | Invalid character in source token stream. | `fn main() -> Int { @ }` | `fn main() -> Int { 0 }` |
| `E0002` | Reserved lexer diagnostic code in the stable registry. | `fn main() -> Int { @ }` | `fn main() -> Int { 0 }` |
| `E0003` | Reserved lexer diagnostic code in the stable registry. | `fn main() -> Int { @ }` | `fn main() -> Int { 0 }` |
| `E0004` | Invalid escape sequence in string literal. | `fn main() -> Int { @ }` | `fn main() -> Int { 0 }` |
| `E0005` | Unterminated block comment. | `fn main() -> Int { @ }` | `fn main() -> Int { 0 }` |
| `E0006` | Unterminated string literal. | `fn main() -> Int { @ }` | `fn main() -> Int { 0 }` |
| `E0007` | Invalid float literal token. | `fn main() -> Int { @ }` | `fn main() -> Int { 0 }` |
| `E0008` | Invalid char literal (must contain exactly one Unicode codepoint). | `fn main() -> Char { 'ab' }` | `fn main() -> Char { 'a' }` |
| `E0009` | Invalid integer literal suffix. | `fn main() -> Int { 1i33 }` | `fn main() -> Int { 1i32 }` |
| `E0010` | Float literals do not allow integer suffixes. | `fn main() -> Float { 1.5u8 }` | `fn main() -> Float { 1.5 }` |
| `E1001` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1002` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1003` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1004` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1005` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1006` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1007` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1008` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1009` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1010` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1011` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1012` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1013` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1014` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1015` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1016` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1017` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1018` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1019` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1020` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1021` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1022` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1023` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1024` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1025` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1026` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1027` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1028` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1029` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1030` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1031` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1032` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1033` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1034` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1035` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1036` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1037` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1038` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1039` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1040` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1041` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1042` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1043` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1044` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1045` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1046` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1047` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1048` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1049` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1050` | Parser grammar diagnostic in declaration or expression parsing. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1051` | Use of null literal, which is not part of AIC syntax. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1052` | Invalid async item form; parser expects async fn. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1053` | Trait or impl declaration syntax error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1054` | Trait or impl declaration syntax error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1055` | Trait or impl declaration syntax error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1056` | Trait or impl declaration syntax error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1057` | Trait or impl declaration syntax error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1058` | Trait or impl declaration syntax error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1059` | Trait or impl declaration syntax error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1060` | Assignment statement parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1061` | Assignment statement parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1062` | Assignment statement parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1063` | Extern or unsafe declaration parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1064` | Extern or unsafe declaration parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1065` | Extern or unsafe declaration parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1066` | Extern or unsafe declaration parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1067` | Extern or unsafe declaration parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1068` | Extern or unsafe declaration parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1069` | Function-type or closure parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1070` | Function-type or closure parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1071` | Function-type or closure parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1072` | Function-type or closure parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1073` | Function-type or closure parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1074` | Function-type or closure parsing error. | `fn sum(a: Int b: Int) -> Int { a + b }` | `fn sum(a: Int, b: Int) -> Int { a + b }` |
| `E1075` | Type alias declaration requires an identifier after `type`. | `type = Int;` | `type Count = Int;` |
| `E1076` | Type alias declaration requires `=` before the target type. | `type Count Int;` | `type Count = Int;` |
| `E1077` | Type alias declaration must end with `;`. | `type Count = Int` | `type Count = Int;` |
| `E1078` | Const declaration requires an identifier after `const`. | `const : Int = 1;` | `const BASE: Int = 1;` |
| `E1079` | Const declaration requires `:` after the const name. | `const BASE Int = 1;` | `const BASE: Int = 1;` |
| `E1080` | Const declaration requires `=` before the initializer. | `const BASE: Int 1;` | `const BASE: Int = 1;` |
| `E1081` | Const declaration must end with `;`. | `const BASE: Int = 1` | `const BASE: Int = 1;` |
| `E1090` | Malformed visibility modifier (expected `pub` or `pub(crate)`). | `pub(package) fn main() -> Int { 0 }` | `pub(crate) fn main() -> Int { 0 }` |
| `E1091` | Visibility modifiers are not supported on `type` aliases or `const` items. | `pub type Count = Int;` | `type Count = Int;` |
| `E1093` | Invalid intrinsic declaration form (missing `fn`/`;`, body present, or unsupported contracts/generics). | `intrinsic fn aic_fs_exists_intrinsic(path: String) -> Bool { false }` | `intrinsic fn aic_fs_exists_intrinsic(path: String) -> Bool;` |
| `E1100` | Name-resolution diagnostic for scopes, imports, or symbol ownership. | `fn main() -> Int { missing_name }` | `fn main() -> Int { let missing_name = 1; missing_name }` |
| `E1101` | Name-resolution diagnostic for scopes, imports, or symbol ownership. | `fn main() -> Int { missing_name }` | `fn main() -> Int { let missing_name = 1; missing_name }` |
| `E1102` | Name-resolution diagnostic for scopes, imports, or symbol ownership. | `fn main() -> Int { missing_name }` | `fn main() -> Int { let missing_name = 1; missing_name }` |
| `E1103` | Unknown trait referenced in impl declaration. | `fn main() -> Int { missing_name }` | `fn main() -> Int { let missing_name = 1; missing_name }` |
| `E1104` | Trait implementation arity mismatch. | `fn main() -> Int { missing_name }` | `fn main() -> Int { let missing_name = 1; missing_name }` |
| `E1105` | Conflicting duplicate trait implementation. | `fn main() -> Int { missing_name }` | `fn main() -> Int { let missing_name = 1; missing_name }` |
| `E1200` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1201` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1202` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1203` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1204` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1205` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1206` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1207` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1208` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1209` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1210` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1211` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1212` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1213` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1214` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1215` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1216` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1217` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1218` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1219` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1220` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1221` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1222` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1223` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1224` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1225` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1226` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1227` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1228` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1229` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1230` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1231` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1232` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1233` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1234` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1235` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1236` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1237` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1238` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1239` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1240` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1241` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1242` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1243` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1244` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1245` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1246` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1247` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1248` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1249` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1250` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1251` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1252` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1253` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1254` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1255` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1256` | Await used outside an async function. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1257` | Await operand is not Async[T]. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1258` | Trait bound is not satisfied by concrete type arguments. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1259` | Invalid or unknown trait bound declaration. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1260` | Question-mark operand is not Result[T, E]. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1261` | Question-mark used in function without Result return type. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1262` | Question-mark error type does not match enclosing function Result error type. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1263` | Conflicting mutable borrow detected. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1264` | Immutable borrow attempted while mutable borrow is active. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1265` | Assignment attempted while borrow is active. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1266` | Assignment to immutable binding. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1267` | Mutable borrow attempted on immutable binding. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1268` | Invalid borrow target; non-local expression cannot be borrowed. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1269` | Assignment type mismatch. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1270` | Match guard expression must have type Bool. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1271` | Or-pattern alternatives bind different variable sets. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1272` | Or-pattern alternatives bind incompatible variable types. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1273` | While condition must have type Bool. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1274` | Break expression type does not match loop break type. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1275` | Break used outside loop context. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1276` | Continue used outside loop context. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1280` | Closure parameter type must be explicit. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1281` | Closure body type does not match declared closure return type. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1282` | Generic function value requires explicit specialization. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1283` | Effectful function cannot be converted to first-class function value. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1284` | Closure parameter count mismatches expected Fn type. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1285` | Closure parameter type mismatches expected Fn type. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1286` | Closure return type mismatches expected Fn type. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1287` | Const initializer used an expression that is not compile-time evaluable. | `const BAD: Int = value();` | `const GOOD: Int = 1 + 2;` |
| `E1288` | Const declaration type/initializer mismatch or missing initializer. | `const BAD: Int = true;` | `const GOOD: Int = 3;` |
| `E1300` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E1301` | Type-checking or pattern/exhaustiveness diagnostic. | `fn main() -> Int { if true { 1 } else { "x" } }` | `fn main() -> Int { if true { 1 } else { 0 } }` |
| `E2001` | Effectful call used without required effects declaration. | `fn read_cfg() -> Result[String, FsError] { std.fs.read_file("cfg.txt") }` | `fn read_cfg() -> Result[String, FsError] effects { fs } { std.fs.read_file("cfg.txt") }` |
| `E2002` | Effectful operation used inside a contract expression that must remain pure. | `fn read_cfg() -> Result[String, FsError] { std.fs.read_file("cfg.txt") }` | `fn read_cfg() -> Result[String, FsError] effects { fs } { std.fs.read_file("cfg.txt") }` |
| `E2003` | Unknown effect name in function signature. | `fn read_cfg() -> Result[String, FsError] { std.fs.read_file("cfg.txt") }` | `fn read_cfg() -> Result[String, FsError] effects { fs } { std.fs.read_file("cfg.txt") }` |
| `E2004` | Duplicate effect listed in one function signature. | `fn read_cfg() -> Result[String, FsError] { std.fs.read_file("cfg.txt") }` | `fn read_cfg() -> Result[String, FsError] effects { fs } { std.fs.read_file("cfg.txt") }` |
| `E2005` | Transitive required effect is missing from caller boundary. | `fn read_cfg() -> Result[String, FsError] { std.fs.read_file("cfg.txt") }` | `fn read_cfg() -> Result[String, FsError] effects { fs } { std.fs.read_file("cfg.txt") }` |
| `E2006` | Resource protocol violation (closed/consumed handle reuse). | `let h = std.fs.open_read("x"); std.fs.file_close(h); std.fs.file_read_line(h)` | `let h = std.fs.open_read("x"); std.fs.file_read_line(h); std.fs.file_close(h)` |
| `E2007` | Unknown capability name in function signature. | `fn read_cfg() -> Result[String, FsError] capabilities { file } { std.fs.read_file("cfg.txt") }` | `fn read_cfg() -> Result[String, FsError] capabilities { fs } { std.fs.read_file("cfg.txt") }` |
| `E2008` | Duplicate capability listed in one function signature. | `fn read_cfg() -> Result[String, FsError] capabilities { fs, fs } { std.fs.read_file("cfg.txt") }` | `fn read_cfg() -> Result[String, FsError] capabilities { fs } { std.fs.read_file("cfg.txt") }` |
| `E2009` | Missing capability authority for declared/transitive effects. | `fn read_cfg() -> Result[String, FsError] effects { fs } { std.fs.read_file("cfg.txt") }` | `fn read_cfg() -> Result[String, FsError] effects { fs } capabilities { fs } { std.fs.read_file("cfg.txt") }` |
| `E2100` | Module/package/registry workflow diagnostic. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2101` | Module/package/registry workflow diagnostic. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2102` | Module/package/registry workflow diagnostic. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2103` | Module/package/registry workflow diagnostic. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2104` | Module/package/registry workflow diagnostic. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2105` | Module/package/registry workflow diagnostic. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2106` | Module/package/registry workflow diagnostic. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2107` | Module/package/registry workflow diagnostic. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2108` | Module/package/registry workflow diagnostic. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2109` | Module/package/registry workflow diagnostic. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2110` | Invalid package install spec or version requirement. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2111` | Package manifest read or write failure in registry workflow. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2112` | Duplicate package version publish attempted. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2113` | Package metadata missing or invalid for name/version fields. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2114` | Semantic-version resolution conflict across install requirements. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2115` | Requested package/version not found in configured registry. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2116` | Registry/index/package IO or integrity failure. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2117` | Private registry authentication missing or invalid. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2118` | Registry configuration or credential source is invalid. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2119` | Trust policy denied package install. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2120` | Invalid or unsupported extern ABI declaration. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2121` | Extern function signature uses unsupported language features. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2122` | Extern call requires explicit unsafe boundary. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2123` | Unsupported type used in extern C-ABI signature. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2124` | Package signature verification or trusted-key validation failure. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2125` | Workspace manifest/member metadata is invalid or inconsistent. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E2126` | Workspace package dependency cycle detected. | `module app.main; import app.util; fn main() -> Int { app.util.missing() }` | `module app.main; import app.util; fn main() -> Int { app.util.answer() }` |
| `E4001` | Requires contract proved always false. | `fn halve(x: Int) -> Int requires { x > 0 } { 0 }` | `fn halve(x: Int) -> Int requires { x > 0 } { x / 2 }` |
| `E4002` | Ensures contract proved always false. | `fn halve(x: Int) -> Int requires { x > 0 } { 0 }` | `fn halve(x: Int) -> Int requires { x > 0 } { x / 2 }` |
| `E4003` | Contract obligation left as residual runtime check (note severity). | `fn halve(x: Int) -> Int requires { x > 0 } { 0 }` | `fn halve(x: Int) -> Int requires { x > 0 } { x / 2 }` |
| `E4004` | Struct invariant proved always false. | `fn halve(x: Int) -> Int requires { x > 0 } { 0 }` | `fn halve(x: Int) -> Int requires { x > 0 } { x / 2 }` |
| `E4005` | Contract obligation discharged at compile time (note severity). | `fn halve(x: Int) -> Int requires { x > 0 } { 0 }` | `fn halve(x: Int) -> Int requires { x > 0 } { x / 2 }` |
| `E5001` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5002` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5003` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5004` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5005` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5006` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5007` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5008` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5009` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5010` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5011` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5012` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5013` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5014` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5015` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5016` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5017` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5018` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5019` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5020` | Code generation or runtime-lowering diagnostic. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5021` | Backend lowering failed for invalid question-mark operand/result layout. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5022` | Backend lowering failed for incompatible function Result return layout. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5023` | Backend does not lower guarded match arms. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5024` | Backend extern wrapper/link ABI mismatch or unsupported extern lowering. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5025` | Backend encountered break outside loop context. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5026` | Backend encountered continue outside loop context. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5031` | Closure capture is unavailable in the current lowering scope. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5032` | Indirect call target is not a first-class function value. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5033` | Closure parameter type is missing during backend lowering. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5034` | Backend cannot lower referenced function as a first-class function value. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5035` | Closure helper return type mismatch during backend lowering. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E5036` | JSON encode/decode for function values is unsupported. | `fn main() -> Int { print_int("x") }` | `fn main() -> Int { print_int(1); 0 }` |
| `E6001` | Deprecated std API usage warning. | `fn main() -> Int effects { time } { std.time.now() }` | `fn main() -> Int effects { time } { std.time.now_ms() }` |
| `E6002` | Std-compat check found a baseline compatibility break. | `fn main() -> Int effects { time } { std.time.now() }` | `fn main() -> Int effects { time } { std.time.now_ms() }` |
| `E6003` | Typed hole (`_`) accepted with inferred type/context. | `fn id(x: _) -> _ { x }` | `fn id(x: Int) -> Int { x }` |
| `E6004` | Import is declared but not used. | `module app.main; import std.io; fn main() -> Int { 0 }` | `module app.main; fn main() -> Int { 0 }` |
| `E6005` | Function is unreachable from entrypoint or otherwise unused. | `fn dead() -> Int { 1 } fn main() -> Int { 0 }` | `fn main() -> Int { 0 }` |
| `E6006` | Local variable is never used. | `fn main() -> Int { let value = 1; 0 }` | `fn main() -> Int { let _value = 1; 0 }` |

## Catalog Coverage Backfill

The following registered diagnostics are included for catalog completeness:
`E1082`, `E1083`, `E1086`, `E1087`, `E1088`, `E1089`, `E1092`, `E1106`, `E1107`, `E1108`, `E1109`, `E6007`.
