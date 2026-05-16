# Engine Simplicity Prompt Audit Blocker

Date: 2026-05-17

Plan path: `BOON_DD_ENGINE_SIMPLICITY_PLAN.md`

## Summary

The engine-simplicity implementation is currently blocked only by prompt-audit
acceptance/freshness. The deterministic engine gates that previously blocked on
DD purity, stateful lowering, source routing, dynamic owner routing, generated
freshness, generated-crate replay, fixture dispatch, output drain efficiency,
stress, complexity, and Firefox proof now pass in the current checkout.

This is still not a success state. `cargo xtask verify all --format json`
continues to write `success: false` because prompt audit reports are stale or
not accepted, and the honest compiler aggregate gate depends on that prompt
audit verdict.

## Checkpoint Git State

Current HEAD before this blocker update:

```text
716f3980a4d36fe569d91cb56b4073782d162a74
```

Current dirty status at the time of the failing aggregate run:

```text
M crates/boon_codegen_rust/src/lib.rs
M generated/cells/src/graph.rs
M generated/counter/src/graph.rs
M generated/counter_hold/src/graph.rs
M generated/crud/src/graph.rs
M generated/flight_booker/src/graph.rs
M generated/interval/src/graph.rs
M generated/interval_hold/src/graph.rs
M generated/latest/src/graph.rs
M generated/list_map_block/src/graph.rs
M generated/list_map_external_dep/src/graph.rs
M generated/list_object_state/src/graph.rs
M generated/list_retain_count/src/graph.rs
M generated/list_retain_reactive/src/graph.rs
M generated/list_retain_remove/src/graph.rs
M generated/pong/src/graph.rs
M generated/shopping_list/src/graph.rs
M generated/temperature_converter/src/graph.rs
M generated/then/src/graph.rs
M generated/todo_mvc/src/graph.rs
M generated/todo_mvc_physical/src/graph.rs
M generated/when/src/graph.rs
M generated/while/src/graph.rs
```

## Failing Command

```bash
cargo xtask verify all --format json
```

Exact terminal result:

```text
Error: verification failed
```

The refreshed `target/boon-artifacts/verify-report.json` records exactly these
failed gates:

```text
verify-honest-compiler
verify-prompt-audit
verify-engine-prompt-audit
verify-engine-simplicity
```

The gate errors are:

```text
verify-honest-compiler: honest compiler is not implemented yet; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/honest-compiler-report.json
verify-prompt-audit: prompt audit is incomplete; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/prompt-audit-report.json
verify-engine-prompt-audit: engine-simplicity gate cargo xtask verify-engine-prompt-audit --format json reported blocked; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/prompt-audit-report.json
verify-engine-simplicity: engine-simplicity gate cargo xtask verify-engine-simplicity --format json reported blocked; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
```

## Current Artifact Paths

```text
target/boon-artifacts/verify-report.json
target/boon-artifacts/success.json
target/boon-artifacts/honest-compiler-report.json
target/boon-artifacts/prompt-audit-report.json
target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
target/boon-artifacts/engine-simplicity/prompt-audit-report.json
```

Current artifact hashes:

```text
8b95b221d36a1384d265101b5cb8a145658a69c520b47d0888766d1f35158610  target/boon-artifacts/verify-report.json
788247d6ccf006117cbed6837962ca667e1c962d2b23301f468bf1f6b2908367  target/boon-artifacts/success.json
2dfe6c4bbcd48ffb56a0340e00cb315c56fcdfcb2b754960f170fd771186dbae  target/boon-artifacts/honest-compiler-report.json
e583a1fb9902504cc2dbd1bfc73dbd7003705a9f9a656930fcb3b5f69ad731f6  target/boon-artifacts/prompt-audit-report.json
d7080059df0405daca505cec92ae7f5586137bc262a94c138be3c2af219f7eeb  target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
e33dd5aad1eb9705bffcbd0af243eba52b0bd356c9a78197ac0cd7c3c2a3876f  target/boon-artifacts/engine-simplicity/prompt-audit-report.json
```

## Passing Evidence

These commands were rerun after the typed generated-code owner routing update:

```bash
cargo test -p boon_codegen_rust
cargo xtask write-generated-artifacts --format json
cargo xtask verify-generated-crates --format json
cargo xtask verify-dd-purity --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-source-routing --format json
cargo xtask verify-dynamic-owner-routing --format json
cargo xtask verify-generated-freshness --format json
```

Current passing artifact hashes:

```text
1b1538abac12170b00ff2dcb1c1c22ed4f6809fb6f224d2e318409044723c9fc  target/boon-artifacts/generated-crates.json
a728d52842f077475aa86e78296ba6d5483d6460306b06229afa4a00878a5307  target/boon-artifacts/generated-freshness-report.json
25f48dcd2d798310ee554f59f65c2538ccf353207f50ca794e92905d649faefd  target/boon-artifacts/engine-simplicity/dd-purity-report.json
2620bfe463ebdf16fd22639c8ef4b4decae0375017d14d6946f39321cca23438  target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
```

`target/boon-artifacts/success.json` currently reports:

```text
success: false
engine_simplicity.verdict: blocked
engine_simplicity.dd_purity: pass
engine_simplicity.source_routing_wrong_id_failures: 0
engine_simplicity.dynamic_owner_leaks: 0
engine_simplicity.stale_artifacts: 0
engine_simplicity.fixture_dispatch_paths: 0
engine_simplicity.full_output_vector_clones_in_execution_paths: 0
engine_simplicity.stateful_lowering_shortcuts: 0
engine_simplicity.prompt_audit_verdict: blocked
prompt_audit_report.verdict: fail
prompt_audit_report.audits_required: 7
prompt_audit_report.audits_passed: 0
prompt_audit_report.hash_mismatches: 14
prompt_audit_report.critical_findings_open: 0
```

## Minimized Repro

```bash
cargo xtask verify-prompt-audit --format json
cargo xtask verify-engine-prompt-audit --format json
cargo xtask verify-engine-simplicity --format json
cargo xtask verify all --format json
```

Expected current behavior: all four commands fail or report blocked until the
prompt-audit artifacts are regenerated for the current deterministic report and
current repo-state hash, accepted by the verifier schema, and no longer stale.

## Next Decision

Do not weaken the gate. The next implementation pass should regenerate or
replace the prompt-audit artifacts so every required audit has the current
prompt hash, current deterministic report hash, current repo-state hash, an
accepted verdict, and zero open critical findings. Then rerun:

```bash
cargo xtask verify-prompt-audit --format json
cargo xtask verify-engine-prompt-audit --format json
cargo xtask verify-engine-simplicity --format json
cargo xtask verify all --format json
```

Any browser/native GUI verification that opens windows must keep wrapping the
actual window-creating process with:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```
