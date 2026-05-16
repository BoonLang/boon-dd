# Engine Simplicity DD Purity Blocker

Date: 2026-05-16

Plan path: `BOON_DD_ENGINE_SIMPLICITY_PLAN.md`

## Summary

The engine-simplicity goal is blocked because generated Rust still carries
generic `DdValue` evaluators on execution paths. This violates the plan's
non-negotiable requirement that terminal, native, and browser hosts inject
facts and drain/render outputs without executing Boon semantics, and that
stateful Boon semantics lower to explicit Timely/Differential graph state.

This is not a prompt-audit paperwork gap. A prompt-audit pass would be false
until these generic evaluators are removed or replaced with typed DD lowerings.

## Checkpoint Git State

Current HEAD:

```text
173ed5996dc4adf43eab599a5ecabfebe1cafac5
```

Current dirty status:

```text
M crates/boon_runtime_host/src/lib.rs
M docs/blockers/engine-simplicity-dd-purity-blocker.md
```

The previous checkpoint removed fixture dispatch from `boon_examples`,
xtask, and browser/WASM proof. The current dirty change removes the
`RuntimeValue` DD tuple path from `boon_runtime_host` and replaces it with
typed DD collections for text, number, and bool values plus source-id-filtered
source streams. Runtime-host scanner hits for `RuntimeValue`,
`runtime_value`, and `runtime_call_value` are now zero.

After tightening the scan to include `crates/boon_wasm_smoke`, fixture dispatch
now passes with zero hits. Deterministic honesty verification also passes.
The DD purity blocker remains: `boon_codegen_rust` plus the checked generated
crates still emit `DdValue` semantics.

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
dd_purity failure count: 201
dd_purity categories: generic_dd_value_semantics=201

dd_stateful_lowering verdict: blocked
dd_stateful_lowering failure count: 201
dd_stateful_lowering categories: generic_value_evaluator=201
```

Relevant artifact:

```text
target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
sha256: b7dc54aa666b9b750593bb9cebc2f8fc09c3003b9006eac826e052ed88336da4

target/boon-artifacts/engine-simplicity/dd-purity-report.json
sha256: c53be71aa10e8f1e74ff0abcecf9eb6f36897639c84313e20ebd3cb42591395f

target/boon-artifacts/engine-simplicity/no-fixture-dispatch-report.json
sha256: afc60fe3da4cf0d05438d0d7b22df5a52a854e8b65d421a42737ca644f63adf5

target/boon-artifacts/engine-simplicity/complexity-report.json
sha256: 6d2bddcebc3790bf5d7b2f80c8f679aaa60a4bbbb7e73cf5280ab2fa6389394b

target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
sha256: f45479ada56056a8b6e554f580b308a10f70454a11ce282a210fbf2a29468eb7
```

Related passing artifacts from the same checkout:

```text
target/boon-artifacts/generated-freshness-report.json
sha256: 887d1fbb8e0644b54e83107b41547a5289d59bddeef6d93ae055ca7f3dd34886

target/boon-artifacts/generated-crates.json
sha256: 1b1538abac12170b00ff2dcb1c1c22ed4f6809fb6f224d2e318409044723c9fc

target/boon-artifacts/engine-simplicity/prompt-audit-report.json
sha256: 7147fa3f0e6e0ec8c14a5f08134e82340b62a3612a95bce719410ad9cd821f78

target/boon-artifacts/honest-compiler-report.json
sha256: 30bca7f83de9cfd602c07bbe01d1041504e83d472a441068194f5ed845b575be

target/boon-artifacts/honesty-deterministic-report.json
sha256: f74561a3a782a352fec45a76e234a888a4a9b8ccaaedcbb4b329feb753cd8af2

target/boon-artifacts/prompt-audit-report.json
sha256: 996019c659735df91243af175ff435f3a4b863a37a6c4c7a1c6380e45fce5128

target/boon-artifacts/verify-report.json
sha256: ef61461dda815da926e0f74eeb2b847611bdf8e286637a52df07a28afcbd7d12

target/boon-artifacts/success.json
sha256: 8c0b3a10c15522148146c8f78d503bca33e3cb148472f7edee4e8e65abd395e5
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

```bash
cargo xtask verify-wasm-dd --required --browser firefox
```

Result: pass. The xtask Firefox launcher wraps the actual browser process with
`cosmic-background-launch --workspace boon-dd -- ...`.

```bash
cargo xtask verify-playgrounds --format json
```

Result: pass. `browser-playground-result.json` contains
`wasm_compiled_manifest` for all 22 examples.

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

The latest aggregate `success.json` was refreshed after the runtime-host
typed-collection change. It is current evidence for the blocked state, not a
success artifact. It reports:

```text
success: false
engine_simplicity.fixture_dispatch_paths: 0
engine_simplicity.stateful_lowering_shortcuts: 201
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

- `crates/boon_runtime_host/src/lib.rs` no longer contains `RuntimeValue`,
  `runtime_value`, or `runtime_call_value`. It now builds typed DD collections
  for runtime-host text/number/bool paths, but this is only a partial step.
- `crates/boon_wasm_smoke/src/lib.rs` no longer contains the generated fixture
  matrix, and its compiled-manifest proof still passes through Firefox after
  the runtime-host typed-collection change.
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

1. Replace shared `DdValue` expression lowering with typed DD node lowerings
   for each accepted Boon operation. Static literals may become typed constant
   collections, but dynamic semantics must be expressed as collections,
   arrangements, reductions, joins, or generated typed operators with declared
   source/owner/generation keys.
2. Lower list map/retain/count/latest, HOLD, LATEST, text input state, timers,
   and command acknowledgements as keyed DD state. Do not use host `Vec`
   operations as the execution path for list semantics.
3. Add negative samples that mutate source ids, owner keys, generations, and
   stale generated artifacts and prove the relevant gates fail.
4. Rerun `cargo xtask write-generated-artifacts --format json`, then
   `cargo xtask verify all --format json`.

## Smallest Safe Next Step

Start with a narrow counter plus two-source routing slice:

- replace the `DdValue` codegen path in `crates/boon_codegen_rust/src/lib.rs`
  with typed generated DD collections for source id, owner key, generation,
  number, text, bool/tag, and render text patch output;
- regenerate `generated/*/src/graph.rs` so checked generated crates no longer
  import or emit `DdValue`;
- update generated crate tests to prove the typed generated graph path still
  executes all 22 manifest scenarios;
- only then broaden the typed lowering table to list/text/library operations
  until `verify-dd-purity` and `verify-dd-stateful-lowering` both reach zero
  hits.
