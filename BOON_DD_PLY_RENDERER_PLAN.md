# Boon DD Pure Ply Renderer Replacement Plan

## Goal

Replace the current custom renderer paths with a pure Ply renderer that works on both native and browser targets.

The final application UI must be built in Rust with `ply-engine`. Native and browser must share the same Ply UI/component code. The browser may use the minimal Ply/macroquad HTML and JavaScript loader needed to start the WASM app, but it must not render UI with custom DOM, Canvas2D, or WebGPU JavaScript.

This plan is scoped to `/home/martinkavik/repos/boon-dd-ply` on branch `ply-renderer`. The original `/home/martinkavik/repos/boon-dd` worktree must remain untouched.

## `/goal` Objective

Implement `BOON_DD_PLY_RENDERER_PLAN.md` end to end in `/home/martinkavik/repos/boon-dd-ply`: replace native and browser playground rendering with shared Rust/Ply code, remove old custom WGPU/app_window/browser-WebGPU renderer code, preserve Boon DD example behavior, and finish only after deterministic verification plus independent AI-prompt review artifacts prove the work was honestly and fully completed.

## Pre-Review Findings

The current code does not yet satisfy the target state. These are the concrete gaps this plan must close:

- Native rendering is custom WGPU/vector code in `crates/boon_backend_app_window/src/bin/native_playground.rs`. It creates a WGPU instance, shader, render pipeline, vertex buffers, local rectangle/text primitives, a bitmap glyph table, CPU PNG rasterization, and manual hit testing.
- Native smoke currently reports `backend: "app_window+wgpu"` and `mode: "native-vector-playground"`.
- Browser playground rendering is inline HTML/JavaScript in `xtask/src/main.rs::browser_playground_html`. It creates DOM buttons, uses `navigator.gpu`, configures a WebGPU canvas, clears a frame manually, and reports `backend: "browser-webgpu"`.
- `crates/boon_backend_browser` depends on `boon_backend_wgpu` and `wgpu` but does not provide a Ply browser renderer.
- `xtask verify-playgrounds` currently requires old native WGPU fields and browser WebGPU/canvas fields.
- Generated render evidence currently writes `native_render_1280x720.json` with `backend: "wgpu-command-schema"` and `browser_render_1280x720.json` with `backend: "browser-webgpu-command-schema"`.
- The previous version of this plan was too native-focused and explicitly kept browser WebGPU checks unchanged. That is incompatible with the new requirement.

## Dependency Decision

Use `ply-engine`, not `ply`.

The crate named `ply` is a PLY polygon-file parser. The Rust UI/rendering library is the crates.io package `ply-engine`, whose Rust crate name is `ply_engine`.

Pin the first implementation to:

```toml
ply-engine = { version = "=1.1.1", default-features = false }
```

Use `plyx` only as a build helper for web packaging, pinned separately:

```bash
cargo install plyx --version =0.2.2 --locked --root .boon-local/tools/plyx-0.2.2
```

Before implementation, refresh the assumption:

```bash
cargo info ply-engine
cargo info ply
cargo info plyx
```

Expected result:

- `ply-engine` is the app/UI/graphics engine.
- `ply` is only a Polygon File Format parser and must not be added.
- `plyx web` builds a Ply WASM bundle with `build/web/app.wasm`, `build/web/index.html`, and `build/web/ply_bundle.js`.

## Target Architecture

Create one shared Ply backend:

```text
crates/boon_backend_ply/
  src/lib.rs
  src/app.rs
  src/evidence.rs
  src/bin/native_playground.rs
  src/bin/web_playground.rs
  index.html
  assets/
```

Responsibilities:

- `app.rs`: shared Boon DD playground state and pure Ply UI/component tree.
- `evidence.rs`: deterministic evidence from `Ply::new_headless`, render-command summaries, per-example state transitions, and smoke-report serialization.
- `native_playground.rs`: visible native Ply app using `#[macroquad::main(window_conf)]`.
- `web_playground.rs`: browser Ply app compiled to `wasm32-unknown-unknown` and packaged with `plyx web`.
- `index.html`: minimal Ply/macroquad host shell only. It may contain the Ply canvas and loader. It must not create UI controls or draw the app.

Final allowed rendering surfaces:

- `boon_backend_ply` for native and browser app UI.
- `boon_backend_ratatui` for terminal-only verification/playground output.

Final retired or deleted renderer surfaces:

- `crates/boon_backend_app_window` unless reduced to a thin compatibility wrapper that calls `boon_backend_ply` and has no `app_window` or `wgpu` dependency.
- `crates/boon_backend_wgpu`.
- `crates/boon_backend_browser` unless reduced to a thin browser/WASM runtime helper with no `wgpu` dependency and no rendering responsibility.
- `shaders/common/ui_rect.wgsl` if no non-render compiler test still needs it.
- Inline browser renderer HTML/JS in `xtask`.

## Key Ply APIs

Use only core Ply APIs at first:

- `use ply_engine::prelude::*;`
- `Ply::<()>::new(&DEFAULT_FONT).await`
- `Ply::<()>::new_headless(Dimensions::new(width, height))`
- `ply.begin()`
- `ui.element()`
- `ui.text(...)`
- `ui.element().id(...).on_press(...)`
- `ply.is_just_pressed(...)`
- `ply.eval()`
- `ply.show(|command| { ... }).await`

Use embedded font bytes for deterministic native and web builds. Avoid path-loaded fonts unless the path is included in the web bundle and verified.

## Non-Goals

- Do not rewrite Boon DD runtime semantics, source syntax, compiler lowering, generated graph semantics, or terminal Ratatui behavior.
- Do not preserve old native WGPU/vector rendering as a fallback.
- Do not preserve old browser WebGPU/DOM rendering as a fallback.
- Do not add JavaScript or native shims that render the app outside Ply.
- Do not use AI-review results as a substitute for deterministic tests.

## Implementation Steps

### 1. Bootstrap And Dependency Boundary

1. Add `ply-engine = { version = "=1.1.1", default-features = false }` to workspace dependencies.
2. Add `crates/boon_backend_ply` as the shared native/browser renderer crate.
3. Move reusable playground state from the native app-window binary into `boon_backend_ply::app`.
4. Keep Boon DD execution independent of Ply by passing `PlaygroundExample` state and `SmokeOutput` into the UI layer.
5. Add repo-local `plyx` bootstrap under `.boon-local/tools/plyx-0.2.2` or an equivalent xtask bootstrap path.

Verification:

```bash
cargo tree -i ply-engine
cargo tree -i ply
cargo check -p boon_dd
cargo check -p boon_backend_ply
```

Pass criteria:

- `ply-engine` appears only through `boon_backend_ply`.
- `ply` is absent.
- Core runtime/compiler crates compile without depending on Ply.

### 2. Port Native UI To Shared Ply Components

1. Rebuild the current native playground as a shared Ply component tree:
   - sidebar with all `boon_dd::REQUIRED_EXAMPLES`,
   - selected example view,
   - output/status panel,
   - TodoMVC, CRUD, Flight Booker, Temperature Converter, Cells, Pong, Shopping List, Counter, signal examples, list examples, and fallback workbench views.
2. Replace the old drawing helpers with Ply component helpers:
   - `sidebar(...)`
   - `app_frame(...)`
   - `input_like(...)`
   - `button(...)`
   - `pill(...)`
   - `table(...)`
   - `todo_row(...)`
   - `example_view(...)`
3. Preserve interactions:
   - sidebar selection,
   - up/down/left/right selection,
   - pointer/click activation,
   - counter increment/decrement,
   - interval auto tick,
   - quit key for manual native runs.
4. Use Ply text for all labels and output values.

Remove from the native path:

- `Scene`
- `Primitive`
- `Vertex`
- `SHADER`
- `create_wgpu_surface`
- `render_frame`
- `scene_vertices`
- `rect_vertices`
- `text_vertices`
- `glyph`
- custom PNG rasterization derived from old vertices
- direct `wgpu` imports
- direct `app_window` imports

Verification:

```bash
cargo check -p boon_backend_ply --bins
cargo test -p boon_backend_ply
```

Pass criteria:

- Native Ply code compiles.
- All required examples load.
- No native path contains the removed custom renderer symbols.

### 3. Build Browser UI From The Same Ply Code

