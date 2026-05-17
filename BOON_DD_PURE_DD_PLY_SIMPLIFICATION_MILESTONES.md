# Boon DD Pure Compiler, Ply Renderer, And Simplicity Milestones

This file is the execution contract for the next `/goal` run. It layers on top
of:

- `BOON_DD_HONEST_COMPILER_PLAN.md`
- `BOON_DD_ENGINE_SIMPLICITY_PLAN.md`
- `docs/blockers/engine-simplicity-runtime-semantics-blocker.md`
- `docs/blockers/phase0-honest-compiler-blockers.md`

The goal is not a green smoke test. The goal is a small, reliable Boon DD
implementation where accepted Boon semantics compile into real Timely /
Differential Dataflow, where host code does not execute Boon semantics, and
where the renderer is imported from the `boon-dd-ply` worktree so native/browser
UI uses Ply instead of custom rendering code.

## Non-Negotiables

- No fallback interpreter, runtime-host semantic evaluator, source-text
  heuristic, fixture dispatcher, smoke semantic shortcut, app-specific lowering,
  or host-side Boon behavior may remain on an execution path.
- Generated code must build and run typed Timely/Differential graph logic. It
  must not hide precomputed Boon results behind `map(|_| ...)`, generic folded
  values, or Rust-side list/text/record/control evaluators.
- Source routing must reject unknown source paths and preserve source id,
  source family, owner key, generation, Boon time, payload shape, and diff
  through DD keys and outputs.
- Hosts may compile/load graphs, inject typed events, advance time, drain
  probes, render UI shells, and persist/deliver effect facts. Hosts must not
  decide Boon behavior.
- Any command that creates a window must wrap the actual window-creating process
  with `cosmic-background-launch --workspace boon-dd -- ...`.
- If the goal is blocked, do not fake success. Write or update a checked-in
  report under `docs/blockers/` with the exact failing command, output, artifact
  paths, minimized repro, and next fix decision.

## Baseline Inputs

Use these local inputs and record their hashes in the final reports:

- Current `boon-dd` checkout at goal start.
- `~/repos/boon-dd-ply` branch `ply-renderer`, expected current commit:
  `1a80482` (`Replace custom renderer with Ply`).
- `~/repos/boon` parser implementation, especially its Chumsky parser modules
  under `crates/boon/src/parser/`.
- Current blocker reports under `docs/blockers/`.

Before changing code, write:

```text
target/boon-artifacts/pure-dd-ply/baseline.json
```

The baseline must include:

- `git rev-parse HEAD`
- `git status --short`
- `git -C ~/repos/boon-dd-ply rev-parse HEAD`
- `git -C ~/repos/boon-dd-ply status --short`
- parser dependency notes for `~/repos/boon`
- LOC/module/dependency counts for current `boon-dd`
- current failing gate summaries

## Milestone 0: Guardrails Before Feature Work

Keep or strengthen the existing failing gates before removing shortcuts.

Required checks:

```bash
cargo xtask verify-no-shortcuts --format json
cargo xtask verify-dd-purity --format json
cargo xtask verify-source-routing --format json
cargo xtask verify-dynamic-owner-routing --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-engine-simplicity --format json
```

If any gate can be bypassed by renaming a helper, by stale artifacts, or by
deleting blocker docs, harden the gate first. Add negative verifier tests that
prove renamed shortcut functions still fail.

Completion criteria:

- Existing blockers are reproduced.
- Reports identify real code paths, not only old symbol names.
- Stale `success.json` cannot coexist with failing current reports.

## Milestone 1: Parser And Frontend Simplification Decision

The current hand-written `boon_syntax` parser must not be kept by inertia.
Measure three options and choose the smallest reliable path:

1. Extract or port the original Chumsky parser from `~/repos/boon` into a small
   parser-focused crate in this repo.
2. Depend on a split-out local parser crate if it can avoid pulling unrelated
   UI/runtime dependencies.
3. Keep the current parser only if it is demonstrably smaller and reaches full
   accepted syntax coverage without ad hoc parsing.

Do not add a dependency on the whole `boon` crate if that imports unrelated
renderer, web, or engine dependencies just to parse source.

Preferred outcome:

- `boon_syntax` uses a real parser-library implementation, likely Chumsky,
  with structured spans and diagnostics.
