# Phase 0 Honest Compiler Blockers

Date: 2026-05-17

Status: resolved

## Summary

Phase 0 shortcut and purity guardrails now pass on the current checkout. The
runtime-host semantic executor is no longer used as a fallback executor, and the
deterministic scenario protocol now executes generated Timely/Differential graph
outputs for all accepted examples.

The broader `/goal` is no longer blocked by this report. Parser simplification
against `/home/martinkavik/repos/boon` is resolved for the current accepted
syntax surface, engine prompt-audit freshness now passes, and the Ply renderer
import plus old renderer removal are covered by passing `verify-ply-renderer`
and `verify-playgrounds` artifacts.

## Passing Commands

```bash
cargo xtask verify-no-shortcuts --format json
cargo xtask verify-dd-purity --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-source-routing --format json
cargo xtask verify-dynamic-owner-routing --format json
cargo xtask verify-honesty-deterministic --format json
cargo xtask verify-ply-renderer --format json
cargo xtask verify-playgrounds --format json
```

Observed artifacts:

```text
target/boon-artifacts/no-shortcuts-report.json
target/boon-artifacts/engine-simplicity/dd-purity-report.json
target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
target/boon-artifacts/engine-simplicity/source-routing-report.json
target/boon-artifacts/engine-simplicity/dynamic-owner-routing-report.json
target/boon-artifacts/honesty-deterministic-report.json
target/boon-artifacts/ply/verify-ply-renderer.json
target/boon-artifacts/verify-playgrounds.json
```

Current result: all listed Phase 0 and renderer gates report pass with zero
shortcut/DD-purity/stateful hits. `verify-engine-simplicity` also reports pass
with a current engine prompt-audit artifact.

## Blocking Commands

The former blocker was represented by these aggregate gates, which now pass:

```bash
cargo xtask verify-engine-prompt-audit --format json
cargo xtask verify-engine-simplicity --format json
cargo xtask verify-pure-dd-ply-milestones --format json
```

Observed blocker artifacts:

```text
target/boon-artifacts/pure-dd-ply/parser-decision.json
target/boon-artifacts/pure-dd-ply/milestone-verification.json
target/boon-artifacts/pure-dd-ply/ply-merge-report.json
target/boon-artifacts/pure-dd-ply/simplicity-report.json
target/boon-artifacts/engine-simplicity/prompt-audit-report.json
docs/blockers/parser-simplification-blocker.md
```

## Current Blockers

None for this report.

## Verification Rule

The blocker is resolved only when:

- The legacy renderer scan is clean or only finds explicitly non-render wrapper
  code.
- Parser decision is `pass` with measured evidence, not `deferred`.
- Engine prompt-audit report is `pass` with current prompt hashes, current repo
  state, current deterministic evidence, and zero open critical findings.
- `cargo fmt --check` passes.
- `cargo check --workspace` passes.
- `cargo test --workspace` passes.
- `cargo xtask verify all --format json` passes on the current checkout and
  writes a successful `success.json`.
- Any browser/native verification that creates a window is launched through
  `cosmic-background-launch --workspace boon-dd -- ...`.

Until then, the current work should be treated as substantial progress, not
full goal completion.
