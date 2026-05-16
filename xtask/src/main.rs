use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod ply_human_surfaces;

const WASM_BINDGEN_VERSION: &str = "0.2.120";
const PLY_ENGINE_VERSION: &str = "1.1.1";
const PLYX_VERSION: &str = "0.2.2";
const COSMIC_WORKSPACE: &str = "boon-dd";

#[derive(Debug, Serialize)]
struct GateReport {
    name: String,
    command: String,
    status: String,
    duration_ms: u128,
    artifacts: Vec<String>,
    details: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct VerifyReport {
    success: bool,
    gates: Vec<GateReport>,
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        bail!(
            "usage: cargo xtask <bootstrap|run|test|verify-deps|verify-wasm-dd|verify-render-deps|verify-playgrounds|verify-ply-renderer|verify> ..."
        );
    }

    match args.remove(0).as_str() {
        "bootstrap" => bootstrap(&args),
        "run" => run_example(&args),
        "test" => test_target(&args),
        "verify-deps" => verify_deps(&args).map(|_| ()),
        "verify-wasm-dd" => verify_wasm_dd(&args).map(|_| ()),
        "verify-render-deps" => verify_render_deps(&args).map(|_| ()),
        "verify-playgrounds" => verify_playgrounds(&args).map(|_| ()),
        "verify-ply-renderer" => verify_ply_renderer(&args),
        "verify-ply-headless" => verify_ply_headless(&args).map(|_| ()),
        "verify-ply-native" => verify_ply_native(&args).map(|_| ()),
        "verify-ply-browser" => verify_ply_browser(&args).map(|_| ()),
        "verify-ply-no-old-renderers" => verify_ply_no_old_renderers(&args).map(|_| ()),
        "verify-ply-fresh-artifacts" => verify_ply_fresh_artifacts(&args).map(|_| ()),
        "write-ply-ai-review-prompts" => write_ply_ai_review_prompts(&args).map(|_| ()),
        "verify-ply-ai-review-reports" => verify_ply_ai_review_reports(&args).map(|_| ()),
        "verify-ply-human-terminal" => {
            ply_human_surfaces::verify_ply_human_terminal(&args).map(|_| ())
        }
        "verify-ply-human-native" => ply_human_surfaces::verify_ply_human_native(&args).map(|_| ()),
        "verify-ply-human-browser" => {
            ply_human_surfaces::verify_ply_human_browser(&args).map(|_| ())
        }
        "verify-ply-human-screenshots" => {
            ply_human_surfaces::verify_ply_human_screenshots(&args).map(|_| ())
        }
        "verify-ply-human-fresh-artifacts" => {
            ply_human_surfaces::verify_ply_human_fresh_artifacts(&args).map(|_| ())
        }
        "write-ply-human-ai-review-prompts" => {
            ply_human_surfaces::write_ply_human_ai_review_prompts(&args).map(|_| ())
        }
        "verify-ply-human-ai-review-reports" => {
            ply_human_surfaces::verify_ply_human_ai_review_reports(&args).map(|_| ())
        }
        "verify-ply-human-surfaces" => ply_human_surfaces::verify_ply_human_surfaces(&args),
        "serve-ply-browser" => serve_ply_browser(&args),
        "verify" => verify(&args),
        other => bail!("unknown xtask command: {other}"),
    }
}

fn repo_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("failed to run git rev-parse")?;
    if !output.status.success() {
        bail!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(PathBuf::from(String::from_utf8(output.stdout)?.trim()))
}

fn artifacts_dir() -> Result<PathBuf> {
    let path = repo_root()?.join("target/boon-artifacts");
    fs::create_dir_all(&path)?;
    Ok(path)
}

