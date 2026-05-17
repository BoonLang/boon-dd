# Phase 0 Honest Compiler Blockers

Date: 2026-05-17

Status: blocked

## Summary

Phase 0 cannot be called complete on the current checkout. The current review
found execution-path shortcuts that are not covered by the old forbidden names
alone. The guardrails were updated to catch the renamed runtime/codegen
evaluator surfaces, and the gates now fail deterministically.

## Blocking Commands

```bash
cargo xtask verify-no-shortcuts --format json
cargo xtask verify-dd-purity --format json
cargo xtask verify-dd-stateful-lowering --format json
```

Observed failure artifacts:

```text
target/boon-artifacts/no-shortcuts-report.json
target/boon-artifacts/engine-simplicity/dd-purity-report.json
target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
```

## Current Blockers

- Runtime execution still uses `fold_literal` and `fold_call_literal` in
  `crates/boon_runtime_host/src/lib.rs`.
- Runtime execution still lowers render/text/bool behavior through generic
  Rust closures in `lower_render_text_collection` and `lower_call_stream`.
- Codegen still uses `FoldedRender`, `fold_graph_value`, and
  `fold_call_value` in `crates/boon_codegen_rust/src/lib.rs`.
- Those paths implement Boon text/list/record/bool/match/library behavior in
  Rust instead of proving that the accepted Boon semantics lower into typed
  Timely/Differential graph operators.
- Generated render output can still be pre-folded into
  `render_events.clone().map(|_| ...)`.
- Unknown source paths can still fall through as raw source ids, and dynamic
  generation is not part of the current source-routing predicate.

## Verification Rule

The blocker is resolved only when:

- `cargo xtask verify-no-shortcuts --format json` reports zero shortcut hits.
- `cargo xtask verify-dd-purity --format json` passes.
- `cargo xtask verify-dd-stateful-lowering --format json` passes.
- `cargo xtask verify all --format json` passes on the current checkout and
  writes a successful `success.json`.
- Any browser/native verification that creates a window is launched through
  `cosmic-background-launch --workspace boon-dd -- ...`.

Until then, success artifacts from earlier runs must be treated as stale or
invalid for the honest-compiler goal.
