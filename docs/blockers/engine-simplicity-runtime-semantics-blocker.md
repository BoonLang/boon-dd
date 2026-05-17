# Engine Simplicity Runtime Blocker

Date: 2026-05-17

Status: resolved

## Summary

The old app-window/WGPU renderer blocker is resolved: active native/browser
playground proof now uses `crates/boon_backend_ply`, and
`cargo xtask verify-ply-renderer --format json` passes.

The runtime-session blocker is resolved on the current checkout. The Ply UI now
stores live generated graph sessions per example, uses generated session drain
APIs for interactions, and no longer constructs `build_dataflow(&mut worker)`
or rewinds/replays counter clicks in `crates/boon_backend_ply`.

## Passing Commands

```bash
cargo check --workspace
cargo test --workspace
cargo xtask verify-ply-renderer --format json
cargo xtask verify-playgrounds --format json
cargo xtask verify-no-shortcuts --format json
cargo xtask verify-dd-purity --format json
cargo xtask verify-honesty-deterministic --format json
cargo xtask verify-persistent-runtime --format json
```

Current passing artifacts:

```text
target/boon-artifacts/ply/verify-ply-renderer.json
target/boon-artifacts/ply/no-old-renderers.json
target/boon-artifacts/ply/native-smoke.json
target/boon-artifacts/ply/browser-smoke.json
target/boon-artifacts/verify-playgrounds.json
target/boon-artifacts/honesty-deterministic-report.json
target/boon-artifacts/engine-simplicity/persistent-runtime-report.json
```

Observed passing results:

- `verify-ply-renderer.json`: `success = true`
- native Ply smoke: `backend = "ply-engine"`, `target = "native"`,
  `example_count = 22`, `ply_frame_presented = true`
- Firefox browser Ply smoke: `backend = "ply-engine"`, `target = "browser"`,
  `example_count = 22`, `firefox = true`, `canvas_nonblank = true`
- `persistent-runtime-report.json`: `verdict = "pass"`, failures `[]`
- `counter_100_interactions.json`: `runtime_graph_builds_per_interaction_session = 1`,
  `interactions = 100`, `final_text = "100"`

## Former Failing Command

```bash
cargo xtask verify-persistent-runtime --format json
```

Former observed output:

```text
engine-simplicity gate cargo xtask verify-persistent-runtime --format json reported fail
```

Current failing artifact:

```text
target/boon-artifacts/engine-simplicity/persistent-runtime-report.json
```

The report includes this hit:

```text
path: crates/boon_backend_ply/src/app.rs
pattern: build_dataflow(&mut worker)
reason: runtime/test path constructs a new graph inside interaction or scenario execution
```

Current repro now passes:

```bash
cargo xtask verify-persistent-runtime --format json
jq '.verdict, .failures, .evidence' \
  target/boon-artifacts/engine-simplicity/persistent-runtime-report.json
```

Current result: the persistent runtime report passes, the failure list is empty,
and the counter stress artifact proves one generated graph build for the
100-interaction session.

## Fix Decision

Fixed in this repository. The resolution path was:

1. Added generated `GeneratedGraphSession` APIs and regenerated the generated
   crates.
2. Changed the Ply app to hold generated sessions instead of output vectors or
   counter-specific sessions.
3. Removed fake decrement behavior because the accepted counter source has no
   decrement source binding.
4. Added Ply smoke runtime-session evidence and hardened old-renderer asset
   scanning.

The full goal still must not be marked complete until the remaining prompt-audit
milestone gates also pass.
