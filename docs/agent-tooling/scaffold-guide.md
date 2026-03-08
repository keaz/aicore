# Scaffold Guide

Reference guide for `aic scaffold` happy-path usage.

## Struct

Command:

```bash
aic scaffold struct User --field name:String --field age:Int --with-invariant "age >= 0"
```

Output:

```aic
struct User {
    name: String,
    age: Int,
} invariant age >= 0
```

## Enum

Command:

```bash
aic scaffold enum AppError --variant NotFound --variant "InvalidInput:String"
```

Output:

```aic
enum AppError {
    NotFound,
    InvalidInput(String),
}
```

## Function

Command:

```bash
aic scaffold fn process_user --param u:User --return "Result[Int, AppError]" --effect io --capability io --requires "u.age >= 0" --ensures "true"
```

Output:

```aic
fn process_user(u: User) -> Result[Int, AppError] effects { io } capabilities { io } requires u.age >= 0 ensures true {
    Ok(0)
}
```

JSON mode:

```bash
aic scaffold fn process_user --param u:User --return "Result[Int, AppError]" --effect io --capability io --requires "u.age >= 0" --ensures "true" --json
```

Machine-readable fields:

```json
{
  "kind": "fn",
  "name": "process_user",
  "content": "fn process_user(u: User) -> Result[Int, AppError] effects { io } capabilities { io } requires u.age >= 0 ensures true {\n    Ok(0)\n}"
}
```

## Match

Command:

```bash
aic scaffold match maybe_user --arm "Some(v)=>v.age" --arm "None=>0" --exhaustive
```

Output:

```aic
match maybe_user {
    Some(v) => v.age,
    None => 0,
}
```

Non-exhaustive matches must include an explicit fallback arm, for example `_=>0`.

## Test

Command:

```bash
aic scaffold test --for process_user
```

Output:

```aic
#[test]
fn test_process_user_run_pass() -> () {
    // add a valid process_user invocation when fixture inputs are available
    assert(true);
}

// compile-fail fixture template:
// #[test]
// fn test_process_user_compile_fail() -> () {
//     // add intentionally invalid process_user usage and assert diagnostics
//     // assert(true);
// }
```

## Validation

The runnable example project for these command shapes lives at `examples/e7/scaffold_examples` and is exercised by CI with:

```bash
aic check examples/e7/scaffold_examples
aic test examples/e7/scaffold_examples --json
```
