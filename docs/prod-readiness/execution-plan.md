# Language Production Roadmap: Execution Plan

Tracking epic: [#420]. Scope, phases and the full issue list: [language-production-roadmap.md](language-production-roadmap.md). Memory model: `docs/adr/0001-memory-model.md` (lands with Wave 0).

This file is the work queue for the roadmap. It records the state the work starts from, the order of the remaining work, and the rules that keep parallel agent work mergeable. Snapshot: 2026-11-15.

## How to use this plan

- GitHub issues are the source of truth for status. This file sets the order. Update it in the same change when you finish a wave, reorder work, or find work that belongs in a wave.
- **Wave 0 blocks everything.** Do not start any other item until Wave 0 is on `origin/main`. Every branch must start from the integrated `main`.
- After Wave 0, waves set priority. Take the first unclaimed item in the lowest wave whose dependencies are closed. If every item in that wave is claimed or blocked, take an unblocked item from a later wave.
- Claim an item before you start: add the `in-progress` label to its issue and comment `Claimed by <agent>, branch <branch>`. Remove the label if you stop.
- Items marked **needs issue** have no GitHub issue yet. Create one first from the "Language roadmap task" template (`.github/ISSUE_TEMPLATE/roadmap-task.md`) with the labels `lang-prod-readiness` and `story`, and add its link here.
- One issue per branch, created from `origin/main` and named after the issue key (for example `verify-t1-checked-arithmetic`). Do not build on another unmerged branch. If you need its work, wait until it lands.

## Rules for every item

1. Follow the workflow and Definition of Done in [AGENTS.md](../../AGENTS.md).
2. Change the Rust compiler, std and runtime only. The self-host compiler is frozen ([scope decision](../selfhost/scope-decision.md)). Keep `make selfhost-frozen-check` green and write `Self-host: not required (frozen, see docs/selfhost/scope-decision.md)` in the evidence.
3. Take new diagnostic codes from your issue's block in [Diagnostic code blocks](#diagnostic-code-blocks). For each code, register it in `src/diagnostic_codes.rs`, add a row with a verified trigger and fix to `docs/errors/catalog.md`, update the `(N codes)` count, and run `cargo test --locked --test diagnostic_catalog_tests`.
4. If your change makes an existing catalog trigger report a different code, fix that row in the same commit.
5. Run execution tests with `make test-exec` or `cargo test --locked --test execution_tests` (the same command since #501). Both run tests in parallel and need no `RUST_MIN_STACK`; the test helpers compile on a thread with the CLI's 64 MiB stack. A test that compiles in-process outside these helpers must use `driver::run_on_compiler_stack`.
6. A new language feature gets an example under `examples/core/`, listed in `scripts/ci/examples.sh` (`check_pass`, plus the run list when its output is deterministic).
7. A new golden case takes the next free `tests/golden/caseNN_*` number when you rebase, not when you start.
8. Before you push, run `make ci`, the targeted tests for your subsystem, the placeholder scan from AGENTS.md on the touched paths, and `graphify update .`.
9. Rebase onto `origin/main` before landing, then fast-forward `main`, as earlier roadmap issues did. Resolve conflicts in `src/diagnostic_codes.rs`, `docs/diagnostic-codes.md` and `docs/errors/catalog.md` by keeping both sides, sorted by code. Registry and catalog edits in a small last commit keep these conflicts easy.
10. Close the issue only after the commit is on `origin/main`, with this comment:

    ```text
    Implemented in <sha> on origin/main.
    - Acceptance criteria: <one line each, with the test that proves it>
    - Tests: make ci (pass); <targeted commands>
    - Docs: <files>
    - Examples: <files>
    - Diagnostic codes: <codes, or none>
    - Self-host: not required (frozen, see docs/selfhost/scope-decision.md)
    ```

## Starting state (2026-11-15)

| Ref | Head | Contents compared with `origin/main` | Pushed |
|---|---|---|---|
| `origin/main` | `8976f47` | Roadmap work up to and including #446 | yes |
| local `main` | `8c812c5` | #447, impl methods as values (#445), release manifest fix, #448, #449, #450, diagnostic catalog rewrite with `tests/diagnostic_catalog_tests.rs` and `docs/errors/catalog-remaining-work.md`, `std.pool` lease-leak fix, channel slot docs and brief, per-run temp paths in `scripts/ci/examples.sh` | no |
| `lang-prod-roadmap` | `7056f16`, then the commit that adds this plan | #447, impl methods as values (#445), release manifest fix, #448 to #451, `E1303` coverage, `E1313` (std type used without its module) | yes |
| `corr-t9-codegen-shadowing-roadmap` | `c96d8d1` | `7056f16` plus `6fa2b21` (CORR-T9: calls to user functions that shadow std generics) and `c96d8d1` (roadmap follow-ups) | no |
| `mem-t1-memory-model-adr` | `6adf99d` | Branched at `7cc99b3`: #448 to #451 again (same patches as on `lang-prod-roadmap`), then #452 to #463 and #469 (ADR 0001) | no |
| `chain-std-compat-repo-std` | `92ef59f` | One commit on the #457 commit of the branch above: `make docs-check` checks the std API baseline against the repository's `std/` | no |

Every other local branch (`corr-t*`, `parity-t*`, `syntax-t*`, `rel-t*`, `fmt-fix-merge`, `chain-prep`, `issue-*`, `claude/*`, `codex/*`) holds only commits that are already in the refs above, some as older copies with other hashes. This was checked by patch id and subject on 2026-11-15.

GitHub: every issue from #420 to #498 is open, and only [#429] has a progress comment. CI: the last `main` run failed only in the macOS perf gate (Wave 0, step 6).

## Wave 0: Integrate finished work

One agent. Goal: every finished commit is on `origin/main`, CI is green on Linux, macOS and Windows, and the implemented issues are closed with evidence.

Steps 1 to 4 were tried on 2026-11-15 in a scratch clone. Each step lists what that trial found.

### Wave 0 progress

| Step | State | Result |
|---|---|---|
| 1 | done (2026-11-15) | On `lang-prod-roadmap`: `6fa2b21` -> `26fb336`, `c96d8d1` -> `58e49e2`, then `ded9079` merges the two "Open follow-ups" sections. Not pushed. |
| 2 | done (2026-11-15) | Local `main` merge commit `1f2565c`. Kept `main`'s catalog format, added the `E1313` row, added `import std.map;` to the `E1292` trigger and fix. `diagnostic_catalog_tests` passed after the merge. |
| 3 | done (2026-11-15) | `mem-t1-memory-model-adr` rebased onto `1f2565c` (the four duplicate commits were skipped): #452 to #463 and #469 are `a26825b`..`2152b1a`. Local `main` was fast-forwarded to it, plus `1c54542` (`cargo fmt` of the registry). Registry conflicts took the union of codes. Catalog conflicts kept `main`'s format, and each commit's new rows were ported with mode `check`: `E1110`, `E1302`, `E1304` to `E1312` (catalog has 326 rows). Their triggers and fixes pass `diagnostic_catalog_tests`. `docs/diagnostic-codes.md` and `docs/reference/types.md` kept both sides. `cargo build` and `cargo fmt --check` pass. Not pushed. |
| 4 | done (2026-11-15) | `92ef59f` cherry-picked cleanly onto local `main` as `96d0c52`. `AIC_STD_ROOT=$PWD/std target/debug/aic std-compat --check --baseline docs/std-api-baseline.json` passes (`ok (1 additions)`). Not pushed. |
| 5 | done (2026-11-15) | `39442f5` restores `E1238`/`E1240`/`E1242` for `V(a, b)` on a non-tuple payload (the parser folds the sub-patterns into one tuple, #454), with a unit test; new catalog triggers for `E0001`, `E1011`, `E1074`; `E0002`/`E0003` are `check` rows; `E5012`/`E5031` are `internal` and left "Known parity gaps"; two typecheck unit tests declare `std.vec`'s `Vec`. `e7c1e74` makes `scripts/ci/cli-smoke.sh` check the std baseline against the repository's `std/`. CI then exposed two TLS runtime bugs on Linux, both fixed: `a4d074e` (the TLS async worker freed an uninitialized pointer, `free(): invalid pointer`) and `c50dc03` (async `SSL_read`/`SSL_write` on one stream ran concurrently on two threads). `32b902b` adds the missing #459 fuzz seeds and a #444 depth-3 payload test found during step 8's review. `make ci` passes locally except `exec_tls_local_server_handshake_and_certificate_paths`, which fails only with OpenSSL 4.1 (Homebrew) and is now [#503] (Wave 1, item 1.9). |
| 6 | done (2026-11-15) | `91a98c0`, `b3c6e0e`: the soak gate builds the test binaries once and times the binary alone (cargo startup was about 90% of the parse pipeline sample), runs interleaved rounds with the median of 7, and times a fixed CPU calibration loop each round. A slower host widens the regression limit by the same factor, capped at 2x; `max_ms` never scales. A 25 ms floor covers sub-100 ms scenarios. Linux and macOS baselines come from CI run 43745071167; Windows values are scaled from Linux (the gate does not run on Windows in CI). Policy validation is tested in `tests/e8_conformance_tests.rs`. Docs: `docs/verification-quality/perf-sla-playbook.md`. |
| 7 | done (2026-11-16) | `main` pushed to `origin/main` (`8976f47..8c1cc94`). CI run 43764628476 (push) is green on Linux, macOS and Windows; run 43775737760 (dispatch) on the same head is green too. `lang-prod-roadmap` is fully merged. |
| 8 | done except [#429] (2026-11-16) | Each issue's acceptance criteria were checked against the code before closing. Closed with evidence: [#430] to [#444], [#446] to [#463], [#469] (34 issues) and the epic [#423]. Gaps found in that review were fixed first: #459 fuzz seeds and #444 depth-3 test (`32b902b`); #447's decision is linked from #419; #469's ADR is linked from #470 to #474 and #484. [#445] stays open with a progress comment (Wave 1, item 1.1). [#429] stays open: its code is done, `main` is protected with the required check `CI Status Summary` (admins can still push directly), and `Nightly Fuzz` is enabled again (dispatched run 43775691986 is green). Close it once three consecutive `ci.yml` runs on `main` are green and a **scheduled** `Nightly Fuzz` run (03:15 UTC) is green. |
| 9 | done (2026-11-16) | All local branches except `main` deleted (each one's commits are on `main` by patch id or subject); stale worktree removed; temporary remote branch `wave0-integration` deleted. The owner chose to keep the untracked files in the checkout root. The roadmap's "Open follow-ups" and the resolved notes in `docs/errors/catalog-remaining-work.md` were cleaned up (`18c8f3d`). |

**Wave 0 exit:** met except the last part of [#429] (a green scheduled `Nightly Fuzz` run and three consecutive green `main` runs). Wave 1 can start.

Notes for step 5, found during step 3:

- The "Catalog rows are missing" bullet is done.
- `diagnostic_catalog_tests` on `main` now fails exactly as the trial predicted, and nothing else: `E0001`, `E1011`, `E1074`, `E1238`, `E1240`, `E1242`, `E5012`, `E5031`, plus the `@` case in `run_program_reports_codes_for_each_mode`.
- #459 makes the compiler emit `E0002` (unclosed block comment, trigger `fn main() -> Int { 0 } /* note`) and `E0003` (digit not allowed by the literal's base, trigger `0b102`). The catalog still lists both as `reserved`, which the test does not catch. Rewrite both rows as `check` rows. #459 used `$` as the new `E0001` trigger.
- `cargo test --lib`, the other test suites and `make ci` were not run after the rebase.

1. **Collect the CORR-T9 follow-up.** On `lang-prod-roadmap`, cherry-pick `6fa2b21` and `c96d8d1`. `c96d8d1` adds a second `## Open follow-ups` section to `language-production-roadmap.md`. Merge the two sections into one.
2. **Merge `lang-prod-roadmap` into `main`.** Conflicts: `docs/diagnostic-codes.md` and `docs/errors/catalog.md`. Keep the format of `main`'s rewritten catalog and add the branch's changes. The catalog test then needs:
   - a row for `E1313`;
   - the `E1292` fix program uses `Map` without `import std.map;` and now reports `E1313`, so add the import.
3. **Rebase `mem-t1-memory-model-adr` onto the merged `main`.** Git skips the four duplicate commits (#448 to #451). Expected conflicts:
   - #452: `src/diagnostic_codes.rs`, `docs/diagnostic-codes.md`, `docs/errors/catalog.md`;
   - #455, #456, #457, #458, #459 and #461: `docs/errors/catalog.md`;
   - #462: `docs/reference/types.md`.

   Keep both sides. #452 registers `E1110` again with a new meaning (a refutable pattern in a plain `let`). #449 had removed it, and no release ever shipped it, so it only needs a catalog row.
4. **Cherry-pick `92ef59f`** from `chain-std-compat-repo-std`.
5. **Fix the fallout.** The trial found these failures. Fix them, then run the full `make ci`.
   - Catalog rows are missing for `E1110`, `E1302`, `E1304` to `E1312`, and `E1313` (step 2).
   - Some catalog triggers report another code because of the new syntax:
     - `E0001`: `1 @ 2` now reports `E1030`, because `@` is a token since #458. `run_program_reports_codes_for_each_mode` in `tests/diagnostic_catalog_tests.rs` also uses `@` for `E0001`.
     - `E1011`: `struct P ( x: Int )` now reports `E1014` (tuple structs, #462).
     - `E1074`: `|x: Int| { x }` is valid since #460. The code now covers `-> T` followed by a non-block body.
     - `E1238`, `E1240`, `E1242`: `Some(x, y)`, `Ok(x, y)` and `A(x, y)` now parse as one tuple sub-pattern (#454) and report `E1245`. Prefer restoring the specific code when the payload type is not a tuple. Otherwise find new triggers, or reclassify the rows.
     - `E5012`, `E5031`: the triggers now build, because #460 fixed closure capture. Move both rows to `internal` and delete them from "Known parity gaps" in `docs/diagnostic-codes.md`.
   - `cargo test --lib`: `typecheck::tests::place_assignment_targets_are_type_checked` and `typecheck::tests::compound_assignment_is_type_checked` fail with `E1313`. Their programs use std types without importing the module. Add the imports.
   - Passed in the trial: `unit_tests`, `golden_tests`, `check_build_parity_tests`, `formatter_tests`, `repo_hygiene_tests`, and the execution tests except `exec_tls_local_server_handshake_and_certificate_paths`. That test failed the same way (prints 12, expects 42) on `lang-prod-roadmap` before integration and passes in CI, so it is a local environment problem unless CI also fails it.
   - Not run in the trial: `test-e7`, `test-e8`, `test-e9`, `test-selfhost`, `examples-check`, `examples-run`, `docs-check` and the other `make ci` gates.
6. **Make the perf gate stable.** The last `main` CI run failed in "Execution Matrix (macos-latest)": `parse_pipeline_benchmark` took 290.4 ms against a 290 ms limit (200 ms baseline plus 45 % tolerance, set in `benchmarks/service_baseline/rest-runtime-soak-gate.v1.json`). `scripts/ci/rest-runtime-soak-gate.py` times whole `cargo test` invocations, so runner load decides the result. The CORR-T9 follow-up saw 70 % to 104 % overruns on a loaded host. Time the prebuilt test binary (or the benchmark inside it), take the median of more runs, and keep the budget meaningful. Do not just raise the numbers.
7. **Push and check CI.** Push `main`. CI must be green on all three platforms. `lang-prod-roadmap` is then fully merged.
8. **Close the implemented issues.** Use the evidence template and cite the rebased commits on `origin/main`. Check each issue's acceptance criteria against the code first. If a criterion is not met, keep that issue open, comment what is missing (AGENTS.md), and add the missing work to the right wave.
   - Close [#429] to [#444], [#446] to [#463], and [#469] (35 issues). Close [#429] only once CI is green.
   - Keep [#445] open. Its remaining scope is Wave 1, item 1.1.
   - Close the epic [#423] (SYNTAX), whose children are then all closed. [#421] (CORR) stays open for [#499], [#500] and [#502].
9. **Clean up.**
   - Delete local branches whose work is on `origin/main`. `git cherry -v main <branch>` shows rebased copies.
   - Remove the stale worktree `.claude/worktrees/strange-colden-d6bfc9` (detached at `c96d8d1`, no changes) with `git worktree remove`.
   - Ask the owner before deleting untracked files in the checkout root (`=`, `source_diagnostics_check`, `Cargo.lock_bk`, `skills/`, `AI-Core.png`, `AI-Core-icon.png`, `tools/vscode-aic/pnpm-lock.yaml`).
   - In `language-production-roadmap.md`, drop the "Open follow-ups" entries that this plan or an issue now tracks. The note about code collisions between unmerged branches no longer applies.
   - In `docs/errors/catalog-remaining-work.md`, drop the items Wave 0 resolved: the `E5012` and `E5031` parity gaps, and the `v[0]` index read (#455).
   - Run `graphify update .`.

Exit: `origin/main` contains every commit listed in "Starting state", CI is green on all three platforms, and the 35 issues and the epic [#423] are closed with evidence.

## Wave 1: Finish soundness

Goal: no known silent wrong result, and every program that passes `aic check` builds.

### Wave 1 progress

All nine items are on `origin/main` (pushed 2026-11-17 as `dd243fb..08bcda1`), each rebased onto the previous one and fast-forwarded. `make ci` passes locally on the result. The GitHub Actions workflows (`CI`, `Nightly Fuzz`, `Release`) are disabled on the repository, so no hosted run exists for this head; at the owner's request the nine issues were closed with the rule 10 evidence and a note that it is a local macOS `make ci` pass. The epic [#421] (CORR) has no open children left but is still open.

| Item | Issue | Commits on local `main` |
|---|---|---|
| 1.1 | [#445] | `0ea30f8`, `c4e6936` |
| 1.2 | [#499] | `e91556e`, `a815dda` |
| 1.3 | [#486] | `8216263`, `2189ed2`, `2ca35c6` |
| 1.4 | [#488] | `05663a6`..`ecd8966` |
| 1.5 | [#468] | `1d0ae30` |
| 1.6 | [#500] | `66e00c1` |
| 1.7 | [#501] | `fba03e7`, `f56c66a` |
| 1.8 | [#502] | `0175857` |
| 1.9 | [#503] | `79904ac` |

Verification: targeted suites per item, then `make ci` on the integrated `main`. The first run failed only in `test-selfhost`: 1.3 had rewritten `examples/core/opt_levels_demo.aic`, which is in the self-host parity manifest, with `wrapping_mul`, which the frozen self-host compiler cannot check. `2ca35c6` reduces the hash state with `%` instead; the remaining gates then passed. The REST soak gate passes with checked arithmetic, but `json_runtime_hardening` (+27 %) and `async_net_lifecycle_sustained_churn` (+24 %) have the least headroom left.

Watch when CI is enabled again: the 1.3 panic fallback for `obj`/`lib` artifacts has a Windows `_write` path that only a unit test on the generated IR covers; it was never built or run on Windows. Linux and Windows have not run any Wave 1 change. [#429] also still needs its three green `main` runs and a scheduled `Nightly Fuzz` run.

**Wave 1 exit:** met (local `make ci`). The parity table has no gaps (`E5037` is the only `build` row), overflow traps, `extern` calls carry effects, and `make examples-check` covers every example file.

| # | Item | Issue | Size | Needs |
|---|---|---|---|---|
| 1.1 | Close the remaining check/build parity gaps | [#445] | M | Wave 0 |
| 1.2 | Injective generic name mangling | [#499] (CORR-T11) | S | Wave 0 |
| 1.3 | Checked integer arithmetic | [#486] | M | Wave 0 |
| 1.4 | Effects for `extern` calls | [#488] | S | Wave 0 |
| 1.5 | Every example passes `check` and `build` | [#468] | S | Wave 0 (1.1 helps) |
| 1.6 | Channel slot reclamation | [#500] (CORR-T12) | M | Wave 0 |
| 1.7 | Execution tests run outside `make` | [#501] (REL-T8) | S | Wave 0 |
| 1.8 | Codegen signature lookups by symbol | [#502] (CORR-T13), low priority | S | Wave 0 |
| 1.9 | TLS handshake test fails with OpenSSL 4.x | [#503] (REL-T9) | S | Wave 0 |

Notes:

- **1.1** After Wave 0 the gaps are `E5002`, `E5004`, `E5006`, `E5007`, `E5010`, `E5011`, `E5019`, `E5020`, `E5023` and `E5036` (table "Known parity gaps" in `docs/diagnostic-codes.md`, details in `docs/errors/catalog-remaining-work.md` section 1). Two gaps need a decision first, recorded in `docs/diagnostic-codes.md`:
  - May a user file declare `module std.*;`? This decides `E5010` and `E5011`.
  - May user code declare `intrinsic fn`? This decides `E5020`.

  For each gap, add the checker rule or make code generation handle the program, move the catalog row, and delete the table line.

  Done on branch `parity-t4-check-build-parity` (2026-11-16). Decisions (recorded in `docs/diagnostic-codes.md`, "Decisions behind the parity rules"): `std.*` module names are reserved for files under a std root (`E1314`); user code may declare `intrinsic fn` only for intrinsics the backend lowers, with the expected signature (`E1315`). The checker now evaluates consts and field defaults with the backend's evaluator (`src/const_eval.rs`: `E1316` cycles, `E1317` overflow, division by zero, shift range), rejects cyclic aliases (`E1318`) and non-serializable `std.json` type arguments (`E1319`); `-x` on `UInt` reports `E1222`. Found and fixed on the way: `type A = Option[A]` overflowed the checker's stack, a generic call fixed only by the expected type (`let r: Option[Int8] = keep(None)`) failed in codegen, and an intrinsic declaration with effects could never pass `E2009`. All ten former gaps are `internal` catalog rows; `E5037` is the only `build` row left.
- **1.2** `f[B, Int]` and `f_B[Int]` both mangle to `fn_f_B_Int`, so a valid program is rejected with `E1255` (catalog-remaining-work section 2). Make mangling injective and keep `E1255` as an `internal` backstop. Done on branch `corr-t11-injective-mangling` (2026-11-17): `src/mangle.rs` escapes `_` inside components (`__`), brackets and commas (`_L`, `_R`, `_C`), other characters (`_x<hex>_`), and joins type arguments with `_S`; the checker and code generation share it (instantiations, the cross-module symbol suffix as `#<symbol>`, dyn wrappers and vtables). `E1255` is an `internal` row.
- **1.3** Moved up from Phase 5 because overflow is a silent wrong result: `Int` max + 1 prints `-9223372036854775808` today. Done on branch `verify-t1-checked-arithmetic` (2026-11-17): integer `+`, `-`, `*`, unary `-` and `+=`/`-=`/`*=` trap with `integer_overflow{op:<op>,type:<T>}` on every width (`llvm.*.with.overflow`, `src/codegen/generator_arith.rs`); `wrapping_*`, `checked_*` and `saturating_*` methods choose another policy (`E1321` for misuse); literal-only arithmetic that always overflows is `E1320`, evaluated exactly on every width (`UInt128` up to 2^128-1); `src/codegen/overflow_facts.rs` drops checks that literals, local types, `requires` bounds and immutable `let` initializers prove safe. A negated literal (`-128i8`) is a constant, not a checked negation. `obj`/`lib` artifacts get a weak fallback `aic_rt_panic`, so a C harness links them without the runtime. Code that relied on wrapping (`examples/core/opt_levels_demo.aic`, the O2 benchmark workload, the `e7` build-opt fixture) uses `wrapping_*`. Rules: `docs/reference/integer-arithmetic.md`. The REST soak gate passes within budget with checked arithmetic (parse 21.9 ms, router 1449 ms, JSON 1856 ms, async churn 2750 ms, multi-worker 1305 ms; all below their regression limits).
- **1.4** Moved up from Phase 5: a pure function can call `extern "C" fn puts` inside `unsafe { }` and print.
  Done on branch `verify-t7-extern-effects` (2026-11-17): every `extern` declaration must carry `effects { ... }` (`E2010`; `effects { }` declares a pure C function), and callers inherit extern effects through the normal `E2001`/`E2005`/`E2009`/`E2002` rules. Example `examples/core/extern_effects.aic`; design in `docs/reference/effects.md`. `E2011` is unused.
- **1.5** Before Wave 0, 60 of 315 example files failed `aic check` (some on purpose). 20 failed only with `E2009`, because they declare `effects` without `capabilities`. `scripts/ci/examples.sh` only checks fixed lists, so other files go stale unnoticed. Make every file pass, or list it as an expected failure with its code. `examples/core/std_name_shadowing.aic` also fails `aic fmt --check`. Recount with:

  ```bash
  cargo build --release
  for f in $(find examples -name '*.aic' -not -path '*/pkg/*'); do
    AIC_STD_ROOT="$PWD/std" target/release/aic check "$f" >/dev/null 2>&1 || echo "$f"
  done
  ```

  Done on branch `rel-t3-examples-gate` (2026-11-17): `make examples-check` runs `aic check` on every `*.aic` file under `examples/` (`scripts/ci/examples-check-all.py`). A file must pass or be listed with the exact codes it reports in `scripts/ci/example-check-failures.txt` (diagnostic demonstrations, `aic test` files, tool inputs, each with a reason); a listed file that passes, reports other codes or no longer exists fails the gate. 19 examples got the `capabilities` their `effects` need, `examples/io/binary_file_copy.aic` was rewritten for the `Bytes` API of `std.fs` (and now uses temp files), and `examples/core/std_name_shadowing.aic` is formatted. At landing: 339 files, 296 pass, 43 fail as expected.
- **1.6** Closed channels never free their runtime slot, so the 129th channel in a process gets a zero handle, and `std.pool` hits this after about 125 calls. Long-running services fail. The brief with evidence and acceptance criteria is `docs/io-runtime/channel-slot-reclamation.md`.

  Done on branch `corr-t12-channel-slot-reclamation` (2026-11-17). A channel slot is released once the channel is closed, its buffer is empty and no `send`/`recv` waits on it; slot mutexes are created once and never destroyed, and allocation reuses free slots round-robin under a table lock. Handles are `generation * 128 + slot + 1` (first 128 unchanged, at most `2^30`), so an old copy of a handle behaves like a closed channel (`Closed`, close returns `Ok`) and never reaches the channel that reuses the slot. `close_receiver` now drops buffered payloads so its slot is released; `close_sender`/`close_channel` keep buffered values receivable. New `try_buffered_channel` reports exhaustion as `Err(Io)`; `std.pool` widened its handle encoding for the larger handles and `acquire` no longer sends a request with a zero reply handle. No diagnostic codes. Design and evidence: `docs/io-runtime/channel-slot-reclamation.md`; example `examples/io/channel_slot_reuse.aic`.
- **1.7** `cargo test --test execution_tests` overflows the 2 MiB test-thread stack (`exec_string_ops_full_surface_and_edge_cases`), and `exec_pool_discarded_connection_is_replaced` fails under parallel load. Make the test binary set its own stack size and isolate the pool test, or document that only `make test-exec` is supported.

  Done on branch `rel-t8-exec-tests-outside-make` (2026-11-17). Cause of the overflow: an unoptimized `Checker::check_expr_with_expected` frame is about 64 KiB, so the 40-term `+` chain in that test needs 34 nested frames, more than 2 MiB. `src/driver.rs` now exports `COMPILER_THREAD_STACK_BYTES` (64 MiB) and `run_on_compiler_stack`; the CLI and the `run_frontend`/`emit_llvm_*` wrappers in `tests/execution_tests.rs` use it. Cause of the pool flake: the test gave `acquire` 30 one-millisecond waits, and a manager thread delayed by 30 ms on a loaded machine made the second acquire time out. A replacement connection that takes 50 ms to create reproduces it every time with the old timeout. The test now creates its replacement slowly on purpose and uses a 5000 ms timeout; `exec_pool_idle_connections_are_recycled_after_timeout` (failed 1 of 320 runs under load: it slept 20 ms instead of waiting for the reap) and `exec_runtime_pool_churn_sustained_cycles_are_leak_free` got the same treatment. It is not the #500 channel-slot bug, but the idle test caps its `pool_stats` polls at 100 because of it. `make test-exec` drops `RUST_MIN_STACK` and `--test-threads=1` (`EXEC_TEST_THREADS=N` still sets a thread count).
- **1.8** CORR-T9 follow-up: `gen_named_function_call_with_receiver` and `resolve_call_sig_for_types` look up `fn_sigs` and `generic_fn_instances` by bare name, so the last same-named signature wins. No failing program is known. Key the lookups by symbol.

  Done on branch `corr-t13-symbol-keyed-lookups` (2026-11-17). Two failing programs found, both now execution tests. (1) Two modules that each declare a `Meter` type with `Meter::read`/`Meter::bump` and call them inside the module passed `aic check` but failed `aic build` with `E5014`/`E5012`: method calls looked up `Meter::read` by bare name, got the other module's signature, and emitted a call to the first function's LLVM name. (2) A user module `codec` with generic `encode[T]` called as `codec.encode(p)` was lowered as the std.json builtin (a qualified call skipped the user-function check), then `resolve_call_sig_for_types` found both `encode` instances at `Point` (`E5014`); with only one instance it silently ran `json.encode`. Method calls, `Type::method` calls and the json builtins now pass the checker's symbol; `callee_symbol` keeps it only when it names a function declared under the called name, and signature, generic instance, `&mut self` flag and LLVM name come from that symbol. Bare-name lookups remain only where no symbol exists, each with a comment (`for` desugaring, invariant helpers, std intrinsics, trait-bound calls in generic bodies). No diagnostic codes. Docs: `docs/llvm-backend.md`, `docs/ai-agent-implementation.md`.
- **1.9** Found in Wave 0: with Homebrew OpenSSL 4.1 the three `verify_server: true` connects in `exec_tls_local_server_handshake_and_certificate_paths` fail (the test prints 12, expects 42); CI uses OpenSSL 3.5 and passes. Fix the runtime or the test certificate, and document the supported OpenSSL versions.
  Done on branch `rel-t9-tls-openssl4`: OpenSSL 4.1 no longer matches the host name against the subject CN when a certificate has no DNS subjectAltName, and the test certificate was CN-only. The runtime now sets `X509_CHECK_FLAG_NEVER_CHECK_SUBJECT`, so host name checks use the SAN only on 3.x and 4.x alike; the test certificates carry `subjectAltName=DNS:localhost` (also the Postgres TLS fixture). New tests `exec_tls_hostname_verification_ignores_subject_cn_without_san` and `exec_tls_hostname_verification_matches_subject_alt_name`; all TLS execution tests pass against Homebrew OpenSSL 4.1.0 and 3.5.9. Supported versions are in `docs/std-api/tls.md`.

All items can run in parallel. 1.1, 1.3 and 1.4 all touch `src/typecheck.rs` and code generation, so rebase often.

Exit: the parity table is empty or each remaining row is documented as intended, overflow traps, `extern` calls carry effects, and `make examples-check` covers every example.

## Wave 2: Agent fix loop and first release

Goal: agents fix most errors in one iteration, and a tagged release exists.

| # | Item | Issue | Size | Needs |
|---|---|---|---|---|
| 2.1 | Did-you-mean suggestions | [#464] | S | – |
| 2.2 | Misleading parse errors | [#466] | S | – |
| 2.3 | `panic` and `diverge` effects | [#487] | M | – |
| 2.4 | Fixes for missing imports and non-exhaustive matches; `unreachable(msg)` | [#465] | S | 2.3 |
| 2.5 | Per-code `aic explain` text | [#467] | S | – |
| 2.6 | Std essentials: CSPRNG, CLI arguments, assertions | [#498] | M | – |
| 2.7 | Docs match the implementation | [#475] | M | 2.1 to 2.6 |
| 2.8 | First tagged release | [#476] | M | Wave 1, 2.7 |

Notes:

- **2.1** Cases seen on 2026-11-15 with no useful help: `Vec::new()` and `println(..)` (`E1218`), `n.to_string()` on an `Int` and `opt.unwrap()` (`E1228`, whose help says "define an inherent method"). A failed inference also cascades: one `Vec::new()` produced five errors.
- **2.2** After Wave 0, indexing, `as` and `+=` exist, and #450 lets statements that end in a block omit `;`. A missing `;` after an assignment already reports `E1062` on `lang-prod-roadmap`. Re-test the issue's cases first, then cover these mistakes seen on 2026-11-15: `println!(..)` and `format!(..)` (`E1030` at the `!`), `let o: Option<Int>` (`E1032`), `impl Show for Point` (`E1058`). Add the smaller findings in catalog-remaining-work section 7.
- **2.3** Moved up from Phase 5. `panic` currently needs `effects { io }`, so code that can fail cannot stay pure. `unwrap` and `expect` were left out on purpose until this lands (`docs/std-api/option-result.md`). Land the effects first. Termination checking for `total` functions needs `decreases` from [#490] and may move to a follow-up issue.
- **2.4** `unreachable(msg)` uses the `panic` effect from 2.3.
- **2.5** Generate the text from the rows of `docs/errors/catalog.md`. Also settle catalog-remaining-work sections 3 to 5:
  - self-host codes that reuse Rust codes with other meanings;
  - package rows without a covering test;
  - rows whose claims were never run.
- **2.7** Includes a doc test that compiles every `aic` code block, a grammar that is generated from or tested against the parser, and an accurate README status matrix. For example, `docs/reference/effects.md` still names the frozen self-host type checker as the implementation.
- **2.8** The release notes list the known limitations at release time: heap memory is not freed, no recursive types, no trait default methods.

2.1, 2.2, 2.3, 2.5 and 2.6 can run in parallel. Then 2.4, then 2.7, then 2.8.

## Wave 3: Memory

Design: ADR 0001 (hybrid reference counting with copy-on-write, last-use transfer, and single-owner resource handles).

| # | Item | Issue | Size | Needs |
|---|---|---|---|---|
| 3.1 | Free String, Vec, closure, struct and enum memory; leak gate | [#470] | L | Wave 0 |
| 3.2 | Moves, last-use transfer, moves of resource handles | [#471] | L | Wave 0 |
| 3.3 | `Box[T]` and recursive types | [#473] | L | 3.1 |
| 3.4 | Dereference, `RefMut` write-through, flow-sensitive borrows | [#472] | L | 3.2 |
| 3.5 | Panic recovery boundary and drop on panic | [#474] | M | 3.1 |

Notes:

- **3.1** A loop that builds a 1,000-item `Vec[String]` leaks 1,009 allocations today (`aic run --check-leaks`). Freeing must be correct without elision; elision is 3.2.
- **3.3** With multi-field variants (#454), `enum Tree { Node(Tree, Tree) }` reports `E1294` (recursive type not supported). Point that diagnostic at `Box`.
- **3.4** References never keep heap data alive and never point into reference-counted storage beyond the borrow (ADR rule 7).
- **3.5** Includes per-request recovery in `std.http_server`.

3.1 and 3.2 can run in parallel, then 3.3 and 3.4. 3.5 follows 3.1. Wave 4 items that do not need memory work can run alongside.

## Wave 4: Traits

| # | Item | Issue | Size | Needs |
|---|---|---|---|---|
| 4.1 | Core traits `Eq`, `Ord`, `Hash`, `Display`, `Debug`, `Clone` | [#481] | M | Wave 0 |
| 4.2 | Default trait method bodies | [#477] | M | Wave 0 |
| 4.3 | Supertraits | [#479] | M | Wave 0 |
| 4.4 | Generic inherent impls and blanket impls | [#480] | L | Wave 0 |
| 4.5 | Associated types and constants | [#478] | L | 4.4 recommended |
| 4.6 | `#[derive(...)]` | [#482] | M | 4.1 |
| 4.7 | Generic `Map`/`Set` keys and values | [#483] | M | 4.1 |
| 4.8 | Explicit error conversion for `?` | [#485] | S | Wave 0 |
| 4.9 | Native concurrency payloads | [#484] | M | 3.1 |

Notes:

- **4.1** Struct `==` reports `E1291` today. Interpolating a user type reports `E1204` until `Display` exists (#463).
- **4.2** Default bodies report `E1098` today.
- **4.4** `impl[T] Wrapper[T]` is a parse error today.
- **4.5** Includes reworking the iterator protocol to use an associated `Item` type.
- **4.9** Replaces the JSON copy at thread boundaries and keeps the ADR concurrency rule.

4.1, 4.2, 4.3, 4.4 and 4.8 do not depend on Wave 3. They all touch trait resolution in `src/typecheck.rs`, so run at most two at a time.

## Wave 5: Verification

| # | Item | Issue | Size | Needs |
|---|---|---|---|---|
| 5.1 | SMT contract verification with call-site `requires` | [#489] | L | Wave 0 |
| 5.2 | `old()`, quantifiers, loop invariants, `decreases` | [#490] | M | 5.1 |
| 5.3 | Contracts on trait methods | [#491] | M | Wave 0 (4.2 helps) |
| 5.4 | Refinement types and opaque newtypes | [#494] | M | 5.1 |
| 5.5 | `aic testgen` without placeholder tests | [#496] | S | 5.1 |
| 5.6 | Effect polymorphism | [#492] | L | Wave 0 |
| 5.7 | User-declared effects | [#493] | M | Wave 0 |
| 5.8 | Typestate and unforgeable handles | [#495] | L | 3.2 |
| 5.9 | Capabilities as values | [#497] | L | Wave 0 (ADR first) |

Notes:

- **5.1** `safe_div(10, 0)` against `requires b != 0` passes `aic check` today and fails only at run time. Put z3 behind a cargo feature and keep results deterministic.
- **5.2** Unblocks `total` functions from 2.3.
- **5.6** Using a function that has effects as a value reports `E1283` today.
- **5.9** Until this lands, `capabilities { }` must repeat `effects { }` (`E2009`).

5.1, 5.6, 5.7 and the 5.9 ADR can start any time after Wave 0.

## Diagnostic code blocks

Codes collided when parallel branches each took "the next free code". Each open item below owns a block. Take codes from your block in order and register only the ones you emit; unused codes stay unregistered. An item without a block, or one that runs out, takes the lowest unassigned code in the right range and adds a row to this table in its branch. A conflict on this table means two branches claimed the same codes.

Subsystem ranges are defined in `docs/diagnostic-codes.md`. The parser range `E1001` to `E1099` is full: new syntax errors use the type checker range, as #457 and #458 did. New `E5xxx` codes are allowed only as internal backstops (check/build parity policy).

| Range | Owner |
|---|---|
| `E1314`-`E1319` | [#445] parity gaps (1.1) |
| `E1320`-`E1321` | [#486] checked arithmetic (1.3) |
| `E1322`-`E1326` | [#466] parse errors (2.2) |
| `E1327`-`E1331` | [#471] moves (3.2) |
| `E1332`-`E1336` | [#472] references and borrows (3.4) |
| `E1337`-`E1339` | [#473] `Box` and recursive types (3.3) |
| `E1340`-`E1341` | [#474] panic boundary (3.5) |
| `E1342`-`E1344` | [#477] default methods (4.2) |
| `E1345`-`E1348` | [#478] associated items (4.5) |
| `E1349`-`E1350` | [#479] supertraits (4.3) |
| `E1351`-`E1354` | [#480] generic and blanket impls (4.4) |
| `E1355`-`E1357` | [#481] core traits (4.1) |
| `E1358`-`E1360` | [#482] derive (4.6) |
| `E1361`-`E1362` | [#483] generic map keys (4.7) |
| `E1363`-`E1364` | [#484] concurrency payloads (4.9) |
| `E1365`-`E1366` | [#485] error conversion (4.8) |
| `E1367`-`E1369` | [#494] refinement types and newtypes (5.4) |
| `E1370`-`E1399` | unassigned |
| `E2010`-`E2011` | [#488] `extern` effects (1.4) |
| `E2012`-`E2015` | [#487] `panic` and `diverge` effects (2.3) |
| `E2016`-`E2020` | [#492] effect polymorphism (5.6) |
| `E2021`-`E2024` | [#493] user-declared effects (5.7) |
| `E2025`-`E2029` | [#495] typestate (5.8) |
| `E2030`-`E2034` | [#497] capabilities as values (5.9) |
| `E2035`-`E2099` | unassigned |
| `E4006`-`E4009` | [#489] SMT verification (5.1) |
| `E4010`-`E4014` | [#490] contract language (5.2) |
| `E4015`-`E4017` | [#491] trait contracts (5.3) |
| `E4018`-`E4019` | [#496] testgen (5.5) |
| `E4020`-`E4099` | unassigned |
| `E0011`+, `E1111`-`E1199`, `E2127`+, `E5038`+, `E6008`+ | unassigned (lexer, resolver, packages, codegen backstops, std policy) |

## Conflict hot spots

Most items change some of these files, so rebase daily while you work on them:

- `src/typecheck.rs`, `src/parser.rs`, `src/ast.rs`, `src/ir.rs`, `src/formatter.rs`, `src/driver.rs`
- `src/codegen/generator_core.rs`, `src/codegen/mod.rs`
- `src/diagnostic_codes.rs`, `docs/diagnostic-codes.md`, `docs/errors/catalog.md`
- `docs/ai-agent-implementation.md`, `tests/execution_tests.rs`, `tests/golden_tests.rs`

## Exit criteria

The roadmap's exit criteria and the waves that meet them:

| Exit criterion | Waves |
|---|---|
| No known silent-wrong-result bugs, and `aic check` passing implies `aic build` succeeds | 0, 1 |
| Heap memory is reclaimed and long-running services run in bounded memory | 1 (channel slots), 3 |
| The core constructs of the SYNTAX, MEM and TRAIT epics are available | 0, 3, 4 |
| Contracts are checked at call sites, arithmetic is checked, and effects are polymorphic | 1, 5 |
| CI is green on Linux, macOS and Windows, docs match the implementation, and a tagged release exists | 0, 2 |

[#420]: https://github.com/keaz/aicore/issues/420
[#421]: https://github.com/keaz/aicore/issues/421
[#423]: https://github.com/keaz/aicore/issues/423
[#429]: https://github.com/keaz/aicore/issues/429
[#430]: https://github.com/keaz/aicore/issues/430
[#444]: https://github.com/keaz/aicore/issues/444
[#445]: https://github.com/keaz/aicore/issues/445
[#446]: https://github.com/keaz/aicore/issues/446
[#463]: https://github.com/keaz/aicore/issues/463
[#464]: https://github.com/keaz/aicore/issues/464
[#465]: https://github.com/keaz/aicore/issues/465
[#466]: https://github.com/keaz/aicore/issues/466
[#467]: https://github.com/keaz/aicore/issues/467
[#468]: https://github.com/keaz/aicore/issues/468
[#469]: https://github.com/keaz/aicore/issues/469
[#470]: https://github.com/keaz/aicore/issues/470
[#471]: https://github.com/keaz/aicore/issues/471
[#472]: https://github.com/keaz/aicore/issues/472
[#473]: https://github.com/keaz/aicore/issues/473
[#474]: https://github.com/keaz/aicore/issues/474
[#475]: https://github.com/keaz/aicore/issues/475
[#476]: https://github.com/keaz/aicore/issues/476
[#477]: https://github.com/keaz/aicore/issues/477
[#478]: https://github.com/keaz/aicore/issues/478
[#479]: https://github.com/keaz/aicore/issues/479
[#480]: https://github.com/keaz/aicore/issues/480
[#481]: https://github.com/keaz/aicore/issues/481
[#482]: https://github.com/keaz/aicore/issues/482
[#483]: https://github.com/keaz/aicore/issues/483
[#484]: https://github.com/keaz/aicore/issues/484
[#485]: https://github.com/keaz/aicore/issues/485
[#486]: https://github.com/keaz/aicore/issues/486
[#487]: https://github.com/keaz/aicore/issues/487
[#488]: https://github.com/keaz/aicore/issues/488
[#489]: https://github.com/keaz/aicore/issues/489
[#490]: https://github.com/keaz/aicore/issues/490
[#491]: https://github.com/keaz/aicore/issues/491
[#492]: https://github.com/keaz/aicore/issues/492
[#493]: https://github.com/keaz/aicore/issues/493
[#494]: https://github.com/keaz/aicore/issues/494
[#495]: https://github.com/keaz/aicore/issues/495
[#496]: https://github.com/keaz/aicore/issues/496
[#497]: https://github.com/keaz/aicore/issues/497
[#498]: https://github.com/keaz/aicore/issues/498
[#499]: https://github.com/keaz/aicore/issues/499
[#500]: https://github.com/keaz/aicore/issues/500
[#501]: https://github.com/keaz/aicore/issues/501
[#502]: https://github.com/keaz/aicore/issues/502
[#503]: https://github.com/keaz/aicore/issues/503
