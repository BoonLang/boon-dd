# Phase 0 Honest Compiler Blockers

Date: 2026-05-17

Plan path: `BOON_DD_HONEST_COMPILER_PLAN.md`

## Summary

The Phase 0 command surface and deterministic reports exist, and most
deterministic honesty gates now pass. The aggregate honest-compiler gate is
still blocked because prompt-audit verification is not passing for the current
checkout. This report supersedes the older language-corpus and deterministic
honesty blocker text: those gates now pass in the current artifacts.

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

`cargo xtask verify-prompt-audit --format json` currently fails with:

```text
Error: prompt audit is incomplete; see target/boon-artifacts/prompt-audit-report.json
```

The current `target/boon-artifacts/prompt-audit-report.json` summary is:

```text
verdict: fail
audits_required: 7
audits_passed: 0
audit_json_files_found: 7
missing_required: []
critical_findings_open: 0
hash_mismatches: 14
schema_errors: []
blockers: prompt audit outputs are missing, stale, inconclusive, failing, or schema-invalid
```

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
```

Current key artifact hashes:

```text
8b95b221d36a1384d265101b5cb8a145658a69c520b47d0888766d1f35158610  target/boon-artifacts/verify-report.json
788247d6ccf006117cbed6837962ca667e1c962d2b23301f468bf1f6b2908367  target/boon-artifacts/success.json
2dfe6c4bbcd48ffb56a0340e00cb315c56fcdfcb2b754960f170fd771186dbae  target/boon-artifacts/honest-compiler-report.json
e583a1fb9902504cc2dbd1bfc73dbd7003705a9f9a656930fcb3b5f69ad731f6  target/boon-artifacts/prompt-audit-report.json
```

## Passing Evidence

These commands now pass in the current checkout:

```bash
cargo xtask verify-honesty-deterministic --format json
cargo xtask verify-language-corpus --format json
cargo xtask verify-no-shortcuts --format json
cargo xtask verify-generated-freshness --format json
cargo xtask verify-generated-crates --format json
```

Current deterministic honesty summary:

```text
verdict: pass
missing_deterministic_gates: []
accepted_features_without_full_coverage: 0
shortcut_symbols_in_execution_paths: 0
stale_artifact_failures: 0
host_semantics_violations: 0
adversarial_heuristic_cases_failed: 0
```

Current language-corpus summary:

```text
verdict: pass
language_status: accepted
blockers: []
structural_errors: false
```

## Minimized Repro

```bash
cargo xtask verify-honest-compiler --format json
cargo xtask verify-prompt-audit --format json
cargo xtask verify all --format json
```

Expected current behavior: these commands fail until prompt-audit verification
accepts all seven required audits for the current prompt hashes, deterministic
report hash, and repo-state hash.

## Next Decision

Do not change `verify-honest-compiler` to pass independently of prompt audit.
The next implementation pass should make the prompt-audit workflow deterministic
and current, then rerun:

```bash
cargo xtask verify-prompt-audit --format json
cargo xtask verify-honest-compiler --format json
cargo xtask verify all --format json
```

Any browser/native GUI verification that opens windows must keep wrapping the
actual window-creating process with:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```
