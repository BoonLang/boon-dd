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

## Checkpoint Git State

Parent HEAD for this checkpoint:

```text
6fac85ef6ab8f07cd01cc36cbcc69b737d9701f6
```

Changes captured by this checkpoint:

```text
M docs/blockers/engine-simplicity-dd-purity-blocker.md
M examples/crud/scenario.toml
M examples/flight_booker/scenario.toml
M examples/list_object_state/expected.render.json
M examples/list_retain_remove/expected.render.json
M examples/list_retain_remove/scenario.toml
M generated/crud/src/lib.rs
M generated/flight_booker/src/lib.rs
M generated/list_object_state/monitor_snapshot.json
M generated/list_object_state/src/lib.rs
M generated/list_retain_remove/monitor_snapshot.json
M generated/list_retain_remove/src/lib.rs
M xtask/src/main.rs
```

These changes are fixture/source-id and expected-owner corrections
found after tightening source routing. `crud`, `flight_booker`, and
`list_retain_remove` now inject the compiler-resolved source ids instead of
shorthand path fragments, and owner-preserving examples now expect `item-1`
instead of flattening monitor output to `Root`. The generated files were
refreshed with `cargo xtask write-generated-artifacts --format json`.
`xtask/src/main.rs` also tightens `verify-no-fixture-dispatch` so it scans the
checked generated registry in `crates/boon_examples` and `xtask`, instead of
reporting a false zero while fixture-index execution remains. The aggregate
honesty deterministic gate now also catches the checked generated registry as a
`generated-only-runtime` violation.

## Failing Command

```bash
cargo xtask verify-dd-stateful-lowering --format json
```

Exact output:

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s
Running `target/debug/xtask verify-dd-stateful-lowering --format json`
Error: engine-simplicity gate cargo xtask verify-dd-stateful-lowering --format json reported blocked; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
```

Current report summary:

```text
verdict: blocked
blockers: stateful lowering is blocked by a checked-in engine-simplicity blocker report
failure count: 354
```

Relevant artifact:

```text
target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
sha256: 453b800e79173abf9aaabfceb5a66dd70dade527e7c77c21db47c4208bdc8cf5

target/boon-artifacts/engine-simplicity/dd-purity-report.json
sha256: b1e2ff5d5f96e7e4648106fac907589afbc99f617a1c992eb1838cb193f67f87

target/boon-artifacts/engine-simplicity/no-fixture-dispatch-report.json
sha256: ce9f23e581e3e2fac5f2b8fcd2424c31a950fe9238c6bab614e0cf55099185c5

target/boon-artifacts/engine-simplicity/complexity-report.json
sha256: 0527cc5b17ea6bf6a19010baa36ad0964857a2f7ff6469b3afa1f333e5f6c388

target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
sha256: d7f31753ff994005f68e88f9c3ff6e71a1659e3da6e3ede9f77e715b068c436e
```

Related passing artifacts from the same checkout:

```text
target/boon-artifacts/generated-freshness-report.json
sha256: cc13484cbbaffeaf33f4ffd3aeb9998321922a5dabba9b4a46fee8d179ec0d00

target/boon-artifacts/generated-crates.json
sha256: 1b1538abac12170b00ff2dcb1c1c22ed4f6809fb6f224d2e318409044723c9fc

target/boon-artifacts/engine-simplicity/prompt-audit-report.json
sha256: 08e3cebe541ccec8b6bef523b693ecc8974665cc85f9f83504df3337e1cb4db9

target/boon-artifacts/honest-compiler-report.json
sha256: fbcbd69b3c17abbbedf800b96a5854437b66b16867494372db9e2c2997d32f9a

target/boon-artifacts/honesty-deterministic-report.json
sha256: 563e312e233a575cbee802eb67d37eff8d931ac2e1d6a9d77a1986b776d4576d

target/boon-artifacts/prompt-audit-report.json
sha256: 5a9c32a30917da12e100f24bb9816e91af8b94a1f3b26a1088dfa938542376b4

target/boon-artifacts/verify-report.json
sha256: 6606ff9f6c05f255ca85be50cf96805d00488744ff53049d8057cbe6812dfba4

target/boon-artifacts/success.json
sha256: f30753c71e917d7020690290c270439efcb710505f582bc06df3b8a7f15a965d
```

## Concrete Gates Rechecked

After regenerating artifacts from the current checkout, the concrete terminal
and generated-crate mismatches that originally appeared while tightening source
routing have been corrected. These are no longer the blocker.

```bash
cargo xtask verify-generated-freshness --format json
```

Result: pass.

```bash
cargo xtask verify-generated-crates --format json
```

Result: pass.

```bash
cargo xtask test --target terminal
```

Result: pass.

The remaining deterministic failing or blocked commands are the purity,
fixture-dispatch, stateful-lowering, complexity, prompt-audit, and aggregate
commands:

```bash
cargo xtask verify-dd-purity --format json
cargo xtask verify-no-fixture-dispatch --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-engine-complexity --format json
cargo xtask verify-engine-prompt-audit --format json
cargo xtask verify all --format json
```

Representative output:

```text
Error: engine-simplicity gate cargo xtask verify-dd-purity --format json reported blocked; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/dd-purity-report.json
Error: engine-simplicity gate cargo xtask verify-no-fixture-dispatch --format json reported blocked; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/no-fixture-dispatch-report.json
Error: engine-simplicity gate cargo xtask verify-dd-stateful-lowering --format json reported blocked; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
Error: engine-simplicity gate cargo xtask verify-engine-complexity --format json reported fail; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/complexity-report.json
Error: engine-simplicity gate cargo xtask verify-engine-prompt-audit --format json reported blocked; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/prompt-audit-report.json
```

`cargo xtask verify all --format json` must remain non-passing until these
blocked gates are replaced by real typed DD execution evidence. The latest
aggregate run writes `success: false` and reports these failed gates:

```text
verify-honest-compiler
verify-honesty-deterministic
verify-prompt-audit
verify-dd-purity
verify-no-fixture-dispatch
verify-dd-stateful-lowering
verify-engine-complexity
verify-engine-prompt-audit
verify-engine-simplicity
```

The latest aggregate `success.json` engine summary is also explicitly nonzero
for fixture dispatch:

```text
engine_simplicity.fixture_dispatch_paths: 44
honesty_deterministic.missing_deterministic_gates: ["generated-only-runtime"]
engine_simplicity.stateful_lowering_shortcuts: 354
engine_simplicity.verdict: blocked
```

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
- `crates/boon_examples/src/lib.rs` still contains `GENERATED_CORPUS` and
  `run_generated_steps_at`, so checked generated crates can still be selected by
  fixture index instead of by a general compiled graph artifact.
- `crates/boon_codegen_rust/src/lib.rs` emits `DdValue` based list, text,
  record, number, boolean, and match semantics directly into generated Rust
  expressions.
- List operations such as map, retain, count, sort, and sum still operate over
  `Vec<DdValue>` in Rust expressions instead of keyed DD collections.
- HOLD/count paths still include global count-style lowerings that are not
  sufficient for keyed state, dynamic owners, or generation-aware semantics.
- Prompt-audit verification is correctly blocked because a prompt pass would
  only certify deterministic evidence after the hard DD purity gates pass.

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
