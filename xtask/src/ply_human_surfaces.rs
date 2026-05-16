use crate::{
    GateReport, artifacts_dir, build_ply_web, hash_file, hex_digest, launch_background_process,
    ply_artifacts_dir, read_playground_artifact, repo_root, run_capture, run_status,
    wait_for_json_artifact,
};
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use image::{ImageBuffer, Rgba};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TARGETS: [&str; 3] = ["terminal", "native", "browser"];
const EXPECTED_NATIVE_WIDTH: u32 = 1200;
const EXPECTED_NATIVE_HEIGHT: u32 = 800;

#[derive(Debug, Deserialize)]
struct ControlManifest {
    version: u32,
    targets: Vec<String>,
    examples: BTreeMap<String, ManifestExample>,
}

#[derive(Debug, Deserialize)]
struct ManifestExample {
    required_targets: Vec<String>,
    notes: Option<String>,
    actions: Vec<ManifestAction>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ManifestAction {
    kind: String,
    name: Option<String>,
    value: Option<String>,
    semantic: Option<String>,
    field: Option<String>,
}

#[derive(Clone)]
struct HumanContext {
    root: PathBuf,
    dir: PathBuf,
    source_hash: String,
    control_manifest_hash: String,
    deterministic_ply_report_sha256: String,
    matrix_run_id: String,
    git_commit: String,
    git_dirty: bool,
    git_status_short: String,
    command_line: Vec<String>,
    process_id: u32,
    firefox_executable: String,
    firefox_version: String,
    geckodriver_version: String,
}

#[derive(Debug)]
struct WebDriver {
    port: u16,
    child: Child,
    session_id: String,
}

struct TerminalRunResult {
    trace: serde_json::Value,
    captures: BTreeMap<String, boon_backend_ratatui::human_surface::TerminalScreenCapture>,
    screenshot_paths: Vec<PathBuf>,
}

pub(crate) fn verify_ply_human_terminal(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    let ctx = human_context(false)?;
    let manifest = load_and_validate_manifest(&ctx)?;
    let terminal_dir = ctx.dir.join("terminal");
    if terminal_dir.exists() {
        fs::remove_dir_all(&terminal_dir)?;
    }
    fs::create_dir_all(&terminal_dir)?;

    let mut examples = Vec::new();
    for (index, example) in boon_dd::REQUIRED_EXAMPLES.iter().enumerate() {
        let example_dir = terminal_dir.join(example);
        fs::create_dir_all(&example_dir)?;
        let actions = &manifest.examples[*example].actions;
        let run = run_terminal_pty_probe(&ctx.root, index, example, &example_dir, actions)?;
        for step in required_screenshots(&manifest, example) {
            let path = screenshot_path(&ctx, "terminal", example, &step);
            write_screenshot_sidecar(
                &ctx,
                "terminal",
                example,
                &step,
                &path,
                "ratatui-live-pty-exported-screen-buffer",
            )?;
        }
        let selected = run
            .captures
            .get("selected")
            .context("terminal run did not capture selected screenshot")?;
        let after_action = run
            .captures
            .get("after-action")
            .context("terminal run did not capture after-action screenshot")?;

        validate_terminal_capture(example, selected, after_action)
            .with_context(|| format!("terminal human-surface validation failed for {example}"))?;

        let trace = json!({
            "target": "terminal",
            "example": example,
            "source_hash": ctx.source_hash,
            "control_manifest_hash": ctx.control_manifest_hash,
            "deterministic_ply_report_sha256": ctx.deterministic_ply_report_sha256,
            "matrix_run_id": ctx.matrix_run_id,
            "pty_probe": run.trace,
            "manifest_actions": actions.iter().map(action_to_json).collect::<Vec<_>>(),
            "events": actions.iter().map(action_to_json).collect::<Vec<_>>(),
            "selected": selected,
            "after_action": after_action,
        });
        ensure_manifest_trace(example, "terminal", &manifest, &trace)?;
        let trace_path = example_dir.join("trace.json");
        fs::write(&trace_path, serde_json::to_vec_pretty(&trace)?)?;
        let telemetry_path = example_dir.join("telemetry.json");
        fs::write(&telemetry_path, serde_json::to_vec_pretty(&trace)?)?;
        examples.push(json!({
            "example": example,
            "trace": trace_path,
            "telemetry": telemetry_path,
            "screenshots": run.screenshot_paths,
            "selected_nonblank_cells": selected.nonblank_cells,
            "after_action_nonblank_cells": after_action.nonblank_cells,
            "state_changed": state_changed_terminal(selected, after_action),
        }));
    }
    let details = target_summary(&ctx, "terminal", examples);
    let artifact = ctx.dir.join("terminal-summary.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(GateReport {
        name: "verify-ply-human-terminal".to_owned(),
        command: "cargo xtask verify-ply-human-terminal --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![
            artifact.display().to_string(),
            terminal_dir.display().to_string(),
        ],
        details,
    })
}

pub(crate) fn verify_ply_human_native(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    let ctx = human_context(false)?;
    let manifest = load_and_validate_manifest(&ctx)?;
    run_status(
        "cargo",
        &[
            "check",
            "-p",
            "boon_backend_ply",
            "--bin",
            "native_playground",
        ],
    )?;
    let native_dir = ctx.dir.join("native");
    if native_dir.exists() {
        fs::remove_dir_all(&native_dir)?;
    }
    fs::create_dir_all(&native_dir)?;

    let mut examples = Vec::new();
    for example in boon_dd::REQUIRED_EXAMPLES {
        let example_dir = native_dir.join(example);
        fs::create_dir_all(&example_dir)?;
        let actions_path =
            write_manifest_actions_file(&example_dir, &manifest.examples[*example].actions)?;
        launch_background_process(&[
            "cargo",
            "run",
            "--quiet",
            "-p",
            "boon_backend_ply",
            "--bin",
            "native_playground",
            "--",
            "--human-surface",
            "--example",
            example,
            "--output-dir",
            example_dir.to_str().unwrap(),
            "--actions",
            actions_path.to_str().unwrap(),
            "--source-hash",
            &ctx.source_hash,
            "--control-manifest-hash",
            &ctx.control_manifest_hash,
            "--deterministic-ply-report-sha256",
            &ctx.deterministic_ply_report_sha256,
            "--matrix-run-id",
            &ctx.matrix_run_id,
        ])?;
        let telemetry_path = example_dir.join("telemetry.json");
        wait_for_json_artifact(
            &telemetry_path,
            Duration::from_secs(75),
            "native human-surface telemetry",
        )?;
        let telemetry = read_playground_artifact(&telemetry_path)?;
        validate_native_telemetry(example, &ctx, &telemetry)?;
        for step in required_screenshots(&manifest, example) {
            let screenshot = screenshot_path(&ctx, "native", example, &step);
            write_screenshot_sidecar(
                &ctx,
                "native",
                example,
                &step,
                &screenshot,
                "macroquad-get-screen-data",
            )?;
        }
        let trace_path = example_dir.join("trace.json");
        let mut trace = read_playground_artifact(&trace_path)?;
        trace["manifest_actions"] = serde_json::Value::Array(
            manifest.examples[*example]
                .actions
                .iter()
                .map(action_to_json)
                .collect::<Vec<_>>(),
        );
        fs::write(&trace_path, serde_json::to_vec_pretty(&trace)?)?;
        ensure_manifest_trace(example, "native", &manifest, &trace)?;
        examples.push(json!({
            "example": example,
            "trace": trace_path,
            "telemetry": telemetry_path,
            "screenshots": collect_example_pngs(&example_dir)?,
            "state_changed": native_state_changed(&telemetry),
        }));
    }
    ensure_no_native_human_process_left()?;
    let details = target_summary(&ctx, "native", examples);
    let artifact = ctx.dir.join("native-summary.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(GateReport {
        name: "verify-ply-human-native".to_owned(),
        command: "cargo xtask verify-ply-human-native --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![
            artifact.display().to_string(),
            native_dir.display().to_string(),
        ],
        details,
    })
}

pub(crate) fn verify_ply_human_browser(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    let ctx = human_context(false)?;
    let manifest = load_and_validate_manifest(&ctx)?;
    let build_dir = build_ply_web()?;
    let browser_dir = ctx.dir.join("browser");
    if browser_dir.exists() {
        fs::remove_dir_all(&browser_dir)?;
    }
    fs::create_dir_all(&browser_dir)?;

    let server = StaticServer::start(build_dir)?;
    let driver = WebDriver::start()?;
    let url = format!("http://{}/index.html", server.addr);
    driver.navigate(&url)?;
    driver.set_window_rect(EXPECTED_NATIVE_WIDTH, EXPECTED_NATIVE_HEIGHT)?;
    driver.wait_for_telemetry(Duration::from_secs(45))?;
    driver.focus_canvas()?;

    let mut examples = Vec::new();
    for (index, example) in boon_dd::REQUIRED_EXAMPLES.iter().enumerate() {
        let example_dir = browser_dir.join(example);
        fs::create_dir_all(&example_dir)?;
        let trace = run_browser_manifest(
            &driver,
            index,
            example,
            &example_dir,
            &manifest.examples[*example].actions,
            &ctx,
        )?;
        for step in required_screenshots(&manifest, example) {
            let screenshot = screenshot_path(&ctx, "browser", example, &step);
            write_screenshot_sidecar(
                &ctx,
                "browser",
                example,
                &step,
                &screenshot,
                "firefox-webdriver-screenshot-protocol",
            )?;
        }
        let selected = trace
            .get("selected")
            .cloned()
            .context("browser trace missing selected telemetry")?;
        let after_action = trace
            .get("after_action")
            .cloned()
            .context("browser trace missing after_action telemetry")?;
        let extra_screenshots = trace
            .get("extra_screenshots")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
        let trace = json!({
            "target": "browser",
            "browser": "firefox",
            "example": example,
            "source_hash": ctx.source_hash,
            "control_manifest_hash": ctx.control_manifest_hash,
            "deterministic_ply_report_sha256": ctx.deterministic_ply_report_sha256,
            "matrix_run_id": ctx.matrix_run_id,
            "control_source": "Firefox WebDriver keyboard and screenshot protocol",
            "manifest_actions": manifest.examples[*example].actions.iter().map(action_to_json).collect::<Vec<_>>(),
            "events": manifest.examples[*example].actions.iter().map(action_to_json).collect::<Vec<_>>(),
            "selected": selected,
            "after_action": after_action,
            "extra_screenshots": extra_screenshots,
        });
        let trace_path = example_dir.join("trace.json");
        fs::write(&trace_path, serde_json::to_vec_pretty(&trace)?)?;
        let telemetry_path = example_dir.join("telemetry.json");
        fs::write(&telemetry_path, serde_json::to_vec_pretty(&trace)?)?;
        ensure_manifest_trace(example, "browser", &manifest, &trace)?;
        examples.push(json!({
            "example": example,
            "trace": trace_path,
            "telemetry": telemetry_path,
            "screenshots": collect_example_pngs(&example_dir)?,
            "state_changed": browser_state_changed(&trace),
        }));
    }
    let firefox_version = run_capture("firefox", &["--version"])?;
    driver.shutdown()?;
    server.stop();

    let details = target_summary(&ctx, "browser", examples);
    let details = json!({
        "browser": "firefox",
        "firefox_version": firefox_version,
        "webdriver": "geckodriver",
        "details": details,
        "success": true,
        "source_hash": ctx.source_hash,
        "control_manifest_hash": ctx.control_manifest_hash,
        "deterministic_ply_report_sha256": ctx.deterministic_ply_report_sha256,
        "matrix_run_id": ctx.matrix_run_id,
    });
    let artifact = ctx.dir.join("browser-summary.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(GateReport {
        name: "verify-ply-human-browser".to_owned(),
        command: "cargo xtask verify-ply-human-browser --browser firefox --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![
            artifact.display().to_string(),
            browser_dir.display().to_string(),
        ],
        details,
    })
}

