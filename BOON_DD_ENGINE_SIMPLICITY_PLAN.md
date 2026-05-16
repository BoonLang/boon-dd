# Boon DD Engine Simplicity And Purity Plan

This file is an implementation contract for Codex CLI `/goal` work. Treat it as
the source of truth for making the Boon DD engine genuinely pure
Timely/Differential Dataflow, simple to maintain, fast enough to justify the
DD architecture, and honestly verified.

`BOON_DD_HONEST_COMPILER_PLAN.md` remains the compiler correctness contract.
This plan adds the runtime/engine contract that was exposed by the review: the
current checkout can pass the honest-compiler verifier while still behaving like
a checked-example fixture dispatcher with shallow DD usage. That must stop being
possible.

## Goal

Build and verify a Boon DD implementation where:

- accepted Boon syntax and semantics lower through compiler IR into generated
  Timely/Differential graph construction;
- generated graphs are source-keyed, owner-aware, long-lived, and incrementally
  stepped;
- terminal, native, and browser hosts inject input facts and render/drain
  outputs, but do not execute Boon semantics;
- examples are not selected through source text, source paths, example names, or
  hard-coded generated crate registries;
- runtime work per interaction is incremental rather than rebuilding and
  replaying whole graphs;
- complexity and performance improve against the current engine baseline, and
  regressions are blocked by deterministic gates rather than by human memory.

The intended result is not merely another green smoke report. The result must be
a mechanically checked engine where the important failure modes from the review
cannot silently return.

## Required GUI Launch Rule

