# Phase 0 Honest Compiler Blockers

This blocker report exists because `BOON_DD_HONEST_COMPILER_PLAN.md` is not
implemented end to end yet. The current checkout has the Phase 0 command
surface, generated-artifact freshness checks, generated-crate tests,
no-shortcuts scanning, and DD render-graph lowering in place, but the honest
compiler is still incomplete.

Latest aggregate command, run with the required workspace launcher because it
includes browser/native verification paths:

```bash
cosmic-background-launch --workspace boon-dd -- cargo xtask verify all --format json
```

Current result:

```text
target/boon-artifacts/verify-report.json: "success": false
target/boon-artifacts/success.json: "success": false
```

The aggregate currently fails exactly these gates:

```text
verify-honest-compiler
verify-honesty-deterministic
verify-language-corpus
verify-prompt-audit
```

## Failing Commands

```bash
cargo xtask verify-honest-compiler --format json
```

Result:

```text
Error: honest compiler is not implemented yet; see target/boon-artifacts/honest-compiler-report.json
```

```bash
cargo xtask verify-honesty-deterministic --format json
```

Result:

```text
Error: deterministic honesty verification is not complete; see target/boon-artifacts/honesty-deterministic-report.json
```

Current deterministic summary:

```text
verdict: fail
missing_deterministic_gates: source-truth, resolver-and-shape
accepted_features_without_full_coverage: 6
shortcut_symbols_in_execution_paths: 0
stale_artifact_failures: 0
host_semantics_violations: 0
adversarial_heuristic_cases_failed: 0
```

```bash
cargo xtask verify-language-corpus --format json
```

Result:

```text
Error: language corpus coverage is not complete; see target/boon-artifacts/language-corpus-report.json
```

Current language-corpus summary:

```text
verdict: fail
blockers: manifest is still marked incomplete and examples/features are not accepted
coverage_report_failures: []
language_status: incomplete
incomplete_features_count: 6
incomplete_examples_count: 22
structural_errors: false
```

```bash
cargo xtask verify-prompt-audit --format json
```

Result:

```text
Error: prompt audit is incomplete; see target/boon-artifacts/prompt-audit-report.json
```

Current prompt-audit summary:

```text
verdict: fail
audits_required: 7
audits_passed: 0
audit_json_files_found: 7
missing_required: []
critical_findings_open: 17
inconclusive_audits: 0
hash_mismatches: 14
schema_errors: []
```

## Passing Evidence

- `target/boon-artifacts/generated-freshness-report.json` reports verdict
  `pass`: 374 generated files checked, 0 stale, 0 missing. Snapshot/proof
  artifacts are now written from the generated Timely/DD graph output for the
  source/scenario instead of being copied from `expected.render.json`; expected
  files remain assertion inputs for generated-crate tests.
- `target/boon-artifacts/generated-crates.json` reports verdict `pass`: 22
  generated crates checked. The generated crate tests replay the full parsed
  scenario protocol rather than only the first scenario step.
- `target/boon-artifacts/verify-playgrounds.json` is refreshed and reports 22
  examples through each manual-test surface: terminal, native app-window/WGPU,
  and Firefox/browser.
- `target/boon-artifacts/no-shortcuts-report.json` reports verdict `pass`: 0
  forbidden shortcut hits and 0 shortcut symbols in execution paths.
- `target/boon-artifacts/lowering-coverage-report.json` reports verdict
  `pass`: 22 examples checked, 0 unsupported semantic nodes, all required
  output sink families seen, 164 DD render graph nodes, no missing render graph
  roots, and no generated code execution hits for legacy render-program helper
  paths.
- The parser/shape/DD render IR now has first-class support for sibling-repo
  numeric/equality syntax patterns including `--` comments, unary negative
  numbers, binary subtraction, and binary equality. This is not full cross-repo
  language completion; it removes one parser/IR gap without relying on source
  text scans.
- The parser/HIR/shape/semantic/DD render IR now preserves top-level
  `FUNCTION` definitions, `LINK` and `LINK { target }`, and Zig-style
  `SOURCE { target }` syntax as structured nodes. Execution semantics for
  function calls, pass environments, and link/source propagation are still not
  complete.
- Render lowering now expands user-defined function calls with positional,
  named, and `PASS` bindings into the function body before building the DD
  render graph. `PASSED.*` paths inside those bodies resolve through the
  structured `PASS` value. This is still compiler-side lowering work, not full
  runtime link/source propagation.
