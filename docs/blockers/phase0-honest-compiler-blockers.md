# Phase 0 Honest Compiler Blockers

This blocker report exists because `BOON_DD_HONEST_COMPILER_PLAN.md` is not
implemented end to end yet. The repo now has the Phase 0 command surface and
machine-readable reports, but the actual compiler/runtime remains shortcut
based.

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
cargo xtask verify-negative-corpus --format json
cargo xtask verify-lowering --format json
cargo xtask verify-generated-freshness --format json
cargo xtask verify-prompt-audit --format json
```

These commands also fail intentionally and write their corresponding reports
under `target/boon-artifacts/`.

## Current Evidence

- `target/boon-artifacts/no-shortcuts-report.json` reports `117` shortcut
  pattern hits in execution paths and generated code.
- `target/boon-artifacts/honest-compiler-report.json` reports the main blockers:
  parser AST exists for the current corpus, HIR and shape checking have initial
  AST-derived reports but resolver/type coverage remains incomplete, compiler
  now consumes AST/HIR for compatibility graph construction but real semantic IR
  and DD graph IR are not implemented,
  `TextBehavior` runtime semantics remain, smoke codegen remains,
  scenario parsing now models command actions but runtime command/effect
  execution remains incomplete, and deterministic/prompt audit verification
  remains incomplete.
- `target/boon-artifacts/honesty-deterministic-report.json` reports all
  deterministic honesty gates as missing.
- `target/boon-artifacts/honest-compiler-prompt-pack.json` reports the checked-in
  prompt pack hashes.

## Minimized Repro

```bash
cargo check -p xtask
cargo xtask verify-no-shortcuts --format json
jq '.shortcut_symbols_in_execution_paths, .scan.hits[:12]' target/boon-artifacts/no-shortcuts-report.json
```

The first hits are in `crates/boon_codegen_rust/src/lib.rs`, where generated
dataflow is still selected through `TextBehavior`, `generated_text_collection`,
and `smoke_input_text`.

## Next Pin/Fork/Fix Decision

Continue with the remaining Phase 2 and Phase 3 work in
`BOON_DD_HONEST_COMPILER_PLAN.md`:

1. Finish resolver diagnostics and name/source resolution.
2. Expand shape/type checking from initial reports into full accepted-language
   coverage.
3. Replace the compatibility `StaticGraph`/`TextBehavior` path with semantic IR,
   DD graph IR, and generated-only runtime execution.
4. Keep `verify-no-shortcuts` failing until the execution paths no longer use
   text heuristics, smoke runtime semantics, or host-side Boon behavior.

No dependency fork is needed for this blocker. The next work is compiler and
verification implementation inside this repo.
