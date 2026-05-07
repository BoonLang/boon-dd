# Boon DD Honest Compiler Plan

This plan is the path from the current smoke-level Boon DD compiler to an honest compiler/runtime that supports the full Boon syntax and semantics without source-text heuristics, example-specific lowerings, semantic fallbacks, or host-side shortcut execution.

The existing `boon_timely_dd_transpiler_plan.md` remains the broader product/runtime plan. This file is the stricter compiler-correctness contract for the catch in the current implementation: Boon DD is small, but it is not yet semantically complete.

## Goal

Compile Boon source into a typed semantic graph, lower that graph into Timely/Differential Dataflow construction code, and run the same generated graph in terminal, native, and browser hosts.

Completion requires all supported examples to work because the parser, resolver, type/shape checker, semantic IR, library metadata, and DD lowerer understand Boon generally. They must not work because the compiler recognizes example text or because the runtime interprets a simplified behavior enum.

## Non-Negotiables

- Parse the entire Boon syntax with a real lexer/parser. The parser may inspect characters only as lexical input; semantic phases must consume AST/HIR/typed IR.
- Do not infer operators, sources, output text, document targets, physical scene usage, or monitor nodes from `str::contains`, substring scans, source filename conventions, or example names.
- Do not keep a semantic fallback path. Unsupported syntax, unresolved names, unsupported library calls, or non-lowerable semantics must produce structured compile errors.
- Do not add app-specific lowerings for counter, TodoMVC, Pong, CRUD, cells, or any other fixture.
- Do not let hosts run Boon semantics. Hosts may inject source facts, advance deterministic time, drain probes, render frames, and deliver effect acknowledgements.
- Generated code must construct and execute the Timely/DD graph. It must not call a host interpreter, a JS scheduler, native worker bridge semantics, or `execute_static_graph`.
- Every source file, generated file, expected output, and dependency version used by verification must be hashed into the final report so stale generated artifacts fail verification.

## Current Shortcut Removal Targets

These symbols are allowed during migration only while they are listed as failing blockers in the honest compiler report. The final gate must fail if any remain in compiler/runtime/codegen execution paths:

- `crates/boon_compiler/src/lib.rs`
  - `compile_static_graph`
  - `detect_operators`
  - `infer_sources`
  - `infer_source_paths`
  - `infer_source_shape`
  - `infer_monitor_node`
  - `infer_initial_text`
  - `infer_text_behavior`
  - `infer_document_text`
  - `infer_constant_text`
  - `infer_document_target`
  - `definition_block`
  - `text_literals`
  - `physical_scene: text.contains(...)`
- `crates/boon_dd/src/lib.rs`
  - `TextBehavior`
  - `execute_static_graph`
  - `evaluate_text`
  - smoke text emission as the runtime semantics path
- `crates/boon_codegen_rust/src/lib.rs`
  - `generated_text_collection`
  - `smoke_input_text`
  - any generated dataflow selected by `TextBehavior`
- `crates/boon_runtime_host/src/lib.rs`
  - ad hoc `parse_scenario`
  - ignoring `{ command = ... }` actions
  - `compile_and_run_step` returning only the first scenario step
  - any call into `boon_dd::execute_static_graph`
- Playground and verification code
  - direct shortcut execution through `boon_dd::execute_static_graph`
  - tests that accept text-only smoke output when a structured render/protocol output is required

## Review Findings From Current Checkout

The current repo can report green verification while still depending on shortcut semantics. Future implementation work must treat these as known verifier weaknesses, not evidence that the compiler is already honest:

- `xtask::forbidden_pattern_scan` scans for old scheduler smells, but it does not currently reject the active shortcut symbols listed above.
- `xtask::compiled_example_json` uses `RuntimeHost::compile_and_run_step`, which runs only the first scenario step through shortcut execution.
- `xtask::write_generated_artifacts` writes generated files but derives monitor/render snapshots from `boon_dd::execute_scenario`, not from the generated crate being verified.
- Generated crate tests only call `smoke_input_text()` and assert that some monitor/render output exists.
- Browser verification currently includes raw text checks for example names; it must become a structured proof that the browser ran the generated Timely/DD graph for each example.
- `boon_syntax`, `boon_hir`, `boon_shape`, `boon_host_schema`, `boon_render_ir`, and `boon_verify` are still skeletal compared with the responsibilities required in this plan.
- Scenario parsing is ad hoc and currently drops command actions instead of modeling command/effect semantics.
- Current source hashes use Rust `DefaultHasher`; final verification needs a stable artifact hash such as SHA-256, never a process/version-dependent hash.

