# Phase 0 Honest Compiler Blockers

This blocker report exists because `BOON_DD_HONEST_COMPILER_PLAN.md` is not
implemented end to end yet. The repo now has the Phase 0 command surface and
machine-readable reports, and the named shortcut execution symbols have been
removed from Rust execution paths. The remaining compiler/runtime is still not
honest enough to satisfy the full plan because the generated Rust consumes a DD
graph IR output template that is still derived from the transitional static
graph plan, and the runtime/static graph path still carries a compatibility
scalar DD plan.

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
  template, but that template is still derived from the transitional static
  graph plan, and runtime command/effect
  execution remains incomplete, and deterministic/prompt audit verification
  remains incomplete.
- `target/boon-artifacts/honesty-deterministic-report.json` reports all
  deterministic honesty gates with current evidence, hashes, and tool versions.
  Parser completeness, semantic IR coverage, adversarial no-heuristics, and
  stale-artifact rejection pass; source truth, phase boundary, resolver/shape,
  DD lowering, generated-only runtime, scenario protocol, cross-host parity, and
  verifier self-tests still fail.
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
  unsupported semantic nodes, but still fails because the runtime/static graph
  still carries the compatibility scalar DD plan and the DD graph IR output
  template is transitional.
- `target/boon-artifacts/language-corpus-report.json` reports no structural
  manifest errors, no missing example entries, and no missing negative coverage,
  but still fails because features and examples are explicitly
  `accepted-incomplete`.
- `target/boon-artifacts/honest-compiler-prompt-pack.json` reports the checked-in
  prompt pack hashes, repo-state hash, and deterministic-report hash.
- `target/boon-artifacts/prompt-audit-report.json` now validates the required
  seven prompt-audit JSON outputs against prompt hashes, repo-state hash,
  deterministic-report hash, verdict, and critical findings. It still fails
  because no audit outputs have been produced under
  `target/boon-artifacts/prompt-audit/`.
- Phase-specific report commands now exist for the current manifest corpus:
  `verify-syntax-corpus`, `verify-resolver-corpus`, `verify-shape-corpus`,
  `verify-semantic-ir`, and `verify-generated-crates`. These reports are
  current-corpus evidence, not full accepted-language completion.
- The deterministic scenario-protocol gate now strictly parses every manifest
  scenario and reports preserved command actions. It still fails because
  command/effect/persistence execution and skip-fault self-tests are incomplete.
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
3. Replace the compatibility scalar DD plan path with semantic IR, DD graph IR,
   and generated-only runtime execution.
4. Keep `verify-no-shortcuts` passing while broadening the deterministic gates
   so renamed or newly introduced shortcuts cannot slip through.

No dependency fork is needed for this blocker. The next work is compiler and
verification implementation inside this repo.