pub(crate) fn verify_ply_human_screenshots(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    let ctx = human_context(false)?;
    let manifest = load_and_validate_manifest(&ctx)?;
    let _ = write_matrix_artifact()?;
    let mut records = Vec::new();
    let mut failures = Vec::new();
    let mut hashes_by_target = BTreeMap::<String, BTreeMap<String, Vec<String>>>::new();
    for target in TARGETS {
        for example in boon_dd::REQUIRED_EXAMPLES {
            let required = required_screenshots(&manifest, example);
            for step in required {
                let path = screenshot_path(&ctx, target, example, &step);
                match validate_screenshot_with_sidecar(&ctx, target, example, &step, &path) {
                    Ok(record) => {
                        let hash = record["sha256"].as_str().unwrap_or_default().to_owned();
                        hashes_by_target
                            .entry(target.to_owned())
                            .or_default()
                            .entry(hash)
                            .or_default()
                            .push(format!("{example}/{step}"));
                        records.push(record);
                    }
                    Err(error) => failures.push(json!({
                        "target": target,
                        "example": example,
                        "step": step,
                        "path": path,
                        "error": format!("{error:#}"),
                    })),
                }
            }
        }
    }
    let duplicate_failures = duplicate_placeholder_failures(&hashes_by_target);
    failures.extend(duplicate_failures);
    failures.extend(screenshot_pair_failures(&ctx, &manifest)?);
    let negative_tests = screenshot_negative_tests(&ctx)?;
    let details = json!({
        "success": failures.is_empty(),
        "source_hash": ctx.source_hash,
        "control_manifest_hash": ctx.control_manifest_hash,
        "deterministic_ply_report_sha256": ctx.deterministic_ply_report_sha256,
        "matrix_run_id": ctx.matrix_run_id,
        "screenshot_count": records.len(),
        "screenshots": records,
        "failures": failures,
        "negative_tests": negative_tests,
    });
    let artifact = ctx.dir.join("screenshot-validation.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    if details["success"].as_bool() != Some(true) {
        bail!(
            "human-surface screenshot validation failed; see {}",
            artifact.display()
        );
    }
    Ok(GateReport {
        name: "verify-ply-human-screenshots".to_owned(),
        command: "cargo xtask verify-ply-human-screenshots --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

pub(crate) fn verify_ply_human_fresh_artifacts(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    let ctx = human_context(false)?;
    let required = [
        "terminal-summary.json",
        "native-summary.json",
        "browser-summary.json",
        "matrix.json",
        "screenshot-validation.json",
    ];
    let mut artifact_hashes = serde_json::Map::new();
    for file in required {
        let path = ctx.dir.join(file);
        if !path.exists() {
            bail!(
                "missing human-surface artifact for freshness check: {}",
                path.display()
            );
        }
        let value = read_playground_artifact(&path)?;
        validate_common_fresh_fields(&ctx, &value)
            .with_context(|| format!("stale human-surface artifact {}", path.display()))?;
        artifact_hashes.insert(
            file.to_owned(),
            serde_json::Value::String(hash_file(&path)?),
        );
    }
    validate_all_sidecars_current(&ctx)?;
    let deterministic_report_sha256 = hash_json_map(&artifact_hashes)?;
    let details = json!({
        "success": true,
        "source_hash": ctx.source_hash,
        "control_manifest_hash": ctx.control_manifest_hash,
        "deterministic_ply_report_sha256": ctx.deterministic_ply_report_sha256,
        "human_surface_report_sha256": deterministic_report_sha256,
        "matrix_run_id": ctx.matrix_run_id,
        "artifact_hashes": artifact_hashes,
        "negative_tests": {
            "stale_source_hash_rejected": stale_human_report_rejected(&ctx)?,
            "stale_sidecar_source_hash_rejected": stale_sidecar_rejected(&ctx)?,
        },
    });
    let artifact = ctx.dir.join("fresh-artifacts.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(GateReport {
        name: "verify-ply-human-fresh-artifacts".to_owned(),
        command: "cargo xtask verify-ply-human-fresh-artifacts --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

pub(crate) fn write_ply_human_ai_review_prompts(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    let ctx = human_context(false)?;
    let prompt_dir = ctx.root.join("docs/prompts/renderer-ply-human-surfaces");
    fs::create_dir_all(&prompt_dir)?;
    let prompts = [
        (
            "human-surface-coverage",
            "human-surface-coverage-review.prompt.md",
            "Review target/boon-artifacts/ply-human-surfaces/matrix.json and the live checkout. Pass only if all 22 required examples are controlled on terminal, native, and Firefox browser with no skipped example or target.",
        ),
        (
            "screenshot-authenticity",
            "screenshot-authenticity-review.prompt.md",
            "Inspect screenshot-validation.json plus a sample of terminal/native/browser PNG sidecars. Pass only if screenshots are current, nonblank, target-specific, and cannot be replaced by stale or placeholder images.",
        ),
        (
            "target-control",
            "target-control-review.prompt.md",
            "Review traces and harness code. Pass only if terminal uses a PTY probe, native captures presented Ply frames, and browser uses Firefox WebDriver screenshots and controls.",
        ),
        (
            "stale-artifact",
            "stale-artifact-review.prompt.md",
            "Review fresh-artifacts.json and source hashing. Pass only if stale source hashes, stale sidecars, skipped screenshots, and old matrix runs are rejected.",
        ),
        (
            "fake-pass-harness",
            "fake-pass-harness-review.prompt.md",
            "Attack the xtask verifier. Fail if it can pass by writing JSON without launching surfaces, without screenshots, without Firefox, or without checking image freshness.",
        ),
        (
            "example-behavior",
            "example-behavior-review.prompt.md",
            "Inspect state-changing example traces. Pass only if before/after evidence proves meaningful action through the rendered surface or documented monitor/action-count evidence.",
        ),
    ];
    let mut rows = Vec::new();
    for (slug, file, body) in prompts {
        let path = prompt_dir.join(file);
        let full = format!(
            "# Ply Human Surface Review: {slug}\n\n{body}\n\nUse the live checkout, cite files and line numbers, inspect target/boon-artifacts/ply-human-surfaces, and return JSON with reviewer, model, git_commit, source_hash, control_manifest_hash, deterministic_ply_report_sha256, prompt_file, prompt_sha256, commands_run, files_examined, artifacts_examined, screenshots_examined, findings, verdict.\n"
        );
        fs::write(&path, full)?;
        rows.push(json!({
            "slug": slug,
            "path": path.display().to_string(),
            "sha256": hash_file(&path)?,
        }));
    }
    let details = json!({
        "success": true,
        "source_hash": ctx.source_hash,
        "control_manifest_hash": ctx.control_manifest_hash,
        "deterministic_ply_report_sha256": ctx.deterministic_ply_report_sha256,
        "matrix_run_id": ctx.matrix_run_id,
        "prompt_count": rows.len(),
        "prompts": rows,
        "reports_dir": ctx.dir.join("ai-reviews"),
    });
    let artifact = ctx.dir.join("ai-review-prompts.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(GateReport {
        name: "write-ply-human-ai-review-prompts".to_owned(),
        command: "cargo xtask write-ply-human-ai-review-prompts --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![
            artifact.display().to_string(),
            prompt_dir.display().to_string(),
        ],
        details,
    })
}

pub(crate) fn verify_ply_human_ai_review_reports(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    let ctx = human_context(false)?;
    let prompts_path = ctx.dir.join("ai-review-prompts.json");
    let prompts_manifest = read_playground_artifact(&prompts_path)
        .with_context(|| format!("missing {}", prompts_path.display()))?;
    let mut prompt_hashes = BTreeMap::new();
    let mut prompt_slugs = BTreeMap::new();
    for prompt in prompts_manifest
        .get("prompts")
        .and_then(|value| value.as_array())
        .context("human prompt manifest missing prompts")?
    {
        let path = prompt
            .get("path")
            .and_then(|value| value.as_str())
            .context("human prompt missing path")?
            .to_owned();
        let slug = prompt
            .get("slug")
            .and_then(|value| value.as_str())
            .context("human prompt missing slug")?
            .to_owned();
        let hash = prompt
            .get("sha256")
            .and_then(|value| value.as_str())
            .context("human prompt missing sha256")?
            .to_owned();
        prompt_hashes.insert(path.clone(), hash);
        prompt_slugs.insert(path, slug);
    }
    let reports_dir = ctx.dir.join("ai-reviews");
    fs::create_dir_all(&reports_dir)?;
    let mut paths = fs::read_dir(&reports_dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort();
    if paths.len() < 2 {
        bail!(
            "expected at least two human-surface AI review reports in {}",
            reports_dir.display()
        );
    }
    let current_commit = run_capture("git", &["rev-parse", "HEAD"])?;
    let mut reviewers = BTreeSet::new();
    let mut covered = BTreeSet::new();
    let mut reports = Vec::new();
    for path in paths {
        let report = read_playground_artifact(&path)?;
        validate_human_ai_report(
            &ctx,
            &current_commit,
            &report,
            &prompt_hashes,
            &prompt_slugs,
        )
        .with_context(|| format!("invalid human AI review report {}", path.display()))?;
        reviewers.insert(
            report
                .get("reviewer")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_owned(),
        );
        let prompt_file = report
            .get("prompt_file")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if let Some(slug) = prompt_slugs.get(prompt_file) {
            covered.insert(slug.clone());
        }
        reports.push(json!({"path": path, "report": report}));
    }
    if reviewers.len() < 2 {
        bail!("human-surface AI reports must have at least two distinct reviewers");
    }
    if !covered.contains("screenshot-authenticity") {
        bail!("human-surface AI reviews must include screenshot authenticity coverage");
    }
    if !covered.contains("target-control") && !covered.contains("human-surface-coverage") {
        bail!("human-surface AI reviews must include target-control or full coverage review");
    }
    let details = json!({
        "success": true,
        "source_hash": ctx.source_hash,
        "control_manifest_hash": ctx.control_manifest_hash,
        "deterministic_ply_report_sha256": ctx.deterministic_ply_report_sha256,
        "matrix_run_id": ctx.matrix_run_id,
        "review_count": reports.len(),
        "reviewers": reviewers,
        "covered_prompt_slugs": covered,
        "reports": reports,
        "residual_risks": [],
    });
    let artifact = ctx.dir.join("ai-review-reports.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(GateReport {
        name: "verify-ply-human-ai-review-reports".to_owned(),
        command: "cargo xtask verify-ply-human-ai-review-reports --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

pub(crate) fn verify_ply_human_surfaces(_args: &[String]) -> Result<()> {
    human_context(false)?;
    let mut gates = Vec::new();
    gates.push(crate::capture_gate(
        "verify-ply-human-terminal",
        "cargo xtask verify-ply-human-terminal --format json",
        || verify_ply_human_terminal(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(crate::capture_gate(
        "verify-ply-human-native",
        "cargo xtask verify-ply-human-native --format json",
        || verify_ply_human_native(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(crate::capture_gate(
        "verify-ply-human-browser",
        "cargo xtask verify-ply-human-browser --browser firefox --format json",
        || {
            verify_ply_human_browser(&[
                "--browser".to_owned(),
                "firefox".to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
            ])
        },
    ));
    let matrix = write_matrix_artifact()?;
    gates.push(crate::capture_gate(
        "verify-ply-human-screenshots",
        "cargo xtask verify-ply-human-screenshots --format json",
        || verify_ply_human_screenshots(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(crate::capture_gate(
        "verify-ply-human-fresh-artifacts",
        "cargo xtask verify-ply-human-fresh-artifacts --format json",
        || verify_ply_human_fresh_artifacts(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(crate::capture_gate(
        "write-ply-human-ai-review-prompts",
        "cargo xtask write-ply-human-ai-review-prompts --format json",
        || write_ply_human_ai_review_prompts(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(crate::capture_gate(
        "verify-ply-human-ai-review-reports",
        "cargo xtask verify-ply-human-ai-review-reports --format json",
        || verify_ply_human_ai_review_reports(&["--format".to_owned(), "json".to_owned()]),
    ));
    let success = gates.iter().all(|gate| gate.status == "passed");
    let ctx = human_context(false)?;
    let artifact = ctx.dir.join("verify-ply-human-surfaces.json");
    fs::write(
        &artifact,
        serde_json::to_vec_pretty(&json!({
            "success": success,
            "matrix": matrix,
            "gates": gates,
        }))?,
    )?;
    let success_path = ctx.dir.join("success.json");
    fs::write(
        &success_path,
        serde_json::to_vec_pretty(&json!({
            "success": success,
            "verify_report": artifact,
            "matrix": ctx.dir.join("matrix.json"),
        }))?,
    )?;
    if !success {
        bail!(
            "Ply human-surface verification failed; see {}",
            artifact.display()
        );
    }
    Ok(())
}

pub(crate) fn write_matrix_artifact() -> Result<serde_json::Value> {
    let ctx = human_context(false)?;
    let mut target_summaries = BTreeMap::new();
    let mut failures = Vec::new();
    let mut coverage = serde_json::Map::new();
    let mut matrix_examples = Vec::new();
    for target in TARGETS {
        let path = ctx.dir.join(format!("{target}-summary.json"));
        let summary = read_playground_artifact(&path)
            .with_context(|| format!("missing target summary {}", path.display()))?;
        validate_common_fresh_fields(&ctx, &summary)
            .with_context(|| format!("stale target summary {}", path.display()))?;
        let examples = summary
            .get("examples")
            .or_else(|| summary.pointer("/details/examples"))
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        if examples.len() != boon_dd::REQUIRED_EXAMPLES.len() {
            failures.push(json!({
                "target": target,
                "error": format!("covered {} examples, expected {}", examples.len(), boon_dd::REQUIRED_EXAMPLES.len()),
            }));
        }
        let screenshot_count = examples
            .iter()
            .filter_map(|example| {
                example
                    .get("screenshots")
                    .and_then(|value| value.as_array())
            })
            .map(|array| array.len())
            .sum::<usize>();
        coverage.insert(
            target.to_owned(),
            json!({
                "example_count": examples.len(),
                "screenshot_count": screenshot_count,
            }),
        );
        target_summaries.insert(target.to_owned(), summary);
    }
    for example in boon_dd::REQUIRED_EXAMPLES {
        let mut target_rows = serde_json::Map::new();
        for target in TARGETS {
            let summary = target_summaries.get(target).unwrap();
            let examples = summary
                .get("examples")
                .or_else(|| summary.pointer("/details/examples"))
                .and_then(|value| value.as_array())
                .cloned()
                .unwrap_or_default();
            let row = examples
                .iter()
                .find(|row| row.get("example").and_then(|value| value.as_str()) == Some(example));
            match row {
                Some(row) => {
                    target_rows.insert(target.to_owned(), row.clone());
                }
                None => failures.push(json!({
                    "target": target,
                    "example": example,
                    "error": "missing example coverage",
                })),
            }
        }
        matrix_examples.push(json!({
            "example": example,
            "targets": target_rows,
        }));
    }
    let details = json!({
        "success": failures.is_empty(),
        "source_hash": ctx.source_hash,
        "control_manifest_hash": ctx.control_manifest_hash,
        "deterministic_ply_report_sha256": ctx.deterministic_ply_report_sha256,
        "matrix_run_id": ctx.matrix_run_id,
        "required_examples": boon_dd::REQUIRED_EXAMPLES,
        "targets": TARGETS,
        "coverage": coverage,
        "examples": matrix_examples,
        "failures": failures,
        "residual_risks": [],
    });
    let artifact = ctx.dir.join("matrix.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    if details["success"].as_bool() != Some(true) {
        bail!(
            "human-surface matrix has failures; see {}",
            artifact.display()
        );
    }
    Ok(details)
}

fn human_context(reset: bool) -> Result<HumanContext> {
    let root = repo_root()?;
    let dir = artifacts_dir()?.join("ply-human-surfaces");
    fs::create_dir_all(&dir)?;
    if reset {
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir)?;
    }
    let run_id_path = dir.join("run-id.txt");
    let matrix_run_id = if run_id_path.exists() {
        fs::read_to_string(&run_id_path)?.trim().to_owned()
    } else {
        let run_id = format!(
            "ply-human-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis()
        );
        fs::write(&run_id_path, &run_id)?;
        run_id
    };
    ensure_headless_deterministic_report()?;
    let fresh = ply_artifacts_dir()?.join("fresh-artifacts.json");
    if !fresh.exists() {
        run_status(
            "cargo",
            &["xtask", "verify-ply-fresh-artifacts", "--format", "json"],
        )?;
    }
    let fresh_value = read_playground_artifact(&fresh)?;
    let deterministic_ply_report_sha256 = fresh_value
        .get("deterministic_report_sha256")
        .and_then(|value| value.as_str())
        .context("Ply fresh-artifacts missing deterministic_report_sha256")?
        .to_owned();
    Ok(HumanContext {
        source_hash: hash_human_sources(&root)?,
        control_manifest_hash: hash_file(
            &root.join("docs/ply-human-surfaces/control-manifest.toml"),
        )?,
        deterministic_ply_report_sha256,
        matrix_run_id,
        git_commit: run_capture("git", &["rev-parse", "HEAD"])?,
        git_status_short: run_capture("git", &["status", "--short"])?,
        git_dirty: !run_capture("git", &["status", "--short"])?
            .trim()
            .is_empty(),
        command_line: std::env::args().collect(),
        process_id: std::process::id(),
        firefox_executable: run_capture("bash", &["-lc", "command -v firefox || true"])
            .unwrap_or_default(),
        firefox_version: run_capture("firefox", &["--version"]).unwrap_or_default(),
        geckodriver_version: run_capture("bash", &["-lc", "geckodriver --version | head -n 1"])
            .unwrap_or_default(),
        root,
        dir,
    })
}

fn ensure_headless_deterministic_report() -> Result<()> {
    let ply_dir = ply_artifacts_dir()?;
    let required = [
        "headless-matrix.json",
        "native-smoke.json",
        "browser-smoke.json",
        "no-old-renderers.json",
    ];
    if required.iter().all(|file| ply_dir.join(file).exists()) {
        return Ok(());
    }
    run_status(
        "cargo",
        &["xtask", "verify-ply-headless", "--format", "json"],
    )?;
    Ok(())
}

fn load_and_validate_manifest(ctx: &HumanContext) -> Result<ControlManifest> {
    let path = ctx
        .root
        .join("docs/ply-human-surfaces/control-manifest.toml");
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let manifest: ControlManifest = toml::from_str(&text)?;
    if manifest.version != 1 {
        bail!("control manifest version must be 1");
    }
    if manifest.targets != TARGETS {
        bail!("control manifest targets must be {:?}", TARGETS);
    }
    let required = boon_dd::REQUIRED_EXAMPLES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<BTreeSet<_>>();
    let actual = manifest.examples.keys().cloned().collect::<BTreeSet<_>>();
    if actual != required {
        bail!("control manifest examples differ from REQUIRED_EXAMPLES");
    }
    for example in boon_dd::REQUIRED_EXAMPLES {
        let row = &manifest.examples[*example];
        let _notes = row.notes.as_deref().unwrap_or("");
        if row.required_targets != TARGETS {
            bail!("control manifest {example} has wrong required_targets");
        }
        if !row
            .actions
            .iter()
            .any(|action| action.kind == "select_example")
        {
            bail!("control manifest {example} missing select_example action");
        }
        if !row
            .actions
            .iter()
            .any(|action| action.kind == "screenshot" && action.name.as_deref() == Some("selected"))
        {
            bail!("control manifest {example} missing selected screenshot");
        }
        if !row.actions.iter().any(|action| {
            action.kind == "screenshot" && action.name.as_deref() == Some("after-action")
        }) {
            bail!("control manifest {example} missing after-action screenshot");
        }
    }
    Ok(manifest)
}

fn action_to_json(action: &ManifestAction) -> serde_json::Value {
    json!({
        "kind": &action.kind,
        "name": &action.name,
        "value": &action.value,
        "semantic": &action.semantic,
        "field": &action.field,
    })
}

fn write_manifest_actions_file(dir: &Path, actions: &[ManifestAction]) -> Result<PathBuf> {
    let path = dir.join("manifest-actions.json");
    fs::write(&path, serde_json::to_vec_pretty(actions)?)?;
    Ok(path)
}

fn run_terminal_pty_probe(
    root: &Path,
    example_index: usize,
    example: &str,
    example_dir: &Path,
    actions: &[ManifestAction],
) -> Result<TerminalRunResult> {
    let frames_dir = example_dir.join("pty-frames");
    if frames_dir.exists() {
        fs::remove_dir_all(&frames_dir)?;
    }
    fs::create_dir_all(&frames_dir)?;
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut command = CommandBuilder::new("cargo");
    command.args([
        "run",
        "--quiet",
        "-p",
        "boon_backend_ratatui",
        "--bin",
        "terminal_playground",
        "--",
        "--human-output-dir",
        frames_dir.to_str().unwrap(),
    ]);
    command.cwd(root);
    let mut child = pair.slave.spawn_command(command)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader()?;
    let reader_handle = std::thread::spawn(move || {
        let mut output = Vec::new();
        let _ = reader.read_to_end(&mut output);
        output
    });
    let mut writer = pair.master.take_writer()?;
    wait_for_terminal_frame(&frames_dir, 0, Duration::from_secs(15))?;

    let mut frame_index = 0_usize;
    let mut captures = BTreeMap::new();
    let mut screenshot_paths = Vec::new();
    let mut sent_inputs = Vec::new();
    let mut events = Vec::new();
    for action in actions {
        match action.kind.as_str() {
            "select_example" => {
                send_terminal_key(&mut writer, "Home")?;
                frame_index += 1;
                wait_for_terminal_frame(&frames_dir, frame_index, Duration::from_secs(5))?;
                sent_inputs.push("Home".to_owned());
                for _ in 0..example_index {
                    send_terminal_key(&mut writer, "ArrowDown")?;
                    frame_index += 1;
                    wait_for_terminal_frame(&frames_dir, frame_index, Duration::from_secs(5))?;
                    sent_inputs.push("ArrowDown".to_owned());
                }
                let capture = read_terminal_frame(&frames_dir, frame_index)?;
                if capture.selected_example != example {
                    bail!(
                        "terminal PTY selected {} while manifest requested {example}",
                        capture.selected_example
                    );
                }
            }
            "type_text" => {
                for ch in action.value.as_deref().unwrap_or_default().chars() {
                    let mut text = [0_u8; 4];
                    writer.write_all(ch.encode_utf8(&mut text).as_bytes())?;
                    writer.flush()?;
                    frame_index += 1;
                    wait_for_terminal_frame(&frames_dir, frame_index, Duration::from_secs(5))?;
                    sent_inputs.push(format!("char:{ch}"));
                    std::thread::sleep(Duration::from_millis(15));
                }
            }
            "press_key" => {
                let key = action.value.as_deref().unwrap_or_default();
                send_terminal_key(&mut writer, key)?;
                frame_index += 1;
                wait_for_terminal_frame(&frames_dir, frame_index, Duration::from_secs(5))?;
                sent_inputs.push(key.to_owned());
            }
            "activate" => {
                let key = if action.semantic.as_deref() == Some("decrement") {
                    "-"
                } else {
                    "Enter"
                };
                send_terminal_key(&mut writer, key)?;
                frame_index += 1;
                wait_for_terminal_frame(&frames_dir, frame_index, Duration::from_secs(5))?;
                sent_inputs.push(format!("activate:{key}"));
            }
            "wait_frames" => {
                let frames = action
                    .value
                    .as_deref()
                    .unwrap_or("1")
                    .parse::<u64>()
                    .unwrap_or(1);
                std::thread::sleep(Duration::from_millis(frames.saturating_mul(16)));
            }
            "wait_millis" => {
                let millis = action
                    .value
                    .as_deref()
                    .unwrap_or("1")
                    .parse::<u64>()
                    .unwrap_or(1);
                std::thread::sleep(Duration::from_millis(millis));
            }
            "screenshot" => {
                let name = action.name.as_deref().unwrap_or("screenshot");
                let capture = read_terminal_frame(&frames_dir, frame_index)?;
                let source_txt = terminal_frame_path(&frames_dir, frame_index, "txt");
                let target_txt = example_dir.join(format!("screen-{name}.txt"));
                if source_txt.exists() {
                    fs::copy(&source_txt, &target_txt)?;
                } else {
                    fs::write(&target_txt, capture.lines.join("\n"))?;
                }
                let target_png = example_dir.join(format!("screen-{name}.png"));
                write_terminal_png(&capture.lines, &target_png)?;
                captures.insert(name.to_owned(), capture);
                screenshot_paths.push(target_png);
            }
            kind if kind.starts_with("assert_") => {}
            other => bail!("unsupported terminal manifest action {other} for {example}"),
        }
        events.push(action_to_json(action));
    }
    writer.write_all(b"q")?;
    writer.flush()?;
    drop(writer);
    let status = child.wait()?;
    let output = reader_handle.join().unwrap_or_default();
    if !status.success() {
        bail!("terminal PTY probe exited with {status:?}");
    }
    Ok(TerminalRunResult {
        trace: json!({
            "spawned_interactive_terminal": true,
            "pty_rows": 40,
            "pty_cols": 120,
            "example": example,
            "down_presses": example_index,
            "sent_inputs": sent_inputs,
            "sent_quit": true,
            "events": events,
            "exported_frame_count": frame_index + 1,
            "frames_dir": frames_dir,
            "raw_output_bytes": output.len(),
            "raw_output_contains_example": String::from_utf8_lossy(&output).contains(example),
        }),
        captures,
        screenshot_paths,
    })
}

fn send_terminal_key(writer: &mut Box<dyn Write + Send>, key: &str) -> Result<()> {
    match key {
        "Enter" => writer.write_all(b"\r")?,
        "Space" | " " => writer.write_all(b" ")?,
        "Home" => writer.write_all(b"\x1b[H")?,
        "End" => writer.write_all(b"\x1b[F")?,
        "ArrowDown" | "Down" => writer.write_all(b"\x1b[B")?,
        "ArrowUp" | "Up" => writer.write_all(b"\x1b[A")?,
        "ArrowLeft" | "Left" => writer.write_all(b"\x1b[D")?,
        "ArrowRight" | "Right" => writer.write_all(b"\x1b[C")?,
        "Backspace" => writer.write_all(&[0x7f])?,
        "-" | "Minus" => writer.write_all(b"-")?,
        other if other.chars().count() == 1 => writer.write_all(other.as_bytes())?,
        other => bail!("unsupported terminal key {other}"),
    }
    writer.flush()?;
    std::thread::sleep(Duration::from_millis(25));
    Ok(())
}

fn terminal_frame_path(frames_dir: &Path, frame_index: usize, ext: &str) -> PathBuf {
    frames_dir.join(format!("pty-frame-{frame_index:03}.{ext}"))
}

fn wait_for_terminal_frame(frames_dir: &Path, frame_index: usize, timeout: Duration) -> Result<()> {
    let path = terminal_frame_path(frames_dir, frame_index, "json");
    let start = Instant::now();
    while start.elapsed() <= timeout {
        if path.exists()
            && fs::read(&path)
                .ok()
                .and_then(|bytes| {
                    serde_json::from_slice::<
                        boon_backend_ratatui::human_surface::TerminalScreenCapture,
                    >(&bytes)
                    .ok()
                })
                .is_some()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    bail!("timed out waiting for terminal frame {}", path.display())
}

fn read_terminal_frame(
    frames_dir: &Path,
    frame_index: usize,
) -> Result<boon_backend_ratatui::human_surface::TerminalScreenCapture> {
    let path = terminal_frame_path(frames_dir, frame_index, "json");
    let value = read_playground_artifact(&path)?;
    Ok(serde_json::from_value(value)?)
}

fn validate_terminal_capture(
    example: &str,
    selected: &boon_backend_ratatui::human_surface::TerminalScreenCapture,
    after_action: &boon_backend_ratatui::human_surface::TerminalScreenCapture,
) -> Result<()> {
    if selected.selected_example != example || after_action.selected_example != example {
        bail!("terminal selected example mismatch");
    }
    if selected.nonblank_cells == 0 || after_action.nonblank_cells == 0 {
        bail!("terminal capture is blank");
    }
    if !selected.lines.iter().any(|line| line.contains(example))
        || !after_action.lines.iter().any(|line| line.contains(example))
    {
        bail!("terminal screen does not include selected example name");
    }
    if !state_changed_terminal(selected, after_action) {
        bail!("terminal after-action state did not change for {example}");
    }
    Ok(())
}

fn state_changed_terminal(
    selected: &boon_backend_ratatui::human_surface::TerminalScreenCapture,
    after_action: &boon_backend_ratatui::human_surface::TerminalScreenCapture,
) -> bool {
    selected.render_output_text != after_action.render_output_text
        || selected.selected_action_count != after_action.selected_action_count
        || selected.selected_monitor_count != after_action.selected_monitor_count
        || selected.input_buffer != after_action.input_buffer
        || selected.last_submitted_text != after_action.last_submitted_text
        || selected.interaction_log.len() != after_action.interaction_log.len()
}

fn write_terminal_png(lines: &[String], path: &Path) -> Result<()> {
    let cell_w = 8_u32;
    let cell_h = 14_u32;
    let width = 120 * cell_w;
    let height = 40 * cell_h;
    let mut image = ImageBuffer::from_pixel(width, height, Rgba([17, 24, 39, 255]));
    for (row, line) in lines.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            if ch == ' ' {
                continue;
            }
            let code = ch as u32;
            let color = Rgba([
                80_u8.saturating_add((code % 110) as u8),
                120_u8.saturating_add((code % 80) as u8),
                160_u8.saturating_add((code % 70) as u8),
                255,
            ]);
            let x0 = col as u32 * cell_w + 1;
            let y0 = row as u32 * cell_h + 2;
            for y in y0..(y0 + cell_h - 4).min(height) {
                for x in x0..(x0 + cell_w - 2).min(width) {
                    image.put_pixel(x, y, color);
                }
            }
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    image.save(path)?;
    Ok(())
}

fn target_summary(
    ctx: &HumanContext,
    target: &str,
    examples: Vec<serde_json::Value>,
) -> serde_json::Value {
    json!({
        "success": true,
        "target": target,
        "source_hash": ctx.source_hash,
        "control_manifest_hash": ctx.control_manifest_hash,
        "deterministic_ply_report_sha256": ctx.deterministic_ply_report_sha256,
        "matrix_run_id": ctx.matrix_run_id,
        "provenance": provenance_json(ctx),
        "required_examples": boon_dd::REQUIRED_EXAMPLES,
        "example_count": examples.len(),
        "examples": examples,
    })
}

fn provenance_json(ctx: &HumanContext) -> serde_json::Value {
    json!({
        "git_commit": &ctx.git_commit,
        "git_dirty": ctx.git_dirty,
        "git_status_short": &ctx.git_status_short,
        "command_line": &ctx.command_line,
        "process_id": ctx.process_id,
        "generated_at_unix_ms": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default(),
        "firefox_executable": &ctx.firefox_executable,
        "firefox_version": &ctx.firefox_version,
        "geckodriver_version": &ctx.geckodriver_version,
    })
}

fn validate_native_telemetry(
    example: &str,
    ctx: &HumanContext,
    telemetry: &serde_json::Value,
) -> Result<()> {
    if telemetry.get("target").and_then(|value| value.as_str()) != Some("native") {
        bail!("native telemetry target mismatch");
    }
    if telemetry.get("example").and_then(|value| value.as_str()) != Some(example) {
        bail!("native telemetry example mismatch");
    }
    validate_common_fresh_fields(ctx, telemetry)?;
    if telemetry
        .get("captured_from_macroquad_framebuffer")
        .and_then(|value| value.as_bool())
        != Some(true)
    {
        bail!("native telemetry did not prove Macroquad frame-buffer capture");
    }
    if !native_state_changed(telemetry) {
        bail!("native after-action state did not change for {example}");
    }
    Ok(())
}

fn native_state_changed(telemetry: &serde_json::Value) -> bool {
    telemetry.pointer("/before/selected_output_text")
        != telemetry.pointer("/after_action/selected_output_text")
        || telemetry.pointer("/before/selected_action_count")
            != telemetry.pointer("/after_action/selected_action_count")
        || telemetry.pointer("/before/selected_monitor_count")
            != telemetry.pointer("/after_action/selected_monitor_count")
        || telemetry.pointer("/before/input_buffer")
            != telemetry.pointer("/after_action/input_buffer")
        || telemetry.pointer("/before/last_submitted_text")
            != telemetry.pointer("/after_action/last_submitted_text")
        || telemetry
            .pointer("/before/interaction_log")
            .and_then(|value| value.as_array())
            .map(|values| values.len())
            != telemetry
                .pointer("/after_action/interaction_log")
                .and_then(|value| value.as_array())
                .map(|values| values.len())
}

fn browser_state_changed(trace: &serde_json::Value) -> bool {
    trace.pointer("/selected/live_state/selected_output_text")
        != trace.pointer("/after_action/live_state/selected_output_text")
        || trace.pointer("/selected/live_state/selected_action_count")
            != trace.pointer("/after_action/live_state/selected_action_count")
        || trace.pointer("/selected/live_state/selected_monitor_count")
            != trace.pointer("/after_action/live_state/selected_monitor_count")
        || trace.pointer("/selected/live_state/input_buffer")
            != trace.pointer("/after_action/live_state/input_buffer")
        || trace.pointer("/selected/live_state/last_submitted_text")
            != trace.pointer("/after_action/live_state/last_submitted_text")
        || interaction_log_len(trace.get("selected").unwrap_or(&serde_json::Value::Null))
            != interaction_log_len(
                trace
                    .get("after_action")
                    .unwrap_or(&serde_json::Value::Null),
            )
}

fn ensure_manifest_trace(
    example: &str,
    target: &str,
    manifest: &ControlManifest,
    trace: &serde_json::Value,
) -> Result<()> {
    if trace.get("target").and_then(|value| value.as_str()) != Some(target) {
        bail!("trace target mismatch");
    }
    if trace.get("example").and_then(|value| value.as_str()) != Some(example) {
        bail!("trace example mismatch");
    }
    let manifest_actions = trace
        .get("manifest_actions")
        .and_then(|value| value.as_array())
        .context("trace missing manifest_actions")?;
    let expected = manifest.examples[example]
        .actions
        .iter()
        .map(action_to_json)
        .collect::<Vec<_>>();
    if manifest_actions.as_slice() != expected.as_slice() {
        bail!("trace does not record all manifest actions for {example}/{target}");
    }
    let events = trace
        .get("events")
        .and_then(|value| value.as_array())
        .context("trace missing events")?;
    if events.as_slice() != expected.as_slice() {
        bail!("trace events do not exactly match manifest actions for {example}/{target}");
    }
    Ok(())
}

fn ensure_no_native_human_process_left() -> Result<()> {
    let output = run_capture(
        "bash",
        &[
            "-lc",
            "pgrep -af 'native_playground.*--human-surface' | rg -v 'pgrep|rg' || true",
        ],
    )?;
    if !output.trim().is_empty() {
        bail!("native human-surface process still running:\n{output}");
    }
    Ok(())
}

struct StaticServer {
    addr: String,
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl StaticServer {
    fn start(serve_dir: PathBuf) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?.to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let join = std::thread::spawn(move || {
            while !stop_thread.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = handle_human_static_request(&mut stream, &serve_dir);
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            addr,
            stop,
            join: Some(join),
        })
    }

    fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn handle_human_static_request(stream: &mut TcpStream, serve_dir: &Path) -> Result<()> {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 8192];
    loop {
        let read = stream.read(&mut temp)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let request = String::from_utf8_lossy(&buffer);
    let request_line = request.lines().next().unwrap_or_default();
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/");
    if request_line.starts_with("POST ") && path == "/result" {
        write_http_response(stream, "204 No Content", "text/plain", b"")?;
        return Ok(());
    }
    let relative = path.trim_start_matches('/');
    let file = if relative.is_empty() {
        serve_dir.join("index.html")
    } else {
        serve_dir.join(relative)
    };
    if file.exists() && file.is_file() {
        let bytes = fs::read(&file)?;
        write_http_response(stream, "200 OK", content_type(&file), &bytes)?;
    } else {
        write_http_response(stream, "404 Not Found", "text/plain", b"not found")?;
    }
    Ok(())
}

fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "html" => "text/html",
        "js" => "application/javascript",
        "wasm" => "application/wasm",
        "png" => "image/png",
        _ => "application/octet-stream",
    }
}

fn run_browser_manifest(
    driver: &WebDriver,
    example_index: usize,
    example: &str,
    example_dir: &Path,
    actions: &[ManifestAction],
    ctx: &HumanContext,
) -> Result<serde_json::Value> {
    let mut selected = None;
    let mut after_action = None;
    let mut extra_screenshots = Vec::new();
    let mut events = Vec::new();
    for action in actions {
        match action.kind.as_str() {
            "select_example" => {
                let requested = action.value.as_deref().unwrap_or(example);
                if requested != example {
                    bail!("browser manifest requested {requested}, expected {example}");
                }
                driver.select_example(example_index, example)?;
            }
            "type_text" => {
                let before = driver.wait_for_telemetry(Duration::from_secs(5))?;
                let min_count = interaction_log_len(&before) + 1;
                driver.type_text(action.value.as_deref().unwrap_or_default())?;
                let _ = driver
                    .wait_for_interaction_count_at_least(min_count, Duration::from_secs(10))?;
            }
            "press_key" => {
                let before = driver.wait_for_telemetry(Duration::from_secs(5))?;
                let min_count = interaction_log_len(&before) + 1;
                driver.press_key(action.value.as_deref().unwrap_or_default())?;
                let _ = driver
                    .wait_for_interaction_count_at_least(min_count, Duration::from_secs(10))?;
            }
            "activate" => {
                let before = driver.wait_for_telemetry(Duration::from_secs(5))?;
                let min_count = interaction_log_len(&before) + 1;
                let key = if action.semantic.as_deref() == Some("decrement") {
                    "-"
                } else {
                    "Enter"
                };
                driver.press_key(key)?;
                let _ = driver
                    .wait_for_interaction_count_at_least(min_count, Duration::from_secs(10))?;
            }
            "wait_frames" => {
                let frames = action
                    .value
                    .as_deref()
                    .unwrap_or("1")
                    .parse::<u64>()
                    .unwrap_or(1);
                std::thread::sleep(Duration::from_millis(frames.saturating_mul(16)));
            }
            "wait_millis" => {
                let millis = action
                    .value
                    .as_deref()
                    .unwrap_or("1")
                    .parse::<u64>()
                    .unwrap_or(1);
                std::thread::sleep(Duration::from_millis(millis));
            }
            "screenshot" => {
                let name = action.name.as_deref().unwrap_or("screenshot");
                let telemetry = if name == "after-action" {
                    if let Some(before) = selected.as_ref() {
                        driver.wait_for_action_change(before, Duration::from_secs(15))?
                    } else {
                        driver.wait_for_telemetry(Duration::from_secs(5))?
                    }
                } else {
                    driver.wait_for_telemetry(Duration::from_secs(5))?
                };
                let path = example_dir.join(format!("{name}.png"));
                driver.screenshot(&path)?;
                if name == "selected" {
                    selected = Some(telemetry);
                } else if name == "after-action" {
                    after_action = Some(telemetry);
                } else {
                    extra_screenshots.push(json!({
                        "name": name,
                        "path": path,
                        "telemetry": telemetry,
                    }));
                }
            }
            kind if kind.starts_with("assert_") => {}
            other => bail!("unsupported browser manifest action {other} for {example}"),
        }
        events.push(action_to_json(action));
    }
    Ok(json!({
        "target": "browser",
        "browser": "firefox",
        "example": example,
        "source_hash": ctx.source_hash,
        "control_manifest_hash": ctx.control_manifest_hash,
        "deterministic_ply_report_sha256": ctx.deterministic_ply_report_sha256,
        "matrix_run_id": ctx.matrix_run_id,
        "control_source": "Firefox WebDriver Actions keyboard control and screenshot protocol",
        "events": events,
        "selected": selected.context("browser manifest did not capture selected screenshot")?,
        "after_action": after_action.context("browser manifest did not capture after-action screenshot")?,
        "extra_screenshots": extra_screenshots,
    }))
}

fn webdriver_key_value(key: &str) -> String {
    match key {
        "Enter" => "\u{E006}".to_owned(),
        "Backspace" => "\u{E003}".to_owned(),
        "Tab" => "\u{E004}".to_owned(),
        "Escape" | "Esc" => "\u{E00C}".to_owned(),
        "Space" | " " => " ".to_owned(),
        "Home" => "\u{E011}".to_owned(),
        "ArrowLeft" | "Left" => "\u{E012}".to_owned(),
        "ArrowUp" | "Up" => "\u{E013}".to_owned(),
        "ArrowRight" | "Right" => "\u{E014}".to_owned(),
        "ArrowDown" | "Down" => "\u{E015}".to_owned(),
        "End" => "\u{E010}".to_owned(),
        "PageUp" => "\u{E00E}".to_owned(),
        "PageDown" => "\u{E00F}".to_owned(),
        "Minus" | "-" => "-".to_owned(),
        other => other.to_owned(),
    }
}

fn interaction_log_len(value: &serde_json::Value) -> usize {
    value
        .pointer("/live_state/interaction_log")
        .and_then(|value| value.as_array())
        .map(|values| values.len())
        .unwrap_or(0)
}

impl WebDriver {
    fn start() -> Result<Self> {
        let port = pick_free_port()?;
        let child = Command::new("geckodriver")
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to spawn geckodriver")?;
        wait_for_webdriver_status(port, Duration::from_secs(10))?;
        let body = json!({
            "capabilities": {
                "alwaysMatch": {
                    "browserName": "firefox",
                    "acceptInsecureCerts": true,
                    "moz:firefoxOptions": {
                        "args": ["-headless", "-width", EXPECTED_NATIVE_WIDTH.to_string(), "-height", EXPECTED_NATIVE_HEIGHT.to_string()]
                    }
                }
            }
        });
        let response = retry_webdriver_request(
            port,
            "POST",
            "/session",
            Some(&body),
            Duration::from_secs(15),
        )?;
        let session_id = response
            .pointer("/value/sessionId")
            .and_then(|value| value.as_str())
            .or_else(|| response.get("sessionId").and_then(|value| value.as_str()))
            .context("geckodriver response missing sessionId")?
            .to_owned();
        Ok(Self {
            port,
            child,
            session_id,
        })
    }

    fn navigate(&self, url: &str) -> Result<()> {
        self.request("POST", "/url", Some(&json!({ "url": url })))?;
        Ok(())
    }

    fn set_window_rect(&self, width: u32, height: u32) -> Result<()> {
        self.request(
            "POST",
            "/window/rect",
            Some(&json!({ "x": 0, "y": 0, "width": width, "height": height })),
        )?;
        Ok(())
    }

    fn focus_canvas(&self) -> Result<()> {
        if let Ok(element) = self.find_element("#glcanvas") {
            let endpoint = format!("/element/{element}/click");
            let _ = self.request("POST", &endpoint, Some(&json!({})));
        }
        self.execute(
            r#"
window.__boonWebdriverKeyEvents = window.__boonWebdriverKeyEvents || [];
if (!window.__boonWebdriverKeyLoggerInstalled) {
  window.__boonWebdriverKeyLoggerInstalled = true;
  for (const target of [window, document]) {
    target.addEventListener("keydown", (event) => {
      window.__boonWebdriverKeyEvents.push({
        type: "keydown",
        key: event.key,
        code: event.code,
        target: event.target && event.target.id || event.target && event.target.tagName || null,
        active: document.activeElement && document.activeElement.id || document.activeElement && document.activeElement.tagName || null,
        trusted: event.isTrusted
      });
    }, true);
  }
}
const c = document.getElementById('glcanvas');
if (c) { window.focus(); c.focus(); }
return {found: !!c, active: document.activeElement && document.activeElement.id || null};
"#,
            vec![],
        )?;
        Ok(())
    }

    fn find_element(&self, selector: &str) -> Result<String> {
        let response = self.request(
            "POST",
            "/element",
            Some(&json!({"using": "css selector", "value": selector})),
        )?;
        let value = response
            .get("value")
            .context("find element missing value")?;
        value
            .get("element-6066-11e4-a52e-4f735466cecf")
            .or_else(|| value.get("ELEMENT"))
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .context("find element missing WebDriver element id")
    }

    fn select_example(&self, index: usize, example: &str) -> Result<()> {
        self.press_key("Home")?;
        for _ in 0..index {
            self.press_key("ArrowDown")?;
        }
        let telemetry = self.wait_for_selected(example, Duration::from_secs(15))?;
        if telemetry
            .get("selected_example")
            .and_then(|value| value.as_str())
            != Some(example)
        {
            bail!("Firefox did not select {example}: {telemetry}");
        }
        Ok(())
    }

    fn press_key(&self, key: &str) -> Result<()> {
        self.focus_canvas()?;
        self.send_key_values(&[webdriver_key_value(key)])
    }

    fn type_text(&self, value: &str) -> Result<()> {
        self.focus_canvas()?;
        let keys = value.chars().map(|ch| ch.to_string()).collect::<Vec<_>>();
        self.send_key_values(&keys)
    }

    fn send_key_values(&self, values: &[String]) -> Result<()> {
        if let Ok(element) = self.find_element("#glcanvas") {
            let endpoint = format!("/element/{element}/value");
            let text = values.join("");
            let result = self.request(
                "POST",
                &endpoint,
                Some(&json!({
                    "text": text,
                    "value": values,
                })),
            );
            if result.is_ok() {
                std::thread::sleep(Duration::from_millis(120));
                return Ok(());
            }
        }
        let mut actions = Vec::new();
        for value in values {
            actions.push(json!({"type": "keyDown", "value": value}));
            actions.push(json!({"type": "keyUp", "value": value}));
        }
        self.request(
            "POST",
            "/actions",
            Some(&json!({
                "actions": [{
                    "type": "key",
                    "id": "keyboard",
                    "actions": actions,
                }]
            })),
        )?;
        let _ = self.request("DELETE", "/actions", None);
        std::thread::sleep(Duration::from_millis(120));
        Ok(())
    }

    fn wait_for_selected(&self, example: &str, timeout: Duration) -> Result<serde_json::Value> {
        let start = Instant::now();
        loop {
            let telemetry = self.wait_for_telemetry(Duration::from_secs(2))?;
            if telemetry
                .get("selected_example")
                .and_then(|value| value.as_str())
                == Some(example)
            {
                return Ok(telemetry);
            }
            if start.elapsed() > timeout {
                bail!(
                    "timed out waiting for Firefox selected example {example}; last telemetry {telemetry}"
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn wait_for_action_change(
        &self,
        before: &serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value> {
        let start = Instant::now();
        loop {
            let telemetry = self.wait_for_telemetry(Duration::from_secs(2))?;
            if telemetry.pointer("/live_state/selected_output_text")
                != before.pointer("/live_state/selected_output_text")
                || telemetry.pointer("/live_state/selected_action_count")
                    != before.pointer("/live_state/selected_action_count")
                || telemetry.pointer("/live_state/selected_monitor_count")
                    != before.pointer("/live_state/selected_monitor_count")
                || telemetry.pointer("/live_state/input_buffer")
                    != before.pointer("/live_state/input_buffer")
                || telemetry.pointer("/live_state/last_submitted_text")
                    != before.pointer("/live_state/last_submitted_text")
                || interaction_log_len(&telemetry) != interaction_log_len(before)
            {
                return Ok(telemetry);
            }
            if start.elapsed() > timeout {
                bail!(
                    "timed out waiting for Firefox action change; before {before}, after {telemetry}"
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn wait_for_interaction_count_at_least(
        &self,
        min_count: usize,
        timeout: Duration,
    ) -> Result<serde_json::Value> {
        let start = Instant::now();
        loop {
            let telemetry = self.wait_for_telemetry(Duration::from_secs(2))?;
            if interaction_log_len(&telemetry) >= min_count {
                return Ok(telemetry);
            }
            if start.elapsed() > timeout {
                let debug = self.browser_key_debug().unwrap_or_else(|_| json!({}));
                bail!(
                    "timed out waiting for Firefox interaction count >= {min_count}; last telemetry {telemetry}; key_debug {debug}"
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn browser_key_debug(&self) -> Result<serde_json::Value> {
        self.execute(
            r#"
return {
  active: document.activeElement && (document.activeElement.id || document.activeElement.tagName) || null,
  events: window.__boonWebdriverKeyEvents || []
};
"#,
            vec![],
        )
    }

    fn wait_for_telemetry(&self, timeout: Duration) -> Result<serde_json::Value> {
        let start = Instant::now();
        loop {
            let value = self.execute("return window.__boonPlyLastTelemetry || null;", vec![])?;
            if !value.is_null() {
                if value
                    .get("canvas_nonblank")
                    .and_then(|value| value.as_bool())
                    != Some(true)
                {
                    bail!("Firefox telemetry did not prove nonblank canvas: {value}");
                }
                return Ok(value);
            }
            if start.elapsed() > timeout {
                bail!("timed out waiting for Firefox Ply telemetry");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn screenshot(&self, path: &Path) -> Result<()> {
        let response = self.request("GET", "/screenshot", None)?;
        let encoded = response
            .get("value")
            .and_then(|value| value.as_str())
            .context("Firefox screenshot response missing value")?;
        let bytes = BASE64_STANDARD.decode(encoded)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
        Ok(())
    }

    fn execute(&self, script: &str, args: Vec<serde_json::Value>) -> Result<serde_json::Value> {
        let response = self.request(
            "POST",
            "/execute/sync",
            Some(&json!({ "script": script, "args": args })),
        )?;
        Ok(response
            .get("value")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    fn request(
        &self,
        method: &str,
        endpoint: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        webdriver_request(
            self.port,
            method,
            &format!("/session/{}{}", self.session_id, endpoint),
            body,
        )
    }

    fn shutdown(mut self) -> Result<()> {
        let _ = self.request("DELETE", "", None);
        let _ = self.child.kill();
        let _ = self.child.wait();
        Ok(())
    }
}

fn pick_free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

fn wait_for_webdriver_status(port: u16, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        if webdriver_request(port, "GET", "/status", None).is_ok() {
            return Ok(());
        }
        if start.elapsed() > timeout {
            bail!("timed out waiting for geckodriver port {port}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn webdriver_request(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value> {
    let body_text = body
        .map(serde_json::to_string)
        .transpose()?
        .unwrap_or_default();
    let url = format!("http://127.0.0.1:{port}{path}");
    let mut command = Command::new("curl");
    command
        .arg("-sS")
        .arg("--fail-with-body")
        .arg("-X")
        .arg(method)
        .arg("-H")
        .arg("Content-Type: application/json");
    if body.is_some() {
        command.arg("--data-binary").arg(body_text);
    }
    command.arg(&url);
    let output = command
        .output()
        .with_context(|| format!("failed to run curl for WebDriver {method} {path}"))?;
    if !output.status.success() {
        bail!(
            "WebDriver request {method} {path} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let body = String::from_utf8_lossy(&output.stdout);
    if body.trim().is_empty() {
        return Ok(json!({}));
    }
    Ok(serde_json::from_str(body.trim())?)
}

fn retry_webdriver_request(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
    timeout: Duration,
) -> Result<serde_json::Value> {
    let start = Instant::now();
    loop {
        match webdriver_request(port, method, path, body) {
            Ok(value) => return Ok(value),
            Err(error) => {
                if start.elapsed() > timeout {
                    return Err(error);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn write_screenshot_sidecar(
    ctx: &HumanContext,
    target: &str,
    example: &str,
    step: &str,
    path: &Path,
    capture_source: &str,
) -> Result<()> {
    let metadata = screenshot_metadata(ctx, target, example, step, path, capture_source)?;
    fs::write(sidecar_path(path), serde_json::to_vec_pretty(&metadata)?)?;
    Ok(())
}

fn screenshot_metadata(
    ctx: &HumanContext,
    target: &str,
    example: &str,
    step: &str,
    path: &Path,
    capture_source: &str,
) -> Result<serde_json::Value> {
    let stats = image_stats(path)?;
    Ok(json!({
        "target": target,
        "example": example,
        "step": step,
        "path": path.display().to_string(),
        "source_hash": &ctx.source_hash,
        "control_manifest_hash": &ctx.control_manifest_hash,
        "deterministic_ply_report_sha256": &ctx.deterministic_ply_report_sha256,
        "matrix_run_id": &ctx.matrix_run_id,
        "capture_source": capture_source,
        "git_commit": &ctx.git_commit,
        "git_dirty": ctx.git_dirty,
        "git_status_short": &ctx.git_status_short,
        "command_line": &ctx.command_line,
        "timestamp_unix_ms": SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
        "process_id": ctx.process_id,
        "browser_executable": if target == "browser" { Some(ctx.firefox_executable.clone()) } else { None },
        "browser_version": if target == "browser" { Some(ctx.firefox_version.clone()) } else { None },
        "webdriver": if target == "browser" { Some(ctx.geckodriver_version.clone()) } else { None },
        "sha256": hash_file(path)?,
        "dimensions": {
            "width": stats.width,
            "height": stats.height,
        },
        "expected_dimensions": expected_dimensions_for_target(target)
            .into_iter()
            .map(|(width, height)| json!({"width": width, "height": height}))
            .collect::<Vec<_>>(),
        "byte_size": stats.byte_size,
        "statistics": {
            "unique_colors_sampled": stats.unique_colors_sampled,
            "nontransparent_pixels": stats.nontransparent_pixels,
            "average_luma_hash": stats.average_luma_hash,
        },
    }))
}

fn validate_screenshot_with_sidecar(
    ctx: &HumanContext,
    target: &str,
    example: &str,
    step: &str,
    path: &Path,
) -> Result<serde_json::Value> {
    if !path.exists() {
        bail!("missing screenshot {}", path.display());
    }
    let sidecar_path = sidecar_path(path);
    let sidecar = read_playground_artifact(&sidecar_path)
        .with_context(|| format!("missing sidecar {}", sidecar_path.display()))?;
    let stats = validate_screenshot_file(path, target)?;
    if sidecar.get("target").and_then(|value| value.as_str()) != Some(target)
        || sidecar.get("example").and_then(|value| value.as_str()) != Some(example)
        || sidecar.get("step").and_then(|value| value.as_str()) != Some(step)
    {
        bail!("screenshot sidecar target/example/step mismatch: {sidecar}");
    }
    validate_common_fresh_fields(ctx, &sidecar)?;
    let current_hash = hash_file(path)?;
    if sidecar.get("sha256").and_then(|value| value.as_str()) != Some(current_hash.as_str()) {
        bail!(
            "screenshot hash does not match sidecar for {}",
            path.display()
        );
    }
    if sidecar
        .pointer("/dimensions/width")
        .and_then(|value| value.as_u64())
        != Some(u64::from(stats.width))
        || sidecar
            .pointer("/dimensions/height")
            .and_then(|value| value.as_u64())
            != Some(u64::from(stats.height))
    {
        bail!(
            "screenshot dimensions do not match sidecar for {}",
            path.display()
        );
    }
    if sidecar.get("byte_size").and_then(|value| value.as_u64()) != Some(stats.byte_size) {
        bail!(
            "screenshot byte size does not match sidecar for {}",
            path.display()
        );
    }
    if sidecar
        .pointer("/statistics/average_luma_hash")
        .and_then(|value| value.as_str())
        != Some(stats.average_luma_hash.as_str())
    {
        bail!(
            "screenshot perceptual hash does not match sidecar for {}",
            path.display()
        );
    }
    Ok(json!({
        "target": target,
        "example": example,
        "step": step,
        "path": path,
        "sidecar": sidecar_path,
        "sha256": current_hash,
        "byte_size": stats.byte_size,
        "dimensions": {"width": stats.width, "height": stats.height},
        "average_luma_hash": stats.average_luma_hash,
    }))
}

#[derive(Clone)]
struct ImageStats {
    width: u32,
    height: u32,
    byte_size: u64,
    unique_colors_sampled: usize,
    nontransparent_pixels: u64,
    average_luma_hash: String,
}

fn validate_screenshot_file(path: &Path, target: &str) -> Result<ImageStats> {
    let stats = image_stats(path)?;
    if stats.byte_size < minimum_screenshot_bytes(target) {
        bail!("screenshot too small: {}", path.display());
    }
    let expected = expected_dimensions_for_target(target);
    if !expected
        .iter()
        .any(|(width, height)| *width == stats.width && *height == stats.height)
    {
        bail!(
            "screenshot dimensions {}x{} do not match expected {:?} for {target}: {}",
            stats.width,
            stats.height,
            expected,
            path.display()
        );
    }
    if stats.nontransparent_pixels == 0 || stats.unique_colors_sampled <= 16 {
        bail!(
            "screenshot appears blank or low-color placeholder: {}",
            path.display()
        );
    }
    Ok(stats)
}

fn image_stats(path: &Path) -> Result<ImageStats> {
    let metadata = fs::metadata(path)?;
    let image = image::io::Reader::open(path)?
        .with_guessed_format()?
        .decode()?
        .to_rgba8();
    let mut colors = BTreeSet::new();
    let mut non_transparent = 0_u64;
    for pixel in image.pixels() {
        if pixel.0[3] > 0 {
            non_transparent += 1;
        }
        colors.insert(pixel.0);
        if colors.len() > 512 {
            break;
        }
    }
    Ok(ImageStats {
        width: image.width(),
        height: image.height(),
        byte_size: metadata.len(),
        unique_colors_sampled: colors.len(),
        nontransparent_pixels: non_transparent,
        average_luma_hash: average_luma_hash(&image),
    })
}

fn minimum_screenshot_bytes(target: &str) -> u64 {
    match target {
        "terminal" => 1_000,
        "native" | "browser" => 5_000,
        _ => 1_000,
    }
}

fn expected_dimensions_for_target(target: &str) -> Vec<(u32, u32)> {
    match target {
        "terminal" => vec![(960, 560)],
        "native" => vec![
            (EXPECTED_NATIVE_WIDTH, EXPECTED_NATIVE_HEIGHT),
            (1912, 1948),
            (3828, 1948),
        ],
        "browser" => vec![
            (EXPECTED_NATIVE_WIDTH, EXPECTED_NATIVE_HEIGHT),
            (EXPECTED_NATIVE_WIDTH, 714),
            (EXPECTED_NATIVE_WIDTH, 634),
        ],
        _ => vec![(EXPECTED_NATIVE_WIDTH, EXPECTED_NATIVE_HEIGHT)],
    }
}

fn average_luma_hash(image: &image::RgbaImage) -> String {
    let mut samples = [0_u16; 64];
    let mut total = 0_u32;
    for row in 0..8_u32 {
        for col in 0..8_u32 {
            let x = ((col * image.width()) / 8).min(image.width().saturating_sub(1));
            let y = ((row * image.height()) / 8).min(image.height().saturating_sub(1));
            let pixel = image.get_pixel(x, y).0;
            let luma = ((u32::from(pixel[0]) * 299
                + u32::from(pixel[1]) * 587
                + u32::from(pixel[2]) * 114)
                / 1000) as u16;
            let index = (row * 8 + col) as usize;
            samples[index] = luma;
            total += u32::from(luma);
        }
    }
    let average = total / 64;
    let mut bits = 0_u64;
    for (index, sample) in samples.iter().enumerate() {
        if u32::from(*sample) >= average {
            bits |= 1_u64 << index;
        }
    }
    format!("{bits:016x}")
}

fn sidecar_path(path: &Path) -> PathBuf {
    path.with_extension("png.json")
}

fn screenshot_path(ctx: &HumanContext, target: &str, example: &str, step: &str) -> PathBuf {
    match target {
        "terminal" => ctx
            .dir
            .join(target)
            .join(example)
            .join(format!("screen-{step}.png")),
        _ => ctx
            .dir
            .join(target)
            .join(example)
            .join(format!("{step}.png")),
    }
}

fn required_screenshots(manifest: &ControlManifest, example: &str) -> Vec<String> {
    manifest.examples[example]
        .actions
        .iter()
        .filter(|action| action.kind == "screenshot")
        .filter_map(|action| action.name.clone())
        .collect()
}

fn collect_example_pngs(dir: &Path) -> Result<Vec<String>> {
    let mut paths = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("png"))
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn duplicate_placeholder_failures(
    hashes_by_target: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
) -> Vec<serde_json::Value> {
    let mut failures = Vec::new();
    for (target, by_hash) in hashes_by_target {
        for (hash, uses) in by_hash {
            if uses.len() > 1 {
                failures.push(json!({
                    "target": target,
                    "hash": hash,
                    "uses": uses,
                    "error": "duplicate screenshot hash across required captures",
                }));
            }
        }
    }
    failures
}

fn screenshot_pair_failures(
    ctx: &HumanContext,
    manifest: &ControlManifest,
) -> Result<Vec<serde_json::Value>> {
    let mut failures = Vec::new();
    for target in TARGETS {
        for example in boon_dd::REQUIRED_EXAMPLES {
            let required = required_screenshots(manifest, example);
            if !required.iter().any(|step| step == "selected")
                || !required.iter().any(|step| step == "after-action")
            {
                continue;
            }
            let selected = screenshot_path(ctx, target, example, "selected");
            let after = screenshot_path(ctx, target, example, "after-action");
            match image_difference_score(&selected, &after) {
                Ok(score) if score.changed_pixels >= 20 && score.changed_ratio >= 0.00001 => {}
                Ok(score) => failures.push(json!({
                    "target": target,
                    "example": example,
                    "selected": selected,
                    "after_action": after,
                    "changed_pixels": score.changed_pixels,
                    "changed_ratio": score.changed_ratio,
                    "error": "selected and after-action screenshots are too similar",
                })),
                Err(error) => failures.push(json!({
                    "target": target,
                    "example": example,
                    "selected": selected,
                    "after_action": after,
                    "error": format!("failed to compare selected/after-action screenshots: {error:#}"),
                })),
            }
        }
    }
    Ok(failures)
}

struct ImageDifference {
    changed_pixels: u64,
    changed_ratio: f64,
}

fn image_difference_score(left: &Path, right: &Path) -> Result<ImageDifference> {
    let left = image::io::Reader::open(left)?
        .with_guessed_format()?
        .decode()?
        .to_rgba8();
    let right = image::io::Reader::open(right)?
        .with_guessed_format()?
        .decode()?
        .to_rgba8();
    if left.dimensions() != right.dimensions() {
        return Ok(ImageDifference {
            changed_pixels: u64::from(right.width()) * u64::from(right.height()),
            changed_ratio: 1.0,
        });
    }
    let mut changed = 0_u64;
    for (a, b) in left.pixels().zip(right.pixels()) {
        if a.0 != b.0 {
            changed += 1;
        }
    }
    let total = u64::from(left.width()) * u64::from(left.height());
    Ok(ImageDifference {
        changed_pixels: changed,
        changed_ratio: changed as f64 / total.max(1) as f64,
    })
}

fn screenshot_negative_tests(ctx: &HumanContext) -> Result<serde_json::Value> {
    let dir = ctx.dir.join("negative-screenshot-fixtures");
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    let tiny = dir.join("tiny.png");
    ImageBuffer::from_pixel(1, 1, Rgba([255_u8, 255, 255, 255])).save(&tiny)?;
    let tiny_rejected = validate_screenshot_file(&tiny, "terminal").is_err();

    let blank = dir.join("blank.png");
    ImageBuffer::from_pixel(400, 300, Rgba([0_u8, 0, 0, 255])).save(&blank)?;
    let blank_rejected = validate_screenshot_file(&blank, "terminal").is_err();

    let wrong_dimensions = dir.join("wrong-dimensions.png");
    ImageBuffer::from_pixel(400, 300, Rgba([40_u8, 120, 220, 255])).save(&wrong_dimensions)?;
    let wrong_dimensions_rejected = validate_screenshot_file(&wrong_dimensions, "native").is_err();

    let missing = dir.join("missing.png");
    let missing_rejected =
        validate_screenshot_with_sidecar(ctx, "terminal", "counter", "selected", &missing).is_err();

    let stale = dir.join("stale.png");
    write_terminal_png(&["stale source hash rejection".to_owned()], &stale)?;
    write_screenshot_sidecar(ctx, "terminal", "counter", "selected", &stale, "negative")?;
    let mut sidecar = read_playground_artifact(&sidecar_path(&stale))?;
    sidecar["source_hash"] = serde_json::Value::String("stale".to_owned());
    fs::write(sidecar_path(&stale), serde_json::to_vec_pretty(&sidecar)?)?;
    let stale_rejected =
        validate_screenshot_with_sidecar(ctx, "terminal", "counter", "selected", &stale).is_err();

    Ok(json!({
        "tiny_png_rejected": tiny_rejected,
        "blank_png_rejected": blank_rejected,
        "wrong_dimensions_rejected": wrong_dimensions_rejected,
        "missing_png_rejected": missing_rejected,
        "stale_sidecar_rejected": stale_rejected,
    }))
}

fn validate_common_fresh_fields(ctx: &HumanContext, value: &serde_json::Value) -> Result<()> {
    if value.get("source_hash").and_then(|value| value.as_str()) != Some(ctx.source_hash.as_str()) {
        bail!("source_hash mismatch");
    }
    if value
        .get("control_manifest_hash")
        .and_then(|value| value.as_str())
        != Some(ctx.control_manifest_hash.as_str())
    {
        bail!("control_manifest_hash mismatch");
    }
    if value
        .get("deterministic_ply_report_sha256")
        .and_then(|value| value.as_str())
        != Some(ctx.deterministic_ply_report_sha256.as_str())
    {
        bail!("deterministic_ply_report_sha256 mismatch");
    }
    if value.get("matrix_run_id").and_then(|value| value.as_str())
        != Some(ctx.matrix_run_id.as_str())
    {
        bail!("matrix_run_id mismatch");
    }
    Ok(())
}

fn validate_all_sidecars_current(ctx: &HumanContext) -> Result<()> {
    for target in TARGETS {
        for example in boon_dd::REQUIRED_EXAMPLES {
            let dir = ctx.dir.join(target).join(example);
            if !dir.exists() {
                bail!("missing sidecar directory {}", dir.display());
            }
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("json")
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("")
                        .ends_with(".png.json")
                {
                    let value = read_playground_artifact(&path)?;
                    validate_common_fresh_fields(ctx, &value)
                        .with_context(|| format!("stale sidecar {}", path.display()))?;
                }
            }
        }
    }
    Ok(())
}

fn stale_human_report_rejected(ctx: &HumanContext) -> Result<bool> {
    let mut value = json!({
        "source_hash": ctx.source_hash,
        "control_manifest_hash": ctx.control_manifest_hash,
        "deterministic_ply_report_sha256": ctx.deterministic_ply_report_sha256,
        "matrix_run_id": ctx.matrix_run_id,
    });
    value["source_hash"] = serde_json::Value::String("stale".to_owned());
    Ok(validate_common_fresh_fields(ctx, &value).is_err())
}

fn stale_sidecar_rejected(ctx: &HumanContext) -> Result<bool> {
    let dir = ctx.dir.join("fresh-negative-sidecar");
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    let path = dir.join("stale.png");
    write_terminal_png(&["fresh sidecar stale rejection".to_owned()], &path)?;
    write_screenshot_sidecar(ctx, "terminal", "counter", "selected", &path, "negative")?;
    let sidecar = sidecar_path(&path);
    let mut value = read_playground_artifact(&sidecar)?;
    value["source_hash"] = serde_json::Value::String("stale".to_owned());
    fs::write(&sidecar, serde_json::to_vec_pretty(&value)?)?;
    Ok(validate_screenshot_with_sidecar(ctx, "terminal", "counter", "selected", &path).is_err())
}

fn hash_human_sources(root: &Path) -> Result<String> {
    let mut paths = Vec::new();
    for relative in [
        "BOON_DD_PLY_RENDERER_PLAN.md",
        "BOON_DD_PLY_HUMAN_SURFACE_VERIFICATION_PLAN.md",
        "Cargo.lock",
        "Cargo.toml",
        "xtask/src",
        "xtask/Cargo.toml",
        "crates/boon_backend_ply",
        "crates/boon_backend_ratatui",
        "docs/ply-human-surfaces/control-manifest.toml",
        "examples",
        "generated",
    ] {
        let path = root.join(relative);
        if path.exists() {
            collect_hash_paths(&path, &mut paths)?;
        }
    }
    paths.sort();
    let mut hasher = Sha256::new();
    for path in paths {
        let relative = path.strip_prefix(root).unwrap_or(&path);
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(&path)?);
        hasher.update([0]);
    }
    Ok(hex_digest(hasher.finalize().as_slice()))
}

fn collect_hash_paths(path: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            if name == "target" || name == ".git" || name == "build" {
                continue;
            }
            collect_hash_paths(&path, paths)?;
        }
        return Ok(());
    }
    if path.is_file() {
        let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        if matches!(ext, "rs" | "toml" | "md" | "bn" | "json" | "txt") {
            paths.push(path.to_path_buf());
        }
    }
    Ok(())
}

fn hash_json_map(values: &serde_json::Map<String, serde_json::Value>) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(&serde_json::Value::Object(
        values.clone(),
    ))?);
    Ok(hex_digest(hasher.finalize().as_slice()))
}

fn validate_human_ai_report(
    ctx: &HumanContext,
    current_commit: &str,
    report: &serde_json::Value,
    prompt_hashes: &BTreeMap<String, String>,
    prompt_slugs: &BTreeMap<String, String>,
) -> Result<()> {
    for field in [
        "reviewer",
        "model",
        "git_commit",
        "source_hash",
        "control_manifest_hash",
        "deterministic_ply_report_sha256",
        "prompt_file",
        "prompt_sha256",
        "verdict",
    ] {
        if report
            .get(field)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .is_empty()
        {
            bail!("missing string field {field}");
        }
    }
    if report.get("git_commit").and_then(|value| value.as_str()) != Some(current_commit) {
        bail!("git_commit mismatch");
    }
    if report.get("source_hash").and_then(|value| value.as_str()) != Some(ctx.source_hash.as_str())
    {
        bail!("source_hash mismatch");
    }
    if report
        .get("control_manifest_hash")
        .and_then(|value| value.as_str())
        != Some(ctx.control_manifest_hash.as_str())
    {
        bail!("control_manifest_hash mismatch");
    }
    if report
        .get("deterministic_ply_report_sha256")
        .and_then(|value| value.as_str())
        != Some(ctx.deterministic_ply_report_sha256.as_str())
    {
        bail!("deterministic_ply_report_sha256 mismatch");
    }
    let prompt_file = report
        .get("prompt_file")
        .and_then(|value| value.as_str())
        .unwrap();
    let expected = prompt_hashes
        .get(prompt_file)
        .with_context(|| format!("unknown prompt file {prompt_file}"))?;
    if report.get("prompt_sha256").and_then(|value| value.as_str()) != Some(expected.as_str()) {
        bail!("prompt hash mismatch");
    }
    if !prompt_slugs.contains_key(prompt_file) {
        bail!("prompt file missing from manifest");
    }
    if !matches!(
        report.get("verdict").and_then(|value| value.as_str()),
        Some("pass") | Some("pass_with_risks")
    ) {
        bail!("AI review verdict is not passing");
    }
    for field in [
        "commands_run",
        "files_examined",
        "artifacts_examined",
        "screenshots_examined",
        "findings",
    ] {
        if report
            .get(field)
            .and_then(|value| value.as_array())
            .map(|values| values.is_empty())
            != Some(false)
        {
            bail!("report field {field} must be a nonempty array");
        }
    }
    Ok(())
}