- Shape checking and generated DD render code now cover the numeric/geometry
  library calls used by local sibling Pong/Arkanoid examples:
  `Number/abs`, `Number/neg_abs`, `Number/max`, `Number/clamp`,
  `Number/percent_of_range`, `Number/scale_percent`, `Number/less_than`,
  `Number/greater_than`, and `Geometry/intersects`.
- Shape checking and generated DD render code now cover the common sibling-repo
  element/text helper surface as an explicit render-text projection:
  `Element/block`, `Element/button`, `Element/checkbox`, `Element/container`,
  `Element/grid`, `Element/label`, `Element/link`, `Element/panel`,
  `Element/paragraph`, `Element/rect`, `Element/select`, `Element/slider`,
  `Element/stack`, `Element/stripe`, `Element/svg`, `Element/svg_circle`,
  `Element/text`, `Element/text_input`, `Text/empty`, and `Text/space`. This is
  a typed generated-code path, not a source-text or example-name heuristic, but
  it is still only a text projection of the render tree.
- Physical `Scene/Element/*` calls for the same element family now resolve
  through the structured library-symbol table and lower through the same
  generated render-text projection. This removes the immediate physical
  constructor fallback for sibling examples, but it does not yet implement full
  physical scene geometry/material semantics.
- Render-root selection now uses a structured `document`-or-`scene` definition
  lookup. A pure `scene: Scene/new(...)` module lowers to a generated DD render
  graph instead of silently producing an empty render root. Modules that define
  both keep the existing `document` render root until physical-scene output is
  promoted beyond render-text projection.
- Shape checking and generated DD render code now also cover common sibling-repo
  text helper calls with direct generated DD expressions: `Text/find`,
  `Text/is_empty`, `Text/is_not_empty`, `Text/join_lines`, `Text/length`,
  `Text/repeat`, `Text/starts_with`, `Text/substring`, and `Text/trim`.
  `Text/to_number` remains blocked because upstream examples use a `NaN` branch,
  which needs an honest number/tag union shape instead of being collapsed to a
  plain number.
- Shape checking and generated DD render code now cover sibling-repo boolean
  combinators `Bool/and`, `Bool/or`, and `Bool/xor` with explicit tag output.
- Shape checking and generated DD render code now cover a bounded subset of
  sibling-repo list helpers: `List/any`, `List/every`, `List/get`,
  `List/is_empty`, `List/latest`, `List/range`, and `List/sum`.
- Shape checking and generated DD render code now cover sibling-repo math
  helpers `Math/min` and `Math/round` in the current integer generated-value
  model. This removes the helper-call gap for timer and converter-style
  examples, but it does not claim full numeric semantics for future decimal or
  number/tag union cases.
- `SOURCE { target }` and `LINK { target }` used as render pipe stages now
  preserve the piped render value instead of falling into unsupported generated
  runtime code, while still leaving explicit source/link nodes in the DD render
  graph.
- The generated host dispatch path no longer uses source-text hash fixture
  lookup. It compiles source through the compiler, dispatches by generated graph
  id, and runs checked generated Timely/DD graph crates.
- Generated render code no longer silently coerces unsupported or unresolved
  render expressions to empty text, zero numbers, false booleans, missing
  fields, passthrough values, or `GeneratedValue::Empty`. Those cases now panic
  in the generated crate instead of pretending unsupported compiler output has
  valid Boon semantics.
- Generated render execution now builds from an explicit DD render graph IR
  (`render_graph`) rather than executing the legacy recursive
  `render_program` tree. The old `render_collection_from_program` and
  `render_collection_from_expr` execution helpers are no longer in the codegen
  execution path.
- `examples/counter_hold/scenario.toml` is exercised as a multi-step
  command/source protocol, including `enable_persistence`, source action, and
  `reload`. `examples/counter_hold/expected.render.json` records the final
  generated output at epoch 2.
- The Firefox/browser proof still runs through the generated WASM graph path in
  the aggregate verifier. Browser/native launch-sensitive verification must keep
  using `cosmic-background-launch --workspace boon-dd -- ...`.
- `target/boon-artifacts/honesty-deterministic-report.json` hashes and compares
  structured generated outputs across terminal, Firefox/WASM, and native proof
  artifacts. Cross-host parity reports 22 native structured outputs and no
  terminal/browser or terminal/native mismatches.
