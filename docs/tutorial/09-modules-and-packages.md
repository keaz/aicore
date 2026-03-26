# 09 Modules and Packages

## Goal
Organize code across files and import functionality from local packages.

## Syntax you need
- Define module identity with `module a.b.c;`.
- Import modules explicitly with `import a.b.c;`.
- Refer to imported symbols unqualified (`add(...)`) or qualified (`math.add(...)`) depending on style.
- Package-based imports use module names exposed by installed/resolved packages.
- Exported symbols that cross module boundaries need `pub`.

## Runnable snippet (multi-file module)
`examples/e2/multi_file_app/src/math.aic`
```aic
module app.math;

pub fn add(x: Int, y: Int) -> Int {
    x + y
}
```

`examples/e2/multi_file_app/src/main.aic`
```aic
module app.main;
import app.math;
import std.io;

fn main() -> Int effects { io } capabilities { io } {
    let v = math.add(40, 2);
    print_int(v);
    0
}
```

Run it:
```bash
aic run examples/e2/multi_file_app/src/main.aic
```

Expected output:
```text
42
```

## Runnable snippet (package import)
```aic
module pkg.consume_http_client;

import std.http;
import std.io;
import http_client.main;

fn failure() -> Int {
    1
}

fn emit_report(method_label: String, status_text: String) -> Int effects { io } capabilities { io } {
    print_str(method_label);
    print_str(" ");
    println_str(status_text);
    0
}

fn main() -> Int effects { io } capabilities { io } {
    match http_client.main.health_request() {
        Ok(request) => match http_client.main.ok_response() {
            Ok(response) => match method_name(request.method) {
                Ok(method_label) => match status_reason(response.status) {
                    Ok(status_text) => emit_report(method_label, status_text),
                    Err(_) => failure(),
                },
                Err(_) => failure(),
            },
            Err(_) => failure(),
        },
        Err(_) => failure(),
    }
}
```

Run it:
```bash
aic run examples/pkg/consume_http_client.aic
```

Expected output:
```text
GET OK
```

## What to remember
- Imports are explicit and deterministic.
- Module namespace collisions are diagnostics; keep module paths clear and stable.
- Tail-segment import aliases are what you call at the use site, so `import app.math;` gives you `math.add(...)`.
- Package examples should show real module behavior; this one constructs typed `std.http` values without pretending to perform network I/O.
