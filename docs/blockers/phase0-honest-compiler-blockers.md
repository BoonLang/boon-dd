# Phase 0 Honest Compiler Blockers

This blocker report exists because `BOON_DD_HONEST_COMPILER_PLAN.md` is not
implemented end to end yet. The repo now has the Phase 0 command surface and
machine-readable reports, and the named shortcut execution symbols have been
removed from the runtime host path. The remaining compiler/runtime is still not
honest enough to satisfy the full plan because the compiler still performs
compile-time constant/render shortcut evaluation and the generated Rust consumes
a DD graph IR render program that is explicit and hashed, but still
scalar/text-only instead of a complete typed semantic-to-DD graph lowering for
render, effect, and persistence protocols.

## Failing Commands

```bash
cargo xtask verify-honest-compiler --format json
```

Result:

```text
Error: honest compiler is not implemented yet; see target/boon-artifacts/honest-compiler-report.json
```

```bash
cargo xtask verify-no-shortcuts --format json
```

Result:

```text
Error: shortcut execution patterns are still present; see target/boon-artifacts/no-shortcuts-report.json
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
  `"success": false`. The current report has 18 passed gates and 6 failed
  gates: `verify-honest-compiler`, `verify-no-shortcuts`,
  `verify-honesty-deterministic`, `verify-language-corpus`, `verify-lowering`,
  and `verify-prompt-audit`.
- `target/boon-artifacts/no-shortcuts-report.json` now reports verdict `fail`.
  It catches the remaining compiler execution shortcuts around
  `constant_text`, `constant_value`, `call_constant`, `pipe_constant`,
  `dd_render_operation_from_expr`, and `expression_has_call`. Those functions
  are used to evaluate Boon semantics or choose scalar render behavior in the
  compiler instead of lowering the semantics into typed DD operators.
- `target/boon-artifacts/honest-compiler-report.json` reports the main blockers:
  parser AST exists for the current corpus, HIR and shape checking have initial
  AST-derived reports but resolver/type coverage remains incomplete, compiler
  now consumes AST/HIR for compatibility graph construction and emits reportable
  semantic IR/DD graph IR. Generated Rust now consumes the DD graph IR render
  program, but that program is still scalar/text-only, runtime command/effect
  execution remains incomplete, and deterministic/prompt audit verification
  remains incomplete.
- `target/boon-artifacts/honesty-deterministic-report.json` reports all
  deterministic honesty gates with current evidence, hashes, and tool versions.
  Parser completeness, phase boundary, semantic IR coverage, generated-only
  runtime, adversarial no-heuristics, stale-artifact rejection, cross-host
  parity, scenario protocol, and the verifier self-test pass. Source truth,
  resolver/shape, DD lowering, and the no-shortcuts summary still fail while
  compiler shortcut functions remain.
- `target/boon-artifacts/resolver-shape-report.json` is now the canonical
  resolver/shape artifact. It enumerates every manifest example, definitions,
  source bindings, source shapes, unresolved references, shape diagnostics,
  unknown shapes, and a resolver/shape heuristic scan. The latest focused run
  has 0 unresolved references, 0 shape diagnostics, 0 unknown shapes, and 0
  missing source shapes, but still fails because HIR/shape implementation has
  path/root/library heuristics and all manifest features remain
  `accepted-incomplete`.
- `target/boon-artifacts/phase-boundary-report.json` now writes the canonical
  per-example AST/HIR/shape/semantic/DD summary and scans compiler boundary
  violations. It currently reports verdict `pass` with `0` compiler boundary
  violations after removing path-derived graph IDs, source path shape/dynamic
  fallbacks, and operator/example-specific monitor sink selection from
  `crates/boon_compiler/src/lib.rs`.
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
  and DD graph IR coverage for all 22 required examples. It reports zero
  unsupported semantic nodes, and the old `DdOutputTemplate` execution symbol is
  gone from compiler, runtime, codegen, xtask, and checked-in generated
  artifacts. Generated crates now insert typed `GeneratedSourceEvent` facts
  into Timely/DD instead of pre-collapsing `SourceAction` values into host text;
  `source_action_text`, `submit_text`, and `submit_text_and_drain` are now
  forbidden shortcut patterns. Lowering still fails because `DdRenderProgram` is
  scalar monitor/render text only and full semantic render/effect/persistence
  protocols are not lowered into DD operators yet.
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
  scenario outputs with browser generated-WASM smoke outputs and records native
  structured generated DD outputs. It reports verdict `pass`: browser smoke
  submits checked scenario actions, terminal/browser output hashes match for all
  22 examples, and native proof exposes `generated_output` for all 22 examples.
- `target/boon-artifacts/prompt-audit-report.json` now validates the required
  seven prompt-audit JSON outputs against prompt hashes, repo-state hash,
  deterministic-report hash, verdict, and critical findings. It still fails
  because no audit outputs have been produced under
  `target/boon-artifacts/prompt-audit/`.
- Phase-specific report commands now exist for the current manifest corpus:
  `verify-syntax-corpus`, `verify-resolver-corpus`, `verify-shape-corpus`,
  `verify-semantic-ir`, and `verify-generated-crates`. These reports are
  current-corpus evidence, not full accepted-language completion.
- `verify-generated-crates` now runs all 22 generated crates' own Timely/DD
  graphs against typed first-step scenario source facts and compares the final
  `SmokeOutput` to `examples/<example>/expected.render.json`; this is stronger
  than the earlier "emitted something" smoke test and the later host-text input
  path, but it is still not the full multi-step command/effect assertion.
- `boon_examples::run_embedded_matrix`, backend smoke APIs, xtask example-output
  comparison, and the native/terminal playgrounds now dispatch the generated
  Timely/DD crates directly for the canonical examples instead of compiling
  source through `RuntimeHost`. The old `RuntimeHost::compile_and_run_scenario`
  execution API has been removed; the deterministic generated-only runtime gate
  now passes with 22 generated fixture outputs and zero forbidden runtime helper
  hits. Remaining runtime blockers are the transitional output-template lowerer
  and incomplete command/effect/persistence execution.
- The deterministic scenario-protocol gate now strictly parses every manifest
  scenario, preserves ordered source/command events, and runs every parsed
  scenario step through the generated Timely/DD graph. It reports verdict
  `pass`: `examples/counter_hold/scenario.toml` step 2 preserves the ordered
  protocol `command:enable_persistence`,
  `source:store.sources.increment_button.event.press`, `command:reload`, and
  the generated graph/runtime protocol renders the expected text `1` after
  reload.
- The current deterministic honesty report has 0 stale artifact failures, 0
  adversarial heuristic failures, 6 accepted features without full coverage, and
  0 host-semantics violations. The focused `verify-no-shortcuts` report now
  records remaining compiler shortcut symbols, so those must be removed before
  the deterministic honesty verdict can honestly pass.
- Verification no longer refreshes checked-in generated artifacts as part of
  `verify all`, `verify-wasm-dd`, `verify-playgrounds`, or the terminal example
  matrix. Those paths require `verify-generated-freshness` first and fail on
  stale checked-in generated files instead of rewriting them.
- The current focused generated-crate report is
  `target/boon-artifacts/generated-crates.json`; it has 22 checked generated
  crates, 0 failures, and 0 missing crates after the typed-source input change.

## Minimized Repro

```bash
cargo check -p xtask
cargo xtask verify all --format json
jq '.success, [.gates[] | select(.status == "failed") | {name, command, error: .details.error}]' \
  target/boon-artifacts/verify-report.json
```

The expected current failed gates are `verify-honest-compiler`,
`verify-no-shortcuts`, `verify-honesty-deterministic`,
`verify-language-corpus`, `verify-lowering`, and `verify-prompt-audit`.
`verify-negative-corpus`, `verify-playgrounds`, plan coverage, generated
freshness, generated crate tests, and terminal/native/browser target tests are
expected to pass.

## Next Pin/Fork/Fix Decision

Continue with the remaining Phase 2 and Phase 3 work in
`BOON_DD_HONEST_COMPILER_PLAN.md`:

1. Finish resolver diagnostics and name/source resolution.
2. Expand shape/type checking from initial reports into full accepted-language
   coverage.
3. Replace the scalar `DdRenderProgram` path with semantic IR, DD graph IR, and
   generated-only runtime execution for structured render, effect, monitor, and
   persistence protocols.
4. Remove the compiler shortcut functions now caught by `verify-no-shortcuts`
   rather than renaming or allowlisting them.

No dependency fork is needed for this blocker. The next work is compiler and
verification implementation inside this repo.