## Target Architecture

### 1. Syntax

Replace `boon_syntax::parse_source` with a real parser.

Required parser output:

- module items and definitions
- records, lists, blocks, tags, numbers, text, paths, field access, calls, pipes, lambdas/blocks, library names, and source markers
- indentation and nesting spans
- comments and trivia handling where needed for diagnostics
- structured syntax errors with source spans

Required tests:

- parse snapshot for every `examples/*/source.bn`
- negative syntax corpus with expected diagnostics
- round-trip or AST-normalized snapshots so parser changes are reviewable

### 2. HIR And Name Resolution

Replace `boon_hir::lower` as a path-only pass with a real HIR/resolver pipeline.

Required HIR responsibilities:

- module-level definition table
- lexical scope and block parameter binding
- path resolution for local values, record fields, dynamic owner scopes, and library symbols
- `SOURCE` discovery from AST nodes, not text scans
- stable source IDs and source-family IDs from resolved paths
- document/render target resolution from typed expressions, not marker substrings
- monitor tap selection from semantic graph roots, not example IDs

Required failure mode:

- unresolved names, ambiguous fields, invalid dynamic owner references, and missing host bindings fail compilation with structured diagnostics.

### 3. Shape And Type Checking

Replace shape inference by path-name guessing with a real shape/type pass.

Required shape model:

- records, lists, text, numbers, booleans/tags, empty records, source leaves, commands/effects, element/document/scene values, durations, and optional/pending values where Boon semantics need them
- shape variables and unification for `SOURCE` leaves
- library function signatures with purity, source binding, effect, render, persistence, and DD-lowerability metadata
- diagnostics for invalid operators, incompatible tags, invalid list item shapes, and unsupported host/library contracts

Required output:

- typed HIR or typed semantic IR that is the only compiler input accepted by lower phases.

### 4. Semantic IR

Introduce a compiler-owned semantic IR that represents Boon semantics explicitly.

Required IR coverage:

- source leaves and dynamic source families
- records, field access, lists, tags, text, numbers, constants
- pipe chains and calls
- `THEN`, constant `THEN`, `WHEN`, `WHILE`, `LATEST`, `HOLD`, keyed hold, and persistence taps
- list append/remove/map/retain/count/latest and block captures
- timers, frames, keyboard/mouse/text-input sources, commands, effects, and acknowledgements
- document/element rendering and physical scene rendering
- monitor taps, render sinks, effect sinks, and storage sinks

The IR must be independent of Timely/DD details. It should describe Boon dependencies, ownership, event epochs, source families, and output contracts. Timely/DD lowering is a separate phase.

### 5. Timely/DD Lowering

Lower typed semantic IR into a declarative DD graph IR, then generate Rust code from that graph IR.

Required lowerer behavior:

- every semantic IR node either lowers into a known DD operator/template or emits an unsupported diagnostic
- lowering decisions are selected by typed operator kind and library metadata, never by source text
- source collections preserve source ID, owner key, generation, payload shape, Boon time, and diff
- stateful operators define explicit arrangements/frontiers/persistence interactions
- render/effect/monitor outputs use one structured protocol across terminal, native, and browser
- generated Rust includes source hash, IR hash, lowering hash, dependency versions, and protocol schema version

Required generated code behavior:

- build a Timely/DD dataflow
- expose source injection, time advancement, probe draining, output draining, and shutdown APIs
- never call compiler internals or runtime shortcut evaluators

### 6. Runtime Host

Replace `RuntimeHost::compile_and_run_step` with a generated-graph runtime API.

Required host responsibilities:

- compile source to generated graph or load a verified generated crate
- inject scenario actions and real UI events as typed source facts
- advance a deterministic `TimeSource` in tests and a host clock in playgrounds
- drain Timely probes until the target frontier is reached or fail with a bounded stall diagnostic
- collect monitor/render/effect/persistence outputs from the generated graph
- run every scenario step, including command/effect steps

The runtime host must not evaluate Boon expressions, count actions for app logic, branch on tags for app logic, or construct output text directly.

### 7. Corpus And Coverage

Create a checked-in corpus manifest that defines the full accepted language surface.

Required manifest path:

