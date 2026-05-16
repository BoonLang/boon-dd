# Phase 0 Honest Compiler Blockers

Date: 2026-05-17

Plan path: `BOON_DD_HONEST_COMPILER_PLAN.md`

## Summary

The Phase 0 command surface exists, but the honest compiler aggregate cannot
pass while the engine-simplicity hard gates are exposing runtime execution
shortcuts. This report is intentionally aligned with
`docs/blockers/engine-simplicity-runtime-semantics-blocker.md`: prompt-audit
hashes must not be refreshed to `pass` until the underlying runtime/generated
execution paths are clean.

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
verify-honest-compiler
verify-prompt-audit
verify-dd-purity
verify-source-routing
verify-dynamic-owner-routing
verify-output-drain-efficiency
verify-dd-stateful-lowering
verify-engine-complexity
verify-engine-prompt-audit
verify-engine-simplicity
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

`cargo xtask verify-prompt-audit --format json` currently fails with stale
honest-compiler prompt audit files:

```text
verdict: fail
audits_required: 7
audits_passed: 0
audit_json_files_found: 7
critical_findings_open: 0
hash_mismatches: 14
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
target/boon-artifacts/engine-simplicity/dd-purity-report.json
target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
target/boon-artifacts/engine-simplicity/source-routing-report.json
target/boon-artifacts/engine-simplicity/dynamic-owner-routing-report.json
target/boon-artifacts/engine-simplicity/output-drain-efficiency-report.json
target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
```

Current key artifact hashes:

```text
ab0b6817ab827bc59518c789fbf57315c48c4ad461db8d8a81e0fd74f3d3eebe  target/boon-artifacts/verify-report.json
ac52512639c27502e50e4353238359621737838bfebf57850151342161984de7  target/boon-artifacts/success.json
f9318fd4c13c7dbdbfd72d9c1fe1cd42a7eb1c6a2636619552e33efb2dc33954  target/boon-artifacts/honest-compiler-report.json
021677f203f7985cf82da70490ffcf0e9397620437bfabfb7b209a4504466a0b  target/boon-artifacts/engine-simplicity/dd-purity-report.json
f9cac11423cbd19d0d080ec340b0dbb938b4be3aab39da930c2cb8a38d797b0d  target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
d534b8fffd94364cf997c65aad215dbdbdcf7d9a3dfbb3206242cbbbfd4fa658  target/boon-artifacts/engine-simplicity/source-routing-report.json
5a028b0d4ea5f2e44e059714ef61f25650397b544b89f1e92deb7b11087f7a45  target/boon-artifacts/engine-simplicity/dynamic-owner-routing-report.json
366eebcda67e072f930b2b168c9b8661df612c5b67f07b7216fb9215e5e59885  target/boon-artifacts/engine-simplicity/output-drain-efficiency-report.json
0078198bccf075ae46b1bdd1715118ce8de47f6628315ec09d9ec23f0fde2403  target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
```

## Passing Evidence

These commands still pass in the current checkout:

```bash
cargo xtask verify-honesty-deterministic --format json
cargo xtask verify-language-corpus --format json
cargo xtask verify-no-shortcuts --format json
cargo xtask verify-generated-freshness --format json
cargo xtask verify-generated-crates --format json
```

These passes are not sufficient to declare success because the hardened
engine-simplicity gates now catch runtime-host semantics, generated typed
host-side list/record semantics, source-routing gaps, dynamic owner/generation
identity gaps, and full-output drain paths.

## Minimized Repro

```bash
cargo xtask verify-honest-compiler --format json
cargo xtask verify-prompt-audit --format json
cargo xtask verify-dd-purity --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-source-routing --format json
cargo xtask verify-dynamic-owner-routing --format json
cargo xtask verify-output-drain-efficiency --format json
cargo xtask verify all --format json
```

Expected current behavior: these commands fail or report blocked until the
runtime/generated execution defects in
`docs/blockers/engine-simplicity-runtime-semantics-blocker.md` are fixed.

## Next Decision

Do not change `verify-honest-compiler` to pass independently of prompt audit,
and do not refresh prompt-audit hashes as a substitute for fixing the engine.
The next implementation pass should remove the runtime-host evaluator and
generated typed host-side list/record semantics, then rerun the hard gates
before rerunning prompt audits.

Any browser/native GUI verification that opens windows must keep wrapping the
actual window-creating process with:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```
