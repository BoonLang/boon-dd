# Phase 0 Honest Compiler Blockers

This blocker report exists because `BOON_DD_HONEST_COMPILER_PLAN.md` is not
implemented end to end yet. The repo now has the Phase 0 command surface and
machine-readable reports, and the named shortcut execution symbols have been
removed from Rust execution paths. The remaining compiler/runtime is still not
honest enough to satisfy the full plan because the generated Rust consumes a DD
graph IR output template that is still a transitional scalar render template
instead of a complete typed semantic-to-DD graph lowering.

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
cargo xtask verify-lowering --format json
cargo xtask verify-prompt-audit --format json
```

These commands also fail intentionally and write their corresponding reports
under `target/boon-artifacts/`.

## Current Evidence

- `cargo xtask verify all --format json` exits with status `1` and writes
  `target/boon-artifacts/verify-report.json` plus `success.json` with
  `"success": false`. The current report has 19 passed gates and 5 failed
  gates: `verify-honest-compiler`, `verify-honesty-deterministic`,
  `verify-language-corpus`, `verify-lowering`, and `verify-prompt-audit`.
- `target/boon-artifacts/no-shortcuts-report.json` now reports verdict `pass`
  with `0` shortcut pattern hits in Rust execution paths and generated code.
- `target/boon-artifacts/honest-compiler-report.json` reports the main blockers:
  parser AST exists for the current corpus, HIR and shape checking have initial
  AST-derived reports but resolver/type coverage remains incomplete, compiler
  now consumes AST/HIR for compatibility graph construction and emits reportable
  semantic IR/DD graph IR. Generated Rust now consumes the DD graph IR output
  template, but that template is still transitional, runtime command/effect
  execution remains incomplete, and deterministic/prompt audit verification
  remains incomplete.
- `target/boon-artifacts/honesty-deterministic-report.json` reports all
  deterministic honesty gates with current evidence, hashes, and tool versions.
  Parser completeness, semantic IR coverage, generated-only runtime,
  adversarial no-heuristics, and stale-artifact rejection pass; source truth,
  phase boundary, resolver/shape, DD lowering, scenario protocol, cross-host
  parity still fail. The verifier self-test gate now passes with synthetic
  injected-fault checks.
- `target/boon-artifacts/plan-coverage.json` reports no forbidden-pattern hits
  and no missing required generated artifact paths, including
  `generated/<example>/dd_graph_ir.json` for all 22 required examples.
- `target/boon-artifacts/negative-corpus-report.json` now reports verdict
  `pass` across syntax, resolver, shape, unsupported-library, and
  adversarial no-heuristics cases.
- `target/boon-artifacts/generated-freshness-report.json` now reports verdict
  `pass` after regenerating every required generated artifact into a temporary
  directory and comparing SHA-256 hashes against the checked-in generated tree.
  The current checked set is 374 generated artifacts with 0 stale and 0
  missing paths.
- `target/boon-artifacts/lowering-coverage-report.json` now reports semantic IR
  and DD graph IR coverage for all 22 required examples. It now reports zero
  unsupported semantic nodes, and `StaticGraph` no longer carries a scalar
  runtime plan. It still fails because the DD graph IR output template is a
  transitional scalar render template and full semantic render/effect/
  persistence protocols are not lowered yet.
- `target/boon-artifacts/language-corpus-report.json` reports no structural
  manifest errors, no missing example entries, and no missing negative coverage,
  and now checks the manifest example set against both `boon_dd::REQUIRED_EXAMPLES`
  and embedded example fixtures. It still fails because features and examples
  are explicitly `accepted-incomplete`.
- `target/boon-artifacts/honest-compiler-prompt-pack.json` reports the checked-in
  prompt pack hashes, repo-state hash, and deterministic-report hash.
- `target/boon-artifacts/verification-harness-self-test-report.json` now reports
  verdict `pass` for synthetic injected faults covering shortcut insertion,
  stale artifact hashes, skipped multi-step scenarios, wrong generated fixture
  outputs, and disabled DD lowering.
- `target/boon-artifacts/cross-host-parity-report.json` now compares terminal
  scenario outputs with browser generated-WASM smoke outputs and records the
  native parity gap. It still fails because browser smoke is not yet the
  canonical per-example scenario protocol and native proof does not expose
  structured generated DD outputs. The current terminal/browser mismatches are
  `latest`, `when`, `while`, and `list_retain_reactive`; native structured
  outputs are `0` of `22`.
- `target/boon-artifacts/prompt-audit-report.json` now validates the required
  seven prompt-audit JSON outputs against prompt hashes, repo-state hash,
  deterministic-report hash, verdict, and critical findings. It still fails
  because no audit outputs have been produced under
  `target/boon-artifacts/prompt-audit/`.
- Phase-specific report commands now exist for the current manifest corpus:
  `verify-syntax-corpus`, `verify-resolver-corpus`, `verify-shape-corpus`,
  `verify-semantic-ir`, and `verify-generated-crates`. These reports are
  current-corpus evidence, not full accepted-language completion.
- `verify-generated-crates` still runs each generated crate's own Timely/DD
  graph against the first checked scenario step for that example and compares
  the final `SmokeOutput` to `examples/<example>/expected.render.json`; this is
  stronger than the earlier "emitted something" smoke test, but it is still not
  the full multi-step command/effect assertion.
- `boon_examples::run_embedded_matrix`, backend smoke APIs, xtask example-output
  comparison, and the native/terminal playgrounds now dispatch the generated
  Timely/DD crates directly for the canonical examples instead of compiling
  source through `RuntimeHost`. The old `RuntimeHost::compile_and_run_scenario`
  execution API has been removed; the deterministic generated-only runtime gate
  now passes with 22 generated fixture outputs and zero forbidden runtime helper
  hits. Remaining runtime blockers are the transitional output-template lowerer
  and incomplete command/effect/persistence execution.
- The deterministic scenario-protocol gate now strictly parses every manifest
  scenario, preserves command actions, and runs every parsed scenario step
  through the generated Timely/DD graph. It still fails because command/effect/
  persistence execution and skip-fault self-tests are incomplete. The current
  minimized mismatch is `examples/counter_hold/scenario.toml` step 2: the
  preserved commands are `enable_persistence` and `reload`, the generated graph
  executes the step in epoch 2, and the actual render text is `2` while the
  scenario expects `1`.
- The current deterministic honesty report has 0 stale artifact failures, 0
  shortcut execution symbols, 0 adversarial heuristic failures, 6 accepted
  features without full coverage, and 1 host-semantics violation.

## Minimized Repro

```bash
cargo check -p xtask
cargo xtask verify all --format json
jq '.success, [.gates[] | select(.status == "failed") | {name, command, error: .details.error}]' \
  target/boon-artifacts/verify-report.json
```

The expected current failed gates are `verify-honest-compiler`,
`verify-honesty-deterministic`, `verify-language-corpus`,
`verify-lowering`, and `verify-prompt-audit`.
`verify-no-shortcuts`, `verify-negative-corpus`, `verify-playgrounds`, plan
coverage, generated freshness, generated crate tests, and
terminal/native/browser target tests are expected to pass.

## Next Pin/Fork/Fix Decision

Continue with the remaining Phase 2 and Phase 3 work in
`BOON_DD_HONEST_COMPILER_PLAN.md`:

1. Finish resolver diagnostics and name/source resolution.
2. Expand shape/type checking from initial reports into full accepted-language
   coverage.
3. Replace the transitional scalar output template path with semantic IR, DD
   graph IR, and generated-only runtime execution.
4. Keep `verify-no-shortcuts` passing while broadening the deterministic gates
   so renamed or newly introduced shortcuts cannot slip through.

No dependency fork is needed for this blocker. The next work is compiler and
verification implementation inside this repo.