```text
docs/language/boon-language-manifest.toml
```

The manifest must be the only source of truth for the accepted language surface and canonical examples. Duplicated hard-coded example lists in crates or `xtask` must be generated from, or checked against, this manifest.

Required corpus inputs:

- all current `examples/*/source.bn`
- all current `examples/*/scenario.toml`
- all current `examples/*/expected.render.json`
- imported examples from the other `~/repos/boon-*` implementations when they represent syntax or semantics not covered here
- negative syntax examples
- negative resolver/type examples
- unsupported-library examples that must fail with diagnostics until implemented

Required corpus metadata:

- source path
- semantic features used
- required host features
- expected diagnostics or expected outputs
- source hash, scenario hash, expected-output hash
- accepted language version
- required parser, resolver, shape, semantic IR, DD lowering, generated runtime, and host-parity coverage IDs

Coverage is honest only when every accepted syntax/semantic feature has at least one positive example and at least one relevant negative diagnostic test.

## Implementation Phases

### Phase 0: Baseline And Blocker Report

Add the full `xtask` command surface required by this plan. These commands may initially fail, but they must exist and write structured blocker reports:

```bash
cargo xtask verify-honest-compiler --format json
cargo xtask verify-no-shortcuts --format json
cargo xtask verify-honesty-deterministic --format json
cargo xtask verify-language-corpus --format json
cargo xtask verify-negative-corpus --format json
cargo xtask verify-lowering --format json
cargo xtask verify-generated-freshness --format json
cargo xtask write-honest-compiler-prompts --format json
cargo xtask verify-prompt-audit --format json
```

Also keep the existing target command shape wired into final verification:

```bash
cargo xtask test --target terminal
cargo xtask test --target native
cargo xtask test --target browser
```

`cargo xtask verify-honest-compiler --format json` is the top-level honest-compiler status command.

At first this command should fail and write:

```text
target/boon-artifacts/honest-compiler-report.json
```

The report must list all known shortcut symbols, all examples currently passing through shortcuts, all unsupported parser/resolver/type/lowering features, and the exact commands needed to reproduce the failure.

This phase is complete when the report is deterministic and the repo can honestly say why the compiler is not complete yet.

### Phase 1: Real Parser

Implement the lexer/parser and replace the source-preserving placeholder AST.

Gates:

```bash
cargo test -p boon_syntax
cargo xtask verify-syntax-corpus --format json
```

No semantic behavior may be inferred from raw source text outside the parser after this phase.

### Phase 2: HIR And Resolver

Implement HIR lowering, scopes, definitions, paths, source discovery, and library-symbol resolution.

Gates:

```bash
cargo test -p boon_hir
cargo xtask verify-resolver-corpus --format json
```

`infer_sources`, `infer_source_paths`, `definition_block`, and document-target substring detection must be removed by the end of this phase.

### Phase 3: Shape/Type Pass

Implement shape variables, unification, source leaf shape solving, and library signatures.

Gates:

```bash
cargo test -p boon_shape
cargo xtask verify-shape-corpus --format json
```

`infer_source_shape` and path-name shape guessing must be removed by the end of this phase.

### Phase 4: Semantic IR

Introduce typed semantic IR and compile all accepted examples into it.

Gates:

```bash
cargo test -p boon_compiler
cargo xtask verify-semantic-ir --format json
```

`detect_operators`, `infer_monitor_node`, `infer_initial_text`, `infer_text_behavior`, `infer_constant_text`, `TextBehavior`, and `physical_scene: text.contains(...)` must be removed by the end of this phase.

### Phase 5: DD Graph IR And Codegen

Lower semantic IR to DD graph IR and generate Rust graph-construction code from typed operator nodes.

Gates:

```bash
cargo test -p boon_dd -p boon_codegen_rust
cargo xtask verify-lowering --format json
cargo xtask verify-generated-crates --format json
```

`generated_text_collection`, `smoke_input_text`, and TextBehavior-selected code generation must be removed by the end of this phase.

### Phase 6: Generated Runtime Host

Replace shortcut execution with the generated graph runtime API in test harnesses and playgrounds.

Gates:

```bash
cargo test -p boon_runtime_host
cargo xtask example-matrix --format json
cargo xtask test --target terminal
cargo xtask test --target native
cargo xtask test --target browser
```

`execute_static_graph`, `evaluate_text`, one-step scenario execution, command-skipping scenario parsing, and host-side semantic output construction must be removed by the end of this phase.

