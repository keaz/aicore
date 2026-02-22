# 09 Modules and Packages

## Goal
Organize code across files and import functionality from local packages.

## Syntax you need
- Define module identity with `module a.b.c;`.
- Import modules explicitly with `import a.b.c;`.
- Refer to imported symbols unqualified (`add(...)`) or qualified (`math.add(...)`) depending on style.
- Package-based imports use module names exposed by installed/resolved packages.

## Runnable snippet (multi-file module)
`examples/e2/multi_file_app/src/math.aic`
```aic
module app.math;

fn add(x: Int, y: Int) -> Int {
    x + y
}
```

`examples/e2/multi_file_app/src/main.aic`
```aic
module app.main;
import app.math;
import std.io;

fn main() -> Int effects { io } {
    let v = add(40, 2);
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

import std.io;
import http_client.main;

fn main() -> Int effects { io } {
    print_int(http_client.main.get_status_code());
    0
}
```

Run it:
```bash
aic run examples/pkg/consume_http_client.aic
```

Expected output:
```text
42
```

## What to remember
- Imports are explicit and deterministic.
- Module namespace collisions are diagnostics; keep module paths clear and stable.
