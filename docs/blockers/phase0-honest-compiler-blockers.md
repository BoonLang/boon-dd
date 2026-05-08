# Phase 0 Honest Compiler Blockers

This blocker report exists because `BOON_DD_HONEST_COMPILER_PLAN.md` is not
implemented end to end yet. The current checkout has the Phase 0 command
surface, generated-artifact freshness checks, generated-crate tests, and
no-shortcuts scanning in place, but the honest compiler is still incomplete.

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

```bash
cargo xtask verify-language-corpus --format json
```

Result:

```text
Error: language corpus coverage is not complete; see target/boon-artifacts/language-corpus-report.json
```

```bash
cargo xtask verify-lowering --format json
```

Result:

```text
Error: DD lowering coverage is incomplete; see target/boon-artifacts/lowering-coverage-report.json
```

```bash
cargo xtask verify-prompt-audit --format json
```

Result:

```text
Error: prompt audit is incomplete; see target/boon-artifacts/prompt-audit-report.json
```

## Passing Evidence

- `target/boon-artifacts/generated-freshness-report.json` reports verdict
  `pass`: 374 generated files checked, 0 stale, 0 missing.
- `target/boon-artifacts/generated-crates.json` reports verdict `pass`: 22
  generated crates checked. The generated crate tests now replay the full
  parsed scenario protocol rather than only the first scenario step.
- `target/boon-artifacts/no-shortcuts-report.json` reports verdict `pass`: 0
  forbidden shortcut hits and 0 shortcut symbols in execution paths.
- The generated host dispatch path no longer uses source-text hash fixture
  lookup. It compiles source through the compiler, dispatches by generated graph
  id, and runs checked generated Timely/DD graph crates.
- `examples/counter_hold/scenario.toml` is now exercised as a multi-step
  command/source protocol, including `enable_persistence`, source action, and
  `reload`. `examples/counter_hold/expected.render.json` now records the final
  generated output at epoch 2.
- The Firefox/browser proof still runs through the generated WASM graph path in
  the aggregate verifier. Browser/native launch-sensitive verification must keep
  using `cosmic-background-launch --workspace boon-dd -- ...`.

## Current Blockers

- `target/boon-artifacts/honest-compiler-report.json` still reports that the
  honest compiler is not complete. The repo has AST/HIR/shape/semantic/DD graph
  reporting and generated Timely/DD execution for the current corpus, but not
  full accepted Boon syntax and semantics.
- `target/boon-artifacts/honesty-deterministic-report.json` reports verdict
  `fail`. The failed deterministic gates are `source-truth`,
  `resolver-and-shape`, and `dd-lowering-coverage`; host semantics violations
  are currently 0.
- `target/boon-artifacts/language-corpus-report.json` reports verdict `fail`
  because 6 manifest features remain `accepted-incomplete`: source markers,
  pipes/library calls, then/when/while/latest/hold, lists/records/blocks,
  document rendering, and time/frame/physical scene.
- `target/boon-artifacts/lowering-coverage-report.json` reports verdict `fail`
  because the DD lowering still does not cover the full semantic
  render/effect/persistence protocol required by the plan.
- `target/boon-artifacts/prompt-audit-report.json` reports verdict `fail`: 7
  audit JSON files found, 0 missing, 0 schema errors, 14 hash mismatches, and
  17 open critical findings. The audit outputs are stale against the current
  repo/deterministic hashes and their findings are not all fixed.

## Minimized Repro

```bash
cargo check -p xtask
cargo xtask verify-generated-freshness --format json
cargo xtask verify-generated-crates --format json
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
verify-lowering
verify-prompt-audit
```

## Next Pin/Fork/Fix Decision

No dependency fork is needed for this blocker. Continue implementation inside
this repo:

1. Finish source-truth, resolver/shape, and DD-lowering deterministic gates
   against real compiler artifacts, not synthetic evidence.
2. Expand the language manifest from `accepted-incomplete` to `accepted` only
   when every feature has parser, resolver, shape, semantic IR, DD lowering,
   generated runtime, host parity, positive fixture, and negative diagnostic
   coverage.
3. Complete semantic render/effect/persistence lowering into generated
   Timely/Differential Dataflow execution.
4. Refresh prompt audit inputs after each deterministic report/repo hash change,
   rerun the seven audits, and keep `verify-prompt-audit` failed until all
   critical findings are fixed by code and deterministic tests.