### Phase 7: Browser/Native/Terminal Parity

Run the same generated graph in all hosts.

Gates:

```bash
cargo xtask verify-wasm-dd --required --browser firefox
cargo xtask verify-playgrounds --format json
cargo xtask verify all --format json
```

Any GUI verification command from this repo must wrap the actual window-creating process with:

```bash
cosmic-background-launch --workspace boon-dd -- ...
```

### Phase 8: Full Language Acceptance

Expand the corpus until it covers the entire Boon syntax and semantic contract, not only the current examples.

Gates:

```bash
cargo xtask verify-language-corpus --format json
cargo xtask verify-negative-corpus --format json
cargo xtask verify-no-shortcuts --format json
cargo xtask verify all --format json
```

This phase is complete only when unsupported accepted-language features are zero. If a feature is intentionally out of scope, it must be removed from the accepted-language manifest and listed as a product decision, not hidden behind a runtime fallback.

## Shortcut Guardrail Gate

`cargo xtask verify-no-shortcuts --format json` must scan source and generated artifacts for forbidden compiler/runtime execution patterns.

The final forbidden list must include at least:

```text
detect_operators
infer_sources
infer_source_paths
infer_source_shape
infer_monitor_node
infer_initial_text
infer_text_behavior
infer_document_text
infer_constant_text
infer_document_target
definition_block
text_literals
TextBehavior
execute_static_graph
evaluate_text
generated_text_collection
smoke_input_text
compile_and_run_step
line.contains("command =")
```

The gate should allow the strings only in:

- this plan file
- historical blocker reports
- tests that assert the guardrail catches them

It must also reject source-text semantic scans in compiler/lowering/runtime crates, including `contains("SOURCE")`, `contains("THEN")`, `contains("WHEN")`, `contains("WHILE")`, `contains("LATEST")`, `contains("HOLD")`, `contains("List/")`, `contains("Scene/new(")`, and equivalent regex or substring scans.

Scenario fixtures may still contain command actions. The forbidden pattern is parser/runtime code that detects command text and drops the action instead of modeling the command in the scenario protocol.

## Verification Strategy

There are two verification layers:

1. deterministic gates that must be sufficient to prove the compiler/runtime contract from source, artifacts, and executions; and
2. prompt-driven adversarial audits that look for gaps humans and static checks may miss.

Prompt audits are useful, but they are not a substitute for deterministic gates. A prompt audit may block completion by finding a real issue. A prompt audit passing is never enough by itself to mark the compiler honest.

### Artifact Hashing And Freshness

All verification hashes must use SHA-256 over canonical bytes:

- source, scenario, and expected-output files use repo-relative paths, LF line endings, and raw file bytes after line-ending normalization
- JSON artifacts use canonical JSON with sorted object keys and no insignificant whitespace
- generated Rust uses `rustfmt` output bytes
- command reports include the command argv, tool versions, dependency versions, relevant environment variables, and input artifact hashes

Do not use `DefaultHasher` or any other process/version-dependent hash for verification artifacts.

Freshness verification must be non-mutating:

1. regenerate all generated artifacts into a temporary directory;
2. canonicalize and hash temporary and checked-in/generated artifacts;
3. fail on any missing, extra, changed, stale, or wrong-hash artifact; and
4. write only reports under `target/boon-artifacts/`.

`cargo xtask verify all --format json` may update reports under `target/boon-artifacts/`, but it must not silently rewrite source, examples, prompts, expected outputs, or checked-in generated artifacts.

### Deterministic Verification

Add `cargo xtask verify-honesty-deterministic --format json`.

This command must write:

```text
target/boon-artifacts/honesty-deterministic-report.json
```

The report must be generated from the current checkout and include the git commit or dirty-tree hash, source hashes, corpus hashes, generated-code hashes, expected-output hashes, tool versions, dependency versions, and every gate result.

Required deterministic gates:

- `source-truth`
  - Validate the checked-in language/corpus manifest.
  - Prove every accepted syntax and semantic feature is mapped to at least one positive fixture and one negative diagnostic fixture.
  - Fail if an accepted-language feature has no parser, resolver, shape, semantic IR, DD lowering, generated-code, runtime, and host-parity coverage.