fn run_capture(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {program} {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "{program} {} failed\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn run_status(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("failed to run {program} {}", args.join(" ")))?;
    if !status.success() {
        bail!("{program} {} failed with {status}", args.join(" "));
    }
    Ok(())
}

fn bootstrap(args: &[String]) -> Result<()> {
    let check = args.iter().any(|arg| arg == "--check");
    let rustc = run_capture("rustc", &["--version"])?;
    let cargo = run_capture("cargo", &["--version"])?;
    let targets = run_capture("rustup", &["target", "list", "--installed"])?;
    if !targets
        .lines()
        .any(|line| line.trim() == "wasm32-unknown-unknown")
    {
        bail!("missing rust target wasm32-unknown-unknown");
    }

    let helper = run_capture("bash", &["-lc", "command -v cosmic-background-launch"])
        .or_else(|_| run_capture("which", &["cosmic-background-launch"]))
        .context("missing cosmic-background-launch")?;
    let bus = run_capture(
        "bash",
        &[
            "-lc",
            "busctl --user list | rg 'com\\.system76\\.CosmicComp\\.BackgroundLaunch'",
        ],
    )
    .context("COSMIC BackgroundLaunch D-Bus service is not active")?;

    let wasm_bindgen = find_wasm_bindgen();
    if check {
        let wasm_bindgen = wasm_bindgen.context("missing repo-local or global wasm-bindgen")?;
        let version = run_capture(wasm_bindgen.to_str().unwrap(), &["--version"])?;
        if !version.contains(WASM_BINDGEN_VERSION) {
            bail!("wasm-bindgen version mismatch: expected {WASM_BINDGEN_VERSION}, got {version}");
        }
        let plyx = find_plyx().context("missing repo-local plyx; run cargo xtask bootstrap")?;
        let version = run_capture(plyx.to_str().unwrap(), &["--version"])?;
        if !version.contains(PLYX_VERSION) {
            bail!("plyx version mismatch: expected {PLYX_VERSION}, got {version}");
        }
    } else if wasm_bindgen.is_none() {
        install_wasm_bindgen()?;
    }
    if !check && find_plyx().is_none() {
        install_plyx()?;
    }

    let details = serde_json::json!({
        "rustc": rustc,
        "cargo": cargo,
        "targets": targets.lines().collect::<Vec<_>>(),
        "cosmic_background_launch": helper,
        "background_launch_service": bus,
        "plyx": find_plyx().map(|path| path.display().to_string()),
    });
    let path = artifacts_dir()?.join("bootstrap-check.json");
    fs::write(path, serde_json::to_vec_pretty(&details)?)?;
    Ok(())
}

fn verify_deps(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    run_status("cargo", &["tree", "-e", "features"])?;
    let metadata = run_capture("cargo", &["metadata", "--format-version", "1", "--no-deps"])?;
    let details = serde_json::json!({
        "rustc": run_capture("rustc", &["--version"])?,
        "cargo": run_capture("cargo", &["--version"])?,
        "metadata": serde_json::from_str::<serde_json::Value>(&metadata)?,
        "wasm_bindgen": find_wasm_bindgen().map(|p| p.display().to_string()),
        "firefox": run_capture("firefox", &["--version"]).unwrap_or_else(|e| format!("unavailable: {e}")),
    });
    let artifact = artifacts_dir()?.join("verify-deps.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(GateReport {
        name: "verify-deps".to_owned(),
        command: "cargo xtask verify-deps --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

fn verify_wasm_dd(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    sync_generated_artifacts()?;
    run_status("cargo", &["test", "-p", "boon_dd"])?;
    run_status(
        "cargo",
        &[
            "check",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "boon_dd",
        ],
    )?;
    run_status(
        "cargo",
        &[
            "build",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "boon_wasm_smoke",
            "--release",
        ],
    )?;

    let wasm_bindgen = find_wasm_bindgen().context("missing wasm-bindgen 0.2.120")?;
    let root = repo_root()?;
    let out_dir = root.join("target/boon-artifacts/wasm-bindgen");
    fs::create_dir_all(&out_dir)?;
    run_status(
        wasm_bindgen.to_str().unwrap(),
        &[
            "--target",
            "web",
            "--out-dir",
            out_dir.to_str().unwrap(),
            root.join("target/wasm32-unknown-unknown/release/boon_wasm_smoke.wasm")
                .to_str()
                .unwrap(),
        ],
    )?;

    let html = out_dir.join("index.html");
    fs::write(&html, smoke_html())?;
    let smoke_json = out_dir.join("smoke-result.json");
    run_firefox_smoke(&html, &smoke_json)?;

    let output = fs::read_to_string(&smoke_json)
        .with_context(|| format!("missing Firefox smoke output {}", smoke_json.display()))?;
    let smoke_value: serde_json::Value =
        serde_json::from_str(&output).context("Firefox smoke output is not JSON")?;
    require_wasm_smoke(&smoke_value)?;
    for example in boon_dd::REQUIRED_EXAMPLES {
        if !output.contains(example) {
            bail!("Firefox smoke output did not contain required example {example}: {output}");
        }
    }
    for required in ["CounterHold", "TodoMvcPhysical", "DocumentText"] {
        if !output.contains(required) {
            bail!(
                "Firefox smoke output did not contain expected monitor/render record {required}: {output}"
            );
        }
    }

    Ok(GateReport {
        name: "verify-wasm-dd".to_owned(),
        command: "cargo xtask verify-wasm-dd --required --browser firefox".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![
            out_dir.display().to_string(),
            smoke_json.display().to_string(),
        ],
        details: serde_json::json!({ "smoke_output": output }),
    })
}

fn require_wasm_smoke(smoke_value: &serde_json::Value) -> Result<()> {
    smoke_value
        .get("wasm_smoke")
        .context("Firefox smoke output missing wasm_smoke object")?;
    Ok(())
}

fn verify_render_deps(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    run_status("cargo", &["check", "-p", "boon_backend_ratatui"])?;
    run_status("cargo", &["check", "-p", "boon_backend_ply"])?;
    run_status(
        "cargo",
        &[
            "check",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "boon_backend_ply",
            "--bin",
            "web_playground",
        ],
    )?;
    let headless = verify_ply_headless(&["--format".to_owned(), "json".to_owned()])?;

    let artifact = artifacts_dir()?.join("verify-render-deps.json");
    let details = serde_json::json!({
        "ratatui": "0.30.0",
        "crossterm": "0.29.0",
        "ply-engine": PLY_ENGINE_VERSION,
        "plyx": PLYX_VERSION,
        "native_surface_mode": "macroquad+ply-engine native app",
        "browser_surface_mode": "ply-engine wasm app packaged with Ply loader",
        "headless_ply_evidence": headless.artifacts,
        "viewport": {"width": 1280, "height": 720, "dpr": 1.0},
    });
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(GateReport {
        name: "verify-render-deps".to_owned(),
        command: "cargo xtask verify-render-deps --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

fn verify_playgrounds(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    sync_generated_artifacts()?;
    run_status("cargo", &["check", "-p", "boon_backend_ratatui"])?;
    run_status("cargo", &["check", "-p", "boon_backend_ply"])?;
    run_status(
        "cargo",
        &[
            "build",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "boon_wasm_smoke",
            "--release",
        ],
    )?;

    let dir = artifacts_dir()?;
    let terminal_artifact = dir.join("terminal-playground.json");

    if terminal_artifact.exists() {
        fs::remove_file(&terminal_artifact)?;
    }
    run_status(
        "cargo",
        &[
            "run",
            "--quiet",
            "-p",
            "boon_backend_ratatui",
            "--bin",
            "terminal_playground",
            "--",
            "--smoke",
            terminal_artifact.to_str().unwrap(),
        ],
    )?;
    let terminal_details = read_playground_artifact(&terminal_artifact)?;
    require_playground_examples("terminal", &terminal_details)?;
    let nonblank_cells = terminal_details
        .pointer("/ratatui_test_backend/nonblank_cells")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    if nonblank_cells == 0 {
        bail!("terminal playground rendered no Ratatui cells: {terminal_details}");
    }

    let native_gate = verify_ply_native(&["--format".to_owned(), "json".to_owned()])?;
    let browser_gate = verify_ply_browser(&[
        "--browser".to_owned(),
        "firefox".to_owned(),
        "--format".to_owned(),
        "json".to_owned(),
    ])?;
    let native_details = native_gate.details.clone();
    let browser_details = browser_gate.details.clone();

    let artifact = dir.join("verify-playgrounds.json");
    let details = serde_json::json!({
        "terminal": terminal_details,
        "native": native_details,
        "browser": browser_details,
        "required_examples": boon_dd::REQUIRED_EXAMPLES,
    });
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(GateReport {
        name: "verify-playgrounds".to_owned(),
        command: "cargo xtask verify-playgrounds --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![
            artifact.display().to_string(),
            terminal_artifact.display().to_string(),
            native_gate.artifacts.join(","),
            browser_gate.artifacts.join(","),
        ],
        details,
    })
}

fn ply_artifacts_dir() -> Result<PathBuf> {
    let path = artifacts_dir()?.join("ply");
    fs::create_dir_all(&path)?;
    Ok(path)
}

fn verify_ply_renderer(_args: &[String]) -> Result<()> {
    let mut gates = Vec::new();
    gates.push(capture_gate(
        "verify-ply-headless",
        "cargo xtask verify-ply-headless --format json",
        || verify_ply_headless(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_gate(
        "verify-ply-native",
        "cargo xtask verify-ply-native --format json",
        || verify_ply_native(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_gate(
        "verify-ply-browser",
        "cargo xtask verify-ply-browser --browser firefox --format json",
        || {
            verify_ply_browser(&[
                "--browser".to_owned(),
                "firefox".to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
            ])
        },
    ));
    gates.push(capture_gate(
        "verify-ply-no-old-renderers",
        "cargo xtask verify-ply-no-old-renderers --format json",
        || verify_ply_no_old_renderers(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_gate(
        "verify-ply-fresh-artifacts",
        "cargo xtask verify-ply-fresh-artifacts --format json",
        || verify_ply_fresh_artifacts(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_gate(
        "write-ply-ai-review-prompts",
        "cargo xtask write-ply-ai-review-prompts --format json",
        || write_ply_ai_review_prompts(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_gate(
        "verify-ply-ai-review-reports",
        "cargo xtask verify-ply-ai-review-reports --format json",
        || verify_ply_ai_review_reports(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "verify-ply-human-surfaces",
        "cargo xtask verify-ply-human-surfaces --format json",
        || {
            ply_human_surfaces::verify_ply_human_surfaces(&[
                "--format".to_owned(),
                "json".to_owned(),
            ])?;
            read_playground_artifact(
                &artifacts_dir()?.join("ply-human-surfaces/verify-ply-human-surfaces.json"),
            )
        },
    ));
    let success = gates.iter().all(|gate| gate.status == "passed");
    let artifact = ply_artifacts_dir()?.join("verify-ply-renderer.json");
    fs::write(
        &artifact,
        serde_json::to_vec_pretty(&serde_json::json!({
            "success": success,
            "gates": gates,
        }))?,
    )?;
    if !success {
        bail!(
            "Ply renderer verification failed; see {}",
            artifact.display()
        );
    }
    Ok(())
}

fn verify_ply_headless(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    run_status("cargo", &["check", "-p", "boon_backend_ply"])?;
    let report = boon_backend_ply::evidence::headless_matrix_report();
    if !report.success {
        bail!("Ply headless evidence reported failure");
    }
    if report.examples.len() != boon_dd::REQUIRED_EXAMPLES.len() {
        bail!(
            "Ply headless evidence covered {} examples; expected {}",
            report.examples.len(),
            boon_dd::REQUIRED_EXAMPLES.len()
        );
    }
    if !report.interaction.selection_changed
        || !report.interaction.counter_state_changed
        || !report.interaction.counter_restored
        || !report.interaction.generic_state_changed_or_noop_documented
    {
        bail!(
            "Ply interaction evidence is incomplete: {:?}",
            report.interaction
        );
    }
    let artifact = ply_artifacts_dir()?.join("headless-matrix.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&report)?)?;
    Ok(GateReport {
        name: "verify-ply-headless".to_owned(),
        command: "cargo xtask verify-ply-headless --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details: serde_json::to_value(&report)?,
    })
}

fn verify_ply_native(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
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
    let artifact = ply_artifacts_dir()?.join("native-smoke.json");
    if artifact.exists() {
        fs::remove_file(&artifact)?;
    }
    launch_background_process(&[
        "cargo",
        "run",
        "--quiet",
        "-p",
        "boon_backend_ply",
        "--bin",
        "native_playground",
        "--",
        "--smoke",
        artifact.to_str().unwrap(),
    ])?;
    wait_for_json_artifact(&artifact, Duration::from_secs(60), "native Ply smoke")?;
    let details = read_playground_artifact(&artifact)?;
    require_ply_smoke("native", &details)?;
    if details
        .pointer("/renderer/legacy_native_window_stack_removed")
        .and_then(|value| value.as_bool())
        != Some(true)
    {
        bail!("native Ply smoke did not prove legacy native-window stack removal: {details}");
    }
    Ok(GateReport {
        name: "verify-ply-native".to_owned(),
        command: "cargo xtask verify-ply-native --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

fn verify_ply_browser(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    let build_dir = build_ply_web()?;
    let artifact = ply_artifacts_dir()?.join("browser-smoke.json");
    let html = build_dir.join("index.html");
    run_firefox_smoke(&html, &artifact)?;
    let details = read_playground_artifact(&artifact)?;
    require_ply_smoke("browser", &details)?;
    if details.get("firefox").and_then(|value| value.as_bool()) != Some(true) {
        bail!("browser Ply smoke did not prove Firefox execution: {details}");
    }
    if details
        .get("canvas_nonblank")
        .and_then(|value| value.as_bool())
        != Some(true)
    {
        bail!("browser Ply smoke did not prove nonblank canvas: {details}");
    }
    let interaction = details
        .get("interaction")
        .context("browser Ply smoke missing interaction")?;
    if interaction
        .get("selection_changed")
        .and_then(|value| value.as_bool())
        != Some(true)
        || interaction
            .get("counter_state_changed")
            .and_then(|value| value.as_bool())
            != Some(true)
    {
        bail!("browser Ply smoke did not prove interactions: {details}");
    }
    Ok(GateReport {
        name: "verify-ply-browser".to_owned(),
        command: "cargo xtask verify-ply-browser --browser firefox --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![
            artifact.display().to_string(),
            build_dir.display().to_string(),
        ],
        details,
    })
}

fn require_ply_smoke(target: &str, details: &serde_json::Value) -> Result<()> {
    if details.get("backend").and_then(|value| value.as_str()) != Some("ply-engine") {
        bail!("{target} smoke did not report Ply backend: {details}");
    }
    if details.get("target").and_then(|value| value.as_str()) != Some(target) {
        bail!("{target} smoke reported wrong target: {details}");
    }
    require_playground_examples(target, details)?;
    for pointer in [
        "/renderer/library",
        "/renderer/crate_version",
        "/frame/ply_render_commands",
    ] {
        if details.pointer(pointer).is_none() {
            bail!("{target} Ply smoke missing {pointer}: {details}");
        }
    }
    if details
        .pointer("/frame/ply_render_commands")
        .and_then(|value| value.as_u64())
        .unwrap_or(0)
        == 0
    {
        bail!("{target} Ply smoke has no render commands: {details}");
    }
    Ok(())
}

fn build_ply_web() -> Result<PathBuf> {
    let root = repo_root()?;
    let crate_dir = root.join("crates/boon_backend_ply");
    let plyx = find_plyx().context("missing repo-local plyx; run cargo xtask bootstrap")?;
    let status = Command::new(plyx)
        .current_dir(&crate_dir)
        .args(["web", "--auto"])
        .status()
        .context("failed to run plyx web")?;
    if !status.success() {
        bail!("plyx web failed with {status}");
    }
    let build_dir = crate_dir.join("build/web");
    for file in ["app.wasm", "index.html", "ply_bundle.js"] {
        let path = build_dir.join(file);
        if !path.exists() {
            bail!("Ply web build missing {}", path.display());
        }
    }
    Ok(build_dir)
}

fn verify_ply_no_old_renderers(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    let root = repo_root()?;
    let patterns = old_renderer_patterns();
    let mut hits = scan_old_renderer_paths(&root, &patterns)?;
    hits.retain(|hit| {
        hit.get("path")
            .and_then(|value| value.as_str())
            .map(|path| {
                !path.ends_with("BOON_DD_PLY_RENDERER_PLAN.md")
                    && !path.contains("target/")
                    && !path.ends_with("crates/boon_backend_ply/index.html")
            })
            .unwrap_or(true)
    });
    let negative_tests = run_old_renderer_negative_tests(&patterns)?;
    let artifact = ply_artifacts_dir()?.join("no-old-renderers.json");
    let details = serde_json::json!({
        "success": hits.is_empty(),
        "patterns": patterns.iter().map(|pattern| pattern.name).collect::<Vec<_>>(),
        "hits": hits,
        "negative_tests": negative_tests,
    });
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    if !details["hits"].as_array().unwrap().is_empty() {
        bail!("old renderer scan found hits; see {}", artifact.display());
    }
    Ok(GateReport {
        name: "verify-ply-no-old-renderers".to_owned(),
        command: "cargo xtask verify-ply-no-old-renderers --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

#[derive(Clone, Copy)]
struct OldRendererPattern {
    name: &'static str,
    needle: &'static str,
}

fn old_renderer_patterns() -> Vec<OldRendererPattern> {
    vec![
        OldRendererPattern {
            name: concat!("app", "_window"),
            needle: concat!("app", "_window"),
        },
        OldRendererPattern {
            name: concat!("boon_backend_", "wgpu"),
            needle: concat!("boon_backend_", "wgpu"),
        },
        OldRendererPattern {
            name: concat!("wgpu", "::"),
            needle: concat!("wgpu", "::"),
        },
        OldRendererPattern {
            name: concat!("create_render_", "pipeline"),
            needle: concat!("create_render_", "pipeline"),
        },
        OldRendererPattern {
            name: concat!("create_shader_", "module"),
            needle: concat!("create_shader_", "module"),
        },
        OldRendererPattern {
            name: concat!("Surface", "Target", "Unsafe"),
            needle: concat!("Surface", "Target", "Unsafe"),
        },
        OldRendererPattern {
            name: concat!("Device", "Ext"),
            needle: concat!("Device", "Ext"),
        },
        OldRendererPattern {
            name: "Scene",
            needle: concat!("struct ", "Scene"),
        },
        OldRendererPattern {
            name: "Primitive",
            needle: concat!("enum ", "Primitive"),
        },
        OldRendererPattern {
            name: concat!("scene", "_vertices"),
            needle: concat!("scene", "_vertices"),
        },
        OldRendererPattern {
            name: concat!("rect", "_vertices"),
            needle: concat!("rect", "_vertices"),
        },
        OldRendererPattern {
            name: concat!("text", "_vertices"),
            needle: concat!("text", "_vertices"),
        },
        OldRendererPattern {
            name: concat!("gly", "ph"),
            needle: concat!("gly", "ph", "("),
        },
        OldRendererPattern {
            name: concat!("browser-", "webgpu"),
            needle: concat!("browser-", "webgpu"),
        },
        OldRendererPattern {
            name: concat!("navigator", ".gpu"),
            needle: concat!("navigator", ".gpu"),
        },
        OldRendererPattern {
            name: concat!("request", "Adapter"),
            needle: concat!("request", "Adapter"),
        },
        OldRendererPattern {
            name: concat!("get", "Context", "(\"webgpu\")"),
            needle: concat!("get", "Context", "(\"webgpu\")"),
        },
        OldRendererPattern {
            name: concat!("begin", "Render", "Pass"),
            needle: concat!("begin", "Render", "Pass"),
        },
        OldRendererPattern {
            name: "documentCreateButton",
            needle: concat!("document.", "createElement", "(\"button\")"),
        },
        OldRendererPattern {
            name: concat!("fill", "Rect"),
            needle: concat!("fill", "Rect"),
        },
        OldRendererPattern {
            name: concat!("fill", "Text"),
            needle: concat!("fill", "Text"),
        },
        OldRendererPattern {
            name: concat!("wgpu-command-", "schema"),
            needle: concat!("wgpu-command-", "schema"),
        },
        OldRendererPattern {
            name: concat!("browser-", "webgpu-command-", "schema"),
            needle: concat!("browser-", "webgpu-command-", "schema"),
        },
        OldRendererPattern {
            name: concat!("ui", "_rect", "_wgsl"),
            needle: concat!("ui", "_rect", ".wgsl"),
        },
    ]
}

fn scan_old_renderer_paths(
    root: &Path,
    patterns: &[OldRendererPattern],
) -> Result<Vec<serde_json::Value>> {
    let mut hits = Vec::new();
    for relative in ["Cargo.toml", "xtask/src", "crates", "generated", "shaders"] {
        let path = root.join(relative);
        if path.exists() {
            scan_old_renderer_path(&path, patterns, &mut hits)?;
        }
    }
    Ok(hits)
}

fn scan_old_renderer_path(
    path: &Path,
    patterns: &[OldRendererPattern],
    hits: &mut Vec<serde_json::Value>,
) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if path
                .components()
                .any(|component| component.as_os_str() == "target")
            {
                continue;
            }
            scan_old_renderer_path(&path, patterns, hits)?;
        }
        return Ok(());
    }
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return Ok(());
    };
    if !matches!(ext, "rs" | "toml" | "json" | "wgsl" | "html" | "js") {
        return Ok(());
    }
    let text = fs::read_to_string(path)?;
    for (line_index, line) in text.lines().enumerate() {
        for pattern in patterns {
            if line.contains(pattern.needle) {
                hits.push(serde_json::json!({
                    "path": path.display().to_string(),
                    "line": line_index + 1,
                    "pattern": pattern.name,
                }));
            }
        }
    }
    Ok(())
}

fn run_old_renderer_negative_tests(
    patterns: &[OldRendererPattern],
) -> Result<Vec<serde_json::Value>> {
    let dir = ply_artifacts_dir()?.join("negative-old-renderer-fixtures");
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    let fixtures = [
        (
            "native.rs",
            concat!(
                "fn marker() { let _ = \"",
                "create_render_",
                "pipeline",
                "\"; }\n"
            ),
        ),
        (
            "browser.js",
            concat!("const marker = '", "request", "Adapter", "';\n"),
        ),
        (
            "generated.json",
            concat!(
                "{\"backend\":\"",
                "browser-",
                "webgpu-command-",
                "schema",
                "\"}\n"
            ),
        ),
    ];
    let mut rows = Vec::new();
    for (file, body) in fixtures {
        let path = dir.join(file);
        fs::write(&path, body)?;
        let mut hits = Vec::new();
        scan_old_renderer_path(&path, patterns, &mut hits)?;
        if hits.is_empty() {
            bail!(
                "old renderer negative fixture did not trigger scan: {}",
                path.display()
            );
        }
        rows.push(serde_json::json!({
            "fixture": path,
            "failed_as_expected": true,
            "hits": hits,
        }));
    }
    Ok(rows)
}

fn verify_ply_fresh_artifacts(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    let root = repo_root()?;
    let commit = run_capture("git", &["rev-parse", "HEAD"])?;
    let source_hash = hash_sources(&root)?;
    let required = [
        "headless-matrix.json",
        "native-smoke.json",
        "browser-smoke.json",
        "no-old-renderers.json",
    ];
    let mut artifact_hashes = serde_json::Map::new();
    let ply_dir = ply_artifacts_dir()?;
    for file in required {
        let path = ply_dir.join(file);
        if !path.exists() {
            bail!(
                "missing Ply artifact for freshness check: {}",
                path.display()
            );
        }
        artifact_hashes.insert(
            file.to_owned(),
            serde_json::Value::String(hash_file(&path)?),
        );
    }
    let deterministic_report_sha256 = hash_json_values(&artifact_hashes)?;
    let details = serde_json::json!({
        "git_commit": commit,
        "source_hash": source_hash,
        "deterministic_report_sha256": deterministic_report_sha256,
        "artifact_hashes": artifact_hashes,
        "negative_tests": {
            "stale_source_hash_rejected": true
        },
    });
    validate_fresh_artifact_report(&details, &source_hash)?;
    let mut stale_details = details.clone();
    stale_details["source_hash"] = serde_json::Value::String("stale-source-hash".to_owned());
    if validate_fresh_artifact_report(&stale_details, &source_hash).is_ok() {
        bail!("fresh artifact negative test failed to reject stale source hash");
    }
    let artifact = ply_dir.join("fresh-artifacts.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(GateReport {
        name: "verify-ply-fresh-artifacts".to_owned(),
        command: "cargo xtask verify-ply-fresh-artifacts --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

fn validate_fresh_artifact_report(
    report: &serde_json::Value,
    current_source_hash: &str,
) -> Result<()> {
    if report.get("source_hash").and_then(|value| value.as_str()) != Some(current_source_hash) {
        bail!("fresh artifact report source hash is stale");
    }
    let hashes = report
        .get("artifact_hashes")
        .and_then(|value| value.as_object())
        .context("fresh artifact report missing artifact hashes")?;
    for file in [
        "headless-matrix.json",
        "native-smoke.json",
        "browser-smoke.json",
        "no-old-renderers.json",
    ] {
        if hashes.get(file).and_then(|value| value.as_str()).is_none() {
            bail!("fresh artifact report missing hash for {file}");
        }
    }
    Ok(())
}

fn hash_sources(root: &Path) -> Result<String> {
    let mut paths = Vec::new();
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "BOON_DD_PLY_RENDERER_PLAN.md",
        "xtask/Cargo.toml",
        "xtask/src",
        "crates",
        "examples",
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
        paths.push(path.to_path_buf());
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(fs::read(path).with_context(|| format!("failed to hash {}", path.display()))?);
    Ok(hex_digest(hasher.finalize().as_slice()))
}

fn hash_json_values(values: &serde_json::Map<String, serde_json::Value>) -> Result<String> {
    let bytes = serde_json::to_vec(&serde_json::Value::Object(values.clone()))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex_digest(hasher.finalize().as_slice()))
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct PromptSpec {
    slug: &'static str,
    file_name: &'static str,
    body: String,
}

fn ply_review_prompt_specs() -> Vec<PromptSpec> {
    let native_stack = concat!("app", "_window");
    let old_gpu_crate = concat!("boon_backend_", "wgpu");
    let browser_gpu_label = concat!("browser-", "webgpu");
    let old_native_label = concat!("wgpu-command-", "schema");
    let old_browser_label = concat!("browser-", "webgpu-command-", "schema");
    vec![
        PromptSpec {
            slug: "architecture",
            file_name: "architecture.prompt.md",
            body: format!(
                "Review BOON_DD_PLY_RENDERER_PLAN.md and the current checkout for renderer architecture. Pass only if native and browser both use shared Rust Ply code in crates/boon_backend_ply, browser rendering is limited to Ply/macroquad loader glue, and no parallel renderer path remains. Record commands, files, deterministic artifacts, findings, and verdict as JSON."
            ),
        },
        PromptSpec {
            slug: "old-renderer-removal",
            file_name: "old-renderer-removal.prompt.md",
            body: format!(
                "Search for old renderer leftovers including {native_stack}, {old_gpu_crate}, direct render pipelines, shader modules, custom vector/glyph raster code, inline DOM-created app controls, {browser_gpu_label}, {old_native_label}, and {old_browser_label}. Pass only if repo-owned app rendering no longer uses them outside historical plan text or generated negative fixtures."
            ),
        },
        PromptSpec {
            slug: "native-browser-behavior",
            file_name: "native-browser-behavior.prompt.md",
            body: "Review native and Firefox browser Ply smoke artifacts. Pass only if both surfaces load every required Boon DD example, present a nonblank Ply UI, and prove at least one selection/state-changing interaction without relying on a custom JavaScript renderer.".to_owned(),
        },
        PromptSpec {
            slug: "fake-pass-verifier",
            file_name: "fake-pass-verifier.prompt.md",
            body: "Attack the verification design. Pass only if deterministic gates inspect real artifacts, reject missing or malformed smoke output, include negative old-renderer fixtures, and cannot pass from labels-only or placeholder JSON.".to_owned(),
        },
        PromptSpec {
            slug: "stale-artifact",
            file_name: "stale-artifact.prompt.md",
            body: "Review freshness guarantees. Pass only if source hashes, artifact hashes, git commit, and a stale-artifact negative test are present, current, and used by the final Ply renderer gate.".to_owned(),
        },
        PromptSpec {
            slug: "dependency-boundary",
            file_name: "dependency-boundary.prompt.md",
            body: format!(
                "Review Cargo metadata and crate boundaries. Pass only if active renderer crates depend on ply-engine rather than {native_stack}, {old_gpu_crate}, shader build crates, or repo-owned custom browser rendering backends."
            ),
        },
    ]
}

fn write_ply_ai_review_prompts(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    let root = repo_root()?;
    let prompt_dir = root.join("docs/prompts/renderer-ply");
    fs::create_dir_all(&prompt_dir)?;
    let commit = run_capture("git", &["rev-parse", "HEAD"])?;
    let source_hash = hash_sources(&root)?;
    let mut prompts = Vec::new();
    for spec in ply_review_prompt_specs() {
        let path = prompt_dir.join(spec.file_name);
        let body = format!(
            "# Ply Renderer Review: {}\n\n{}\n\nRequired report schema: reviewer, model, git_commit, deterministic_report_sha256, prompt_file, prompt_sha256, commands_run, files_examined, deterministic_artifacts_examined, findings, verdict.\n",
            spec.slug, spec.body
        );
        fs::write(&path, body)?;
        prompts.push(serde_json::json!({
            "slug": spec.slug,
            "path": path.display().to_string(),
            "sha256": hash_file(&path)?,
        }));
    }
    let details = serde_json::json!({
        "git_commit": commit,
        "source_hash": source_hash,
        "prompt_count": prompts.len(),
        "prompts": prompts,
        "reports_dir": ply_artifacts_dir()?.join("ai-reviews"),
    });
    let artifact = ply_artifacts_dir()?.join("ai-review-prompts.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(GateReport {
        name: "write-ply-ai-review-prompts".to_owned(),
        command: "cargo xtask write-ply-ai-review-prompts --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![
            artifact.display().to_string(),
            prompt_dir.display().to_string(),
        ],
        details,
    })
}

fn verify_ply_ai_review_reports(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    let ply_dir = ply_artifacts_dir()?;
    let prompts_path = ply_dir.join("ai-review-prompts.json");
    let fresh_path = ply_dir.join("fresh-artifacts.json");
    let prompts_manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&prompts_path)
            .with_context(|| format!("missing {}", prompts_path.display()))?,
    )?;
    let fresh: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&fresh_path)
            .with_context(|| format!("missing {}", fresh_path.display()))?,
    )?;
    let current_commit = run_capture("git", &["rev-parse", "HEAD"])?;
    let deterministic_hash = fresh
        .get("deterministic_report_sha256")
        .and_then(|value| value.as_str())
        .context("fresh artifact report missing deterministic_report_sha256")?;

    let mut prompt_hashes = std::collections::BTreeMap::new();
    let mut prompt_slugs = std::collections::BTreeMap::new();
    for prompt in prompts_manifest
        .get("prompts")
        .and_then(|value| value.as_array())
        .context("prompt manifest missing prompts")?
    {
        let path = prompt
            .get("path")
            .and_then(|value| value.as_str())
            .context("prompt missing path")?
            .to_owned();
        let hash = prompt
            .get("sha256")
            .and_then(|value| value.as_str())
            .context("prompt missing sha256")?
            .to_owned();
        let slug = prompt
            .get("slug")
            .and_then(|value| value.as_str())
            .context("prompt missing slug")?
            .to_owned();
        prompt_hashes.insert(path.clone(), hash);
        prompt_slugs.insert(path, slug);
    }

    let reports_dir = ply_dir.join("ai-reviews");
    fs::create_dir_all(&reports_dir)?;
    let mut report_paths = fs::read_dir(&reports_dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    report_paths.sort();
    if report_paths.len() < 2 {
        bail!(
            "expected at least two AI review reports in {}, found {}",
            reports_dir.display(),
            report_paths.len()
        );
    }

    let mut reports = Vec::new();
    let mut reviewers = std::collections::BTreeSet::new();
    let mut covered_slugs = std::collections::BTreeSet::new();
    for path in report_paths {
        let report: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("AI review report is not JSON: {}", path.display()))?;
        validate_ai_review_report(
            &report,
            &current_commit,
            deterministic_hash,
            &prompt_hashes,
            &prompt_slugs,
        )
        .with_context(|| format!("invalid AI review report {}", path.display()))?;
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
            covered_slugs.insert(slug.clone());
        }
        reports.push(serde_json::json!({
            "path": path,
            "report": report,
        }));
    }
    if reviewers.len() < 2 {
        bail!("AI review reports must have at least two distinct reviewers");
    }
    let removal_covered = covered_slugs.contains("old-renderer-removal")
        || covered_slugs.contains("dependency-boundary");
    let behavior_covered =
        covered_slugs.contains("architecture") || covered_slugs.contains("native-browser-behavior");
    if !removal_covered || !behavior_covered {
        bail!("AI reviews must cover both old-renderer removal and native/browser Ply behavior");
    }

    let details = serde_json::json!({
        "success": true,
        "git_commit": current_commit,
        "deterministic_report_sha256": deterministic_hash,
        "review_count": reports.len(),
        "reviewers": reviewers,
        "covered_prompt_slugs": covered_slugs,
        "reports": reports,
    });
    let artifact = ply_dir.join("ai-review-reports.json");
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(GateReport {
        name: "verify-ply-ai-review-reports".to_owned(),
        command: "cargo xtask verify-ply-ai-review-reports --format json".to_owned(),
        status: "passed".to_owned(),
        duration_ms: start.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

fn validate_ai_review_report(
    report: &serde_json::Value,
    current_commit: &str,
    deterministic_hash: &str,
    prompt_hashes: &std::collections::BTreeMap<String, String>,
    prompt_slugs: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    for field in [
        "reviewer",
        "model",
        "git_commit",
        "deterministic_report_sha256",
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
        bail!("report git_commit does not match current checkout");
    }
    if report
        .get("deterministic_report_sha256")
        .and_then(|value| value.as_str())
        != Some(deterministic_hash)
    {
        bail!("report deterministic hash does not match current fresh-artifacts report");
    }
    let prompt_file = report
        .get("prompt_file")
        .and_then(|value| value.as_str())
        .unwrap();
    let expected_prompt_hash = prompt_hashes
        .get(prompt_file)
        .with_context(|| format!("report references unknown prompt file {prompt_file}"))?;
    if report.get("prompt_sha256").and_then(|value| value.as_str())
        != Some(expected_prompt_hash.as_str())
    {
        bail!("report prompt hash does not match prompt manifest");
    }
    if !prompt_slugs.contains_key(prompt_file) {
        bail!("report prompt file is not in prompt manifest");
    }
    if report.get("verdict").and_then(|value| value.as_str()) != Some("pass") {
        bail!("AI review verdict is not pass");
    }
    for field in [
        "commands_run",
        "files_examined",
        "deterministic_artifacts_examined",
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

fn read_playground_artifact(path: &Path) -> Result<serde_json::Value> {
    serde_json::from_str(&fs::read_to_string(path)?)
        .with_context(|| format!("playground artifact is not JSON: {}", path.display()))
}

fn require_playground_examples(name: &str, details: &serde_json::Value) -> Result<()> {
    let count = details
        .get("example_count")
        .and_then(|value| value.as_u64())
        .context("playground artifact missing example_count")?;
    if count != boon_dd::REQUIRED_EXAMPLES.len() as u64 {
        bail!(
            "{name} playground loaded {count} examples; expected {}",
            boon_dd::REQUIRED_EXAMPLES.len()
        );
    }
    let loaded = details
        .get("loaded_examples")
        .and_then(|value| value.as_array())
        .context("playground artifact missing loaded_examples")?;
    for example in boon_dd::REQUIRED_EXAMPLES {
        if !loaded.iter().any(|value| value.as_str() == Some(example)) {
            bail!("{name} playground did not load required example {example}: {details}");
        }
    }
    Ok(())
}

fn wait_for_json_artifact(path: &Path, timeout: Duration, label: &str) -> Result<()> {
    let start = Instant::now();
    loop {
        if path.exists() {
            let text = fs::read_to_string(path).unwrap_or_default();
            if !text.is_empty() && serde_json::from_str::<serde_json::Value>(&text).is_ok() {
                return Ok(());
            }
        }
        if start.elapsed() > timeout {
            bail!(
                "{label} did not write parseable JSON {} within {:?}",
                path.display(),
                timeout
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn verify(args: &[String]) -> Result<()> {
    if args.first().map(String::as_str) != Some("all") {
        bail!("usage: cargo xtask verify all --format json");
    }
    let mut gates = Vec::new();

    gates.push(capture_simple_gate(
        "bootstrap",
        "cargo xtask bootstrap --check",
        || {
            bootstrap(&["--check".to_owned()])?;
            Ok(serde_json::json!({}))
        },
    ));
    gates.push(capture_gate(
        "verify-deps",
        "cargo xtask verify-deps --format json",
        || verify_deps(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_gate(
        "verify-wasm-dd",
        "cargo xtask verify-wasm-dd --required --browser firefox",
        || {
            verify_wasm_dd(&[
                "--required".to_owned(),
                "--browser".to_owned(),
                "firefox".to_owned(),
            ])
        },
    ));
    gates.push(capture_gate(
        "verify-render-deps",
        "cargo xtask verify-render-deps --format json",
        || verify_render_deps(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "verify-ply-renderer",
        "cargo xtask verify-ply-renderer --format json",
        || {
            verify_ply_renderer(&["--format".to_owned(), "json".to_owned()])?;
            let artifact = ply_artifacts_dir()?.join("verify-ply-renderer.json");
            serde_json::from_str(&fs::read_to_string(&artifact)?)
                .context("Ply renderer aggregate artifact is not JSON")
        },
    ));
    gates.push(capture_simple_gate(
        "verify-ply-human-surfaces",
        "cargo xtask verify-ply-human-surfaces --format json",
        || {
            let artifact =
                artifacts_dir()?.join("ply-human-surfaces/verify-ply-human-surfaces.json");
            let details =
                serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&artifact)?)?;
            if details.get("success").and_then(|value| value.as_bool()) != Some(true) {
                bail!("human-surface aggregate artifact is not successful");
            }
            Ok(details)
        },
    ));
    gates.push(capture_gate(
        "verify-playgrounds",
        "cargo xtask verify-playgrounds --format json",
        || verify_playgrounds(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "example-matrix",
        "cargo xtask verify all --format json",
        verify_example_matrix,
    ));
    gates.push(capture_simple_gate(
        "target-terminal",
        "cargo xtask test --target terminal",
        || {
            test_target(&["--target".to_owned(), "terminal".to_owned()])?;
            Ok(serde_json::json!({ "target": "terminal" }))
        },
    ));
    gates.push(capture_simple_gate(
        "target-native",
        "cargo xtask test --target native",
        || {
            test_target(&["--target".to_owned(), "native".to_owned()])?;
            Ok(serde_json::json!({
                "target": "native",
                "mode": "Ply native smoke plus shared render-command verification"
            }))
        },
    ));
    gates.push(capture_simple_gate(
        "target-browser",
        "cargo xtask test --target browser",
        || {
            test_target(&["--target".to_owned(), "browser".to_owned()])?;
            Ok(serde_json::json!({
                "target": "browser",
                "mode": "Ply browser bundle plus Firefox smoke artifact verification"
            }))
        },
    ));
    gates.push(capture_simple_gate(
        "plan-coverage",
        "cargo xtask verify all --format json",
        verify_plan_coverage,
    ));
    gates.push(capture_simple_gate(
        "generated-crates",
        "cargo test --manifest-path generated/<example>/Cargo.toml",
        verify_generated_crates,
    ));

    let success = gates.iter().all(|gate| gate.status == "passed");
    let failed_gates = gates
        .iter()
        .filter(|gate| gate.status != "passed")
        .map(|gate| gate.name.clone())
        .collect::<Vec<_>>();
    let report = VerifyReport { success, gates };
    let dir = artifacts_dir()?;
    let report_path = dir.join("verify-report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    let success_path = dir.join("success.json");
    let matrix_path = dir.join("example-matrix.json");
    let forbidden_scan = forbidden_pattern_scan()?;
    fs::write(
        &success_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "success": success,
            "failed_gates": failed_gates,
            "verify_report": report_path,
            "dependency_tool_versions": collect_dependency_tool_versions()?,
            "canonical_example_matrix_results": serde_json::from_str::<serde_json::Value>(
                &fs::read_to_string(&matrix_path).unwrap_or_else(|_| "{}".to_owned())
            )?,
            "forbidden_pattern_scan": forbidden_scan,
        }))?,
    )?;
    if !success {
        bail!("verification failed");
    }
    Ok(())
}

fn capture_gate<F>(name: &str, command: &str, f: F) -> GateReport
where
    F: FnOnce() -> Result<GateReport>,
{
    let start = Instant::now();
    match f() {
        Ok(mut gate) => {
            gate.name = name.to_owned();
            gate.command = command.to_owned();
            gate
        }
        Err(error) => GateReport {
            name: name.to_owned(),
            command: command.to_owned(),
            status: "failed".to_owned(),
            duration_ms: start.elapsed().as_millis(),
            artifacts: Vec::new(),
            details: serde_json::json!({ "error": format!("{error:#}") }),
        },
    }
}

fn capture_simple_gate<F>(name: &str, command: &str, f: F) -> GateReport
where
    F: FnOnce() -> Result<serde_json::Value>,
{
    let start = Instant::now();
    match f() {
        Ok(details) => GateReport {
            name: name.to_owned(),
            command: command.to_owned(),
            status: "passed".to_owned(),
            duration_ms: start.elapsed().as_millis(),
            artifacts: Vec::new(),
            details,
        },
        Err(error) => GateReport {
            name: name.to_owned(),
            command: command.to_owned(),
            status: "failed".to_owned(),
            duration_ms: start.elapsed().as_millis(),
            artifacts: Vec::new(),
            details: serde_json::json!({ "error": format!("{error:#}") }),
        },
    }
}

fn collect_dependency_tool_versions() -> Result<serde_json::Value> {
    Ok(serde_json::json!({
        "rustc": run_capture("rustc", &["--version"])?,
        "cargo": run_capture("cargo", &["--version"])?,
        "timely": "0.29.0",
        "differential-dataflow": "0.23.0",
        "wasm-bindgen-cli": find_wasm_bindgen()
            .and_then(|path| run_capture(path.to_str().unwrap(), &["--version"]).ok()),
        "firefox": run_capture("firefox", &["--version"]).unwrap_or_else(|error| format!("unavailable: {error}")),
        "cosmic-background-launch": run_capture("bash", &["-lc", "command -v cosmic-background-launch"]).unwrap_or_else(|error| format!("unavailable: {error}")),
    }))
}

fn forbidden_pattern_scan() -> Result<serde_json::Value> {
    let root = repo_root()?;
    let forbidden = [
        concat!("mark", "_dirty"),
        concat!("dirty", "_nodes"),
        concat!("recompute", "_dependents"),
        concat!("native graph", " worker"),
        concat!("browser-side custom", " scheduler"),
        concat!("graph_id", " =="),
        concat!("graph_id", ".contains"),
        concat!("Pong", "/"),
        concat!("A1", ": 1"),
    ];
    let mut hits = Vec::new();
    for base in ["crates", "xtask/src"] {
        let dir = root.join(base);
        if !dir.exists() {
            continue;
        }
        scan_forbidden_in_dir(&dir, &forbidden, &mut hits)?;
    }
    let artifact = artifacts_dir()?.join("forbidden-pattern-scan.json");
    let details = serde_json::json!({
        "patterns": forbidden,
        "hits": hits,
    });
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(details)
}

fn scan_forbidden_in_dir(
    dir: &Path,
    forbidden: &[&str],
    hits: &mut Vec<serde_json::Value>,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            scan_forbidden_in_dir(&path, forbidden, hits)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        for (line_index, line) in text.lines().enumerate() {
            for pattern in forbidden {
                if line.contains(pattern) {
                    hits.push(serde_json::json!({
                        "path": path.display().to_string(),
                        "line": line_index + 1,
                        "pattern": pattern,
                    }));
                }
            }
        }
    }
    Ok(())
}

fn verify_example_matrix() -> Result<serde_json::Value> {
    let root = repo_root()?;
    let required = [
        "counter",
        "counter_hold",
        "interval",
        "interval_hold",
        "latest",
        "when",
        "while",
        "then",
        "list_map_block",
        "list_map_external_dep",
        "list_object_state",
        "list_retain_count",
        "list_retain_reactive",
        "list_retain_remove",
        "shopping_list",
        "todo_mvc",
        "crud",
        "flight_booker",
        "temperature_converter",
        "pong",
        "cells",
        "todo_mvc_physical",
    ];
    let missing = required
        .iter()
        .filter_map(|example| {
            let scenario = root.join("examples").join(example).join("scenario.toml");
            (!scenario.exists()).then(|| scenario.display().to_string())
        })
        .collect::<Vec<_>>();
    let mut terminal_checked = Vec::new();
    let mut terminal_errors = Vec::new();
    for example in implemented_terminal_examples()? {
        match run_terminal_scenario(&example) {
            Ok(details) => terminal_checked.push(details),
            Err(error) => terminal_errors.push(serde_json::json!({
                "example": example,
                "error": format!("{error:#}"),
            })),
        }
    }

    let artifact = artifacts_dir()?.join("example-matrix.json");
    let details = serde_json::json!({
        "required_examples": required,
        "missing_scenarios": missing,
        "terminal_checked": terminal_checked,
        "terminal_errors": terminal_errors,
    });
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;

    if !details["missing_scenarios"].as_array().unwrap().is_empty() {
        bail!("example matrix is incomplete; see {}", artifact.display());
    }
    if !details["terminal_errors"].as_array().unwrap().is_empty() {
        bail!(
            "terminal scenario verification failed; see {}",
            artifact.display()
        );
    }
    Ok(details)
}

fn verify_plan_coverage() -> Result<serde_json::Value> {
    let root = repo_root()?;
    let required_crates = [
        "boon_syntax",
        "boon_hir",
        "boon_shape",
        "boon_host_schema",
        "boon_source",
        "boon_dd",
        "boon_runtime_host",
        "boon_compiler",
        "boon_codegen_rust",
        "boon_render_ir",
        "boon_backend_ratatui",
        "boon_backend_ply",
        "boon_examples",
        "boon_verify",
    ];
    let missing_crates = required_crates
        .iter()
        .filter_map(|krate| {
            let manifest = root.join("crates").join(krate).join("Cargo.toml");
            (!manifest.exists()).then(|| manifest.display().to_string())
        })
        .collect::<Vec<_>>();

    let mut required_paths = vec![
        "ARCHITECTURE.md",
        "target/boon-artifacts/wasm-bindgen/smoke-result.json",
        "target/boon-artifacts/ply/headless-matrix.json",
        "target/boon-artifacts/ply/native-smoke.json",
        "target/boon-artifacts/ply/browser-smoke.json",
        "target/boon-artifacts/ply/no-old-renderers.json",
        "target/boon-artifacts/ply/fresh-artifacts.json",
        "target/boon-artifacts/ply/ai-review-prompts.json",
        "target/boon-artifacts/ply/ai-review-reports.json",
        "BOON_DD_PLY_HUMAN_SURFACE_VERIFICATION_PLAN.md",
        "docs/ply-human-surfaces/control-manifest.toml",
        "target/boon-artifacts/ply-human-surfaces/matrix.json",
        "target/boon-artifacts/ply-human-surfaces/success.json",
        "target/boon-artifacts/ply-human-surfaces/screenshot-validation.json",
        "target/boon-artifacts/ply-human-surfaces/fresh-artifacts.json",
        "target/boon-artifacts/ply-human-surfaces/ai-review-prompts.json",
        "target/boon-artifacts/ply-human-surfaces/ai-review-reports.json",
        "docs/prompts/renderer-ply-human-surfaces/human-surface-coverage-review.prompt.md",
        "docs/prompts/renderer-ply-human-surfaces/screenshot-authenticity-review.prompt.md",
        "docs/prompts/renderer-ply-human-surfaces/target-control-review.prompt.md",
        "docs/prompts/renderer-ply-human-surfaces/stale-artifact-review.prompt.md",
        "docs/prompts/renderer-ply-human-surfaces/fake-pass-harness-review.prompt.md",
        "docs/prompts/renderer-ply-human-surfaces/example-behavior-review.prompt.md",
        "docs/prompts/renderer-ply/architecture.prompt.md",
        "docs/prompts/renderer-ply/old-renderer-removal.prompt.md",
        "docs/prompts/renderer-ply/native-browser-behavior.prompt.md",
        "docs/prompts/renderer-ply/fake-pass-verifier.prompt.md",
        "docs/prompts/renderer-ply/stale-artifact.prompt.md",
        "docs/prompts/renderer-ply/dependency-boundary.prompt.md",
    ];
    for example in boon_dd::REQUIRED_EXAMPLES {
        required_paths.push(Box::leak(
            format!("generated/{example}/graph_static.json").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/Cargo.toml").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/src/lib.rs").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/src/graph.rs").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/src/ids.rs").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/src/source_events.rs").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/src/shapes.rs").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/src/values.rs").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/src/render_bindings.rs").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/src/monitor_bindings.rs").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/src/persist_bindings.rs").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/monitor_snapshot.json").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/terminal_120x40.snapshot.txt").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/native_render_1280x720.json").into_boxed_str(),
        ));
        required_paths.push(Box::leak(
            format!("generated/{example}/browser_render_1280x720.json").into_boxed_str(),
        ));
    }
    let missing_paths = required_paths
        .iter()
        .filter_map(|path| {
            let path = root.join(path);
            (!path.exists()).then(|| path.display().to_string())
        })
        .collect::<Vec<_>>();
    let forbidden_scan = forbidden_pattern_scan()?;
    let forbidden_hits = forbidden_scan["hits"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let artifact = artifacts_dir()?.join("plan-coverage.json");
    let details = serde_json::json!({
        "required_crates": required_crates,
        "missing_crates": missing_crates,
        "missing_paths": missing_paths,
        "forbidden_pattern_scan": forbidden_scan,
    });
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;

    if !details["missing_crates"].as_array().unwrap().is_empty()
        || !details["missing_paths"].as_array().unwrap().is_empty()
        || !forbidden_hits.is_empty()
    {
        bail!("plan coverage is incomplete; see {}", artifact.display());
    }
    Ok(details)
}

fn verify_generated_crates() -> Result<serde_json::Value> {
    let root = repo_root()?;
    let mut checked = Vec::new();
    for example in boon_dd::REQUIRED_EXAMPLES {
        let manifest = root.join("generated").join(example).join("Cargo.toml");
        if !manifest.exists() {
            bail!("missing generated manifest {}", manifest.display());
        }
        let target_dir = artifacts_dir()?.join("generated-check").join(example);
        let status = Command::new("cargo")
            .env("CARGO_TARGET_DIR", &target_dir)
            .args([
                "test",
                "--quiet",
                "--manifest-path",
                manifest.to_str().unwrap(),
            ])
            .status()
            .with_context(|| format!("failed to run cargo test for generated crate {example}"))?;
        if !status.success() {
            bail!("generated crate {example} test failed: {status}");
        }
        checked.push(serde_json::json!({
            "example": example,
            "manifest": manifest,
            "target_dir": target_dir,
        }));
    }
    let artifact = artifacts_dir()?.join("generated-crates.json");
    let details = serde_json::json!({ "checked": checked });
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;
    Ok(details)
}

fn run_example(args: &[String]) -> Result<()> {
    let mut example = None;
    let mut target = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--example" => {
                index += 1;
                example = args.get(index).cloned();
            }
            "--target" => {
                index += 1;
                target = args.get(index).cloned();
            }
            other => bail!("unknown run argument: {other}"),
        }
        index += 1;
    }
    let example = example.context("missing --example")?;
    let target = target.context("missing --target")?;
    if matches!(target.as_str(), "native" | "browser") {
        require_background_launch_env(&target)?;
    } else if target != "terminal" {
        bail!("unknown target {target}");
    }
    match target.as_str() {
        "terminal" => {
            run_status(
                "cargo",
                &[
                    "run",
                    "--quiet",
                    "-p",
                    "boon_backend_ratatui",
                    "--bin",
                    "terminal_playground",
                    "--",
                    "--example",
                    &example,
                ],
            )?;
        }
        "native" => {
            run_status(
                "cargo",
                &[
                    "run",
                    "--quiet",
                    "-p",
                    "boon_backend_ply",
                    "--bin",
                    "native_playground",
                    "--",
                    "--example",
                    &example,
                ],
            )?;
        }
        "browser" => {
            serve_ply_browser(&[
                "--browser".to_owned(),
                "firefox".to_owned(),
                "--example".to_owned(),
                example,
            ])?;
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn test_target(args: &[String]) -> Result<()> {
    let mut target = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--target" => {
                index += 1;
                target = args.get(index).cloned();
            }
            other => bail!("unknown test argument: {other}"),
        }
        index += 1;
    }
    let target = target.context("missing --target")?;
    match target.as_str() {
        "terminal" | "native" | "browser" => {}
        _ => bail!("unknown test target: {target}"),
    }
    for example in implemented_terminal_examples()? {
        run_terminal_scenario(&example)?;
    }
    if target == "browser" {
        verify_ply_browser(&[
            "--browser".to_owned(),
            "firefox".to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ])?;
    } else if target == "native" {
        verify_ply_native(&["--format".to_owned(), "json".to_owned()])?;
    }
    Ok(())
}

fn implemented_terminal_examples() -> Result<Vec<String>> {
    let examples_dir = repo_root()?.join("examples");
    if !examples_dir.exists() {
        return Ok(Vec::new());
    }
    let mut examples = Vec::new();
    for entry in fs::read_dir(examples_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().join("scenario.toml").exists() {
                examples.push(name);
            }
        }
    }
    examples.sort();
    Ok(examples)
}

fn run_terminal_scenario(example: &str) -> Result<serde_json::Value> {
    let example_dir = repo_root()?.join("examples").join(example);
    let scenario = example_dir.join("scenario.toml");
    if !scenario.exists() {
        bail!("missing scenario {}", scenario.display());
    }
    let expected_path = example_dir.join("expected.render.json");
    let expected = fs::read_to_string(&expected_path)
        .with_context(|| format!("missing expected artifact {}", expected_path.display()))?;
    let actual = compiled_example_json(example)?;
    let expected_json: serde_json::Value = serde_json::from_str(&expected)
        .with_context(|| format!("invalid JSON {}", expected_path.display()))?;
    let actual_json: serde_json::Value = serde_json::from_str(&actual)?;
    if actual_json != expected_json {
        bail!(
            "terminal scenario {example} output mismatch\nexpected: {expected_json}\nactual: {actual_json}"
        );
    }
    let artifact_dir = write_generated_artifacts(example)?;
    Ok(serde_json::json!({
        "example": example,
        "expected": expected_path,
        "generated_artifacts": artifact_dir,
        "output": actual_json,
    }))
}

fn compiled_example_json(example: &str) -> Result<String> {
    let root = repo_root()?;
    let example_dir = root.join("examples").join(example);
    let source_path = example_dir.join("source.bn");
    let scenario_path = example_dir.join("scenario.toml");
    let source_text = fs::read_to_string(&source_path)
        .with_context(|| format!("missing source {}", source_path.display()))?;
    let scenario_text = fs::read_to_string(&scenario_path)
        .with_context(|| format!("missing scenario {}", scenario_path.display()))?;
    let scenario = boon_runtime_host::parse_scenario(&scenario_text);
    let source_path_string = format!("examples/{example}/source.bn");
    let output = boon_runtime_host::RuntimeHost
        .compile_and_run_step(&source_path_string, &source_text, &scenario)
        .with_context(|| format!("example {example} has no runnable scenario step"))?;
    serde_json::to_string(&output).context("failed to serialize compiled DD graph output")
}

fn write_generated_artifacts(example: &str) -> Result<String> {
    let root = repo_root()?;
    let example_dir = root.join("examples").join(example);
    let source_path = example_dir.join("source.bn");
    let scenario_path = example_dir.join("scenario.toml");
    let source_text = fs::read_to_string(&source_path)?;
    let scenario_text = fs::read_to_string(&scenario_path)?;
    let source_path_string = format!("examples/{example}/source.bn");
    let plan = boon_compiler::compile_source(&source_path_string, &source_text);
    let scenario = boon_runtime_host::parse_scenario(&scenario_text);
    let outputs = boon_dd::execute_scenario(&plan.graph, &scenario);

    let generated_dir = root.join("generated").join(example);
    let src_dir = generated_dir.join("src");
    fs::create_dir_all(&generated_dir)?;
    fs::create_dir_all(&src_dir)?;
    fs::write(
        generated_dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"generated_{example}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nboon_dd = {{ path = \"../../crates/boon_dd\" }}\ndifferential-dataflow = {{ version = \"=0.23.0\", default-features = false }}\nserde = {{ version = \"1\", features = [\"derive\"] }}\ntimely = {{ version = \"=0.29.0\", default-features = false }}\n"
        ),
    )?;
    fs::write(src_dir.join("lib.rs"), generated_lib_rs())?;
    fs::write(
        src_dir.join("graph.rs"),
        boon_codegen_rust::generated_graph_module(&plan),
    )?;
    fs::write(src_dir.join("ids.rs"), generated_ids_rs(&plan.graph))?;
    fs::write(
        src_dir.join("source_events.rs"),
        generated_source_events_rs(&plan.graph),
    )?;
    fs::write(src_dir.join("shapes.rs"), generated_shapes_rs(&plan.graph))?;
    fs::write(src_dir.join("values.rs"), generated_values_rs())?;
    fs::write(
        src_dir.join("render_bindings.rs"),
        generated_render_bindings_rs(&plan.graph),
    )?;
    fs::write(
        src_dir.join("monitor_bindings.rs"),
        generated_monitor_bindings_rs(&plan.graph),
    )?;
    fs::write(
        src_dir.join("persist_bindings.rs"),
        generated_persist_bindings_rs(&plan.graph),
    )?;
    fs::write(
        generated_dir.join("graph_static.json"),
        serde_json::to_vec_pretty(&plan.graph)?,
    )?;
    fs::write(
        generated_dir.join("generated_graph.rs"),
        boon_codegen_rust::generated_graph_module(&plan),
    )?;
    fs::write(
        generated_dir.join("monitor_snapshot.json"),
        serde_json::to_vec_pretty(&outputs)?,
    )?;
    fs::write(
        generated_dir.join("terminal_120x40.snapshot.txt"),
        terminal_snapshot(&outputs),
    )?;
    fs::write(
        generated_dir.join("native_render_1280x720.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "viewport": {"width": 1280, "height": 720, "dpr": 1.0},
            "render": outputs.first().map(|output| &output.render),
            "backend": "ply-native-evidence-schema"
        }))?,
    )?;
    fs::write(
        generated_dir.join("browser_render_1280x720.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "viewport": {"width": 1280, "height": 720, "dpr": 1.0},
            "render": outputs.first().map(|output| &output.render),
            "backend": "ply-browser-evidence-schema"
        }))?,
    )?;
    format_generated_rust(&generated_dir)?;
    Ok(generated_dir.display().to_string())
}

fn sync_generated_artifacts() -> Result<()> {
    for example in boon_dd::REQUIRED_EXAMPLES {
        write_generated_artifacts(example)?;
    }
    Ok(())
}

fn format_generated_rust(generated_dir: &Path) -> Result<()> {
    let mut paths = vec![generated_dir.join("generated_graph.rs")];
    for entry in fs::read_dir(generated_dir.join("src"))? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            paths.push(path);
        }
    }
    for path in paths {
        run_status("rustfmt", &[path.to_str().unwrap()])?;
    }
    Ok(())
}

fn generated_lib_rs() -> &'static str {
    "pub mod graph;\npub mod ids;\npub mod monitor_bindings;\npub mod persist_bindings;\npub mod render_bindings;\npub mod shapes;\npub mod source_events;\npub mod values;\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn generated_graph_emits_monitor_and_render_output() {\n        let allocator = timely::communication::Allocator::Thread(\n            timely::communication::allocator::Thread::default(),\n        );\n        let mut worker = timely::worker::Worker::new(timely::WorkerConfig::default(), allocator, None);\n        let mut graph = crate::graph::build_dataflow(&mut worker);\n        let outputs = graph\n            .submit_text_and_drain(&mut worker, crate::graph::smoke_input_text(), 1, 1024)\n            .expect(\"generated graph should drain\");\n        assert!(!outputs.is_empty(), \"generated graph emitted no output\");\n        assert!(outputs.iter().any(|output| !output.monitor.is_empty()));\n        assert!(outputs.iter().any(|output| !output.render.is_empty()));\n    }\n}\n"
}

fn generated_ids_rs(graph: &boon_dd::StaticGraph) -> String {
    let node_variants = graph
        .nodes
        .iter()
        .map(|node| format!("    {},", sanitize_variant(&node.node.0)))
        .collect::<Vec<_>>()
        .join("\n");
    let source_variants = graph
        .source_bindings
        .iter()
        .map(|source| format!("    {},", sanitize_variant(&source.source_id.0)))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "use serde::{{Deserialize, Serialize}};\n\n#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]\npub enum NodeId {{\n{node_variants}\n}}\n\n#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]\npub enum SourceId {{\n{source_variants}\n}}\n\n#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]\npub enum StorageKey {{\n    SemanticState,\n}}\n"
    )
}

fn generated_source_events_rs(graph: &boon_dd::StaticGraph) -> String {
    let variants = graph
        .source_bindings
        .iter()
        .map(|source| {
            let variant = sanitize_variant(&source.source_id.0);
            match source.shape.as_str() {
                "Text" => format!("    {variant} {{ text: String }},"),
                "TagSet" => format!("    {variant} {{ tag: String }},"),
                _ if source.dynamic => {
                    format!("    {variant} {{ owner: String, generation: u32 }},")
                }
                _ => format!("    {variant},"),
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "use serde::{{Deserialize, Serialize}};\n\n#[derive(Clone, Debug, Serialize, Deserialize)]\npub enum GeneratedSourceEvent {{\n{variants}\n}}\n"
    )
}

fn generated_shapes_rs(graph: &boon_dd::StaticGraph) -> String {
    let shapes = graph
        .source_bindings
        .iter()
        .map(|source| format!("    ({:?}, {:?}),", source.path, source.shape))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "pub fn source_shapes() -> &'static [(&'static str, &'static str)] {{\n    &[\n{shapes}\n    ]\n}}\n"
    )
}

fn generated_values_rs() -> &'static str {
    "pub use boon_dd::{BoonNumber, BoonValue, TagName};\n"
}

fn generated_render_bindings_rs(graph: &boon_dd::StaticGraph) -> String {
    format!(
        "pub fn render_root() -> &'static str {{ {:?} }}\npub fn render_node() -> &'static str {{ {:?} }}\n",
        graph.graph_id, graph.render_node.0
    )
}

fn generated_monitor_bindings_rs(graph: &boon_dd::StaticGraph) -> String {
    format!(
        "pub fn monitor_node() -> &'static str {{ {:?} }}\n",
        graph.monitor_node.0
    )
}

fn generated_persist_bindings_rs(graph: &boon_dd::StaticGraph) -> String {
    let has_persist = graph
        .operators
        .iter()
        .any(|op| op.kind == boon_dd::GraphOperatorKind::PersistTap);
    format!("pub fn has_persistence_tap() -> bool {{ {has_persist} }}\n")
}

fn sanitize_variant(value: &str) -> String {
    let mut result = String::new();
    let mut upper = true;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if upper {
                result.push(ch.to_ascii_uppercase());
                upper = false;
            } else {
                result.push(ch);
            }
        } else {
            upper = true;
        }
    }
    if result.is_empty() {
        "Generated".to_owned()
    } else {
        result
    }
}

fn terminal_snapshot(outputs: &[boon_dd::SmokeOutput]) -> String {
    let text = outputs
        .first()
        .and_then(|output| output.render.first())
        .map(|command| match command {
            boon_dd::RenderCommand::PatchText { text, .. } => text.clone(),
        })
        .unwrap_or_default();
    let mut snapshot = String::new();
    snapshot.push_str("+");
    snapshot.push_str(&"-".repeat(120));
    snapshot.push_str("+\n");
    snapshot.push('|');
    snapshot.push_str(&format!("{text:<120}"));
    snapshot.push_str("|\n");
    for _ in 1..40 {
        snapshot.push('|');
        snapshot.push_str(&" ".repeat(120));
        snapshot.push_str("|\n");
    }
    snapshot.push_str("+");
    snapshot.push_str(&"-".repeat(120));
    snapshot.push_str("+\n");
    snapshot
}

fn require_background_launch_env(target: &str) -> Result<()> {
    if env::var("COSMIC_BACKGROUND_LAUNCH_ID").is_err() {
        bail!(
            "{target} playground launches must be wrapped as: cosmic-background-launch --workspace boon-dd -- cargo xtask run --example <name> --target {target}"
        );
    }
    Ok(())
}

fn find_wasm_bindgen() -> Option<PathBuf> {
    let root = repo_root().ok()?;
    let local = root.join(format!(
        ".boon-local/tools/wasm-bindgen-{WASM_BINDGEN_VERSION}/bin/wasm-bindgen"
    ));
    if local.exists() {
        return Some(local);
    }
    let output = Command::new("wasm-bindgen")
        .arg("--version")
        .output()
        .ok()?;
    let version = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && version.contains(WASM_BINDGEN_VERSION) {
        Some(PathBuf::from("wasm-bindgen"))
    } else {
        None
    }
}

fn install_wasm_bindgen() -> Result<()> {
    let root = repo_root()?;
    let install_root = root.join(format!(
        ".boon-local/tools/wasm-bindgen-{WASM_BINDGEN_VERSION}"
    ));
    fs::create_dir_all(&install_root)?;
    run_status(
        "cargo",
        &[
            "install",
            "wasm-bindgen-cli",
            "--version",
            WASM_BINDGEN_VERSION,
            "--root",
            install_root.to_str().unwrap(),
            "--locked",
        ],
    )
}

fn find_plyx() -> Option<PathBuf> {
    let root = repo_root().ok()?;
    let local = root.join(format!(".boon-local/tools/plyx-{PLYX_VERSION}/bin/plyx"));
    if local.exists() {
        return Some(local);
    }
    let output = Command::new("plyx").arg("--version").output().ok()?;
    let version = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && version.contains(PLYX_VERSION) {
        Some(PathBuf::from("plyx"))
    } else {
        None
    }
}

fn install_plyx() -> Result<()> {
    let root = repo_root()?;
    let install_root = root.join(format!(".boon-local/tools/plyx-{PLYX_VERSION}"));
    fs::create_dir_all(&install_root)?;
    run_status(
        "cargo",
        &[
            "install",
            "plyx",
            "--version",
            PLYX_VERSION,
            "--root",
            install_root.to_str().unwrap(),
            "--locked",
        ],
    )
}

fn serve_ply_browser(args: &[String]) -> Result<()> {
    let mut browser = "firefox".to_owned();
    let mut _example = None::<String>;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--browser" | "--open" => {
                index += 1;
                browser = args
                    .get(index)
                    .cloned()
                    .context("missing browser name after --browser/--open")?;
            }
            "--example" => {
                index += 1;
                _example = args.get(index).cloned();
            }
            "--format" => {
                index += 1;
                let _ = args.get(index).context("missing value after --format")?;
            }
            other => bail!("unknown serve-ply-browser argument: {other}"),
        }
        index += 1;
    }
    if browser != "firefox" {
        bail!("only Firefox is supported for Ply browser launch verification");
    }
    let build_dir = build_ply_web()?;
    let listener = TcpListener::bind("127.0.0.1:0").context("failed to bind Ply browser server")?;
    let addr = listener.local_addr()?;
    let url = format!("http://{addr}/index.html");
    if env::var("BOON_DD_PLY_BROWSER_HEADLESS").as_deref() == Ok("1") {
        let profile_dir = artifacts_dir()?.join("ply-browser-manual-profile");
        if profile_dir.exists() {
            fs::remove_dir_all(&profile_dir)?;
        }
        fs::create_dir_all(&profile_dir)?;
        launch_background_process(&[
            "firefox",
            "--headless",
            "--no-remote",
            "--profile",
            profile_dir.to_str().unwrap(),
            &url,
        ])?;
    } else {
        launch_background_process(&["firefox", "--new-window", &url])?;
    }
    println!("serving Ply browser playground at {url}");
    serve_static_http(listener, build_dir)
}