1. Add a `web_playground` binary in `boon_backend_ply` that uses the same `app.rs` UI and state as native.
2. Build it for `wasm32-unknown-unknown`.
3. Package with `plyx web --auto` from the `crates/boon_backend_ply` directory, or implement an xtask equivalent that copies only:
   - `app.wasm`,
   - `index.html`,
   - `ply_bundle.js`,
   - required assets.
4. Keep `index.html` minimal. It may host the Ply canvas and loader, but it must not create buttons, output panels, or render example UI.
5. Move browser smoke from `xtask::browser_playground_html` to the packaged Ply web app.
6. If browser smoke needs telemetry, expose only a narrow test hook from Rust/WASM, for example `window.__boonPlySmoke`, containing selected example, loaded examples, render-command counts, and state-transition proof. This hook must not render UI.

Verification:

```bash
cd crates/boon_backend_ply
../../.boon-local/tools/plyx-0.2.2/bin/plyx web --auto
test -f build/web/app.wasm
test -f build/web/index.html
test -f build/web/ply_bundle.js
```

Pass criteria:

- Browser app is compiled from `boon_backend_ply`.
- Browser UI code is Rust/Ply, not inline JavaScript.
- Browser build artifact exists and is served by xtask verification.

### 4. Deterministic Ply Evidence

Add deterministic evidence generation to `boon_backend_ply::evidence`:

1. Run `Ply::new_headless(Dimensions::new(1200.0, 800.0))` for every required example.
2. Build the same component tree used by native and browser.
3. Call `ply.eval()` and record:
   - command count,
   - rectangle count,
   - text count,
   - unique Ply IDs,
   - selected example,
   - render output text,
   - semantic widget labels,
   - interaction simulation result.
4. Simulate at least:
   - selecting the second example,
   - counter increment,
   - counter decrement,
   - one generic example activation.
5. Write deterministic artifacts under:

```text
target/boon-artifacts/ply/
  headless-matrix.json
  native-smoke.json
  browser-smoke.json
  screenshots/
```

Screenshots are optional only if the Ply API cannot provide deterministic capture. If screenshot capture is unavailable, the plan must explicitly document that limitation and rely on headless render-command evidence plus live browser pixel checks.

Verification:

```bash
cargo xtask verify-ply-headless --format json
jq '.success, .examples | length' target/boon-artifacts/ply/headless-matrix.json
```

Pass criteria:

- Every required example has nonzero Ply render commands.
- Every required example has text evidence.
- Simulated interactions change the expected state.
- Artifact records `renderer.library: "ply-engine"` and `renderer.custom_renderer_removed: true`.

### 5. Native Live Verification

Update xtask to launch the native Ply binary for smoke and manual runs.

Required smoke fields:

```json
{
  "backend": "ply-engine",
  "target": "native",
  "renderer": {
    "library": "ply-engine",
    "crate_version": "1.1.1",
    "macroquad_backend": true,
    "custom_wgpu_renderer_removed": true,
    "app_window_removed": true
  },
  "ply_initialized": true,
  "ply_frame_evaluated": true,
  "ply_frame_presented": true,
  "loaded_examples": [],
  "example_count": 0,
  "visible_ui": {
    "sidebar": true,
    "example_labels": true,
    "selected_output_panel": true,
    "ply_render_commands": 1
  }
}
```

Verification:

```bash
cargo xtask verify-ply-native --format json
cosmic-background-launch --workspace boon-dd -- cargo xtask run --example todo_mvc --target native
```

Pass criteria:

- Native smoke artifact proves Ply initialization, frame evaluation, and presentation.
- Visible native window launches through `cosmic-background-launch --workspace boon-dd -- ...`.
- Required examples can be selected.
- At least one state-changing example updates output in the live app.

### 6. Browser Live Verification

Update xtask to serve the packaged Ply web app and validate it in Firefox.

The verification must:

1. Build `boon_backend_ply` for web.
2. Serve `crates/boon_backend_ply/build/web/`.
3. Open the page in Firefox.
4. Prove the canvas is nonblank with a screenshot or pixel sample.
5. Simulate at least one selection and one state-changing interaction.
6. Read telemetry from the Rust/WASM test hook if available.
7. Write `target/boon-artifacts/ply/browser-smoke.json`.

Required browser smoke fields:

```json
{
  "backend": "ply-engine",
  "target": "browser",
  "renderer": {
    "library": "ply-engine",
    "crate_version": "1.1.1",
    "plyx_web_bundle": true,
    "custom_js_renderer_removed": true,
    "custom_webgpu_renderer_removed": true
  },
  "firefox": true,
  "wasm_loaded": true,
  "ply_frame_presented": true,
  "canvas_nonblank": true,
  "loaded_examples": [],
  "example_count": 0,
  "interaction": {
    "selection_changed": true,
    "state_changed": true
  }
}
```

Verification:

```bash
cargo xtask verify-ply-browser --browser firefox --format json
jq '.target, .renderer, .canvas_nonblank, .interaction' target/boon-artifacts/ply/browser-smoke.json
```

Pass criteria:

- Browser UI is the Ply WASM app.
- Browser smoke no longer uses `browser_playground_html`.
- Browser artifact no longer reports `backend: "browser-webgpu"`.
- Browser artifact no longer requires `navigator.gpu`, `requestAdapter`, `requestDevice`, or `canvas.getContext("webgpu")`.

### 7. Remove Old Renderer Code

Delete or retire old renderer surfaces after the Ply path passes the focused checks.

Required removals:

- Remove direct native WGPU rendering from `crates/boon_backend_app_window`.
- Migrate or delete `crates/boon_backend_app_window/src/bin/native_smoke.rs`; `verify-render-deps` must smoke the Ply backend, not an `app_window` placeholder.
- Remove or retire `crates/boon_backend_wgpu`.
- Remove `wgpu` from `crates/boon_backend_browser`.
- Remove inline browser playground HTML/JS renderer from `xtask/src/main.rs`.
- Remove `shaders/common/ui_rect.wgsl` if it has no remaining non-render test purpose.
- Stop generating `wgpu-command-schema` and `browser-webgpu-command-schema` backend labels.
- Remove workspace dependencies that no longer have a legitimate non-render user:
  - `app_window`
  - `wgpu`
  - `wesl`
  - `wgsl_bindgen`
  - `naga`

Allowed exceptions:

- `boon_backend_ratatui` can keep terminal rendering.
- `wasm-bindgen` can remain only if used for non-render browser test hooks or runtime smoke.
- Ply/macroquad internals may use a browser canvas. App code must not render UI through custom JS canvas/WebGPU.

Verification:

```bash
cargo xtask verify-ply-no-old-renderers --format json
rg -n 'app_window|wgpu::|create_render_pipeline|create_shader_module|SurfaceTargetUnsafe|DeviceExt|SHADER|scene_vertices|rect_vertices|text_vertices|glyph\\(' crates xtask/src Cargo.toml
rg -n 'browser-webgpu|navigator\\.gpu|requestAdapter|getContext\\(\"webgpu\"\\)|beginRenderPass|document\\.createElement\\(\"button\"\\)|fillRect|fillText' crates xtask/src
rg -n 'wgpu-command-schema|browser-webgpu-command-schema' crates xtask generated
```

Pass criteria:

- The negative scan has zero hits except allowlisted third-party/generated Ply loader files.
- `cargo tree -p boon_backend_ply -i wgpu` does not show a direct repo dependency on `wgpu`.
- `cargo tree -p boon_backend_ply -i app_window` has no result.
- Old backend crates are deleted or reduced to no-render compatibility wrappers.
- A temp-copy negative test that reintroduces one old native WGPU marker fails the gate.
- A temp-copy negative test that reintroduces one old browser WebGPU/DOM marker fails the gate.
- A temp-copy negative test that reintroduces an old generated backend label fails the gate.

### 8. Update Xtask Gates

Add or update these xtask commands:

```bash
cargo xtask verify-ply-renderer --format json
cargo xtask verify-ply-headless --format json
cargo xtask verify-ply-native --format json
cargo xtask verify-ply-browser --browser firefox --format json
cargo xtask verify-ply-no-old-renderers --format json
cargo xtask verify-ply-fresh-artifacts --format json
cargo xtask write-ply-ai-review-prompts --format json
cargo xtask verify-ply-ai-review-reports --format json
```

Update existing commands:

- `cargo xtask verify-ply-renderer --format json` must be the focused top-level Ply renderer gate. It must include headless, native, browser, old-renderer negative scan, fresh-artifact checks, and AI-review report validation.
- `cargo xtask verify-render-deps --format json` must check Ply, not old WGPU/app_window browser renderer dependencies.
- `cargo xtask verify-playgrounds --format json` must include terminal, native Ply, and browser Ply.
- `cargo xtask run --example <name> --target native` must launch the actual native Ply UI, not only print compiled JSON.
- `cargo xtask run --example <name> --target browser` must launch or serve the actual browser Ply UI, not only print compiled JSON.
- `cargo xtask test --target native` must depend on native Ply evidence and fail if the native Ply app has not been smoked.
- `cargo xtask test --target browser` must depend on browser Ply evidence and fail if the browser Ply app has not been smoked in Firefox.
- `cargo xtask verify all --format json` must include every Ply deterministic and AI-review gate.

Fresh-artifact checks:

- Record the current git commit.
- Hash `Cargo.lock`, `Cargo.toml`, `crates/boon_backend_ply/**`, `xtask/src/main.rs`, and every required example source/scenario.
- Store hashes in `target/boon-artifacts/ply/fresh-artifacts.json`.
- Fail if `native-smoke.json`, `browser-smoke.json`, `headless-matrix.json`, `no-old-renderers.json`, or AI-review reports were produced for a different commit or deterministic report hash.

Final artifacts:

```text
target/boon-artifacts/verify-report.json
target/boon-artifacts/success.json
target/boon-artifacts/ply/headless-matrix.json
target/boon-artifacts/ply/native-smoke.json
target/boon-artifacts/ply/browser-smoke.json
target/boon-artifacts/ply/no-old-renderers.json
target/boon-artifacts/ply/fresh-artifacts.json
target/boon-artifacts/ply/ai-review-prompts.json
target/boon-artifacts/ply/ai-review-reports.json
```

### 9. AI-Prompt-Based Verification

The implementation must include independent AI-review prompts as checked-in or generated artifacts. These prompts are not replacements for deterministic tests; they are a second review layer focused on honesty, quality, and missed old-renderer code.

Add prompt templates under:

```text
docs/prompts/renderer-ply/
  architecture-review.prompt.md
  old-renderer-removal-review.prompt.md
  native-browser-behavior-review.prompt.md
  fake-pass-verifier-review.prompt.md
  stale-artifact-review.prompt.md
  dependency-boundary-review.prompt.md
```

Each prompt must instruct the reviewer to:

- inspect the live checkout, not rely on the implementation summary,
- cite exact files and line numbers,
- verify native and browser share Ply UI/component code,
- verify browser UI is not rendered by custom DOM/Canvas2D/WebGPU JavaScript,
- verify old native WGPU/app_window/vector/glyph renderer code is gone,
- inspect the deterministic artifacts under `target/boon-artifacts/ply/`,
- run at least the relevant `rg` negative scans,
- compare report hashes against `target/boon-artifacts/ply/fresh-artifacts.json`,
- return a machine-readable verdict: `pass`, `pass_with_risks`, or `fail`.

Minimum prompts:

```text
Architecture prompt:
Review whether the final renderer architecture is honestly pure Ply for both native and browser. Confirm shared Rust/Ply UI code, list remaining non-Ply rendering paths, and fail if old WGPU/app_window/browser-JS rendering remains.

Removal prompt:
Search for old renderer code and dependency leftovers. Fail if direct repo app code still uses app_window, wgpu render pipelines, custom shaders, bitmap glyph rendering, inline browser WebGPU, DOM-created app controls, or generated old backend labels outside an explicit historical note.

Behavior prompt:
Compare required example behavior before and after the Ply migration using deterministic artifacts and live smoke reports. Fail if required examples are missing, state-changing examples do not change state, browser/native outputs diverge without explanation, or verification artifacts are stale.

Fake-pass verifier prompt:
Review the xtask verification implementation itself. Fail if a report can pass by writing expected JSON without launching real native/browser Ply surfaces, without Firefox, without negative scans, or without checking artifact freshness.

Stale-artifact prompt:
Verify that all success artifacts are tied to the current git commit and deterministic report hash. Fail if old `target/boon-artifacts` outputs can satisfy the gate after source changes.

Dependency-boundary prompt:
Review Cargo metadata and direct dependencies. Fail if repo-owned app code still directly depends on `app_window`, `wgpu`, old backend crates, custom WGSL renderer assets, or browser custom rendering code outside an explicit non-render historical note.
```

