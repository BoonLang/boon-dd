# Engine Simplicity Runtime Semantics Blocker

Date: 2026-05-17

Plan path: `BOON_DD_ENGINE_SIMPLICITY_PLAN.md`

## Summary

The engine-simplicity goal is still blocked. This checkpoint narrowed the
generated/codegen blocker surface by removing generic generated source payload
text/list/record coercions from generated execution paths, regenerating all
example graphs, and keeping generated artifacts fresh.

The current hard blockers are:

- `boon_runtime_host` still contains host-side Boon render/const semantics.
- backend/browser/native/xtask paths still depend on
  `boon_runtime_host::run_compiled_source_scenario`.
- `boon_codegen_rust` still contains stateful/list semantic patterns in
  compile-time folding and non-folded fallback boundaries that the verifier
  correctly refuses to accept as clean execution-path proof.
- prompt audit remains blocked until deterministic DD purity and stateful
  lowering pass.

This is not a success state. Do not mark the `/goal` complete until
`cargo xtask verify all --format json` passes on the current checkout.

## Failing Command

```bash
cargo xtask verify all --format json
```

Exact terminal result:

```text
Error: verification failed
```

The refreshed aggregate report records these failed gates:

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

Current `target/boon-artifacts/success.json` engine-simplicity summary:

```text
success: false
engine_simplicity.verdict: blocked
engine_simplicity.dd_purity: blocked
engine_simplicity.stateful_lowering_shortcuts: 147
engine_simplicity.dynamic_owner_leaks: 0
engine_simplicity.fixture_dispatch_paths: 0
engine_simplicity.full_output_vector_clones_in_execution_paths: 0
engine_simplicity.runtime_graph_builds_per_interaction_session: 1
engine_simplicity.source_routing_wrong_id_failures: 0
engine_simplicity.stale_artifacts: 0
engine_simplicity.prompt_audit_verdict: blocked
```

## Current Artifact Paths

```text
target/boon-artifacts/verify-report.json
target/boon-artifacts/success.json
target/boon-artifacts/honest-compiler-report.json
target/boon-artifacts/honesty-deterministic-report.json
target/boon-artifacts/prompt-audit-report.json
target/boon-artifacts/verify-playgrounds.json
target/boon-artifacts/native-playground.json
target/boon-artifacts/browser-playground-result.json
target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
target/boon-artifacts/engine-simplicity/dd-purity-report.json
target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
target/boon-artifacts/engine-simplicity/source-routing-report.json
target/boon-artifacts/engine-simplicity/dynamic-owner-routing-report.json
target/boon-artifacts/engine-simplicity/output-drain-efficiency-report.json
target/boon-artifacts/engine-simplicity/no-fixture-dispatch-report.json
target/boon-artifacts/engine-simplicity/persistent-runtime-report.json
target/boon-artifacts/engine-simplicity/prompt-audit-report.json
```

Current key artifact hashes:

```text
04d6a9ded7c90a16ed5a40ff91161c132b8146c96aeebe1a07cccbe32f7be75b  target/boon-artifacts/verify-report.json
ed5d5ebb64c894f21bbd72b20a2fe3e83fa1b36c0523825fd0578f0c8baae658  target/boon-artifacts/success.json
213090a04ad065fd62a26011d480c9cd6e7482d6550182fd9a0b47b5167fa89b  target/boon-artifacts/honest-compiler-report.json
9d4f2f3f97009f512cca5b9089969fbd4367c8d161b7fb5e6e6b2a716db8f0d3  target/boon-artifacts/engine-simplicity/dd-purity-report.json
3bee12a436c17e2d3a35d082c6519ea67ee0e96c3188794aa12a6c1c9f08bc40  target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
bcbdd4c4d8e41f7eef757ca8d94360399ef57ed72de7f2226fd9c1d8074321a4  target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
b2418920756b4bf1573250b3ca36bc187dd7f48c3ab21235184d83d539f855f6  target/boon-artifacts/engine-simplicity/prompt-audit-report.json
```

## Concrete Evidence

`cargo xtask verify-dd-purity --format json` is blocked with 148 failures:

```text
host_runtime_semantics: 148
```

This is an improvement from the previous report: generated semantic purity
failures are currently gone, but the runtime-host semantic evaluator remains.

`cargo xtask verify-dd-stateful-lowering --format json` is blocked with 147
failures:

```text
host_semantics: 131
host_list_semantics: 16
```

This is down from 369 in the previous blocker report. The remaining hits must
be resolved by removing execution-path host semantics, moving any legitimate
compile-time-only static folding behind an auditable non-runtime boundary, and
replacing dynamic aggregate behavior with typed Differential Dataflow
operators.

The aggregate `verify-playgrounds` gate currently fails with:

```text
native playground did not write parseable JSON target/boon-artifacts/native-playground.json within 45s
```

The artifact exists after the aggregate run, so this should be treated as an
aggregate timeout/artifact timing bug, not as proof that the native playground
is absent.

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
verify-no-shortcuts
verify-language-corpus
verify-no-fixture-dispatch
verify-source-routing
verify-dynamic-owner-routing
verify-persistent-runtime
verify-output-drain-efficiency
verify-engine-stress
verify-engine-complexity
generated-crates
```

These passes are not sufficient to declare success because DD purity,
stateful-lowering, prompt audit, and the aggregate still fail.

## Minimized Repro

```bash
cargo xtask verify-dd-purity --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-engine-prompt-audit --format json
cargo xtask verify-engine-simplicity --format json
cargo xtask verify-playgrounds --format json
cargo xtask verify all --format json
```

Expected current behavior: these commands fail or report blocked until the
runtime-host evaluator, stateful generated/codegen semantic patterns, prompt
audit acceptance, and aggregate native-playground timing bug are fixed.

## Next Decision

Do not refresh prompt-audit hashes or relax verifier patterns as a substitute
for the missing implementation. The next implementation pass should:

1. Remove backend/browser/native execution dependence on
   `boon_runtime_host::run_compiled_source_scenario`.
2. Delete or quarantine the `boon_runtime_host` semantic evaluator so runtime
   hosts only inject facts and drain generated graph output.
3. Replace dynamic list/record/text/count/map/retain/sort behavior with typed
   Timely/Differential Dataflow collections and operators.
4. Put any accepted compile-time-only static folding behind a clearly
   non-runtime, non-execution-path module boundary, with verifier coverage that
   proves it cannot execute as Boon semantics at runtime.
5. Fix the aggregate `verify-playgrounds` native artifact timing failure.
6. Rerun generated artifact writing, deterministic gates, prompt audits, and
   the full aggregate.

Any browser/native GUI verification that opens windows must keep wrapping the
actual window-creating process with:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```