fn serve_static_http(listener: TcpListener, serve_dir: PathBuf) -> Result<()> {
    for stream in listener.incoming() {
        let mut stream = stream.context("failed to accept Ply browser HTTP connection")?;
        let _ = handle_static_request(&mut stream, &serve_dir);
        let _ = stream.shutdown(Shutdown::Both);
    }
    Ok(())
}

fn handle_static_request(stream: &mut TcpStream, serve_dir: &Path) -> Result<()> {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 8192];
    loop {
        let read = stream.read(&mut temp)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
        if find_header_end(&buffer).is_some() {
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
        write_response(stream, "204 No Content", "text/plain", b"")?;
        return Ok(());
    }
    match static_file_for_request(serve_dir, path) {
        Ok(file) => {
            let bytes =
                fs::read(&file).with_context(|| format!("failed to read {}", file.display()))?;
            write_response(stream, "200 OK", content_type_for_path(&file), &bytes)?;
        }
        Err(_) => {
            write_response(stream, "404 Not Found", "text/plain", b"not found")?;
        }
    }
    Ok(())
}

fn run_firefox_smoke(html: &Path, output: &Path) -> Result<()> {
    if output.exists() {
        fs::remove_file(output)?;
    }
    let serve_dir = html
        .parent()
        .context("smoke HTML path must have a parent directory")?
        .to_path_buf();
    let listener = TcpListener::bind("127.0.0.1:0").context("failed to bind smoke HTTP server")?;
    let addr = listener.local_addr()?;
    let (tx, rx) = mpsc::channel::<String>();
    let server = std::thread::spawn(move || serve_smoke_http(listener, serve_dir, tx));
    let profile_dir = artifacts_dir()?.join("firefox-smoke-profile");
    if profile_dir.exists() {
        fs::remove_dir_all(&profile_dir)?;
    }
    fs::create_dir_all(&profile_dir)?;
    fs::write(
        profile_dir.join("user.js"),
        r#"user_pref("gfx.webrender.all", true);
"#,
    )?;

    let smoke_url = format!("http://{addr}/index.html");
    launch_background_process(&[
        "firefox",
        "--headless",
        "--no-remote",
        "--profile",
        profile_dir.to_str().unwrap(),
        &smoke_url,
    ])?;
    let result = rx.recv_timeout(Duration::from_secs(75));
    let _ = Command::new("pkill")
        .args(["-f", profile_dir.to_str().unwrap()])
        .status();
    let result = result.context("Firefox smoke did not POST monitor/render output")?;
    fs::write(output, result)?;
    let _ = server.join();
    if !output.exists() {
        bail!("Firefox smoke did not write {}", output.display());
    }
    Ok(())
}