Any command that creates a native window, browser window, or other GUI surface
from this repository must wrap the actual window-creating process with:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```

The wrapper must be as close as possible to the process that creates the window
so the child process inherits the launch context. Wrapping an earlier bootstrap
command is not enough.

## Baselines

The implementation must compare the final checkout against these baselines:

- `engine_start`: `d2cbcb5` (`Wire honest compiler gate to final reports`)
- `pre_honest_compiler`: `a64893b` (`Fix native playground interaction`)
- sibling repositories discovered at runtime under `~/repos/boon-*`

If either baseline commit is unavailable locally, the goal must stop with a
checked-in blocker report under `docs/blockers/` explaining the exact command,
output, and the replacement baseline decision needed from a human.

## Current Known Problems

These are the review findings that this plan is designed to make impossible:

- The backend crates dispatch through
  `boon_examples::run_generated_for_checked_source` instead of compiling and
  executing a general generated graph path.
- `boon_examples` maps source to a known graph id and selects checked-in
  generated crates from a registry.
- Generated graph source bindings exist, but render and latest paths can consume
  all non-host events instead of the resolved source ids they declare.
- Dynamic owners and generations are accepted by the runtime API, then discarded
  before source-event routing.
- Boon/library semantics are still embedded as inline Rust value evaluators and
  host-side collection operations instead of DD graph state.
- Scenario persistence, reload, and command behavior can be performed outside
  the generated graph execution path.
- Native interactions rebuild/replay through the example runner instead of
  keeping a long-lived graph worker alive.
- Output draining clones whole output vectors and callers often use only the
  last element.
- Verification can pass by checking smoke strings or generated fixture output
  instead of proving that the browser/native runtime executed the generated
  Timely/DD graph.
- Hard-coded smoke helper paths still exist and can confuse future verification
  audits even if they are not the primary backend path.

## Non-Negotiables

- No fallback interpreter, fixture dispatcher, or host-side Boon semantic path is
  allowed in runtime execution.
- No semantic decision may depend on `str::contains`, source path fragments,
  example names, graph crate names, test names, or checked-in generated fixture
  registries.
- Source routing must use compiler-assigned source ids and dynamic source-family
  identity. Wrong source ids must not affect output.
- Dynamic owner and generation identity must be preserved through event input,
  DD keys, render/effect output, persistence, and reload.
- Runtime hosts may create windows, receive OS/browser events, inject source
  facts, advance Timely probes, drain output diffs, render frames, and persist
  effect/storage facts. They must not decide Boon behavior.
- Generated code may contain graph-construction code and thin typed wrappers. It
  must not contain large per-example semantic evaluators copied from the host.
- Each interactive app instance must build its generated dataflow once and then
  reuse it across interactions until the source program or host configuration
  changes.
- Verification must include negative checks. A verifier that can be fooled by
  renamed examples, stale generated artifacts, or wrong source ids is not a pass.
- Prompt audits are required, but they can only confirm deterministic evidence.
  A prompt-audit pass cannot override a failing hard gate.

## Required Command Surface

Phase 0 must add these commands before fixing the engine. They may initially
fail, but they must write machine-readable reports that explain the failure.

```bash
cargo xtask verify-engine-simplicity --format json
cargo xtask verify-dd-purity --format json
cargo xtask verify-no-fixture-dispatch --format json
cargo xtask verify-source-routing --format json
cargo xtask verify-dynamic-owner-routing --format json
cargo xtask verify-persistent-runtime --format json
cargo xtask verify-output-drain-efficiency --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-engine-stress --format json
cargo xtask verify-engine-complexity --format json
cargo xtask verify-engine-prompt-audit --format json
cargo xtask compare-engines --format json
cargo xtask verify all --format json
```

`cargo xtask verify all --format json` must include every required command above
plus the existing honest-compiler gates. It must fail if any engine-simplicity
gate fails or is missing.

## Required Artifacts

The aggregate verification must write these files:

```text
target/boon-artifacts/engine-simplicity/engine-simplicity-report.json
target/boon-artifacts/engine-simplicity/dd-purity-report.json
target/boon-artifacts/engine-simplicity/no-fixture-dispatch-report.json
target/boon-artifacts/engine-simplicity/source-routing-report.json
target/boon-artifacts/engine-simplicity/dynamic-owner-routing-report.json
target/boon-artifacts/engine-simplicity/persistent-runtime-report.json
target/boon-artifacts/engine-simplicity/output-drain-efficiency-report.json
target/boon-artifacts/engine-simplicity/dd-stateful-lowering-report.json
target/boon-artifacts/engine-simplicity/stress-report.json
target/boon-artifacts/engine-simplicity/complexity-report.json
target/boon-artifacts/engine-simplicity/cross-engine-comparison.json
target/boon-artifacts/engine-simplicity/prompt-audit-report.json
target/boon-artifacts/success.json
```

Every report must include:

- `schema_version`
- `verdict`: `pass`, `fail`, or `blocked`
- `plan_path`: `BOON_DD_ENGINE_SIMPLICITY_PLAN.md`
- `git_head`
- `dirty_worktree`
- `commands_run`
- `input_hashes`
- `artifact_hashes`
- `failures`
- `blockers`

The final `success.json` must include:

```json
{
  "success": true,
  "engine_simplicity": {
    "verdict": "pass",
    "dd_purity": "pass",
    "fixture_dispatch_paths": 0,
    "source_routing_wrong_id_failures": 0,
    "dynamic_owner_leaks": 0,
    "runtime_graph_builds_per_interaction_session": 1,
    "full_output_vector_clones_in_execution_paths": 0,
    "stateful_lowering_shortcuts": 0,
    "stale_artifacts": 0,
    "prompt_audit_verdict": "pass"
  }
}
```

Any missing field or value that differs from the expected value above fails the
goal. Fields whose expected value is zero must remain zero; the expected graph
build count for a multi-interaction session is exactly one.

## Phase 0: Add Failing Gates First

Before fixing implementation paths, add the command surface and reports above.
The first version of each gate should fail against the current checkout with a
precise reason rather than passing optimistically.

Required Phase 0 checks:

- detect calls from runtime backends into checked-example dispatch such as
  `run_generated_for_checked_source`;
- detect source/path/example-name semantic dispatch in runtime execution paths;
- detect `str::contains` or equivalent source text semantic decisions in
  compiler, codegen, runtime, examples, generated runtime, and verifier paths;
- detect generated graph source bindings that are not used by downstream
  operators;
- detect dynamic owner/generation fields that are dropped before DD keys;
- detect graph rebuild/replay per user interaction;
- detect output APIs that clone all outputs on execution paths;
- detect smoke-string verification that substitutes for structured generated
  graph proof;
- produce baseline complexity/performance numbers before changing behavior.

Completion criteria:

- all commands exist;
- all commands write reports;
- `cargo xtask verify all --format json` includes the new gates;
- failing gates fail for concrete current defects;
- no gate reports `pass` because its implementation is a placeholder.

## Phase 1: Remove Fixture Dispatch From Execution

Replace backend execution paths that select checked-in generated crates by
example name, source path, or source text with a general compiled-artifact path.

Required behavior:

- the compiler accepts a source module and produces a source-hashed generated
  graph artifact or in-process graph factory;
- hosts receive a compiled graph factory/manifest and do not know example
  semantics;
- examples may be used as input files, but the engine cannot branch on their
  names;
- arbitrary temporary Boon source used by tests must compile and run through the
  same execution path as checked examples;
- deleting or renaming an example directory must not change the semantics of a
  compiled source with the same content.

Forbidden end state:

- backend crates calling a checked-example runner;
- registries that map example names to semantic implementation crates;
- verification that only proves the checked examples still have outputs.

Required gate:

```bash
cargo xtask verify-no-fixture-dispatch --format json
```

## Phase 2: Make Source Routing Compiler-Keyed

Source bindings in generated graphs must be real DD keys, not decorative
metadata.

Required tests:

- compile a tiny source with two same-shaped sources and prove only the correct
  source id affects the dependent output;
- inject a wrong source id and prove output is unchanged;
- inject same payloads into two sources and prove outputs remain separated;
- run the same routing checks in terminal/runtime-host, native execution path,
  and browser Firefox proof path when a GUI/browser is involved.

The browser proof must wrap the actual Firefox/window-creating process with:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```

