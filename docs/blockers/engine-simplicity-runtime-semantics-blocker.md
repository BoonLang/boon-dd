# Engine Simplicity Runtime Semantics Blocker

Date: 2026-05-17

Status: blocked

## Summary

The current checkout is not yet an honest pure Timely/Differential Dataflow
compiler/runtime. A review of the uncommitted runtime and codegen rewrite found
that the old host/codegen semantic evaluator was mostly renamed, not removed.
The execution path still folds Boon render graph nodes and library calls through
Rust control flow before or inside generic Timely/Differential operators.

This report is checked in so engine-simplicity and honest-compiler verification
cannot be treated as complete while these shortcuts remain.

## Exact Failing Commands

```bash
cargo xtask verify-no-shortcuts --format json
```

Observed output:

```text
Error: shortcut execution patterns are still present; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/no-shortcuts-report.json
```

Artifact:

```text
target/boon-artifacts/no-shortcuts-report.json
```

Current result: `verdict = fail`, `shortcut_symbols_in_execution_paths = 138`.
First reported hit: `crates/boon_codegen_rust/src/lib.rs:500`, pattern
`FoldedRender`.

```bash
cargo xtask verify-dd-purity --format json
```

Observed output:

```text
Error: engine-simplicity gate cargo xtask verify-dd-purity --format json reported fail; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/dd-purity-report.json
```

Artifact:

```text
target/boon-artifacts/engine-simplicity/dd-purity-report.json
```

Current result: `verdict = blocked`, `hit_count = 147`.

```bash
cargo xtask verify-dd-stateful-lowering --format json
```

Observed output:

```text
Error: engine-simplicity gate cargo xtask verify-dd-stateful-lowering --format json reported fail; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
```

Artifact:

```text
target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
```

Current result: `verdict = blocked`, `hit_count = 145`.

```bash
cargo xtask verify-source-routing --format json
```

Observed output:

```text
Error: engine-simplicity gate cargo xtask verify-source-routing --format json reported fail; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/source-routing-report.json
```

Artifact:

```text
target/boon-artifacts/engine-simplicity/source-routing-report.json
```

Current result: `verdict = fail`, `hit_count = 23`.

```bash
cargo xtask verify-dynamic-owner-routing --format json
```

Observed output:

```text
Error: engine-simplicity gate cargo xtask verify-dynamic-owner-routing --format json reported fail; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/dynamic-owner-routing-report.json
```

Artifact:

```text
target/boon-artifacts/engine-simplicity/dynamic-owner-routing-report.json
```

Current result: `verdict = fail`, `hit_count = 24`.

## Concrete Evidence

- `crates/boon_runtime_host/src/lib.rs:635` calls `fold_literal(...)`
  from the runtime lowering path.
- `crates/boon_runtime_host/src/lib.rs:844` defines `fold_literal`, a
  recursive evaluator for DD render graph nodes.
- `crates/boon_runtime_host/src/lib.rs:967` defines `fold_call_literal`,
  including Boon library semantics for `Text/*`, `Bool/not`, `List/*`,
  `Temperature/c_to_f`, and `Element/*`.
- `crates/boon_runtime_host/src/lib.rs:705` defines `lower_call_stream`,
  which keeps text/bool/math behavior in runtime-host Rust closures.
- `crates/boon_codegen_rust/src/lib.rs:500` defines `FoldedRender`, a generic
  folded render value enum.
- `crates/boon_codegen_rust/src/lib.rs:610` defines `fold_graph_value`, a
  recursive codegen-time evaluator for DD render graph nodes.
- `crates/boon_codegen_rust/src/lib.rs:744` defines `fold_call_value`, a
  Rust implementation of Boon library calls used by generated output.
- `crates/boon_codegen_rust/src/lib.rs:940` can lower a fully folded render
  graph into `render_events.clone().map(|_| ...)`, hiding static or pre-folded
  Boon output behind a DD map.
- `crates/boon_codegen_rust/src/lib.rs:123` preserves unknown source paths as
  raw source ids through `other => other.to_owned()` instead of rejecting them.
- `crates/boon_codegen_rust/src/lib.rs:249` and
  `crates/boon_runtime_host/src/lib.rs:1221` bind dynamic generation into a
  tuple but only test source family plus non-empty owner, so generation is not
  part of the routing predicate.

## Minimized Repro

The issue can be reproduced without a GUI:

```bash
cargo xtask verify-no-shortcuts --format json
jq '.shortcut_symbols_in_execution_paths, .scan.hits[:5]' \
  target/boon-artifacts/no-shortcuts-report.json
```

Expected honest result: zero hits.

Current result: nonzero hits in `boon_codegen_rust` and `boon_runtime_host`.

## Required Fix Decision

Next pin/fork/fix decision: fix in this repository before claiming pass.

Required implementation direction:

1. Remove runtime-host render graph semantic folding from the execution path.
2. Generate typed Timely/Differential dataflow for render, text, list, record,
   match, hold, latest, persistence, effects, dynamic owners, and command output.
3. Keep host code limited to compile, instantiate, submit typed events, advance
   time, drain bounded output diffs, and render host UI shells.
4. Replace `FoldedRender`, `fold_graph_value`, and `fold_call_value` with typed
   DD graph IR lowering or non-execution-path diagnostics.
5. Reject unknown source paths and preserve source family, owner, and generation
   through DD keys and output attribution.
6. Keep the shortcut scans for the renamed evaluator surfaces so future
   rename-only fixes fail deterministically.

Do not mark `cargo xtask verify all --format json` as successful until this
blocker is resolved and the referenced reports pass with zero shortcut hits.
