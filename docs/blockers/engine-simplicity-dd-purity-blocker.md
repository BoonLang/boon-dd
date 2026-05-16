# Engine Simplicity DD Purity Blocker

Date: 2026-05-16

Plan path: `BOON_DD_ENGINE_SIMPLICITY_PLAN.md`

## Summary

The engine-simplicity goal is blocked because the current implementation still
has generic Rust value evaluators on execution paths. This violates the plan's
non-negotiable requirement that terminal, native, and browser hosts inject
facts and drain/render outputs without executing Boon semantics, and that
stateful Boon semantics lower to explicit Timely/Differential graph state.

This is not a prompt-audit paperwork gap. A prompt-audit pass would be false
until these generic evaluators are removed or replaced with typed DD lowerings.

## Current Git State

Current HEAD:

```text
62d07d446b1f9926381678e3a8912e0665fb4945
```

Current dirty status at blocker creation:

```text
M generated/*/{monitor_snapshot.json,terminal_120x40.snapshot.txt,native_render_1280x720.json,browser_render_1280x720.json}
M generated/*/src/graph.rs
M crates/boon_codegen_rust/src/lib.rs
M crates/boon_runtime_host/src/lib.rs
M xtask/src/main.rs
```

The generated artifact changes are the freshness fix required after
`cargo xtask write-generated-artifacts --format json`. `crates/boon_codegen_rust`
and `crates/boon_runtime_host` include a narrow persistence/reload alignment
fix found while checking generated-crate tests. `xtask/src/main.rs` is dirty
because the verifier was tightened to stop missing the generic semantic
evaluator paths.

## Failing Command

```bash
cargo xtask verify-dd-stateful-lowering --format json
```

Exact output:

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.91s
Running `target/debug/xtask verify-dd-stateful-lowering --format json`
Error: engine-simplicity gate cargo xtask verify-dd-stateful-lowering --format json reported fail; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
```

Current report summary:

```text
verdict: fail
blockers: stateful Boon semantics are not proven to lower to keyed Timely/Differential state
failure count: 330
```

Relevant artifact:

```text
target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
sha256: 2c8725e3ecab98db14039290e28d86dbad2055318cfbc83d8c4e515be67d703a
```

Related passing artifacts from the same checkout:

```text
target/boon-artifacts/generated-freshness-report.json
sha256: 7e28684e2efe6a38db8bf578f2e587cb297a3c8d897410d191ffd2d680ae8d23

target/boon-artifacts/engine-simplicity/stress-report.json
sha256: 0e6bbd4fc1ca4de372b17ddf861407f26fdcbe4f266730467eb52d3718051070
```

## Additional Failing Proofs

After regenerating artifacts from the current checkout, generated freshness
passes, but graph execution still exposes semantic gaps.

```bash
cargo xtask verify-generated-freshness --format json
```

Result: pass.

```bash
cargo xtask verify-generated-crates --format json
```

Current failing output:

```text
Running `target/debug/xtask verify-generated-crates --format json`
tests::generated_graph_matches_checked_scenario_output --- FAILED

thread 'tests::generated_graph_matches_checked_scenario_output' panicked at generated/list_map_block/src/lib.rs:81:18:
generated graph emitted no scenario output