- The deterministic `scenario-protocol` gate now compares each example's final
  generated `SmokeOutput` against `examples/<example>/expected.render.json` in
  addition to preserving ordered source/command events and per-step expected
  text. The refreshed report shows 0 scenario failures, 2 commands preserved,
  and 0 structured final-output mismatches.
- `docs/language/boon-language-manifest.toml` now contains a non-acceptance
  `cross_repo_inventory` section for known sibling-repo examples, syntax, and
  library families. This does not reduce the failing accepted-language count; it
  prevents the manifest from silently omitting known Boon surface that still
  needs implementation or an explicit product decision.

## Current Blockers

- `target/boon-artifacts/honest-compiler-report.json` still reports that the
  honest compiler is not complete. The repo has AST/HIR/shape/semantic/DD graph
  reporting and generated Timely/DD execution for the current corpus, but not
  full accepted Boon syntax and semantics.
- `target/boon-artifacts/honesty-deterministic-report.json` reports verdict
  `fail`. The failed deterministic gates are now only `source-truth` and
  `resolver-and-shape`. Both fail because the checked-in language manifest still
  marks the language, all six feature groups, and all 22 examples as
  incomplete.
- `target/boon-artifacts/language-corpus-report.json` reports verdict `fail`
  because the manifest remains an inventory, not an acceptance claim. Its
  required coverage reports now pass, but the manifest status is still
  `incomplete` and all feature/example statuses are still
  `accepted-incomplete`. The separate cross-repo inventory is intentionally
  marked `not-accepted`.
- Prompt-audit outputs are stale and failing: 7 audit JSON files exist, but
  they contain 14 hash mismatches and 17 open critical findings. Some stale
  findings describe shortcuts already removed in code, but others are still real
  blockers.
- The cross-repo prompt audit still identifies unsupported Boon syntax and
  semantics from local sibling repos. Current open gaps include executable
  host event propagation for linked/source-targeted elements, structured
  non-text element render output beyond the current render-text projection,
  complete cross-phase `PASS`/`PASSED` coverage, and any remaining unimported
  sibling-repo language surface. Marking the manifest accepted before either
  implementing these or explicitly excluding them as a product decision would
  make the verifier dishonest.

## Minimized Repro

```bash
cargo check -p xtask
cargo xtask verify-generated-freshness --format json
cargo xtask verify-generated-crates --format json
cargo xtask verify-lowering --format json
cargo xtask verify-no-shortcuts --format json
cosmic-background-launch --workspace boon-dd -- cargo xtask verify all --format json
jq '{success, failed:[.gates[] | select((.details.error? != null) or ((.details.verdict? // .verdict?) == "fail")) | {name, error:.details.error, verdict:(.details.verdict // .verdict)}]}' \
  target/boon-artifacts/verify-report.json
```

Expected current failed gates:

```text
verify-honest-compiler
verify-honesty-deterministic
verify-language-corpus
verify-prompt-audit
```

## Next Pin/Fork/Fix Decision

No dependency fork is needed for the current blocker. Continue implementation
inside this repo:

1. Import or explicitly classify the missing cross-repo Boon language surface:
   host event propagation for linked/source-targeted elements, structured
   element render output beyond the current render-text projection, full
   `PASS`/`PASSED` coverage, honest number/tag union typing for `Text/to_number`,
   remaining list contracts such as `List/chain`, `List/zip`, `List/sort_by`,
   `List/remove_last`, and `List/to_u_bits`, bit-vector/router/memory/canvas/theme
   libraries, and any remaining sibling-repo syntax or library semantics not represented in
   `docs/language/boon-language-manifest.toml`.
2. Expand syntax, resolver, shape, semantic IR, DD graph lowering, generated
   runtime, host parity, positive fixtures, and negative diagnostics for every
   accepted feature.
3. Promote cross-repo inventory entries into accepted manifest examples only
   after they have repo-local sources, scenarios, expected outputs, positive
   coverage, negative diagnostics, and generated Timely/DD host parity.
4. Mark manifest features/examples `accepted` only after the coverage artifacts
   prove the corresponding language surface is implemented by generated
   Timely/Differential execution.
5. Refresh prompt audit inputs after deterministic report and repo hashes
   stabilize, rerun the seven audits, and keep `verify-prompt-audit` failed
   until all critical findings are either fixed in code or closed by fresh
   evidence.
