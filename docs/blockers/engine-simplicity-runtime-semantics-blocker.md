# Engine Simplicity Runtime Semantics Blocker

Date: 2026-05-17

Plan path: `BOON_DD_ENGINE_SIMPLICITY_PLAN.md`

## Summary

The engine-simplicity goal is still blocked, but the current blocker surface is
narrower than the previous report. Source routing, dynamic owner routing,
output drain efficiency, generated freshness, generated crates, persistent
runtime reuse, fixture-dispatch checks, cross-engine comparison, and engine
complexity now pass in the current checkout.

The remaining engine blockers are real runtime/generated-semantics blockers:

- `boon_runtime_host` still contains a runtime-host render evaluator and const
  evaluator (`runtime_render_collection`, `ConstValue`, `const_value`,
  `runtime_payload_text`, `runtime_boon_text`).
- backend/browser/native/xtask paths still call
  `boon_runtime_host::run_compiled_source_scenario`.
- generated Rust still lowers list, record, payload-text, payload-bool, count,
  map, retain, sort, and text operations through host Rust helpers/collections
  instead of typed Differential Dataflow operators.
- prompt audit must stay blocked until the deterministic DD purity and
  stateful-lowering gates pass.

This is not a success state. Do not mark the `/goal` complete until the full
aggregate verifier passes on a clean current checkout.

## Checkpoint Git State

Current HEAD at the refreshed aggregate run:

```text
f00fd5c02e60e967b82248fd72a573ddbfa4ce94
```

Dirty status at the refreshed aggregate run:

```text
 M crates/boon_codegen_rust/src/lib.rs
 M crates/boon_runtime_host/src/lib.rs
 M examples/list_object_state/expected.render.json
 M generated/*/src/graph.rs
 M generated/*/src/lib.rs
 M generated/list_object_state/monitor_snapshot.json
 M xtask/src/main.rs
```

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
target-browser
plan-coverage
verify-honest-compiler
verify-prompt-audit
verify-dd-purity
verify-dd-stateful-lowering
verify-engine-prompt-audit
verify-engine-simplicity
```

Current `target/boon-artifacts/success.json` summary:

```text
success: false
engine_simplicity.verdict: blocked
engine_simplicity.dd_purity: blocked
engine_simplicity.stateful_lowering_shortcuts: 369
engine_simplicity.dynamic_owner_leaks: 0
engine_simplicity.fixture_dispatch_paths: 0
engine_simplicity.full_output_vector_clones_in_execution_paths: 0
engine_simplicity.runtime_graph_builds_per_interaction_session: 1
engine_simplicity.source_routing_wrong_id_failures: 0
engine_simplicity.stale_artifacts: 0
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
target/boon-artifacts/plan-coverage.json
target/boon-artifacts/verify-playgrounds.json
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

## Concrete Evidence

`cargo xtask verify-dd-purity --format json` is blocked with 167 failures:

```text
generated_semantics: 19
host_runtime_semantics: 148
```

Representative hits:

```text
crates/boon_backend_app_window/src/lib.rs:6
boon_runtime_host::run_compiled_source_scenario(source_path, source_text, scenario_text).ok()

crates/boon_runtime_host/src/lib.rs:519
fn runtime_render_collection<'scope>(

crates/boon_runtime_host/src/lib.rs:564
enum ConstValue {

crates/boon_codegen_rust/src/lib.rs:433
"({}).into_iter().map(|item_value| {}).collect::<Vec<_>>().join(\",\")",

crates/boon_codegen_rust/src/lib.rs:1003
"std::collections::BTreeMap::<String, String>::from([{}])",
```

`cargo xtask verify-dd-stateful-lowering --format json` is blocked with 369
failures:

```text
host_semantics: 291
host_list_semantics: 68
host_record_semantics: 10
```

Representative hits:

```text
crates/boon_codegen_rust/src/lib.rs:143
generated_payload_text(payload)

crates/boon_codegen_rust/src/lib.rs:196
Some(payload) => format!("{}({})", name.0, boon_dd::value_to_text(payload)),

crates/boon_codegen_rust/src/lib.rs:462
RenderKind::List(_) => format!("({}).len() as i64", self.code),

crates/boon_codegen_rust/src/lib.rs:1482
"({}).into_iter().filter(|item_value| { let item_value = (*item_value).clone(); {} }).collect::<Vec<_>>()",

crates/boon_codegen_rust/src/lib.rs:1509
"{ let mut values = {}; values.sort_by_key(|item_value| { let item_value = item_value.clone(); {} }); values }",
```

## Passing Evidence

The following engine-simplicity gates pass in the current checkout:

```bash
cargo xtask verify-no-fixture-dispatch --format json
cargo xtask verify-source-routing --format json
cargo xtask verify-dynamic-owner-routing --format json
cargo xtask verify-persistent-runtime --format json
cargo xtask verify-output-drain-efficiency --format json
cargo xtask verify-generated-freshness --format json
cargo xtask verify-generated-crates --format json
cargo xtask compare-engines --format json
cargo xtask verify-engine-complexity --format json
```

The full aggregate still fails because the purity/stateful-lowering blockers
above remain, prompt audit is intentionally blocked, and non-engine aggregate
contracts still report missing or malformed artifacts.

## Minimized Repro

```bash
cargo xtask verify-dd-purity --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-engine-prompt-audit --format json
cargo xtask verify-engine-simplicity --format json
cargo xtask verify all --format json
```

Expected current behavior: these commands fail or report blocked until the
runtime-host evaluator and generated typed host-side list/record/text
semantics are removed from execution paths and replaced with real typed
Timely/Differential Dataflow graph state.

## Next Decision

Do not refresh prompt-audit hashes or relax verifier patterns as a substitute
for the missing implementation. The next implementation pass should:

1. Remove backend/browser/native execution dependence on
   `boon_runtime_host::run_compiled_source_scenario`.
2. Delete or quarantine the `boon_runtime_host` semantic evaluator so runtime
   hosts only inject facts and drain generated graph output.
3. Replace generated list/record/text/count/map/retain/sort lowering with typed
   DD collections/operators.
4. Rerun generated artifact writing and all deterministic gates.
5. Only then rerun prompt audits and require deterministic `pass` verdicts.

Any browser/native GUI verification that opens windows must keep wrapping the
actual window-creating process with:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```
