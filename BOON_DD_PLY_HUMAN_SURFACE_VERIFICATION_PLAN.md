# Boon DD Ply Human Surface Verification Plan

## Goal

Prove that every required Boon DD example works when controlled through the actual user-facing surfaces on all three targets:

- terminal Ratatui playground,
- native Ply window,
- browser Ply/WASM app in Firefox.

This plan extends `BOON_DD_PLY_RENDERER_PLAN.md`. It does not replace the pure Ply renderer migration contract. The renderer migration must still prove that native and browser use shared Rust/Ply UI code and that old custom renderer code is removed.

The purpose of this plan is stronger human-style verification: drive each example through the visible surface, capture screenshots or terminal-screen images, record the controls performed, and reject fake, stale, skipped, or placeholder evidence.

This plan is scoped to `/home/martinkavik/repos/boon-dd-ply` on branch `ply-renderer`. The original `/home/martinkavik/repos/boon-dd` worktree must remain untouched.

## `/goal` Objective

Implement `BOON_DD_PLY_HUMAN_SURFACE_VERIFICATION_PLAN.md` end to end in `/home/martinkavik/repos/boon-dd-ply`: add a deterministic, fresh, per-example human-surface verification harness that controls all required examples on terminal, native Ply, and Firefox browser Ply surfaces, captures screenshots or terminal-screen images for each example and state, validates those artifacts, includes AI-prompt-based review of the evidence quality, and finishes only after all new and existing Ply renderer gates pass with no skipped examples and no stale or fake screenshots.

## Relationship To The Renderer Plan

`BOON_DD_PLY_RENDERER_PLAN.md` proves renderer purity and baseline live smoke:

- native and browser use `boon_backend_ply`,
- old WGPU/app_window/browser-WebGPU renderer code is gone,
- deterministic Ply headless evidence covers all examples,
- native and Firefox browser smoke prove a presented Ply frame and at least one interaction,
- AI review checks architecture and old renderer removal.

This plan adds what that plan intentionally does not prove:

- every required example is selected and controlled on every target,
- screenshots exist for terminal, native, and browser evidence,
- screenshots are tied to the current source state and current command run,
- the verifier rejects skipped examples, stale artifacts, black/blank/duplicate placeholders, and reports produced without controlling the surface.

## Pre-Review Findings

The current checkout has strong renderer-level gates, but it is not yet a full human-surface sweep:

- `crates/boon_backend_ply::evidence` covers all examples through deterministic headless Ply evidence, not through screenshots from every presented surface.
- `cargo xtask verify-ply-native --format json` proves a live native Ply smoke, but not one screenshot and control trace per example.
- `cargo xtask verify-ply-browser --browser firefox --format json` proves one Firefox Ply smoke, a nonblank canvas, and a smoke interaction, but not one Firefox screenshot and control trace per example.
- `crates/boon_backend_ratatui/src/bin/terminal_playground.rs` has an interactive Ratatui playground and a smoke path, but no PTY replay harness that drives every example like a user and stores screen images per example.
- `cargo xtask run --example <name> --target terminal` currently prints compiled JSON instead of launching the interactive terminal playground.
- There is no checked-in manifest that says which human controls are required for each example on each target.
- There is no freshness gate that ties per-example screenshots to the current git commit, source hash, control manifest hash, and deterministic Ply evidence hash.

## Required Examples

The matrix must cover exactly `boon_examples::REQUIRED_FIXTURES`:

```text
counter
counter_hold
interval
interval_hold
latest
when
while
then
list_map_block
list_map_external_dep
list_object_state
list_retain_count
list_retain_reactive
list_retain_remove
shopping_list
todo_mvc
crud
flight_booker
temperature_converter
pong
cells
todo_mvc_physical
```

The verifier must fail if this list diverges from the code or if any example/target pair is missing.

## Definition Of Human-Style Control

A passing human-surface check must control the real surface, not only write expected JSON.

Allowed control mechanisms:

- terminal: spawn the interactive Ratatui playground in a PTY, send keyboard input, parse the terminal screen buffer, and capture the final screen buffer as text and PNG.
- native: open the actual native Ply window, drive the same input/update path used by manual interaction, present frames, capture screenshots from the displayed frame buffer, and write a control trace.
- browser: open the actual Ply/WASM app in Firefox, use WebDriver or an equivalent real browser-control protocol to send keyboard/mouse input, capture screenshots from Firefox, and cross-check with Rust/WASM telemetry.