- Parser output covers the accepted Boon syntax surface from
  `docs/language/boon-language-manifest.toml`.
- Old parser code is deleted or moved to tests/fixtures only.

Required verification:

```bash
cargo test -p boon_syntax
cargo xtask verify-syntax-corpus --format json
cargo xtask verify-negative-corpus --format json
cargo xtask verify-language-corpus --format json
```

Report parser LOC, parser dependency tree, accepted syntax coverage, and why
the selected parser path is simpler than the alternatives.

## Milestone 2: Typed IR And Pure DD Lowering

Remove execution-path use of:

- `Literal`
- `fold_literal`
- `fold_call_literal`
- `RenderStream` as a host-side semantic dispatcher
- `lower_render_text_collection`
- `FoldedRender`
- `fold_graph_value`
- `fold_call_value`
- static/precomputed `render_events.clone().map(|_| ...)`
- unknown source fallback such as `other => other.to_owned()`
- dynamic routing that ignores generation

Replace them with typed compiler IR and typed DD lowering:

- Parse -> HIR -> resolver -> shape/type checker -> semantic IR -> DD graph IR.
- Each accepted semantic node must lower to a typed DD operator/template or
  produce a structured unsupported diagnostic.
- Source collections must be keyed by source id, family, owner, generation,
  time, and payload shape.
- Stateful semantics (`LATEST`, `HOLD`, lists, persistence, effects, commands)
  must be keyed DD state, not Rust closures over generic values.
- Generated graph output must carry structured monitor/render/effect/persist
  protocol data with source and owner provenance.

Required negative checks:

- wrong static source id does not affect output;
- unknown source path is rejected;
- same payload on two source ids remains separated;
- same source family with different owners remains separated;
- stale generation does not update current owner;
- removing and recreating an owner does not leak state;
- static output cannot be accepted if it is precomputed outside DD lowering.

Required verification:

```bash
cargo xtask verify-no-shortcuts --format json
cargo xtask verify-dd-purity --format json
cargo xtask verify-source-routing --format json
cargo xtask verify-dynamic-owner-routing --format json
cargo xtask verify-dd-stateful-lowering --format json
cargo xtask verify-lowering --format json
cargo xtask verify-generated-freshness --format json
cargo xtask verify-generated-crates --format json
```

Completion criteria:

- shortcut hit counts are zero;
- generated crates do not call compiler/runtime semantic evaluators;
- generated artifacts are fresh and hash-checked;
- accepted examples run through generated DD graph execution, not through a
  host fallback.

## Milestone 3: Long-Lived Runtime Host

Replace scenario/replay execution with a long-lived generated graph session.

Required API shape:

- instantiate or load verified generated graph;
- submit typed source actions and host facts;
- advance deterministic test time or host time;
- drain bounded output diffs since the last cursor;
- expose structured outputs without cloning all historical output vectors;
- shutdown cleanly.

Host code must not:

- rebuild the graph per interaction;
- replay a full scenario to answer one click;
- decide app behavior from example names, source paths, tags, or text;
- synthesize render/effect/persistence output text.

Required verification:

```bash
cargo xtask verify-persistent-runtime --format json
cargo xtask verify-output-drain-efficiency --format json
cargo xtask verify-engine-stress --format json
```

Completion criteria:

- one graph build per multi-interaction session;
- no full-output-vector clones on execution paths;
- stress artifacts include event count, drain steps, elapsed time, output count,
  and max buffered outputs.

## Milestone 4: Merge Ply Renderer From `~/repos/boon-dd-ply`

After the pure runtime API is stable, import the Ply renderer work.

Required source:

```bash
git -C /home/martinkavik/repos/boon-dd-ply rev-parse HEAD
git -C /home/martinkavik/repos/boon-dd-ply status --short
```

Expected commit: `1a80482`.

Import strategy:

- Prefer a real merge or cherry-pick from the `ply-renderer` worktree/branch if
  it preserves history cleanly.
- If conflicts with the pure runtime rewrite are too large, manually port only
  the renderer crate and plan artifacts, preserving the current pure runtime
  API.
- Do not overwrite current blocker, verifier, compiler, or runtime work with
  older files from the Ply worktree.

Required renderer outcome:

- Add or preserve `crates/boon_backend_ply`.
- Native and browser playground UI share the same Rust/Ply app code.
- Old `boon_backend_app_window` / `boon_backend_wgpu` custom renderer code is
  deleted or reduced to thin non-render compatibility wrappers.