- `parser-completeness`
  - Parse every accepted source file into AST.
  - Snapshot ASTs and diagnostics.
  - Run parser fuzz/round-trip tests for indentation, comments, records, lists, tags, paths, calls, pipes, blocks, `SOURCE`, and malformed input.
- `phase-boundary`
  - Emit AST, HIR, typed HIR, semantic IR, DD graph IR, and generated Rust summaries for every corpus case.
  - Hash each phase output.
  - Fail if a later phase reads raw source text for semantic decisions instead of consuming the previous structured phase.
- `resolver-and-shape`
  - Verify definitions, path resolution, source families, dynamic owner scopes, host bindings, library calls, shape variables, and type errors against golden reports.
  - Fail on unresolved names, ambiguous paths, guessed source shapes, or path-name-derived types.
- `semantic-ir-coverage`
  - Map every accepted language feature to semantic IR node kinds.
  - Fail if any accepted feature compiles through an `Unknown`, `Smoke`, `TextBehavior`, fixture name, raw string, or fallback node.
- `dd-lowering-coverage`
  - Map every semantic IR node kind to a DD graph IR lowering or a structured unsupported diagnostic.
  - Fail if an accepted feature is unsupported.
  - Fail if any lowering is selected from source text, source path, example name, or expected output.
- `generated-only-runtime`
  - Build and run generated crates for every corpus case.
  - Fail if generated crates or host execution paths link to or call shortcut evaluators such as `execute_static_graph`, `evaluate_text`, `TextBehavior`, `compile_and_run_step`, or smoke-only helpers.
  - Fail if backend crates reconstruct Boon semantics instead of injecting source facts and draining generated graph outputs.
- `scenario-protocol`
  - Parse scenarios with a structured TOML parser.
  - Execute every step, including command/effect/persistence steps.
  - Fail if scenario parsing drops unknown fields, command actions, effect acknowledgements, owners, generations, or timing directives.
- `adversarial-no-heuristics`
  - Create generated negative fixtures that would fool substring-based compilers:
    - comments containing `SOURCE`, `THEN`, `WHEN`, `WHILE`, `LATEST`, `HOLD`, `List/append`, and `Scene/new(`
    - text literals containing operator names
    - renamed example directories and source files
    - reordered definitions
    - irrelevant whitespace and comments
    - inert records with trigger-looking field names
  - Assert these transformations do not change semantics unless the AST semantics changed.
  - Mutate real operators and assert outputs or diagnostics do change.
- `stale-artifact-rejection`
  - Fail if generated Rust, graph JSON, render snapshots, browser artifacts, or WASM artifacts do not match the source/scenario/dependency hashes in the manifest.
  - Include explicit stale, missing, extra, and wrong-hash negative cases.
- `cross-host-parity`
  - Run terminal, native, and browser hosts against the same generated graph protocol.
  - Compare structured monitor/render/effect/persistence outputs, not screenshots alone.
  - Browser proof must be Firefox-hosted and must prove Timely/DD graph execution in the browser process.
- `verification-harness-self-test`
  - Intentionally inject a known shortcut, stale artifact, missing expected file, skipped scenario step, wrong fixture output, and disabled DD lowering in temporary test fixtures.
  - Prove the verifier fails for each injected fault.

The deterministic report must end with:

```json
{
  "verdict": "pass",
  "shortcut_symbols_in_execution_paths": 0,
  "accepted_features_without_full_coverage": 0,
  "stale_artifact_failures": 0,
  "host_semantics_violations": 0,
  "adversarial_heuristic_cases_failed": 0,
  "prompt_audit_required": true
}
```

Any nonzero value or missing field fails the honest compiler goal.

### Prompt-Driven Audit

Add a checked-in prompt pack under:

```text
docs/prompts/honest-compiler/
```

Minimum prompts:

- `01_shortcut_and_fallback_audit.md`
  - Ask the auditor to find any source-text semantic scans, fallback paths, fixture-specific logic, shortcut runtime execution, app-specific lowerings, or host-side Boon semantics.
- `02_language_completeness_audit.md`
  - Ask the auditor to compare the language/corpus manifest, parser, HIR, shape pass, semantic IR, DD lowering, generated code, and tests. The expected output is a list of accepted features with missing coverage.
- `03_runtime_boundary_audit.md`
  - Ask the auditor to trace a source event through generated graph execution into monitor/render/effect outputs and identify any place where a host or smoke helper evaluates Boon semantics.
