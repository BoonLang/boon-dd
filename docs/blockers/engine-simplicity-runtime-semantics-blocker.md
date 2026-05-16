# Engine Simplicity Runtime Semantics Blocker

Date: 2026-05-17

Plan path: `BOON_DD_ENGINE_SIMPLICITY_PLAN.md`

## Summary

The engine-simplicity goal is blocked by real execution-path defects, not by
prompt-audit paperwork. The previous prompt-only blocker was stale: after a
fresh audit, the current checkout still executes Boon semantics in
`boon_runtime_host`, generated Rust still carries typed host-side list/record
semantics, source routing still has broad unbound-event triggers, dynamic owner
and generation identity are still not DD keys, and output APIs still drain all
retained outputs before selecting the last value.

Prompt audits must not be marked `pass` until these deterministic blockers are
fixed or a checked-in product decision removes the affected language surface
from the accepted manifest.

## Checkpoint Git State

Current HEAD:

```text
6e8d46dfc7c6a8023ed88995acf447a192dd1121
```

Dirty status at the refreshed aggregate run:

```text
 D docs/blockers/engine-simplicity-prompt-audit-blocker.md
 M docs/blockers/phase0-honest-compiler-blockers.md
M xtask/src/main.rs
?? docs/blockers/engine-simplicity-runtime-semantics-blocker.md
```

The dirty `xtask` change hardens the verifier so it detects the hidden runtime
and generated-semantics defects named below. The dirty blocker docs replace the
stale prompt-only blocker with the real runtime/generated-semantics blocker.

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

Current `target/boon-artifacts/success.json` summary:

```text
success: false
engine_simplicity.verdict: blocked
engine_simplicity.dd_purity: blocked
engine_simplicity.source_routing_wrong_id_failures: 23
engine_simplicity.dynamic_owner_leaks: 50
engine_simplicity.full_output_vector_clones_in_execution_paths: 94
engine_simplicity.stateful_lowering_shortcuts: 369
engine_simplicity.stale_artifacts: 0
engine_simplicity.fixture_dispatch_paths: 0
engine_simplicity.runtime_graph_builds_per_interaction_session: 1
engine_simplicity.prompt_audit_verdict: blocked
prompt_audit_report.verdict: fail
prompt_audit_report.audits_required: 7
prompt_audit_report.audits_passed: 0
prompt_audit_report.hash_mismatches: 14
```

## Current Artifact Paths

```text
target/boon-artifacts/verify-report.json
target/boon-artifacts/success.json
target/boon-artifacts/honest-compiler-report.json
target/boon-artifacts/prompt-audit-report.json
target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
target/boon-artifacts/engine-simplicity/dd-purity-report.json
target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
target/boon-artifacts/engine-simplicity/source-routing-report.json
target/boon-artifacts/engine-simplicity/dynamic-owner-routing-report.json
target/boon-artifacts/engine-simplicity/output-drain-efficiency-report.json
target/boon-artifacts/engine-simplicity/prompt-audit-report.json
```

Current artifact hashes:

```text
ab0b6817ab827bc59518c789fbf57315c48c4ad461db8d8a81e0fd74f3d3eebe  target/boon-artifacts/verify-report.json
ac52512639c27502e50e4353238359621737838bfebf57850151342161984de7  target/boon-artifacts/success.json
f9318fd4c13c7dbdbfd72d9c1fe1cd42a7eb1c6a2636619552e33efb2dc33954  target/boon-artifacts/honest-compiler-report.json
021677f203f7985cf82da70490ffcf0e9397620437bfabfb7b209a4504466a0b  target/boon-artifacts/engine-simplicity/dd-purity-report.json
f9cac11423cbd19d0d080ec340b0dbb938b4be3aab39da930c2cb8a38d797b0d  target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
d534b8fffd94364cf997c65aad215dbdbdcf7d9a3dfbb3206242cbbbfd4fa658  target/boon-artifacts/engine-simplicity/source-routing-report.json
5a028b0d4ea5f2e44e059714ef61f25650397b544b89f1e92deb7b11087f7a45  target/boon-artifacts/engine-simplicity/dynamic-owner-routing-report.json
366eebcda67e072f930b2b168c9b8661df612c5b67f07b7216fb9215e5e59885  target/boon-artifacts/engine-simplicity/output-drain-efficiency-report.json
c3afff5b8a98583785e2c597efd8fb8098cbe66de8e932af23bb08665030bee6  target/boon-artifacts/engine-simplicity/prompt-audit-report.json
0078198bccf075ae46b1bdd1715118ce8de47f6628315ec09d9ec23f0fde2403  target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
```

## Concrete Evidence

`cargo xtask verify-dd-purity --format json` is blocked with 166 hits:

```text
host_runtime_semantics: 147
generated_semantics: 19
```

Representative hits:

```text
crates/boon_backend_app_window/src/lib.rs:6
boon_runtime_host::run_compiled_source_scenario(source_path, source_text, scenario_text).ok()

crates/boon_runtime_host/src/lib.rs:562
enum ConstValue {

crates/boon_runtime_host/src/lib.rs:622
fn runtime_collection<'scope>(

crates/boon_runtime_host/src/lib.rs:843
fn const_value(
```

`cargo xtask verify-dd-stateful-lowering --format json` is blocked with 369 hits:

```text
host_semantics: 291
host_list_semantics: 68
host_record_semantics: 10
```

Representative hits:

```text
crates/boon_codegen_rust/src/lib.rs:143
generated_payload_text(payload)

crates/boon_codegen_rust/src/lib.rs:1000
"std::collections::BTreeMap::<String, String>::from([{}])",

crates/boon_codegen_rust/src/lib.rs:1451
"({}).into_iter().map(|item_value| {}).collect::<Vec<_>>()",

generated/list_map_block/src/graph.rs:358
.into_iter()
```

`cargo xtask verify-source-routing --format json` fails with 23 hits. The
representative defect is the source-unbound fallback:

```text
crates/boon_codegen_rust/src/lib.rs:385
let rendered_owners = if generated_bound_source_ids().is_empty()

generated/list_map_block/src/graph.rs:371
let rendered_owners = if generated_bound_source_ids().is_empty()
```

This allows any injected event to drive output for graphs with no declared
source binding instead of requiring an explicit host tick/source key.

`cargo xtask verify-dynamic-owner-routing --format json` fails with 50 hits.
Representative hits:

```text
crates/boon_codegen_rust/src/lib.rs:241
let _owner_key_is_part_of_dd_identity = owner_key;

crates/boon_codegen_rust/src/lib.rs:242
let _generation_is_part_of_dd_identity = generation;

crates/boon_runtime_host/src/lib.rs:1214
GeneratedSourceEvent::Dynamic { family_id, .. } =>
```

`cargo xtask verify-output-drain-efficiency --format json` fails with 94 hits.
Representative hits:

```text
crates/boon_codegen_rust/src/lib.rs:67
pub fn take_outputs(&mut self) -> Vec<SmokeOutput>

crates/boon_runtime_host/src/lib.rs:488
Ok(outputs.drain(..).last().unwrap_or_else(|| SmokeOutput {

generated/counter/src/lib.rs:78
let mut drained_outputs = graph.sources.take_outputs();
```

## Minimized Repro

```bash
cargo xtask verify-dd-purity --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-source-routing --format json
cargo xtask verify-dynamic-owner-routing --format json
cargo xtask verify-output-drain-efficiency --format json
cargo xtask verify-engine-simplicity --format json
cargo xtask verify all --format json
```

Expected current behavior: all commands above fail or report blocked until the
runtime-host evaluator, generated typed host-side list/record semantics,
unbound event trigger, dynamic owner/generation parking, and full-output drain
paths are removed or replaced with real Timely/Differential graph state and
incremental output APIs.

## Why This Is A Real Blocker

The plan requires runtime hosts to inject facts and drain/render outputs without
executing Boon behavior. The current backend/browser/native paths still call
`boon_runtime_host::run_compiled_source_scenario`, and `boon_runtime_host`
contains a generic `ConstValue` evaluator for DD render graph operations and
library calls. That is an execution-path fallback interpreter.

The generated graph path is also not yet pure DD for list/record/text semantics:
it uses Rust vectors, maps, iterators, field lookup, text conversions, and
length/count operations inside generated closures. That is better than the old
`DdValue` path, but it does not satisfy Phase 4's requirement that stateful and
list semantics lower to explicit Timely/Differential state, keys, arrangements,
reductions, joins, or equivalent DD operators.

## Next Fix Decision

Do not refresh prompt-audit hashes or write pass audit JSONs yet. The next
implementation pass should:

1. Replace backend/browser/native `run_compiled_source_scenario` execution with
   generated graph factories or a non-interpreting DD graph builder.
2. Remove `ConstValue`, `const_value`, `const_call_value`,
   `runtime_collection`, `runtime_payload_text`, and `runtime_boon_text` from
   runtime execution paths.
3. Replace generated `Vec`/`BTreeMap` list and record semantics with typed DD
   collections keyed by source id, owner, generation, item identity, and time.
4. Make unbound constant graph output depend on an explicit host tick fact
   rather than any event.
5. Preserve owner and generation identity in DD keys, not unused variables.
6. Replace `take_outputs() -> Vec<SmokeOutput>` and drain-all-then-last paths
   with incremental output cursors or bounded diff drains.
7. Rerun the minimized repro, then rerun the prompt audits only after the hard
   gates pass.

Any browser/native GUI verification that opens windows must keep wrapping the
actual window-creating process with:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```
