# Phase 0 Honest Compiler Blockers

Date: 2026-05-17

Plan path: `BOON_DD_HONEST_COMPILER_PLAN.md`

## Summary

The Phase 0 command surface exists, but the honest compiler aggregate cannot
pass yet. This checkpoint improved generated-code purity and kept generated
artifacts fresh, but the full contract is still blocked by:

- engine DD purity failures,
- engine stateful-lowering failures,
- deterministic honesty failure caused by native playground artifact timing,
- prompt-audit acceptance/hash mismatch failures,
- honest-compiler aggregate failure,
- engine-simplicity aggregate failure.

This is not a success state. Do not mark the `/goal` complete until
`cargo xtask verify all --format json` passes and writes
`target/boon-artifacts/success.json` with `success: true`.

## Failing Command

```bash
cargo xtask verify all --format json
```

Exact terminal result:

```text
Error: verification failed
```

Current failed aggregate gates:

```text
verify-playgrounds
verify-honest-compiler
verify-honesty-deterministic
verify-prompt-audit
verify-dd-purity
verify-dd-stateful-lowering
verify-engine-prompt-audit
verify-engine-simplicity
```

Current aggregate failure details:

```text
verify-playgrounds: native playground did not write parseable JSON target/boon-artifacts/native-playground.json within 45s
verify-honest-compiler: honest compiler is not implemented yet; see target/boon-artifacts/honest-compiler-report.json
verify-honesty-deterministic: deterministic honesty verification is not complete; see target/boon-artifacts/honesty-deterministic-report.json
verify-prompt-audit: prompt audit is incomplete; see target/boon-artifacts/prompt-audit-report.json
verify-dd-purity: engine-simplicity DD purity blocked
verify-dd-stateful-lowering: engine-simplicity stateful lowering blocked
verify-engine-prompt-audit: engine-simplicity prompt audit blocked
verify-engine-simplicity: engine-simplicity aggregate blocked
```

`target/boon-artifacts/honest-compiler-report.json` currently says:

```text
verdict: fail
blockers:
- deterministic honesty verification is not passing
- honest compiler prompt pack could not be refreshed
- prompt-audit verification is not passing
```

`target/boon-artifacts/prompt-audit-report.json` currently says:

```text
verdict: fail
audits_required: 7
audits_passed: 0
critical_findings_open: 0
hash_mismatches: 14
blockers:
- prompt audit outputs are missing, stale, inconclusive, failing, or schema-invalid
```

All seven prompt-audit JSON files report `verdict: pass`, but none are accepted
because their repo-state and deterministic-report hashes are stale for the
current checkout. Refreshing those hashes is not allowed as a substitute for
fixing the DD purity and stateful-lowering blockers first.

## Current Artifact Paths

```text
target/boon-artifacts/verify-report.json
target/boon-artifacts/success.json
target/boon-artifacts/honest-compiler-report.json
target/boon-artifacts/honesty-deterministic-report.json
target/boon-artifacts/language-corpus-report.json
target/boon-artifacts/no-shortcuts-report.json
target/boon-artifacts/prompt-audit-report.json
target/boon-artifacts/generated-crates.json
target/boon-artifacts/generated-freshness-report.json
target/boon-artifacts/verify-playgrounds.json
target/boon-artifacts/native-playground.json
target/boon-artifacts/browser-playground-result.json
target/boon-artifacts/engine-simplicity/dd-purity-report.json
target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
target/boon-artifacts/engine-simplicity/source-routing-report.json
target/boon-artifacts/engine-simplicity/dynamic-owner-routing-report.json
target/boon-artifacts/engine-simplicity/output-drain-efficiency-report.json
target/boon-artifacts/engine-simplicity/no-fixture-dispatch-report.json
target/boon-artifacts/engine-simplicity/persistent-runtime-report.json
target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
```

Current key artifact hashes:

```text
04d6a9ded7c90a16ed5a40ff91161c132b8146c96aeebe1a07cccbe32f7be75b  target/boon-artifacts/verify-report.json
ed5d5ebb64c894f21bbd72b20a2fe3e83fa1b36c0523825fd0578f0c8baae658  target/boon-artifacts/success.json
213090a04ad065fd62a26011d480c9cd6e7482d6550182fd9a0b47b5167fa89b  target/boon-artifacts/honest-compiler-report.json
9d4f2f3f97009f512cca5b9089969fbd4367c8d161b7fb5e6e6b2a716db8f0d3  target/boon-artifacts/engine-simplicity/dd-purity-report.json
3bee12a436c17e2d3a35d082c6519ea67ee0e96c3188794aa12a6c1c9f08bc40  target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
bcbdd4c4d8e41f7eef757ca8d94360399ef57ed72de7f2226fd9c1d8074321a4  target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
```

## Passing Evidence

These commands passed during this checkpoint:

```bash
cargo fmt -p boon_codegen_rust
cargo check -p boon_codegen_rust
cargo xtask write-generated-artifacts --format json
cargo xtask verify-generated-crates --format json
cargo xtask verify-generated-freshness --format json
```

The aggregate also reports these relevant gates as passed:

```text
verify-deps
verify-generated-freshness
verify-wasm-dd
target-browser
plan-coverage
verify-syntax-corpus
parser-completeness
verify-resolver-corpus
verify-shape-corpus
verify-semantic-ir
semantic-ir-coverage
verify-no-shortcuts
verify-language-corpus
verify-negative-corpus
verify-lowering
compare-engines
verify-no-fixture-dispatch
verify-source-routing
verify-dynamic-owner-routing
verify-persistent-runtime
verify-output-drain-efficiency
verify-engine-stress
verify-engine-complexity
generated-crates
```

These passes are not sufficient to declare success because the full aggregate
still fails and engine DD purity/stateful-lowering are blocked.

## Minimized Repro

```bash
cargo xtask verify-honest-compiler --format json
cargo xtask verify-honesty-deterministic --format json
cargo xtask verify-prompt-audit --format json
cargo xtask verify-dd-purity --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-engine-simplicity --format json
cargo xtask verify-playgrounds --format json
cargo xtask verify all --format json
```

Expected current behavior: these commands fail or report blocked until the
runtime/generated execution defects in
`docs/blockers/engine-simplicity-runtime-semantics-blocker.md`, prompt-audit
acceptance, and native-playground aggregate timing are fixed.

## Next Decision

Do not change `verify-honest-compiler` to pass independently of deterministic
honesty and prompt audit, and do not refresh prompt-audit hashes as a substitute
for fixing the engine. The next implementation pass should remove the
runtime-host evaluator, complete typed DD lowering for dynamic aggregate
semantics, quarantine or replace compile-time-only static folding so it cannot
be mistaken for runtime Boon semantics, fix the native-playground aggregate
timing issue, and then rerun prompt audits on the fixed checkout.

Any browser/native GUI verification that opens windows must keep wrapping the
actual window-creating process with:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```
