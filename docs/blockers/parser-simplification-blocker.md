# Parser Simplification Blocker

Date: 2026-05-17

Status: resolved

## Summary

The parser simplification milestone is resolved for the current accepted Boon
syntax surface. The repo keeps the current `boon_syntax` parser because the
measured accepted-corpus parser is smaller and dependency-minimal compared with
pulling the original Chumsky parser core from `/home/martinkavik/repos/boon`.

The original Chumsky parser remains the preferred source when the language
manifest promotes more cross-repo Boon inventory into accepted syntax.

## Exact Failing Command

```bash
cargo xtask verify-pure-dd-ply-milestones --format json
```

Expected result: pass when
`target/boon-artifacts/pure-dd-ply/parser-decision.json` has
`verdict = "pass"` with measured parser LOC, dependency tree, full accepted
syntax coverage, and a documented decision that is not heuristic or deferred.

Current result: the parser decision is `pass`; the broader milestone gate is
now blocked by engine-simplicity prompt-audit freshness, not by parser
simplification.

## Current Artifact Paths

```text
target/boon-artifacts/pure-dd-ply/parser-decision.json
target/boon-artifacts/pure-dd-ply/milestone-verification.json
docs/blockers/parser-simplification-blocker.md
```

## Minimized Repro

```bash
jq '.verdict, .decision, .reason' \
  target/boon-artifacts/pure-dd-ply/parser-decision.json
cargo xtask verify-pure-dd-ply-milestones --format json
```

The first command shows the accepted parser decision and measured comparison.
The second command is the aggregate milestone gate; any current failure is not
attributed to this parser blocker unless `parser-decision.json` regresses.

## Required Fix Decision

No parser fix is currently required. Keep this blocker resolved as long as:

1. `target/boon-artifacts/pure-dd-ply/parser-decision.json` remains
   `verdict = "pass"`.
2. `cargo test -p boon_syntax`, `cargo xtask verify-syntax-corpus --format json`,
   `cargo xtask verify-negative-corpus --format json`, and
   `cargo xtask verify-language-corpus --format json` pass.
3. Any newly accepted Boon syntax is added to the manifest before claiming
   parser coverage.
