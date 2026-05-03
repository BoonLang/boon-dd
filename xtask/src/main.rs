use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
    if !output.contains("CounterHold")
        || !output.contains("TodoMvcPhysical")
        || !output.contains("DocumentText")
    {
        bail!("Firefox smoke output did not contain expected monitor/render records: {output}");
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
                "mode": "noninteractive render-command verification"
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
                "mode": "browser-hosted wasm smoke artifact verification"
            }))
        },
    ));
    gates.push(capture_simple_gate(
        "plan-coverage",
        "cargo xtask verify all --format json",
        verify_plan_coverage,
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
    fs::write(
        &success_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "success": success,
            "failed_gates": failed_gates,
            "verify_report": report_path,
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
            Ok(()) => terminal_checked.push(example),
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

    let required_paths = [
        "ARCHITECTURE.md",
        "target/boon-artifacts/wasm-bindgen/smoke-result.json",
    ];
    let missing_paths = required_paths
        .iter()
        .filter_map(|path| {
            let path = root.join(path);
            (!path.exists()).then(|| path.display().to_string())
        })
        .collect::<Vec<_>>();

    let artifact = artifacts_dir()?.join("plan-coverage.json");
    let details = serde_json::json!({
        "required_crates": required_crates,
        "missing_crates": missing_crates,
        "missing_paths": missing_paths,
    });
    fs::write(&artifact, serde_json::to_vec_pretty(&details)?)?;

    if !details["missing_crates"].as_array().unwrap().is_empty()
        || !details["missing_paths"].as_array().unwrap().is_empty()
    {
        bail!("plan coverage is incomplete; see {}", artifact.display());
    }
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
    if target != "terminal" {
        bail!("only terminal target is implemented for run so far");
    }
    let output = boon_dd_smoke_json(&example)?;
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
        let output = fs::read_to_string(
            artifacts_dir()?
                .join("wasm-bindgen")
                .join("smoke-result.json"),
        )
        .context("missing browser WASM smoke artifact; run verify-wasm-dd first")?;
        for example in boon_dd::REQUIRED_EXAMPLES {
            if !output.contains(example) {
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

fn run_terminal_scenario(example: &str) -> Result<()> {
    let example_dir = repo_root()?.join("examples").join(example);
    let scenario = example_dir.join("scenario.toml");
    if !scenario.exists() {
        bail!("missing scenario {}", scenario.display());
    }
    let expected_path = example_dir.join("expected.render.json");
    let expected = fs::read_to_string(&expected_path)
        .with_context(|| format!("missing expected artifact {}", expected_path.display()))?;
    let actual = boon_dd_smoke_json(example)?;
    let expected_json: serde_json::Value = serde_json::from_str(&expected)
        .with_context(|| format!("invalid JSON {}", expected_path.display()))?;
    let actual_json: serde_json::Value = serde_json::from_str(&actual)?;
    if actual_json != expected_json {
        bail!(
            "terminal scenario {example} output mismatch\nexpected: {expected_json}\nactual: {actual_json}"
        );
    }
    Ok(())
}

fn boon_dd_smoke_json(example: &str) -> Result<String> {
    let output = boon_dd::run_named_example_smoke(example)
        .with_context(|| format!("example {example} is not implemented yet"))?;
    serde_json::to_string(&output).context("failed to serialize DD smoke output")
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

    let script = format!(
        "cosmic-background-launch -- firefox --headless --no-remote --profile '{}' 'http://{addr}/index.html'",
        profile_dir.display()
    );
    let status = Command::new("bash")
        .args(["-lc", &script])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .status()
        .context("failed to launch Firefox smoke through cosmic-background-launch")?;
    if !status.success() {
        bail!("Firefox smoke failed with {status}");
    }
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
  const result = run_smoke_json();
  document.body.textContent = result;
  await fetch("/result", { method: "POST", body: result });
} catch (error) {
  document.body.textContent = String(error && error.stack || error);
  await fetch("/result", { method: "POST", body: document.body.textContent });
  throw error;
}
</script>
"#
}