Disallowed shortcuts:

- marking an example passed because headless render evidence exists,
- creating screenshots from generated JSON or static HTML without launching the target,
- using browser DOM/Canvas2D/WebGPU test code to render the app,
- counting a native/browser smoke of one example as coverage for all examples,
- accepting screenshots that are stale, blank, near-identical placeholders, or produced for another commit,
- skipping examples because they are "not interactive".

Selecting an example through the UI counts as a control action for every example. Examples with meaningful mutable behavior must also perform at least one domain action.

## Control Manifest

Add a checked-in manifest:

```text
docs/ply-human-surfaces/control-manifest.toml
```

The manifest must list every required example and its minimum controls:

```toml
[examples.counter]
required_targets = ["terminal", "native", "browser"]
actions = [
  { kind = "select_example", value = "counter" },
  { kind = "screenshot", name = "selected" },
  { kind = "activate", semantic = "increment" },
  { kind = "screenshot", name = "after_increment" },
  { kind = "assert_state_changed", field = "render_text" },
]
```

The action grammar must support at least:

```text
select_example
press_key
click_semantic
click_relative
type_text
wait_frames
wait_millis
activate
screenshot
assert_text_contains
assert_text_changed
assert_state_changed
assert_monitor_changed
assert_canvas_nonblank
assert_terminal_nonblank
```

The manifest must include per-example notes when a target has no domain-specific interactive widget and selection/render verification is the only meaningful human action.

Minimum expected domain actions:

- `counter`: increment and decrement.
- `counter_hold`: press or hold the counter action long enough to prove state changes.
- `interval`: wait for an interval-driven change.
- `interval_hold`: wait and hold/activate the relevant control if available.
- `latest`: trigger or wait for the latest-value update exposed by the example.
- `when`, `while`, `then`: perform the scenario activation or verify the rendered branch/output transition.
- `list_map_block`, `list_map_external_dep`, `list_object_state`, `list_retain_count`, `list_retain_reactive`, `list_retain_remove`: select, inspect list output, and activate the example control if one exists; otherwise assert the scenario-driven list output and monitor evidence.
- `shopping_list`: add/toggle/remove or execute the equivalent exposed action.
- `todo_mvc`: add a todo, toggle it, and verify the text remains visible.
- `crud`: create/select/update or execute the equivalent exposed action.
- `flight_booker`: change date/type state or execute the equivalent exposed action.
- `temperature_converter`: edit one field and verify the converted field changes.
- `pong`: wait for movement and verify the frame/state changes.
- `cells`: edit or select a cell and verify visible cell output.
- `todo_mvc_physical`: perform the same visible user path as `todo_mvc` unless the physical example exposes different controls, then document the difference.

If the current UI does not expose a required domain action, the implementation must either add that visible control to the shared app or mark the example as a blocker. It must not silently downgrade the test to a passive screenshot.

## Harness Architecture

Add a shared evidence crate or module, preferably under `xtask` plus small target helpers:

```text
xtask/src/ply_human_surfaces.rs
crates/boon_backend_ply/src/human_surface.rs
crates/boon_backend_ratatui/src/human_surface.rs
docs/ply-human-surfaces/control-manifest.toml
docs/prompts/renderer-ply-human-surfaces/
```

Target responsibilities:

- `xtask/src/ply_human_surfaces.rs`: parse the manifest, run the matrix, enforce freshness, validate screenshots, aggregate reports, and run negative tests.
- `boon_backend_ratatui::human_surface`: expose an interactive PTY-friendly mode and a deterministic screen-buffer export path after controls.
- `boon_backend_ply::human_surface`: expose native/browser semantic IDs, screenshot capture metadata, and optional test hooks that do not render UI outside Ply.
- browser WebDriver helper: launch Firefox against the packaged Ply web app, send real browser input, capture Firefox screenshots, and collect telemetry.

All semantic IDs used for testing must correspond to real user-visible controls or selectable UI regions. Hidden telemetry can verify state, but hidden telemetry must not be the only evidence.

## Terminal Surface Plan

Implement:

```bash
cargo xtask verify-ply-human-terminal --format json
```

The command must:

1. Start the interactive Ratatui playground in a PTY.
2. For each required example, send keyboard controls to select the example from the sidebar.
3. Perform the example's manifest actions.
4. Capture the terminal screen after each `screenshot` action.
5. Store raw PTY trace, normalized screen text, ANSI dump if available, and a PNG rendering of the screen buffer.
6. Verify nonblank cells, selected example label, output text, and state-change assertions.

Required artifacts:

```text
target/boon-artifacts/ply-human-surfaces/terminal/<example>/trace.json
target/boon-artifacts/ply-human-surfaces/terminal/<example>/screen-selected.txt
target/boon-artifacts/ply-human-surfaces/terminal/<example>/screen-selected.png
target/boon-artifacts/ply-human-surfaces/terminal/<example>/screen-after-action.txt
target/boon-artifacts/ply-human-surfaces/terminal/<example>/screen-after-action.png
```

Pass criteria:

- Every required example has a terminal trace.
- Every terminal trace contains the input events sent.
- Every terminal screenshot/text capture contains the selected example name.
- Every terminal PNG is nonblank and unique enough not to be a repeated placeholder.
- Stateful examples satisfy the manifest state-change assertions.

## Native Surface Plan

Implement:

```bash
cargo xtask verify-ply-human-native --format json
```

The command must:

1. Launch the actual native Ply app through `cosmic-background-launch --workspace boon-dd -- ...`.
2. Run in a visible-surface test mode that presents frames.
3. For each required example, select the example through the same navigation or pointer path a user would use.
4. Perform the example's manifest actions.
5. Capture screenshots from the presented native frame after each `screenshot` action.
6. Store control traces, frame metadata, screenshot metadata, and state telemetry.
7. Clean up the native process after the run and prove no test process is left behind.

Required artifacts:

```text
target/boon-artifacts/ply-human-surfaces/native/<example>/trace.json
target/boon-artifacts/ply-human-surfaces/native/<example>/selected.png
target/boon-artifacts/ply-human-surfaces/native/<example>/after-action.png
target/boon-artifacts/ply-human-surfaces/native/<example>/telemetry.json
```

Pass criteria:

- Every required example has native screenshots.
- Screenshot dimensions match the configured native test window.
- Screenshots are nonblank and not identical placeholders.
- The selected screenshot includes the example-specific visible output or label according to telemetry/OCR/semantic checks.
- Stateful examples satisfy the manifest state-change assertions.
- The native trace proves a presented Ply frame happened after each control step.

## Browser Surface Plan

Implement:

```bash
cargo xtask verify-ply-human-browser --browser firefox --format json
```

The command must:

1. Build the Ply web bundle from `crates/boon_backend_ply`.
2. Serve the actual bundle directory.
3. Launch Firefox.
4. Use WebDriver, Marionette, or another real Firefox automation protocol to send keyboard/mouse input.
5. For each required example, select the example through the browser UI.
6. Perform the example's manifest actions.
7. Capture Firefox screenshots after each `screenshot` action.
8. Cross-check with Rust/WASM telemetry for selected example, interaction result, and frame presentation.
9. Clean up Firefox and the static server after the run.

Required artifacts:

```text
target/boon-artifacts/ply-human-surfaces/browser/<example>/trace.json
target/boon-artifacts/ply-human-surfaces/browser/<example>/selected.png
target/boon-artifacts/ply-human-surfaces/browser/<example>/after-action.png
target/boon-artifacts/ply-human-surfaces/browser/<example>/telemetry.json
```

Pass criteria:

- Every required example has Firefox screenshots.
- The report records `browser: "firefox"` and the Firefox executable/version used.
- Screenshots are captured from Firefox, not from a generated image helper.
- The canvas is nonblank for each example.
- Stateful examples satisfy the manifest state-change assertions.
- Browser telemetry proves the Ply/WASM app, not a custom JavaScript renderer, handled the state.

## Screenshot Validation

Implement:

```bash
cargo xtask verify-ply-human-screenshots --format json
```

The command must inspect every screenshot artifact and write:

```text
target/boon-artifacts/ply-human-surfaces/screenshot-validation.json
```

Validation rules:

- file exists,
- file is a valid PNG,
- dimensions match the expected target dimensions,
- byte size is above a target-specific minimum,
- alpha and RGB histograms are not blank,
- at least one non-background color is present,
- perceptual hash is not identical across unrelated examples,
- selected and after-action screenshots differ for state-changing examples,
- screenshot sidecar metadata references the current matrix run ID,
- screenshot sidecar metadata references current git commit and source hash.

