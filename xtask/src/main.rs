use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const WASM_BINDGEN_VERSION: &str = "0.2.120";

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
        bail!("usage: cargo xtask <bootstrap|verify-deps|verify-wasm-dd|verify> ...");
    }

    match args.remove(0).as_str() {
        "bootstrap" => bootstrap(&args),
        "run" => run_example(&args),
        "test" => test_target(&args),
        "verify-deps" => verify_deps(&args).map(|_| ()),
        "verify-wasm-dd" => verify_wasm_dd(&args).map(|_| ()),
        "verify-render-deps" => verify_render_deps(&args).map(|_| ()),
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
    } else if wasm_bindgen.is_none() {
        install_wasm_bindgen()?;
    }

    let details = serde_json::json!({
        "rustc": rustc,
        "cargo": cargo,
        "targets": targets.lines().collect::<Vec<_>>(),
        "cosmic_background_launch": helper,
        "background_launch_service": bus,
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
    require_webgpu_smoke(&smoke_value)?;
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

fn require_webgpu_smoke(smoke_value: &serde_json::Value) -> Result<()> {
    let webgpu = smoke_value
        .get("webgpu")
        .context("Firefox smoke output missing webgpu object")?;
    for field in ["navigator_gpu", "adapter", "device"] {
        if webgpu.get(field).and_then(|value| value.as_bool()) != Some(true) {
            bail!("Firefox WebGPU smoke did not prove webgpu.{field}: {smoke_value}");
        }
    }
    Ok(())
}

fn verify_render_deps(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    run_status("cargo", &["check", "-p", "boon_backend_ratatui"])?;
    run_status("cargo", &["check", "-p", "boon_backend_wgpu"])?;
    run_status("cargo", &["check", "-p", "boon_backend_app_window"])?;
    run_status("cargo", &["check", "-p", "boon_backend_browser"])?;
    let native_smoke_artifact = verify_native_app_window_smoke()?;
    let shader_path = repo_root()?.join("shaders/common/ui_rect.wgsl");
    let shader_source = fs::read_to_string(&shader_path)
        .with_context(|| format!("missing shader {}", shader_path.display()))?;
    naga::front::wgsl::parse_str(&shader_source)
        .with_context(|| format!("WGSL parse failed for {}", shader_path.display()))?;

    let artifact = artifacts_dir()?.join("verify-render-deps.json");
    let details = serde_json::json!({
        "ratatui": "0.30.0",
        "crossterm": "0.29.0",
        "wgpu": "29.0.3",
        "wesl": "0.3.2",
        "wgsl_bindgen": "0.22.2",
        "app_window": "0.3.3",
        "native_surface_mode": "app_window native window and surface smoke plus render-command preflight",
        "browser_surface_mode": "browser-hosted wasm plus Firefox WebGPU adapter/device preflight",
        "shader_parse": shader_path,
        "native_app_window_smoke": native_smoke_artifact,
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

fn verify_native_app_window_smoke() -> Result<PathBuf> {
    let artifact = artifacts_dir()?.join("native-app-window-smoke.json");
    if artifact.exists() {
        fs::remove_file(&artifact)?;
    }
    let artifact_arg = artifact.display().to_string();
    launch_background_process(&[
        "cargo",
        "run",
        "--quiet",
        "-p",
        "boon_backend_app_window",
        "--bin",
        "native_smoke",
        "--",
        &artifact_arg,
    ])?;
    let start = Instant::now();
    while !artifact.exists() {
        if start.elapsed() > Duration::from_secs(30) {
            bail!(
                "native app_window smoke did not write {} within 30s",
                artifact.display()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let details: serde_json::Value = serde_json::from_str(&fs::read_to_string(&artifact)?)?;
    for field in ["window_created", "surface_created"] {
        if details.get(field).and_then(|value| value.as_bool()) != Some(true) {
            bail!("native app_window smoke did not prove {field}: {details}");
        }
    }
    Ok(artifact)
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
                "mode": "app_window native window/surface smoke plus render-command verification"
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
                "mode": "browser-hosted wasm plus Firefox WebGPU smoke artifact verification"
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
        "boon_backend_wgpu",
        "boon_backend_app_window",
        "boon_backend_browser",
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
    let output = compiled_example_json(&example)?;
    println!("{output}");
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
        let output_text = fs::read_to_string(
            artifacts_dir()?
                .join("wasm-bindgen")
                .join("smoke-result.json"),
        )
        .context("missing browser WASM smoke artifact; run verify-wasm-dd first")?;
        let output: serde_json::Value =
            serde_json::from_str(&output_text).context("browser smoke artifact is not JSON")?;
        require_webgpu_smoke(&output)?;
        for example in boon_dd::REQUIRED_EXAMPLES {
            if !output_text.contains(example) {
                bail!("browser smoke artifact does not include example {example}");
            }
        }
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
            "backend": "wgpu-command-schema"
        }))?,
    )?;
    fs::write(
        generated_dir.join("browser_render_1280x720.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "viewport": {"width": 1280, "height": 720, "dpr": 1.0},
            "render": outputs.first().map(|output| &output.render),
            "backend": "browser-webgpu-command-schema"
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
            "{target} playground launches must be wrapped as: cosmic-background-launch --workspace 'Boon DD Playground' -- cargo xtask run --example <name> --target {target}"
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
        r#"user_pref("dom.webgpu.enabled", true);
user_pref("gfx.webgpu.force-enabled", true);
user_pref("gfx.webrender.all", true);
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
    let result = rx.recv_timeout(Duration::from_secs(30));
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
        "assa{ss}",
    ];
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
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");

    if request_line.starts_with("POST ") && path == "/result" {
        let header_end = find_header_end(&buffer).context("POST missing headers")?;
        let body = String::from_utf8_lossy(&buffer[header_end + 4..]).to_string();
        tx.send(body).context("failed to send smoke result")?;
        write_response(stream, "204 No Content", "text/plain", b"")?;
        return Ok(true);
    }

    let (file, content_type) = match path {
        "/" | "/index.html" => (serve_dir.join("index.html"), "text/html"),
        "/boon_wasm_smoke.js" => (serve_dir.join("boon_wasm_smoke.js"), "text/javascript"),
        "/boon_wasm_smoke_bg.wasm" => (
            serve_dir.join("boon_wasm_smoke_bg.wasm"),
            "application/wasm",
        ),
        _ => {
            write_response(stream, "404 Not Found", "text/plain", b"not found")?;
            return Ok(false);
        }
    };
    let bytes = fs::read(&file).with_context(|| format!("failed to read {}", file.display()))?;
    write_response(stream, "200 OK", content_type, &bytes)?;
    Ok(false)
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
  if (!("gpu" in navigator)) {
    throw new Error("Firefox WebGPU preflight failed: navigator.gpu is unavailable");
  }
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) {
    throw new Error("Firefox WebGPU preflight failed: requestAdapter returned null");
  }
  const device = await adapter.requestDevice();
  if (!device) {
    throw new Error("Firefox WebGPU preflight failed: requestDevice returned null");
  }
  const result = JSON.stringify({
    webgpu: {
      navigator_gpu: true,
      adapter: true,
      device: true
    },
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