AI-review report schema:

```json
{
  "reviewer": "codex-subagent-or-external-ai",
  "model": "string",
  "git_commit": "string",
  "deterministic_report_sha256": "string",
  "prompt_file": "string",
  "prompt_sha256": "string",
  "commands_run": [],
  "files_examined": [],
  "deterministic_artifacts_examined": [],
  "findings": [],
  "verdict": "pass"
}
```

Required AI-review gates:

```bash
cargo xtask write-ply-ai-review-prompts --format json
cargo xtask verify-ply-ai-review-reports --format json
```

Pass criteria:

- At least two independent AI-review reports exist.
- At least one report focuses on architecture/removal and one on behavior.
- Every report references the current git commit.
- Every report references the current deterministic report SHA-256 from `fresh-artifacts.json`.
- Every report references the exact prompt SHA-256 it answered.
- No report has `verdict: "fail"`.
- Any `pass_with_risks` item must be copied into `target/boon-artifacts/ply/ai-review-reports.json` and either resolved or explicitly accepted in a `residual_risks` array.

### 10. Full Verification

Run:

```bash
cargo xtask bootstrap --check
cargo xtask verify-deps --format json
cargo xtask verify-render-deps --format json
cargo xtask verify-ply-renderer --format json
cargo xtask verify-ply-headless --format json
cargo xtask verify-ply-native --format json
cargo xtask verify-ply-browser --browser firefox --format json
cargo xtask verify-ply-no-old-renderers --format json
cargo xtask verify-ply-fresh-artifacts --format json
cargo xtask write-ply-ai-review-prompts --format json
cargo xtask verify-ply-ai-review-reports --format json
cargo xtask verify-playgrounds --format json
cargo xtask verify all --format json
```

Inspect:

```bash
jq '.success, .failed_gates' target/boon-artifacts/success.json
jq '.gates[] | {name, status}' target/boon-artifacts/verify-report.json
jq '.renderer, .example_count' target/boon-artifacts/ply/native-smoke.json
jq '.renderer, .canvas_nonblank, .interaction' target/boon-artifacts/ply/browser-smoke.json
jq '.verdicts, .residual_risks' target/boon-artifacts/ply/ai-review-reports.json
```

Manual native surface:

```bash
cosmic-background-launch --workspace boon-dd -- cargo xtask run --example todo_mvc --target native
```

Manual browser surface:

```bash
cargo xtask serve-ply-browser --open firefox
```

Pass criteria:

- `target/boon-artifacts/success.json` reports `success: true`.
- Native and browser artifacts both report `backend: "ply-engine"`.
- Native and browser artifacts both report all required examples.
- Native and browser artifacts both prove a state-changing interaction.
- Old renderer negative scan passes.
- AI-review gate passes.
- The original `/home/martinkavik/repos/boon-dd` worktree is unchanged.

## Final Done Criteria

The plan is complete only when all are true:

- `boon_backend_ply` is the shared renderer for native and browser.
- Native and browser app UI are built in Rust with `ply-engine`.
- Browser app uses only minimal Ply/macroquad loader JavaScript and no custom JS rendering.
- Old native WGPU/app_window/vector/glyph renderer code is removed.
- Old browser WebGPU/DOM renderer code is removed.
- Old render backend labels are removed from generated artifacts.
- Deterministic Ply headless, native, browser, and no-old-renderer gates pass.
- At least two independent AI-review reports pass the schema gate.
- `cargo xtask verify all --format json` passes.
- Manual native and browser surfaces were launched and checked.

## Blockers To Report Honestly

Stop and report instead of hiding these:

- `ply-engine` cannot compile on this repo's Rust toolchain or Linux environment.
- `plyx web` cannot package the workspace member cleanly and an xtask equivalent cannot be made deterministic.
- Ply cannot expose enough layout/render evidence for headless deterministic verification.
- Browser interaction or pixel verification cannot be made reliable in Firefox.
- The only passing path requires keeping the old native WGPU renderer.
- The only passing path requires keeping the old browser WebGPU/DOM renderer.
- Native and browser cannot share the same Ply UI/component code without a broader runtime rewrite.