- Browser UI is not rendered by custom DOM/Canvas/WebGPU JavaScript.
- Ply is isolated to renderer crates; compiler/runtime crates must not depend on
  Ply.

Required verification:

```bash
cargo tree -i ply-engine
cargo tree -i ply
cargo check -p boon_backend_ply --bins
cargo test -p boon_backend_ply
```

GUI/browser proof commands must use:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```

Completion criteria:

- `ply-engine` is present only through renderer code;
- crate `ply` is absent unless explicitly justified as the polygon parser and
  unused by UI;
- native/browser/terminal parity uses generated DD runtime output, not cached
  smoke rows;
- screenshots/browser artifacts prove the Ply surface is visible and
  interactive.

## Milestone 5: Code Size And Reliability Simplification

After correctness and Ply merge, simplify aggressively but safely.

Targets:

- remove duplicate generated graph snapshots where one canonical generated
  source is enough;
- move common generated runtime support into a normal crate instead of emitting
  duplicated helper code into every generated crate;
- collapse crates only when ownership boundaries are not useful;
- replace large raw string code generation with a smaller structured generator
  only if it reduces defects and line count;
- remove obsolete WGPU/app-window/browser smoke code after Ply proof passes;
- remove stale blocker docs only when their gates pass and the report includes
  the fixing commit/artifact paths.

Library policy:

- Prefer mature libraries when they reduce custom compiler/parser/render code
  and do not pull unrelated runtime/UI dependencies.
- Prefer the original `~/repos/boon` Chumsky parser approach over extending a
  fragile hand parser, if measured complexity supports it.
- Do not add a library just to hide complexity. Every new dependency must have
  a before/after LOC, dependency-tree, and failure-surface note.

Required reports:

```text
target/boon-artifacts/pure-dd-ply/simplicity-report.json
target/boon-artifacts/pure-dd-ply/parser-decision.json
target/boon-artifacts/pure-dd-ply/ply-merge-report.json
target/boon-artifacts/pure-dd-ply/final-comparison.json
```

Compare:

- current goal-start `boon-dd`;
- final `boon-dd`;
- `~/repos/boon-dd-ply`;
- sibling `~/repos/boon-*` engines;
- relevant parser-only subset from `~/repos/boon`.

Metrics:

- first-party LOC excluding `target`, build outputs, and vendored/generated
  artifacts unless reported separately;
- generated LOC;
- crate/package count;
- dependency count and dependency tree hot spots;
- functions/types/match count or equivalent complexity proxy;
- verifier count and failure mode coverage;
- runtime performance stress metrics.

Completion criteria:

- final hand-written engine/compiler/runtime code is smaller or has a written
  defensible reason for any increase;
- generated duplication is reduced or justified;
- parser/frontend code is simpler and more complete than the starting parser;
- no deleted blocker can still be reproduced.

## Milestone 6: Final Honesty Verification

Do not mark complete until this exact aggregate passes on the current checkout:

```bash
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo xtask verify all --format json
```

Final `target/boon-artifacts/success.json` must report:

- `success: true`
- honest compiler verdict pass
- deterministic honesty verdict pass
- prompt-audit verdict pass
- engine simplicity verdict pass
- DD purity pass
- zero shortcut execution paths
- zero stale artifacts
- zero fixture dispatch paths
- zero source-routing wrong-id failures
- zero dynamic-owner leaks
- one graph build per interaction session
- zero full-output-vector clones on execution paths
- zero stateful-lowering shortcuts
- Firefox/browser proof that the generated Timely/DD graph runs in the browser
  process
- native proof that the visible Ply surface runs through the generated DD
  runtime output

Also run independent audits:

- one audit focused on pure DD semantics and no shortcuts;
- one audit focused on parser/full syntax coverage and diagnostics;
- one audit focused on Ply merge and UI proof;
- one audit focused on simplicity/code size/performance metrics.

Each audit must write a JSON artifact with prompt hash, repo hash, artifact
hashes, verdict, findings, and file/line evidence.

## Final Rule

If any milestone is blocked, stop claiming completion and leave the repo in a
truthful blocked state with checked-in blocker documentation. Do not replace
failing evidence with prompt-audit optimism.