- `04_verifier_fake_pass_audit.md`
  - Ask the auditor to attack the verification harness: stale artifacts, skipped steps, weak assertions, string-only checks, browser/native smoke that does not prove generated graph execution, and missing negative tests.
- `05_cross_repo_semantics_audit.md`
  - Ask the auditor to compare the accepted Boon syntax/semantics against the other `~/repos/boon-*` implementations and list syntax or semantics still missing from this repo.

Add `cargo xtask write-honest-compiler-prompts --format json` to regenerate the prompt pack and write:

```text
target/boon-artifacts/honest-compiler-prompt-pack.json
```

The prompt-pack report must include every prompt hash, the repo state hash, the corpus manifest hash, and the deterministic report hash that the prompts ask auditors to inspect.

Add `cargo xtask verify-prompt-audit --format json` to validate prompt audit outputs stored under:

```text
target/boon-artifacts/prompt-audit/
```

Each prompt audit output must use this schema:

```json
{
  "prompt_id": "01_shortcut_and_fallback_audit",
  "prompt_hash": "...",
  "repo_state_hash": "...",
  "deterministic_report_hash": "...",
  "verdict": "pass | fail | inconclusive",
  "critical_findings": [
    {
      "summary": "...",
      "path": "...",
      "line": 0,
      "evidence": "...",
      "required_fix": "..."
    }
  ],
  "reviewed_files": ["..."],
  "reviewed_artifacts": ["..."],
  "commands_reviewed": ["..."]
}
```

Prompt audit rules:

- `fail` blocks the honest compiler goal.
- `inconclusive` blocks the honest compiler goal until rerun or explicitly converted into a checked-in blocker report.
- `pass` is accepted only if prompt hash, repo state hash, and deterministic report hash match the current run.
- At least two independent auditors must run `01_shortcut_and_fallback_audit.md` and `04_verifier_fake_pass_audit.md`.
- Any critical finding must be resolved by code changes plus a deterministic regression test. Marking a prompt finding as "won't fix" requires a checked-in product decision that removes the affected feature from the accepted-language manifest or explains why the finding is not in an execution path.

The prompt-audit summary must be written to:

```text
target/boon-artifacts/prompt-audit-report.json
```

The report must include:

```json
{
  "verdict": "pass",
  "audits_required": 7,
  "audits_passed": 7,
  "critical_findings_open": 0,
  "inconclusive_audits": 0,
  "hash_mismatches": 0
}
```

Prompt auditing determines whether the implementation looks clean and honest under adversarial review. Deterministic gates determine whether the compiler is mechanically proven clean enough to accept. The final goal requires both.

## Required Final Verification

The honest compiler goal is complete only when this command passes:

```bash
cargo xtask verify all --format json
```

That command must call, or strictly include the results from, `verify-honest-compiler`, `verify-no-shortcuts`, `verify-honesty-deterministic`, `verify-generated-freshness`, and `verify-prompt-audit`.

The final report must include:

```text
target/boon-artifacts/success.json
target/boon-artifacts/verify-report.json
target/boon-artifacts/honest-compiler-report.json
target/boon-artifacts/honesty-deterministic-report.json
target/boon-artifacts/language-corpus-report.json
target/boon-artifacts/lowering-coverage-report.json
target/boon-artifacts/generated-freshness-report.json
target/boon-artifacts/no-shortcuts-report.json
target/boon-artifacts/honest-compiler-prompt-pack.json
target/boon-artifacts/prompt-audit-report.json
```

`success.json` must report:

- `success: true`
- zero failed gates
- zero shortcut symbols in execution paths
- zero accepted-language features without parser, resolver, type, semantic IR, lowering, generated-code, and runtime-host coverage
- exact source, scenario, expected-output, generated-code, dependency, and tool hashes
- browser Timely/DD proof for Firefox
- terminal/native/browser output parity for the canonical corpus
- deterministic honesty verdict `pass`
- prompt-audit verdict `pass`

## Definition Of Done

The compiler is honest when a new Boon program either:

1. parses, resolves, type-checks, lowers, generates, and runs through the generated Timely/DD graph with deterministic outputs; or
2. fails before runtime with a structured diagnostic that names the unsupported syntax, unresolved name, invalid shape, or non-lowerable library contract.

There is no third state where the compiler guesses from source text, maps a fixture to canned behavior, runs an interpreter fallback, drops command steps, or lets a host reconstruct Boon semantics.