Required gate:

```bash
cargo xtask verify-source-routing --format json
```

## Phase 3: Preserve Dynamic Owner And Generation Identity

Dynamic source families, list item owners, keyed holds, and generated outputs
must keep owner/generation identity through the DD graph.

Required tests:

- two dynamic owners with identical payloads produce isolated outputs;
- stale generation events do not update current owner state;
- remove/recreate owner sequences do not leak state across generations;
- persistence/reload restores only the intended owner state;
- list retain/map/count/latest cases run with multiple owners and repeated
  generations.

Required gate:

```bash
cargo xtask verify-dynamic-owner-routing --format json
```

## Phase 4: Lower Stateful Semantics To DD State

`LATEST`, `HOLD`, keyed hold, list map/retain/count/latest, text-input state,
timers, and command acknowledgements must lower to explicit Timely/DD state,
keys, arrangements, reductions, joins, or equivalent DD operators.

Required checks:

- no global count/reduce shortcut may stand in for keyed state where keys exist;
- no `Vec<GeneratedValue>` host-side list semantic loop may be the execution
  path for list semantics;
- no Rust `truthy`, field lookup, text concatenation, or arithmetic helper may
  make a Boon semantic decision unless it is part of a generated DD operator
  whose inputs are typed IR nodes and whose scope is reported in the lowering
  plan;
- every supported library operation has lowering metadata and a verifier sample;
- unsupported operations fail compilation with structured diagnostics rather
  than falling back to host evaluation.

Required gate:

```bash
cargo xtask verify-dd-stateful-lowering --format json
```

## Phase 5: Keep Runtime Graphs Long-Lived

Interactive runtime instances must construct the Timely/DD dataflow once per
compiled app load and then reuse it for source facts.

Required implementation shape:

- introduce a runtime session object that owns the worker, inputs, probes,
  output drains, artifact hash, and render/effect/storage sinks;
- native and browser interactions call `submit_action` or equivalent on the
  long-lived session;
- reload/source-code change is the only normal path that builds a new graph;
- graph build counts are recorded in verification artifacts.

Required tests:

- run at least 100 interactions in one session and assert graph build count is
  one;
- run source-code reload and assert graph build count increments exactly once;
- run native playground interaction path as a human-style input test, not only a
  direct unit function call;
- run Firefox/browser proof that source facts enter the browser-process
  generated DD graph.

Required gate:

```bash
cargo xtask verify-persistent-runtime --format json
```

## Phase 6: Drain Outputs Incrementally

Runtime output APIs must expose incremental drains or cursors. Execution paths
must not clone whole output vectors and then keep only the last value.

Required implementation shape:

- generated graph output handles provide `take_outputs`, cursor iteration, or a
  bounded drain API;
- callers consume diffs since the previous step;
- reports count emitted diffs, retained outputs, and clone-heavy paths.

Required gate:

```bash
cargo xtask verify-output-drain-efficiency --format json
```

## Phase 7: Add Stress And Human-Style Interaction Proof

Performance verification must cover workloads that expose replay, cloning, and
owner-key bugs.

Required workloads:

- 1,000 and 10,000 input events into a counter-like graph;
- 1,000 dynamic owners with repeated update/remove/recreate cycles;
- growing list map/retain/count workloads;
- repeated text input edits;
- persistence and reload after many events;
- native mouse/keyboard interaction where applicable;
- Firefox browser interaction where applicable.

The stress gate must record:

- wall-clock timing;
- graph build count;
- peak retained outputs;
- total emitted diffs;
- per-step average and p95 latency where deterministic enough to measure;
- memory estimate where available;
- host/runtime path used.

Timing thresholds should be conservative and checked into the report schema.
If a threshold is too environment-sensitive, the gate must still fail on
structural regressions such as graph rebuild count, full replay count, or output
clone count.

Required gate:

```bash
cargo xtask verify-engine-stress --format json
```

