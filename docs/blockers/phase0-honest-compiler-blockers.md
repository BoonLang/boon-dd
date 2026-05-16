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
  `pass`: 374 generated files checked, 0 stale, 0 missing.
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
  `accepted-incomplete`.
- Prompt-audit outputs are stale and failing: 7 audit JSON files exist, but
  they contain 14 hash mismatches and 17 open critical findings. Some stale
  findings describe shortcuts already removed in code, but others are still real
  blockers.
- The cross-repo prompt audit still identifies unsupported Boon syntax and
  semantics from local sibling repos. Current open gaps include `LINK`,
  `PASS`/`PASSED`, `FUNCTION`, remaining comparison behavior, and `Number/*`
  and `Geometry/*` style library semantics. Marking the manifest accepted
  before either implementing these or explicitly excluding them as a product
  decision would make the verifier dishonest.

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
   `LINK`, `PASS`/`PASSED`, `FUNCTION`, remaining comparison operators, and
   numeric/geometry library semantics.
2. Expand syntax, resolver, shape, semantic IR, DD graph lowering, generated
   runtime, host parity, positive fixtures, and negative diagnostics for every
   accepted feature.
3. Mark manifest features/examples `accepted` only after the coverage artifacts
   prove the corresponding language surface is implemented by generated
   Timely/Differential execution.
4. Refresh prompt audit inputs after deterministic report and repo hashes
   stabilize, rerun the seven audits, and keep `verify-prompt-audit` failed
   until all critical findings are either fixed in code or closed by fresh
   evidence.
