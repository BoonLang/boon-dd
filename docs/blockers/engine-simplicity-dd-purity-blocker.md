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

HEAD before this checkpoint commit:

```text
7974ef7af3faa7a0495dd1190617781a9a7fee32
```

Prepared checkout changes:

```text
M Cargo.lock
M crates/boon_examples/Cargo.toml
M crates/boon_examples/src/lib.rs
M crates/boon_runtime_host/src/lib.rs
M crates/boon_wasm_smoke/Cargo.toml
M crates/boon_wasm_smoke/src/lib.rs
M docs/blockers/engine-simplicity-dd-purity-blocker.md
M generated/*/Cargo.toml
M xtask/Cargo.toml
M xtask/src/main.rs
```

The prepared changes remove the checked generated fixture registry from
`crates/boon_examples`, remove xtask's dependency on that registry, remove the
browser/WASM generated-crate fixture matrix, and route deterministic scenario
proof through compiled graph sessions for all manifest examples. The checked
generated crates now include a local empty `[workspace]` table so
`cargo xtask verify-generated-crates --format json` can run each generated
crate by manifest path without inheriting the parent workspace.

After tightening the scan to include `crates/boon_wasm_smoke`, fixture dispatch
now passes with zero hits. Deterministic honesty verification also passes.
The DD purity blocker remains: `boon_runtime_host` still uses `RuntimeValue`,
and `boon_codegen_rust` plus the checked generated crates still emit `DdValue`
semantics.

## Failing Command

```bash
cargo xtask verify all --format json
```

Exact output:

```text
Error: verification failed
```

The refreshed `target/boon-artifacts/verify-report.json` records the exact
failed gates and failing subcommands:

```text
verify-honest-compiler: cargo xtask verify-honest-compiler --format json
verify-prompt-audit: cargo xtask verify-prompt-audit --format json
verify-dd-purity: cargo xtask verify-dd-purity --format json
verify-dd-stateful-lowering: cargo xtask verify-dd-stateful-lowering --format json
verify-engine-prompt-audit: cargo xtask verify-engine-prompt-audit --format json
verify-engine-simplicity: cargo xtask verify-engine-simplicity --format json
```

Current report summary:

```text
dd_purity verdict: blocked
dd_purity failure count: 351
dd_purity categories: generic_dd_value_semantics=201, host_runtime_semantics=150

dd_stateful_lowering verdict: blocked
dd_stateful_lowering failure count: 354
dd_stateful_lowering categories: generic_value_evaluator=201, host_semantics=152, global_count_state=1
```

Relevant artifact:

```text
target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
sha256: 08ae88649e3064bad9d3ba0a4ab0afa6e54fec5f96744d3c63c78e4b738a894e

target/boon-artifacts/engine-simplicity/dd-purity-report.json
sha256: dba326ebfc119c826293992581a5546bddf2252eda38e962838ae2b1c730ee7b

target/boon-artifacts/engine-simplicity/no-fixture-dispatch-report.json
sha256: 0ee40a9ece790976099fd7d4bc24f7ac4acb2db683b339cad75aff8ffdf16fa9

target/boon-artifacts/engine-simplicity/complexity-report.json
sha256: 5c77afe6795bf80bcd4906ae36bea30eae786a7ed668e086a7d10e3eb81f675a

target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
sha256: 33aa5631fa3de4fd70efa02af63776f170d7bf32256ae4ba5942f6af4852ac11
```

Related passing artifacts from the same checkout:

```text
target/boon-artifacts/generated-freshness-report.json
sha256: 887d1fbb8e0644b54e83107b41547a5289d59bddeef6d93ae055ca7f3dd34886

target/boon-artifacts/generated-crates.json
sha256: 1b1538abac12170b00ff2dcb1c1c22ed4f6809fb6f224d2e318409044723c9fc

target/boon-artifacts/engine-simplicity/prompt-audit-report.json
sha256: 8fed33566d86d6c58ab143c5c9b084fd31dc938918ccd90b74163e0b2b51fe0d

target/boon-artifacts/honest-compiler-report.json
sha256: 3d6ae0e0101d56d3427319f0da867e399211ed9606efc5755d3ed3d28629afed

target/boon-artifacts/honesty-deterministic-report.json
sha256: fe733750778fb933d3e38470ad323f2822e64b5482dbc237d8044f9760e1644d

target/boon-artifacts/prompt-audit-report.json
sha256: 030458b85c1b89a360dba9e051f15226a70a5abd5275dc0e059f766a906cf775

target/boon-artifacts/verify-report.json
sha256: 433e3e011de754e3ae01d807ff669bdc287b2acdada07c3e873f0c09f7de5329

target/boon-artifacts/success.json
sha256: 90f2dc44c741e48bdb94501001c3dc86f88a245066260f1f449701f0510e5197
```

## Concrete Gates Rechecked

After removing the checked generated registry from `boon_examples`, the
host/xtask registry blocker is gone. After removing and scanning the
browser/WASM generated matrix, fixture dispatch is also gone. Source routing,
dynamic owner routing, persistent runtime, output drain efficiency, stress,
complexity, generated freshness, generated crate tests, cross-host parity, and
cross-engine comparison pass in the current aggregate engine-simplicity report.
These are no longer the blocker.

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

```bash
cargo xtask verify-no-fixture-dispatch --format json
```

Result: pass. `hit_count: 0`.

```bash
cargo xtask verify-honesty-deterministic --format json
```

Result: pass. `host_semantics_violations: 0`.

The remaining failing or blocked commands are purity, stateful-lowering,
prompt-audit, honest-compiler, and aggregate commands:

```bash
cargo xtask verify-dd-purity --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-engine-prompt-audit --format json
cargo xtask verify-honest-compiler --format json
cargo xtask verify-prompt-audit --format json
cargo xtask verify all --format json
```

Representative output:

```text
Error: engine-simplicity gate cargo xtask verify-dd-purity --format json reported blocked; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/dd-purity-report.json
Error: engine-simplicity gate cargo xtask verify-dd-stateful-lowering --format json reported blocked; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
Error: engine-simplicity gate cargo xtask verify-engine-prompt-audit --format json reported blocked; see /home/martinkavik/repos/boon-dd/target/boon-artifacts/engine-simplicity/prompt-audit-report.json
```

`cargo xtask verify all --format json` must remain non-passing until these
blocked gates are replaced by real typed DD execution evidence. The latest
aggregate run writes `success: false` and reports these failed gates:

```text
verify-honest-compiler
verify-prompt-audit
verify-dd-purity
verify-dd-stateful-lowering
verify-engine-prompt-audit
verify-engine-simplicity
```

The latest aggregate `success.json` was refreshed after the browser/WASM matrix
removal and after generated crates were made standalone workspaces. It is
current evidence for the blocked state, not a success artifact. It reports:

```text
success: false
engine_simplicity.fixture_dispatch_paths: 0
engine_simplicity.stateful_lowering_shortcuts: 354
engine_simplicity.dd_purity: blocked
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

crates/boon_runtime_host/src/lib.rs:711
fn runtime_value(...)

crates/boon_runtime_host/src/lib.rs:827
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
- `crates/boon_wasm_smoke/src/lib.rs` no longer contains the generated fixture
  matrix, but its compiled-manifest proof still routes through
  `boon_runtime_host` and therefore inherits the same `RuntimeValue` execution
  path until the runtime is made typed-DD-only.
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
