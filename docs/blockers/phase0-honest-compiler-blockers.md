# Phase 0 Honest Compiler Blockers

Date: 2026-05-17

Plan path: `BOON_DD_HONEST_COMPILER_PLAN.md`

## Summary

The Phase 0 command surface exists, but the honest compiler aggregate cannot
pass yet. The current checkout has made concrete engine progress, but the full
`cargo xtask verify all --format json` contract is still blocked by:

- engine DD purity failures,
- engine stateful-lowering failures,
- intentionally blocked prompt audit,
- honest-compiler aggregate failure,
- playground artifact schema mismatch,
- browser WASM smoke artifact wiring,
- incomplete plan coverage.

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
target-browser
plan-coverage
verify-honest-compiler
verify-prompt-audit
verify-dd-purity
verify-dd-stateful-lowering
verify-engine-prompt-audit
verify-engine-simplicity
```

Current aggregate failure details:

```text
verify-playgrounds: playground artifact missing example_count
target-browser: missing browser WASM smoke artifact; run verify-wasm-dd first: No such file or directory (os error 2)
plan-coverage: plan coverage is incomplete; see target/boon-artifacts/plan-coverage.json
verify-honest-compiler: honest compiler is not implemented yet; see target/boon-artifacts/honest-compiler-report.json
verify-prompt-audit: prompt audit is incomplete; see target/boon-artifacts/prompt-audit-report.json
verify-dd-purity: engine-simplicity DD purity blocked
verify-dd-stateful-lowering: engine-simplicity stateful lowering blocked
verify-engine-prompt-audit: engine-simplicity prompt audit blocked
verify-engine-simplicity: engine-simplicity aggregate blocked
```

`cargo xtask verify-honest-compiler --format json` currently fails with:

```text
Error: honest compiler is not implemented yet; see target/boon-artifacts/honest-compiler-report.json
```

The current `target/boon-artifacts/honest-compiler-report.json` summary is:

```text
verdict: fail
blockers: prompt-audit verification is not passing
```

`cargo xtask verify-prompt-audit --format json` currently fails with:

```text
verdict: fail
audits_required: 7
audits_passed: 0
critical_findings_open: 0
hash_mismatches: 14
blockers: prompt audit outputs are missing, stale, inconclusive, failing, or schema-invalid
```

Those audits must remain non-accepted until the engine hard-gate failures are
fixed and the audits are rerun on the current checkout.

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
target/boon-artifacts/plan-coverage.json
target/boon-artifacts/verify-playgrounds.json
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
0dba1ca5d37fabd5af0df679b56de48d4f9b042d269c8ae3bda3ba48bea3b544  target/boon-artifacts/verify-report.json
af22601dab22538d24f4f4c14ee5179a70fce0d5f19b135809ca5187b7e9b604  target/boon-artifacts/success.json
f978c8758b5ff1e20f91feaf916f82fab2b5844042e1b2f6a1180f62a3b49d68  target/boon-artifacts/honest-compiler-report.json
3b9f3af498e1f017342993889c6709e8ada1245c6e20fd1f2e76c19c3dcd6667  target/boon-artifacts/engine-simplicity/dd-purity-report.json
dcfdaae16be096d097d8352ed65a5ce8f8634b7affa017c93ef4843d46b89578  target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
5b6b6ad44e63f30404b2856b175b174c11180a27666d977b68f8014caa8f7f2b  target/boon-artifacts/engine-simplicity/source-routing-report.json
d05e74dd92ebb091163c5e4d07e15755b7dba06e4031245606ee2dad924e6b44  target/boon-artifacts/engine-simplicity/dynamic-owner-routing-report.json
8745a92012b6d52bb3121bd3ae433e8867189b6fa7b60b3d515d43d225dedb71  target/boon-artifacts/engine-simplicity/output-drain-efficiency-report.json
39f4dae86ab3a2c849190c2fb25aff0eeba031c5b1b57328728e0a418c103de2  target/boon-artifacts/engine-simplicity/prompt-audit-report.json
3fd696b4fe15a6806ceddb920ea3a79c9c7ff2e08813a6f0389507dcac817b68  target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
```

## Passing Evidence

These commands pass in the current checkout:

```bash
cargo xtask verify-honesty-deterministic --format json
cargo xtask verify-language-corpus --format json
cargo xtask verify-no-shortcuts --format json
cargo xtask verify-generated-freshness --format json
cargo xtask verify-generated-crates --format json
cargo xtask verify-no-fixture-dispatch --format json
cargo xtask verify-source-routing --format json
cargo xtask verify-dynamic-owner-routing --format json
cargo xtask verify-persistent-runtime --format json
cargo xtask verify-output-drain-efficiency --format json
cargo xtask verify-engine-complexity --format json
```

These passes are not sufficient to declare success because the full aggregate
still fails and engine DD purity/stateful-lowering are blocked.

## Minimized Repro

```bash
cargo xtask verify-honest-compiler --format json
cargo xtask verify-prompt-audit --format json
cargo xtask verify-dd-purity --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-engine-simplicity --format json
cargo xtask verify-playgrounds --format json
cargo xtask test --target browser
cargo xtask verify all --format json
```

Expected current behavior: these commands fail or report blocked until the
runtime/generated execution defects in
`docs/blockers/engine-simplicity-runtime-semantics-blocker.md` are fixed and
the aggregate artifact contracts are brought into sync.

## Next Decision

Do not change `verify-honest-compiler` to pass independently of prompt audit,
and do not refresh prompt-audit hashes as a substitute for fixing the engine.
The next implementation pass should remove the runtime-host evaluator and
generated typed host-side list/record/text semantics, then repair the remaining
aggregate artifact wiring before rerunning prompt audits.

Any browser/native GUI verification that opens windows must keep wrapping the
actual window-creating process with:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```