## Phase 8: Enforce Simplicity And Code-Size Budgets

The project goal is a simpler engine, not just a different engine. Complexity
must be measured every time the verifier runs.

Required measurements:

- handwritten Rust code lines for `crates/` and `xtask/`;
- generated Rust code lines under `generated/`;
- generated total code/data lines under `generated/`;
- function, struct, enum, impl, match, and branch-like token counts;
- per-module deltas against `engine_start`;
- deltas against `pre_honest_compiler`;
- sibling-repo comparison against local `~/repos/boon-*` checkouts.

Hard requirements:

- generated graph source must not be written twice under different filenames for
  the same crate;
- common runtime helpers must live in shared crates instead of being copied into
  every generated crate;
- no module may grow by more than 15 percent over `engine_start` without the
  report naming the exact feature and replacement/deletion that justifies it;
- generated Rust LOC must be lower than `engine_start` unless a blocker report
  explains why pure DD correctness currently requires the increase;
- `xtask` may add gates, but semantic implementation logic must move into the
  compiler/runtime crates rather than becoming a second engine inside `xtask`.

Required gate:

```bash
cargo xtask verify-engine-complexity --format json
```

Required comparison:

```bash
cargo xtask compare-engines --format json
```

`compare-engines` must not fail only because a sibling repo is missing. Missing
sibling repos must be reported as `unavailable`; missing required baselines
inside this repo are blockers.

## Phase 9: Make Verification Honest Against Stale Artifacts

Verification must prove that the current checkout generated and executed the
current artifacts.

Required checks:

- hash source Boon files, compiler crates, runtime crates, codegen crates,
  generated graph files, generated manifests, expected outputs, and dependency
  lockfiles;
- fail when generated artifacts are stale;
- fail when browser/native output comes from precomputed JSON or smoke strings;
- fail when generated crate tests do not execute the graph they claim to test;
- fail when a forbidden shortcut exists in an execution path even if it is not
  currently used by a passing example.

Required gate:

```bash
cargo xtask verify-dd-purity --format json
```

## Phase 10: Prompt-Audit Verification

After deterministic gates pass, run adversarial prompt audits from
`docs/prompts/engine-simplicity/`.

Required audit prompts:

- `01_pure_dd_architecture_audit.md`
- `02_runtime_simplicity_and_performance_audit.md`
- `03_verifier_honesty_audit.md`

Each prompt must be answered by an independent review run that inspects the
current checkout. The report must include:

- prompt path;
- model/tool identity if available;
- `verdict`: `pass`, `fail`, or `inconclusive`;
- file/line evidence for every finding;
- whether deterministic artifacts were checked;
- whether the reviewer found a path that can fake a pass.

Any `fail` or `inconclusive` blocks the final goal until fixed or converted into
a checked-in blocker report with exact evidence and the next decision needed.

Required gate:

```bash
cargo xtask verify-engine-prompt-audit --format json
```

## Phase 11: Final Aggregate Verification

The goal is complete only when this command passes on the current checkout:

```bash
cargo xtask verify all --format json
```

When the aggregate verification opens a GUI or browser window, the actual
window-creating command must be launched through:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```

Final success requires:

- existing honest compiler verdict `pass`;
- engine simplicity verdict `pass`;
- DD purity verdict `pass`;
- no fixture dispatch execution paths;
- source routing wrong-id checks pass;
- dynamic owner leak checks pass;
- graph build count is one for multi-interaction sessions;
- no full output-vector clone in execution paths;
- stateful semantics lower to DD graph state;
- stress reports are present and pass structural thresholds;
- complexity report is present and no hard budget fails;
- cross-engine comparison report is present;
- prompt audit verdict `pass`;
- stale artifact failures are zero;
- Firefox proof shows generated Timely/DD graph running in the browser process;
- `target/boon-artifacts/success.json` reports `success: true`.

## Blocker Protocol

If the full goal is blocked, do not fake success and do not weaken a gate to
pass. Write a checked-in blocker report under `docs/blockers/` with:

- title and date;
- plan path;
- failing command;
- exact command output or artifact path containing full output;
- current git head and dirty status;
- relevant artifact paths and hashes;
- minimized repro;
- why this is a real blocker rather than unfinished implementation work;
- next pin/fork/fix decision needed;
- the smallest safe next step.

The aggregate verifier must report `blocked`, not `pass`, while a blocker report
is active for a required gate.

## Suggested `/goal` Prompt

Use `docs/prompts/engine-simplicity/00_goal_prompt.md` as the paste-ready
prompt. It intentionally points at this file and requires the agent to add gates
before fixing behavior so the work cannot forget the review findings halfway
through.