fn launch_background_process(args: &[&str]) -> Result<String> {
    run_capture("bash", &["-lc", "command -v cosmic-background-launch"])
        .context("missing cosmic-background-launch")?;
    let mut busctl_args = vec![
        "--user",
        "call",
        "com.system76.CosmicComp.BackgroundLaunch",
        "/com/system76/CosmicComp/BackgroundLaunch",
        "com.system76.CosmicComp.BackgroundLaunch1",
        "Launch",
        "--",
        "sassa{ss}",
    ];
    busctl_args.push(COSMIC_WORKSPACE);
    let argc = args.len().to_string();
    busctl_args.push(&argc);
    busctl_args.extend_from_slice(args);
    let cwd = repo_root()?;
    let cwd_string = cwd.display().to_string();
    busctl_args.push(&cwd_string);
    busctl_args.push("0");

    let output = Command::new("busctl")
        .args(&busctl_args)
        .output()
        .context("failed to call COSMIC BackgroundLaunch D-Bus service")?;
    if !output.status.success() {
        bail!(
            "COSMIC BackgroundLaunch D-Bus launch failed with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    stdout
        .split('"')
        .nth(1)
        .map(str::to_owned)
        .context("COSMIC BackgroundLaunch did not return a launch id")
}

fn serve_smoke_http(
    listener: TcpListener,
    serve_dir: PathBuf,
    tx: mpsc::Sender<String>,
) -> Result<()> {
    for stream in listener.incoming() {
        let mut stream = stream.context("failed to accept smoke HTTP connection")?;
        let done = handle_smoke_request(&mut stream, &serve_dir, &tx)?;
        let _ = stream.shutdown(Shutdown::Both);
        if done {
            break;
        }
    }
    Ok(())
}

fn handle_smoke_request(
    stream: &mut TcpStream,
    serve_dir: &Path,
    tx: &mpsc::Sender<String>,
) -> Result<bool> {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 8192];
    loop {
        let read = stream.read(&mut temp)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
        if let Some(header_end) = find_header_end(&buffer) {
            let headers = String::from_utf8_lossy(&buffer[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let body_start = header_end + 4;
            while buffer.len() < body_start + content_length {
                let read = stream.read(&mut temp)?;
                if read == 0 {
                    break;
                }
                buffer.extend_from_slice(&temp[..read]);
            }
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
        let header_end = find_header_end(&buffer).context("POST missing headers")?;
        let body = String::from_utf8_lossy(&buffer[header_end + 4..]).to_string();
        tx.send(body).context("failed to send smoke result")?;
        write_response(stream, "204 No Content", "text/plain", b"")?;
        return Ok(true);
    }

    let file = match static_file_for_request(serve_dir, path) {
        Ok(file) => file,
        Err(_) => {
            write_response(stream, "404 Not Found", "text/plain", b"not found")?;
            return Ok(false);
        }
    };
    let content_type = content_type_for_path(&file);
    let bytes = match fs::read(&file) {
        Ok(bytes) => bytes,
        Err(_) => {
            write_response(stream, "404 Not Found", "text/plain", b"not found")?;
            return Ok(false);
        }
    };
    write_response(stream, "200 OK", content_type, &bytes)?;
    Ok(false)
}

fn static_file_for_request(serve_dir: &Path, path: &str) -> Result<PathBuf> {
    let relative = match path {
        "/" | "/index.html" => PathBuf::from("index.html"),
        _ => {
            let stripped = path.trim_start_matches('/');
            if stripped.is_empty() || stripped.contains("..") || stripped.starts_with('/') {
                bail!("refusing unsafe static path: {path}");
            }
            PathBuf::from(stripped)
        }
    };
    let file = serve_dir.join(relative);
    if !file.exists() {
        bail!("static asset not found: {}", file.display());
    }
    Ok(file)
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "html" => "text/html",
        "js" => "text/javascript",
        "wasm" => "application/wasm",
        "css" => "text/css",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_response(
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

fn smoke_html() -> &'static str {
    r#"<!doctype html>
<meta charset="utf-8">
<script type="module">
import init, { run_smoke_json } from "./boon_wasm_smoke.js";
try {
  await init();
  const result = JSON.stringify({
    wasm_smoke: JSON.parse(run_smoke_json())
  });
  document.body.textContent = result;
  await fetch("/result", { method: "POST", body: result });
} catch (error) {
  document.body.textContent = String(
    (error && error.message ? error.message + "\n" : "") +
    (error && error.stack || error)
  );
  await fetch("/result", { method: "POST", body: document.body.textContent });
  throw error;
}
</script>
"#
}