Negative tests:

- Replace one screenshot with a 1x1 PNG and verify the gate fails.
- Replace one screenshot with a valid all-black or all-white PNG and verify the gate fails.
- Copy one example screenshot over another example and verify duplicate-placeholder detection fails.
- Delete one required screenshot and verify the gate fails.
- Change the source hash in one sidecar and verify freshness validation fails.

## Freshness And Provenance

Implement:

```bash
cargo xtask verify-ply-human-fresh-artifacts --format json
```

The source hash must include:

- `BOON_DD_PLY_RENDERER_PLAN.md`,
- `BOON_DD_PLY_HUMAN_SURFACE_VERIFICATION_PLAN.md`,
- `Cargo.lock`,
- `Cargo.toml`,
- `xtask/src/**`,
- `crates/boon_backend_ply/**`,
- `crates/boon_backend_ratatui/**`,
- `docs/ply-human-surfaces/control-manifest.toml`,
- `examples/*/source.bn`,
- `examples/*/scenario.toml`,
- `generated/**/terminal_120x40.snapshot.txt`,
- `generated/**/native_render_1280x720.json`,
- `generated/**/browser_render_1280x720.json`.

Each target report and screenshot sidecar must record:

- git commit or dirty-tree marker,
- source hash,
- control manifest hash,
- deterministic Ply report hash,
- matrix run ID,
- command line,
- target,
- example,
- step name,
- timestamp,
- process ID where applicable,
- browser executable and version for browser artifacts,
- screenshot dimensions and hash.

The verifier must fail if artifacts from an older source hash, older manifest hash, older deterministic report hash, or older matrix run are reused.

## Aggregate Command

Implement:

```bash
cargo xtask verify-ply-human-surfaces --format json
```

This focused gate must run:

```bash
cargo xtask verify-ply-human-terminal --format json
cargo xtask verify-ply-human-native --format json
cargo xtask verify-ply-human-browser --browser firefox --format json
cargo xtask verify-ply-human-screenshots --format json
cargo xtask verify-ply-human-fresh-artifacts --format json
cargo xtask write-ply-human-ai-review-prompts --format json
cargo xtask verify-ply-human-ai-review-reports --format json
```

It must write:

```text
target/boon-artifacts/ply-human-surfaces/matrix.json
target/boon-artifacts/ply-human-surfaces/success.json
```

Required `matrix.json` fields:

```json
{
  "success": true,
  "source_hash": "sha256",
  "control_manifest_hash": "sha256",
  "deterministic_ply_report_sha256": "sha256",
  "required_examples": [],
  "targets": ["terminal", "native", "browser"],
  "coverage": {
    "terminal": { "example_count": 22, "screenshot_count": 0 },
    "native": { "example_count": 22, "screenshot_count": 0 },
    "browser": { "example_count": 22, "screenshot_count": 0 }
  },
  "examples": [],
  "failures": [],
  "residual_risks": []
}
```

Pass criteria:

- success is true,
- there are zero failures,
- target coverage is exactly terminal/native/browser,
- every required example appears once per target,
- screenshot count is greater than or equal to the manifest-required screenshot count,
- no skipped example is allowlisted,
- no stale artifact is accepted.

## Integration With Existing Gates

Update these commands:

```bash
cargo xtask verify-playgrounds --format json
cargo xtask verify-ply-renderer --format json
cargo xtask verify all --format json
```

Required behavior:

- `verify-playgrounds` must continue to prove terminal/native/browser playgrounds, and it must link to the human-surface matrix when present.
- `verify-ply-renderer` must keep the original pure Ply renderer gates and add `verify-ply-human-surfaces` only after the human-surface plan is implemented.
- `verify all --format json` must fail if the human-surface matrix fails, skips examples, or has stale screenshots.

Do not weaken existing renderer-removal checks to make this plan pass.

## AI-Prompt-Based Verification

Add prompt templates:

```text
docs/prompts/renderer-ply-human-surfaces/
  human-surface-coverage-review.prompt.md
  screenshot-authenticity-review.prompt.md
  target-control-review.prompt.md
  stale-artifact-review.prompt.md
  fake-pass-harness-review.prompt.md
  example-behavior-review.prompt.md
```

Each prompt must instruct the reviewer to inspect the live checkout and artifacts, cite exact files and lines, and return JSON using this schema:

```json
{
  "reviewer": "codex-subagent-or-external-ai",
  "model": "string",
  "git_commit": "string",
  "source_hash": "string",
  "control_manifest_hash": "string",
  "deterministic_ply_report_sha256": "string",
  "prompt_file": "string",
  "prompt_sha256": "string",
  "commands_run": [],
  "files_examined": [],
  "artifacts_examined": [],
  "screenshots_examined": [],
  "findings": [],
  "verdict": "pass"
}
```

Minimum review focus:

- coverage review: prove all 22 examples are covered on terminal, native, and browser.
- screenshot authenticity review: inspect screenshot metadata and sample screenshots; fail on placeholders or stale images.
- target-control review: verify each target was actually controlled, not only simulated through JSON.
- stale-artifact review: verify source/manifest/report hashes and negative tests.
- fake-pass harness review: inspect xtask code for ways to pass without launching surfaces or capturing screenshots.
- example-behavior review: inspect state-changing examples and fail if screenshots/traces do not prove meaningful action.

Required commands:

```bash
cargo xtask write-ply-human-ai-review-prompts --format json
cargo xtask verify-ply-human-ai-review-reports --format json
```

Pass criteria:

- At least two independent AI-review reports exist.
- At least one report covers screenshot authenticity.
- At least one report covers target control and example coverage.
- Reports reference the current source hash, manifest hash, and deterministic Ply report hash.
- No report has `verdict: "fail"`.
- Any `pass_with_risks` finding is copied into `matrix.json` under `residual_risks` and either resolved or explicitly accepted.

## Final Verification Commands

Run all commands from the renderer plan plus the new human-surface gates:

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
cargo xtask verify-ply-human-terminal --format json
cargo xtask verify-ply-human-native --format json
cargo xtask verify-ply-human-browser --browser firefox --format json
cargo xtask verify-ply-human-screenshots --format json
cargo xtask verify-ply-human-fresh-artifacts --format json
cargo xtask write-ply-human-ai-review-prompts --format json
cargo xtask verify-ply-human-ai-review-reports --format json
cargo xtask verify-ply-human-surfaces --format json
cargo xtask write-ply-ai-review-prompts --format json
cargo xtask verify-ply-ai-review-reports --format json
cargo xtask verify-playgrounds --format json
cargo xtask verify all --format json
```

Inspect:

```bash
jq '.success, .coverage, .failures, .residual_risks' target/boon-artifacts/ply-human-surfaces/matrix.json
jq '.success' target/boon-artifacts/ply-human-surfaces/screenshot-validation.json
jq '.examples[] | {example, targets}' target/boon-artifacts/ply-human-surfaces/matrix.json
find target/boon-artifacts/ply-human-surfaces -path '*/*.png' | sort
```

Manual spot-check commands:

```bash
cargo xtask run --example todo_mvc --target terminal
cosmic-background-launch --workspace boon-dd -- cargo xtask run --example todo_mvc --target native
cosmic-background-launch --workspace boon-dd -- cargo xtask run --example todo_mvc --target browser
```

## Final Done Criteria

The plan is complete only when all are true:

- The original pure Ply renderer plan still passes.
- Every required example is controlled on terminal, native, and browser.
- Every required example has target-specific screenshots or terminal-screen images.
- Stateful examples have before/after evidence that proves meaningful state change.
- Browser evidence comes from Firefox.
- Native evidence comes from the actual Ply native window.
- Terminal evidence comes from the interactive Ratatui playground in a PTY.
- Screenshot validation rejects blank, missing, stale, duplicate, and placeholder screenshots.
- Freshness validation ties every trace and screenshot to the current source hash and matrix run ID.
- AI-review prompts and reports validate coverage, screenshot authenticity, and fake-pass resistance.
- `cargo xtask verify all --format json` passes with the human-surface gate included.
- The original `/home/martinkavik/repos/boon-dd` worktree is unchanged.

## Blockers To Report Honestly

Stop and report instead of hiding these:

- A target cannot be controlled through a real presented surface.
- Firefox cannot be automated reliably enough to capture screenshots.
- Native screenshot capture cannot be tied to presented Ply frames.
- Terminal PTY replay cannot produce deterministic screen evidence.
- A required example has no meaningful visible action and the UI cannot expose one without a broader product decision.
- Screenshot validation can be bypassed by stale or placeholder artifacts.
- The only passing path requires weakening the pure Ply renderer or old-renderer-removal gates.