Error: generated crate list_map_block test failed: exit status: 101
```

```bash
cargo xtask test --target terminal
```

Current failing output:

```text
Error: terminal scenario interval output mismatch
expected: {"effects":[{"Requested":{"name":"Timer/interval","node":"EffectSink"}}],"monitor":[{"NodeValue":{"epoch":1,"node":"Counter","owner":"Root","value_preview":"1"}}],"persistence":[],"render":[{"PatchText":{"node":"DocumentText","text":"1"}}]}
actual: {"effects":[],"monitor":[{"NodeValue":{"epoch":1,"node":"Counter","owner":"Root","value_preview":"1"}}],"persistence":[],"render":[{"PatchText":{"node":"DocumentText","text":"1"}}]}
```

The aggregate command was also run:

```bash
cargo xtask verify all --format json
```

Result: fail. The written `target/boon-artifacts/success.json` reports
`success: false` and includes the engine-simplicity blocked gates. The earlier
aggregate run also exposed generated-crate and target mismatches; because the
checkout changed afterward to fix the narrow counter_hold persistence/reload
path, the individual commands above are the current minimized repros.

## Minimized Repro

Run:

```bash
cargo xtask verify-dd-stateful-lowering --format json
jq '{verdict, blockers, hit_count:.evidence.hit_count, first_failures:.failures[0:8]}' target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
```

Representative deterministic hits:

```text
crates/boon_runtime_host/src/lib.rs:16
enum RuntimeValue {

crates/boon_runtime_host/src/lib.rs:532
fn runtime_value(...)

crates/boon_runtime_host/src/lib.rs:643
fn runtime_call_value(...)

crates/boon_codegen_rust/src/lib.rs:10
use boon_dd::{..., DdValue, ...}

crates/boon_codegen_rust/src/lib.rs:347
({input}).map(|_event| ()).count().map(|(_key, count)| DdValue::Number(count as i64))

crates/boon_codegen_rust/src/lib.rs:961
match input { DdValue::List(values) => DdValue::List(values.into_iter().map(...).collect()), ... }
```

## Why This Is A Real Blocker

The current runtime can build a long-lived Timely worker and the generated
artifact freshness/stress gates can pass, but the semantic path is still too
generic:

- `crates/boon_runtime_host/src/lib.rs` contains `RuntimeValue`,
  `runtime_value`, and `runtime_call_value`, so a runtime host can recursively
  evaluate DD render graph nodes and Boon library calls as Rust values.
- `crates/boon_codegen_rust/src/lib.rs` emits `DdValue` based list, text,
  record, number, boolean, and match semantics directly into generated Rust
  expressions.
- List operations such as map, retain, count, sort, and sum still operate over
  `Vec<DdValue>` in Rust expressions instead of keyed DD collections.
- HOLD/count paths still include global count-style lowerings that are not
  sufficient for keyed state, dynamic owners, or generation-aware semantics.
- Effect commands such as `Timer/interval` are present in expected outputs but
  are not emitted by the current runtime-host path.

That means a green prompt audit or final `verify all` would not honestly prove
the plan. The hard gate must remain failing or blocked until this path is
rewritten.

## Next Pin/Fork/Fix Decision

Fix in this repo. No external dependency pin or fork decision is needed yet.

The next implementation decision is the internal lowering boundary:

1. Remove `RuntimeValue` and recursive `runtime_value`/`runtime_call_value`
   from execution paths. Runtime hosts should instantiate a compiled graph
   factory and submit typed source facts only.
2. Replace shared `DdValue` expression lowering with typed DD node lowerings
   for each accepted Boon operation. Static literals may become typed constant
   collections, but dynamic semantics must be expressed as collections,
   arrangements, reductions, joins, or generated typed operators with declared
   source/owner/generation keys.
3. Lower list map/retain/count/latest, HOLD, LATEST, text input state, timers,
   and command acknowledgements as keyed DD state. Do not use host `Vec`
   operations as the execution path for list semantics.
4. Add negative samples that mutate source ids, owner keys, generations, and
   stale generated artifacts and prove the relevant gates fail.
5. Rerun `cargo xtask write-generated-artifacts --format json`, then
   `cargo xtask verify all --format json`.

## Smallest Safe Next Step

Start with a narrow counter plus two-source routing slice:

- introduce a typed value/lowering plan in the compiler IR for source id,
  owner key, generation, number, text, and render text patch output;
- make `boon_runtime_host` execute only that typed graph path without
  `RuntimeValue`;
- update `verify-dd-stateful-lowering` so the counter/two-source slice must
  pass and the remaining unsupported operations fail compilation with structured
  diagnostics rather than falling back to `DdValue`;
- only then broaden the typed lowering table to list/text/library operations.
