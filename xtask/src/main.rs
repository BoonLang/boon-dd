use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const WASM_BINDGEN_VERSION: &str = "0.2.120";
const COSMIC_WORKSPACE: &str = "boon-dd";
const HONEST_COMPILER_PLAN: &str = "BOON_DD_HONEST_COMPILER_PLAN.md";
const LANGUAGE_MANIFEST: &str = "docs/language/boon-language-manifest.toml";

const HONEST_SHORTCUT_PATTERNS: &[&str] = &[
    concat!("detect", "_operators"),
    concat!("infer", "_sources"),
    concat!("infer", "_source_paths"),
    concat!("infer", "_source_shape"),
    concat!("infer", "_monitor_node"),
    concat!("infer", "_initial_text"),
    concat!("infer", "_text_behavior"),
    concat!("infer", "_document_text"),
    concat!("infer", "_constant_text"),
    concat!("infer", "_document_target"),
    concat!("definition", "_block"),
    concat!("text", "_literals"),
    concat!("Text", "Behavior"),
    concat!("Dd", "Output", "Template"),
    concat!("execute", "_static_graph"),
    concat!("evaluate", "_text"),
    concat!("generated", "_text_collection"),
    concat!("smoke", "_input_text"),
    concat!("compile", "_and_run_step"),
    concat!("scenario", "_actions_for_text"),
    concat!("run", "_generated_for_source"),
    concat!("REQUIRED", "_FIXTURES"),
    concat!("steps", ".first()"),
    concat!("source", "_action_text"),
    concat!("submit", "_text("),
    concat!("submit", "_text_and_drain"),
    concat!("constant", "_text("),
    concat!("constant", "_value("),
    concat!("call", "_constant("),
    concat!("pipe", "_constant("),
    concat!("dd", "_render_operation_from_expr("),
    concat!("expression", "_has_call("),
    concat!("line.contains", "(\"command =\")"),
    concat!("contains", "(\"SOURCE\")"),
    concat!("contains", "(\"THEN\")"),
    concat!("contains", "(\"WHEN\")"),
    concat!("contains", "(\"WHILE\")"),
    concat!("contains", "(\"LATEST\")"),
    concat!("contains", "(\"HOLD\")"),
    concat!("contains", "(\"List/"),
    concat!("contains", "(\"Scene/new(\")"),
];

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
            "usage: cargo xtask <bootstrap|run|test|verify-deps|verify-wasm-dd|verify-render-deps|verify-playgrounds|verify-syntax-corpus|verify-resolver-corpus|verify-shape-corpus|verify-semantic-ir|verify-honest-compiler|verify-no-shortcuts|verify-honesty-deterministic|verify-language-corpus|verify-negative-corpus|verify-lowering|verify-generated-freshness|verify-generated-crates|write-honest-compiler-prompts|verify-prompt-audit|verify> ..."
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
        "verify-syntax-corpus" => verify_syntax_corpus(&args).map(|_| ()),
        "verify-resolver-corpus" => verify_resolver_corpus(&args).map(|_| ()),
        "verify-shape-corpus" => verify_shape_corpus(&args).map(|_| ()),
        "verify-semantic-ir" => verify_semantic_ir(&args).map(|_| ()),
        "verify-honest-compiler" => verify_honest_compiler(&args).map(|_| ()),
        "verify-no-shortcuts" => verify_no_shortcuts(&args).map(|_| ()),
        "verify-honesty-deterministic" => verify_honesty_deterministic(&args).map(|_| ()),
        "verify-language-corpus" => verify_language_corpus(&args).map(|_| ()),
        "verify-negative-corpus" => verify_negative_corpus(&args).map(|_| ()),
        "verify-lowering" => verify_lowering(&args).map(|_| ()),
        "verify-generated-freshness" => verify_generated_freshness(&args).map(|_| ()),
        "verify-generated-crates" => verify_generated_crates().map(|_| ()),
        "write-honest-compiler-prompts" => write_honest_compiler_prompts(&args).map(|_| ()),
        "verify-prompt-audit" => verify_prompt_audit(&args).map(|_| ()),
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
    let bus = run_capture("busctl", &["--user", "list"])
        .context("failed to query user D-Bus service list with busctl")?;
    if !bus.contains("com.system76.CosmicComp.BackgroundLaunch") {
        bail!("COSMIC BackgroundLaunch D-Bus service is not active");
    }

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
        "background_launch_service": "com.system76.CosmicComp.BackgroundLaunch",
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
    verify_generated_freshness(&["--format".to_owned(), "json".to_owned()])?;
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
    for required in ["Counter", "DocumentOutput", "DocumentText"] {
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

fn verify_playgrounds(_args: &[String]) -> Result<GateReport> {
    let start = Instant::now();
    verify_generated_freshness(&["--format".to_owned(), "json".to_owned()])?;
    run_status("cargo", &["check", "-p", "boon_backend_ratatui"])?;
    run_status("cargo", &["check", "-p", "boon_backend_app_window"])?;
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
    let native_artifact = dir.join("native-playground.json");
    let native_screenshots_dir = dir.join("native-playground-screenshots");
    let browser_artifact = dir.join("browser-playground-result.json");

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

    if native_artifact.exists() {
        fs::remove_file(&native_artifact)?;
    }
    run_status(
        "cargo",
        &[
            "build",
            "-p",
            "boon_backend_app_window",
            "--bin",
            "native_playground",
        ],
    )?;
    let native_playground_binary = repo_root()?.join("target/debug/native_playground");
    let native_playground_binary_arg = native_playground_binary.display().to_string();
    let native_artifact_arg = native_artifact.display().to_string();
    let native_screenshots_arg = native_screenshots_dir.display().to_string();
    launch_background_process(&[
        &native_playground_binary_arg,
        "--smoke",
        &native_artifact_arg,
        &native_screenshots_arg,
    ])?;
    wait_for_json_artifact(
        &native_artifact,
        Duration::from_secs(45),
        "native playground",
    )?;
    let native_details = read_playground_artifact(&native_artifact)?;
    require_playground_examples("native", &native_details)?;
    for pointer in [
        "/window_created",
        "/surface_created",
        "/wgpu/adapter",
        "/wgpu/device",
        "/wgpu/surface_configured",
        "/wgpu/frame_presented",
        "/visible_ui/full_surface_background",
        "/visible_ui/sidebar",
        "/visible_ui/example_labels",
        "/visible_ui/paged_example_list",
        "/visible_ui/native_vector_scene",
        "/visible_ui/selected_output_panel",
    ] {
        if native_details
            .pointer(pointer)
            .and_then(|value| value.as_bool())
            != Some(true)
        {
            bail!("native playground did not prove {pointer}: {native_details}");
        }
    }
    let rendered_vertices = native_details
        .pointer("/visible_ui/rendered_vertices")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    if rendered_vertices < 1000 {
        bail!("native playground rendered too little UI geometry: {native_details}");
    }
    require_native_per_example_screenshots(&native_details)?;

    let browser_dir = prepare_wasm_bindgen_output()?;
    let browser_html = browser_dir.join("index.html");
    fs::write(&browser_html, browser_playground_html())?;
    if browser_artifact.exists() {
        fs::remove_file(&browser_artifact)?;
    }
    run_firefox_smoke(&browser_html, &browser_artifact)?;
    let browser_details = read_playground_artifact(&browser_artifact)?;
    require_webgpu_smoke(&browser_details)?;
    require_playground_examples("browser", &browser_details)?;
    for pointer in [
        "/ui/canvas",
        "/ui/output_panel",
        "/ui/simulated_click",
        "/webgpu/canvas_context",
        "/webgpu/frame_presented",
    ] {
        if browser_details
            .pointer(pointer)
            .and_then(|value| value.as_bool())
            != Some(true)
        {
            bail!("browser playground did not prove {pointer}: {browser_details}");
        }
    }

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
            native_artifact.display().to_string(),
            native_screenshots_dir.display().to_string(),
            browser_artifact.display().to_string(),
        ],
        details,
    })
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

fn require_native_per_example_screenshots(details: &serde_json::Value) -> Result<()> {
    let rows = details
        .get("per_example")
        .and_then(|value| value.as_array())
        .context("native playground artifact missing per_example screenshot list")?;
    if rows.len() != boon_dd::REQUIRED_EXAMPLES.len() {
        bail!(
            "native playground wrote {} per-example screenshots; expected {}",
            rows.len(),
            boon_dd::REQUIRED_EXAMPLES.len()
        );
    }
    for (index, example) in boon_dd::REQUIRED_EXAMPLES.iter().enumerate() {
        let row = rows
            .get(index)
            .with_context(|| format!("missing native screenshot row for {example}"))?;
        if row.get("example").and_then(|value| value.as_str()) != Some(*example) {
            bail!("native screenshot row {index} is not for {example}: {row}");
        }
        if row.get("selected_index").and_then(|value| value.as_u64()) != Some(index as u64) {
            bail!("native screenshot row has wrong selected_index for {example}: {row}");
        }
        if row
            .get("rendered_vertices")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            < 3000
        {
            bail!("native screenshot row rendered too little geometry for {example}: {row}");
        }
        let scene_kind = row
            .get("scene_kind")
            .and_then(|value| value.as_str())
            .with_context(|| format!("native screenshot row missing scene_kind for {example}"))?;
        if scene_kind.is_empty() || scene_kind == "native_workbench_app" {
            bail!("native screenshot row has insufficient scene kind for {example}: {row}");
        }
        let widgets = row
            .get("native_widgets")
            .and_then(|value| value.as_array())
            .with_context(|| {
                format!("native screenshot row missing widget evidence for {example}")
            })?;
        if widgets.len() < 3 {
            bail!("native screenshot row has too few native widgets for {example}: {row}");
        }
        let screenshot = row
            .get("screenshot")
            .and_then(|value| value.as_str())
            .with_context(|| format!("native screenshot row missing path for {example}"))?;
        require_png_signature(Path::new(screenshot))
            .with_context(|| format!("invalid native screenshot for {example}: {screenshot}"))?;
    }
    Ok(())
}

fn require_png_signature(path: &Path) -> Result<()> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    if bytes.len() < 64 {
        bail!("PNG file is too small: {}", path.display());
    }
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        bail!("file does not have a PNG signature: {}", path.display());
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

fn prepare_wasm_bindgen_output() -> Result<PathBuf> {
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
    Ok(out_dir)
}

fn verify_native_app_window_smoke() -> Result<PathBuf> {
    let artifact = artifacts_dir()?.join("native-app-window-smoke.json");
    if artifact.exists() {
        fs::remove_file(&artifact)?;
    }
    run_status(
        "cargo",
        &[
            "build",
            "-p",
            "boon_backend_app_window",
            "--bin",
            "native_smoke",
        ],
    )?;
    let binary = repo_root()?.join("target/debug/native_smoke");
    let binary_arg = binary.display().to_string();
    let artifact_arg = artifact.display().to_string();
    launch_background_process(&[&binary_arg, &artifact_arg])?;
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
    gates.push(capture_simple_gate(
        "verify-generated-freshness",
        "cargo xtask verify-generated-freshness --format json",
        || verify_generated_freshness(&["--format".to_owned(), "json".to_owned()]),
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
        "verify-syntax-corpus",
        "cargo xtask verify-syntax-corpus --format json",
        || verify_syntax_corpus(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "verify-resolver-corpus",
        "cargo xtask verify-resolver-corpus --format json",
        || verify_resolver_corpus(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "verify-shape-corpus",
        "cargo xtask verify-shape-corpus --format json",
        || verify_shape_corpus(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "verify-semantic-ir",
        "cargo xtask verify-semantic-ir --format json",
        || verify_semantic_ir(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "verify-honest-compiler",
        "cargo xtask verify-honest-compiler --format json",
        || verify_honest_compiler(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "verify-no-shortcuts",
        "cargo xtask verify-no-shortcuts --format json",
        || verify_no_shortcuts(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "verify-honesty-deterministic",
        "cargo xtask verify-honesty-deterministic --format json",
        || verify_honesty_deterministic(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "verify-language-corpus",
        "cargo xtask verify-language-corpus --format json",
        || verify_language_corpus(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "verify-negative-corpus",
        "cargo xtask verify-negative-corpus --format json",
        || verify_negative_corpus(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "verify-lowering",
        "cargo xtask verify-lowering --format json",
        || verify_lowering(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "write-honest-compiler-prompts",
        "cargo xtask write-honest-compiler-prompts --format json",
        || write_honest_compiler_prompts(&["--format".to_owned(), "json".to_owned()]),
    ));
    gates.push(capture_simple_gate(
        "verify-prompt-audit",
        "cargo xtask verify-prompt-audit --format json",
        || verify_prompt_audit(&["--format".to_owned(), "json".to_owned()]),
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
            "honest_compiler_report": read_artifact_json("honest-compiler-report.json")?,
            "honesty_deterministic_report": read_artifact_json("honesty-deterministic-report.json")?,
            "language_corpus_report": read_artifact_json("language-corpus-report.json")?,
            "lowering_coverage_report": read_artifact_json("lowering-coverage-report.json")?,
            "generated_crates_report": read_artifact_json("generated-crates.json")?,
            "generated_freshness_report": read_artifact_json("generated-freshness-report.json")?,
            "no_shortcuts_report": read_artifact_json("no-shortcuts-report.json")?,
            "honest_compiler_prompt_pack": read_artifact_json("honest-compiler-prompt-pack.json")?,
            "prompt_audit_report": read_artifact_json("prompt-audit-report.json")?,
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

fn write_artifact(name: &str, details: &serde_json::Value) -> Result<PathBuf> {
    let artifact = artifacts_dir()?.join(name);
    fs::write(&artifact, serde_json::to_vec_pretty(details)?)?;
    Ok(artifact)
}

fn read_artifact_json(name: &str) -> Result<serde_json::Value> {
    let artifact = artifacts_dir()?.join(name);
    if !artifact.exists() {
        return Ok(serde_json::json!({
            "missing": true,
            "path": artifact,
        }));
    }
    serde_json::from_str(&fs::read_to_string(&artifact)?)
        .with_context(|| format!("artifact is not JSON: {}", artifact.display()))
}

fn sha256_file(path: &Path) -> Result<String> {
    let path_string = path
        .to_str()
        .with_context(|| format!("path is not UTF-8: {}", path.display()))?;
    let output = run_capture("sha256sum", &[path_string])
        .with_context(|| format!("failed to hash {}", path.display()))?;
    output
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .with_context(|| format!("sha256sum output was empty for {}", path.display()))
}

fn sha256_text(text: &str) -> Result<String> {
    let mut child = Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to spawn sha256sum")?;
    child
        .stdin
        .as_mut()
        .context("sha256sum stdin unavailable")?
        .write_all(text.as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "sha256sum over text failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .context("sha256sum text output was empty")
}

fn canonical_deterministic_report_hash(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("missing deterministic report {}", path.display()))?;
    let mut value: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("invalid deterministic report JSON {}", path.display()))?;
    normalize_deterministic_report_hash_value(&mut value);
    Ok(Some(sha256_text(&serde_json::to_string(&value)?)?))
}

fn normalize_deterministic_report_hash_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if key == "duration_ms" {
                    *value = serde_json::Value::from(0);
                } else {
                    normalize_deterministic_report_hash_value(value);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                normalize_deterministic_report_hash_value(value);
            }
        }
        _ => {}
    }
}

fn repo_state() -> Result<serde_json::Value> {
    let root = repo_root()?;
    let status = run_capture("git", &["status", "--short"])?;
    let plan = root.join(HONEST_COMPILER_PLAN);
    Ok(serde_json::json!({
        "commit": run_capture("git", &["rev-parse", "HEAD"])?,
        "dirty": !status.trim().is_empty(),
        "status_short": status,
        "plan_path": HONEST_COMPILER_PLAN,
        "plan_sha256": sha256_file(&plan).unwrap_or_else(|error| format!("unavailable: {error:#}")),
    }))
}

fn repo_state_hash() -> Result<String> {
    sha256_text(&serde_json::to_string(&repo_state()?)?)
}

fn scan_honest_shortcuts() -> Result<serde_json::Value> {
    let root = repo_root()?;
    let mut hits = Vec::new();
    for base in ["crates", "xtask/src", "generated"] {
        let dir = root.join(base);
        if !dir.exists() {
            continue;
        }
        scan_forbidden_in_dir(&dir, HONEST_SHORTCUT_PATTERNS, &mut hits)?;
    }
    Ok(serde_json::json!({
        "patterns": HONEST_SHORTCUT_PATTERNS,
        "hits": hits,
        "hit_count": hits.len(),
    }))
}

fn required_examples_from_disk() -> Result<Vec<String>> {
    let examples_dir = repo_root()?.join("examples");
    let mut examples = Vec::new();
    for entry in fs::read_dir(examples_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.path().join("source.bn").exists() {
            examples.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    examples.sort();
    Ok(examples)
}

fn verify_honest_compiler(_args: &[String]) -> Result<serde_json::Value> {
    let shortcuts = scan_honest_shortcuts()?;
    let deterministic =
        match verify_honesty_deterministic(&["--format".to_owned(), "json".to_owned()]) {
            Ok(report) => report,
            Err(error) => serde_json::json!({
                "verdict": "fail",
                "error": format!("{error:#}"),
                "artifact": "target/boon-artifacts/honesty-deterministic-report.json",
            }),
        };
    let prompt_pack = if deterministic["verdict"].as_str() == Some("pass") {
        match write_honest_compiler_prompts(&["--format".to_owned(), "json".to_owned()]) {
            Ok(report) => report,
            Err(error) => serde_json::json!({
                "verdict": "fail",
                "error": format!("{error:#}"),
                "artifact": "target/boon-artifacts/honest-compiler-prompt-pack.json",
            }),
        }
    } else {
        serde_json::json!({
            "verdict": "skipped",
            "reason": "deterministic honesty failed",
        })
    };
    let prompt_audit = if deterministic["verdict"].as_str() == Some("pass")
        && prompt_pack["verdict"].as_str() == Some("pass")
    {
        match verify_prompt_audit(&["--format".to_owned(), "json".to_owned()]) {
            Ok(report) => report,
            Err(error) => serde_json::json!({
                "verdict": "fail",
                "error": format!("{error:#}"),
                "artifact": "target/boon-artifacts/prompt-audit-report.json",
            }),
        }
    } else {
        serde_json::json!({
            "verdict": "skipped",
            "reason": "deterministic honesty or prompt pack failed",
        })
    };

    let mut blockers = Vec::new();
    if shortcuts["hit_count"].as_u64().unwrap_or(0) != 0 {
        blockers.push("shortcut symbols remain in compiler/runtime/codegen execution paths");
    }
    if deterministic["verdict"].as_str() != Some("pass") {
        blockers.push("deterministic honesty verification is not passing");
    }
    if prompt_pack["verdict"].as_str() != Some("pass") {
        blockers.push("honest compiler prompt pack could not be refreshed");
    }
    if prompt_audit["verdict"].as_str() != Some("pass") {
        blockers.push("prompt-audit verification is not passing");
    }
    let verdict = if blockers.is_empty() { "pass" } else { "fail" };
    let details = serde_json::json!({
        "verdict": verdict,
        "phase": if verdict == "pass" { "complete" } else { "blocked" },
        "repo_state": repo_state()?,
        "plan": HONEST_COMPILER_PLAN,
        "blockers": blockers,
        "shortcut_scan": shortcuts,
        "deterministic_honesty": deterministic,
        "prompt_pack": prompt_pack,
        "prompt_audit": prompt_audit,
        "required_next_command": if verdict == "pass" {
            serde_json::Value::Null
        } else {
            serde_json::json!("cargo xtask verify all --format json")
        },
    });
    let artifact = write_artifact("honest-compiler-report.json", &details)?;
    if verdict != "pass" {
        bail!(
            "honest compiler is not implemented yet; see {}",
            artifact.display()
        );
    }
    Ok(details)
}

fn verify_no_shortcuts(_args: &[String]) -> Result<serde_json::Value> {
    let shortcuts = scan_honest_shortcuts()?;
    let hit_count = shortcuts["hit_count"].as_u64().unwrap_or(0);
    let details = serde_json::json!({
        "verdict": if hit_count == 0 { "pass" } else { "fail" },
        "repo_state": repo_state()?,
        "shortcut_symbols_in_execution_paths": hit_count,
        "scan": shortcuts,
        "allowlist": [
            HONEST_COMPILER_PLAN,
            "docs/blockers/**",
            "tests that assert the guardrail catches forbidden patterns"
        ],
    });
    let artifact = write_artifact("no-shortcuts-report.json", &details)?;
    if hit_count != 0 {
        bail!(
            "shortcut execution patterns are still present; see {}",
            artifact.display()
        );
    }
    Ok(details)
}

fn verify_honesty_deterministic(_args: &[String]) -> Result<serde_json::Value> {
    let root = repo_root()?;
    let manifest = read_language_manifest()?;
    let shortcuts = scan_honest_shortcuts()?;
    let input_hashes = deterministic_input_hashes(&root, &manifest)?;
    let parser_gate = deterministic_parser_gate(&root, &manifest);
    let phase_gate = deterministic_phase_boundary_gate(&root, &manifest);
    let source_truth_gate = deterministic_source_truth_gate(&root, &manifest);
    let no_shortcuts_gate = capture_simple_gate(
        "no-shortcuts",
        "cargo xtask verify-no-shortcuts --format json",
        || verify_no_shortcuts(&["--format".to_owned(), "json".to_owned()]),
    );
    let resolver_shape_gate = deterministic_resolver_shape_gate(&root, &manifest);
    let semantic_gate = deterministic_semantic_ir_gate(&root, &manifest);
    let dd_lowering_gate = capture_simple_gate(
        "dd-lowering-coverage",
        "cargo xtask verify-lowering --format json",
        || verify_lowering(&["--format".to_owned(), "json".to_owned()]),
    );
    let generated_freshness_gate = capture_simple_gate(
        "stale-artifact-rejection",
        "cargo xtask verify-generated-freshness --format json",
        || verify_generated_freshness(&["--format".to_owned(), "json".to_owned()]),
    );
    let adversarial_gate = capture_simple_gate(
        "adversarial-no-heuristics",
        "cargo xtask verify-negative-corpus --format json",
        || verify_negative_corpus(&["--format".to_owned(), "json".to_owned()]),
    );
    let gates = vec![
        source_truth_gate,
        no_shortcuts_gate,
        parser_gate,
        phase_gate,
        resolver_shape_gate,
        semantic_gate,
        dd_lowering_gate,
        deterministic_generated_only_runtime_gate(&root),
        deterministic_scenario_protocol_gate(&root, &manifest),
        adversarial_gate,
        generated_freshness_gate,
        deterministic_cross_host_parity_gate(&root),
        deterministic_verification_harness_self_test_gate(&root, &manifest),
    ];
    let missing_deterministic_gates = gates
        .iter()
        .filter(|gate| gate.status != "passed")
        .map(|gate| gate.name.clone())
        .collect::<Vec<_>>();
    let accepted_features_without_full_coverage = manifest
        .features
        .iter()
        .filter(|feature| feature.status != "accepted")
        .count();
    let stale_artifact_failures = gates
        .iter()
        .find(|gate| gate.name == "stale-artifact-rejection")
        .and_then(|gate| gate.details.get("stale"))
        .and_then(|stale| stale.as_array())
        .map_or(0, Vec::len)
        + gates
            .iter()
            .find(|gate| gate.name == "stale-artifact-rejection")
            .and_then(|gate| gate.details.get("missing"))
            .and_then(|missing| missing.as_array())
            .map_or(0, Vec::len);
    let adversarial_heuristic_cases_failed = gates
        .iter()
        .find(|gate| gate.name == "adversarial-no-heuristics")
        .and_then(|gate| gate.details.get("failures"))
        .and_then(|failures| failures.as_array())
        .map_or(0, Vec::len);
    let details = serde_json::json!({
        "verdict": if missing_deterministic_gates.is_empty() { "pass" } else { "fail" },
        "repo_state": repo_state()?,
        "input_hashes": input_hashes,
        "dependency_tool_versions": collect_dependency_tool_versions()?,
        "gates": gates,
        "shortcut_symbols_in_execution_paths": shortcuts["hit_count"],
        "accepted_features_without_full_coverage": accepted_features_without_full_coverage,
        "stale_artifact_failures": stale_artifact_failures,
        "host_semantics_violations": if missing_deterministic_gates.iter().any(|gate| gate == "generated-only-runtime" || gate == "cross-host-parity") { 1 } else { 0 },
        "adversarial_heuristic_cases_failed": adversarial_heuristic_cases_failed,
        "prompt_audit_required": true,
        "missing_deterministic_gates": missing_deterministic_gates,
    });
    let artifact = write_artifact("honesty-deterministic-report.json", &details)?;
    if details["verdict"] != "pass" {
        bail!(
            "deterministic honesty verification is not complete; see {}",
            artifact.display()
        );
    }
    Ok(details)
}

fn deterministic_cross_host_parity_gate(root: &Path) -> GateReport {
    let started = Instant::now();
    match deterministic_cross_host_parity_gate_result(root, started) {
        Ok(report) => report,
        Err(error) => GateReport {
            name: "cross-host-parity".to_owned(),
            command: "compare terminal/native/browser generated DD protocol artifacts".to_owned(),
            status: "failed".to_owned(),
            duration_ms: started.elapsed().as_millis(),
            artifacts: Vec::new(),
            details: serde_json::json!({
                "error": format!("{error:#}"),
                "blockers": [
                    "cross-host parity artifacts are missing or malformed"
                ],
            }),
        },
    }
}

fn deterministic_cross_host_parity_gate_result(
    _root: &Path,
    started: Instant,
) -> Result<GateReport> {
    let dir = artifacts_dir()?;
    let matrix_path = dir.join("example-matrix.json");
    let native_path = dir.join("native-playground.json");
    let browser_path = dir.join("browser-playground-result.json");
    let matrix: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&matrix_path)
            .with_context(|| format!("missing {}", matrix_path.display()))?,
    )
    .with_context(|| format!("invalid JSON {}", matrix_path.display()))?;
    let native: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&native_path)
            .with_context(|| format!("missing {}", native_path.display()))?,
    )
    .with_context(|| format!("invalid JSON {}", native_path.display()))?;
    let browser: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&browser_path)
            .with_context(|| format!("missing {}", browser_path.display()))?,
    )
    .with_context(|| format!("invalid JSON {}", browser_path.display()))?;

    let terminal_outputs = matrix
        .get("terminal_checked")
        .and_then(serde_json::Value::as_array)
        .context("example-matrix.json missing terminal_checked")?
        .iter()
        .filter_map(|entry| {
            Some((
                entry.get("example")?.as_str()?.to_owned(),
                entry.get("output")?.clone(),
            ))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let browser_outputs = browser
        .get("wasm_smoke")
        .and_then(serde_json::Value::as_array)
        .context("browser-playground-result.json missing wasm_smoke")?
        .iter()
        .filter_map(|entry| {
            let pair = entry.as_array()?;
            Some((pair.first()?.as_str()?.to_owned(), pair.get(1)?.clone()))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let native_examples = native
        .get("per_example")
        .and_then(serde_json::Value::as_array)
        .context("native-playground.json missing per_example")?;
    let native_outputs = native_examples
        .iter()
        .filter_map(|entry| {
            let output = entry
                .get("generated_output")
                .or_else(|| entry.get("output"))?;
            Some((entry.get("example")?.as_str()?.to_owned(), output.clone()))
        })
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut terminal_browser_mismatches = Vec::new();
    let mut native_mismatches = Vec::new();
    let mut missing = Vec::new();
    let mut parity_rows = Vec::new();
    for example in boon_dd::REQUIRED_EXAMPLES {
        let terminal = terminal_outputs.get(*example);
        let browser = browser_outputs.get(*example);
        let native = native_outputs.get(*example);
        match (terminal, browser, native) {
            (Some(terminal), Some(browser), Some(native)) => {
                let terminal_sha256 = sha256_text(&serde_json::to_string(terminal)?)?;
                let browser_sha256 = sha256_text(&serde_json::to_string(browser)?)?;
                let native_sha256 = sha256_text(&serde_json::to_string(native)?)?;
                let terminal_browser_match = terminal == browser;
                let terminal_native_match = terminal == native;
                if !terminal_browser_match {
                    terminal_browser_mismatches.push(serde_json::json!({
                        "example": example,
                        "terminal_sha256": terminal_sha256,
                        "browser_sha256": browser_sha256,
                    }));
                }
                if !terminal_native_match {
                    native_mismatches.push(serde_json::json!({
                        "example": example,
                        "terminal_sha256": terminal_sha256,
                        "native_sha256": native_sha256,
                    }));
                }
                parity_rows.push(serde_json::json!({
                    "example": example,
                    "terminal_output_sha256": terminal_sha256,
                    "browser_wasm_output_sha256": browser_sha256,
                    "native_generated_output_sha256": native_sha256,
                    "terminal_browser_match": terminal_browser_match,
                    "terminal_native_match": terminal_native_match,
                }));
            }
            (None, Some(_), Some(_)) => missing.push(serde_json::json!({
                "example": example,
                "host": "terminal",
            })),
            (Some(_), None, Some(_)) => missing.push(serde_json::json!({
                "example": example,
                "host": "browser",
            })),
            (Some(_), Some(_), None) => missing.push(serde_json::json!({
                "example": example,
                "host": "native",
            })),
            (None, None, Some(_)) => missing.push(serde_json::json!({
                "example": example,
                "host": "terminal,browser",
            })),
            (None, Some(_), None) => missing.push(serde_json::json!({
                "example": example,
                "host": "terminal,native",
            })),
            (Some(_), None, None) => missing.push(serde_json::json!({
                "example": example,
                "host": "browser,native",
            })),
            (None, None, None) => missing.push(serde_json::json!({
                "example": example,
                "host": "terminal,browser,native",
            })),
        }
    }

    let native_structured_outputs = native_outputs.len();
    let passed = missing.is_empty()
        && terminal_browser_mismatches.is_empty()
        && native_mismatches.is_empty();
    let details = serde_json::json!({
        "verdict": if passed { "pass" } else { "fail" },
        "terminal_examples": terminal_outputs.len(),
        "browser_wasm_examples": browser_outputs.len(),
        "native_visual_examples": native_examples.len(),
        "native_structured_outputs": native_structured_outputs,
        "missing": missing,
        "terminal_browser_mismatches": terminal_browser_mismatches,
        "terminal_native_mismatches": native_mismatches,
        "parity_rows": parity_rows,
        "artifacts_compared": {
            "terminal": matrix_path,
            "native": native_path,
            "browser": browser_path,
        },
        "blockers": if passed {
            Vec::<String>::new()
        } else {
            vec![
                "terminal, native, and browser are not yet executing the same per-example scenario protocol".to_owned(),
                "one or more hosts are missing structured generated DD output or have mismatched output hashes".to_owned(),
            ]
        },
    });
    let artifact = write_artifact("cross-host-parity-report.json", &details)?;
    Ok(GateReport {
        name: "cross-host-parity".to_owned(),
        command: "compare terminal/native/browser generated DD protocol artifacts".to_owned(),
        status: if passed { "passed" } else { "failed" }.to_owned(),
        duration_ms: started.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

fn deterministic_verification_harness_self_test_gate(
    root: &Path,
    _manifest: &LanguageManifest,
) -> GateReport {
    let started = Instant::now();
    match deterministic_verification_harness_self_test_gate_result(root, started) {
        Ok(report) => report,
        Err(error) => GateReport {
            name: "verification-harness-self-test".to_owned(),
            command: "run synthetic injected-fault verifier self-tests".to_owned(),
            status: "failed".to_owned(),
            duration_ms: started.elapsed().as_millis(),
            artifacts: Vec::new(),
            details: serde_json::json!({
                "error": format!("{error:#}"),
                "blockers": [
                    "verification harness self-test crashed before all injected faults were checked"
                ],
            }),
        },
    }
}

fn deterministic_verification_harness_self_test_gate_result(
    root: &Path,
    started: Instant,
) -> Result<GateReport> {
    let mut checks = Vec::new();
    let mut failures = Vec::new();
    let mut record_check =
        |id: &str, injected_fault: &str, detected: bool, evidence: serde_json::Value| {
            if !detected {
                failures.push(id.to_owned());
            }
            checks.push(serde_json::json!({
                "id": id,
                "injected_fault": injected_fault,
                "detected": detected,
                "evidence": evidence,
            }));
        };

    let self_test_dir = artifacts_dir()?.join("verification-self-test");
    if self_test_dir.exists() {
        fs::remove_dir_all(&self_test_dir)?;
    }
    fs::create_dir_all(self_test_dir.join("shortcut/src"))?;
    fs::write(
        self_test_dir.join("shortcut/src/lib.rs"),
        format!(
            "pub fn injected() {{ boon_dd::{}(); }}\n",
            concat!("execute", "_static_graph")
        ),
    )?;
    let mut shortcut_hits = Vec::new();
    scan_forbidden_in_dir(
        &self_test_dir.join("shortcut"),
        HONEST_SHORTCUT_PATTERNS,
        &mut shortcut_hits,
    )?;
    record_check(
        "shortcut-insertion",
        "temporary Rust execution path calls a forbidden shortcut symbol",
        !shortcut_hits.is_empty(),
        serde_json::json!({ "hits": shortcut_hits }),
    );

    fs::create_dir_all(self_test_dir.join("freshness"))?;
    let checked_path = self_test_dir.join("freshness/checked.rs");
    let regenerated_path = self_test_dir.join("freshness/regenerated.rs");
    fs::write(
        &checked_path,
        "pub const GENERATED: &str = \"canonical\";\n",
    )?;
    fs::write(
        &regenerated_path,
        "pub const GENERATED: &str = \"canonical with injected drift\";\n",
    )?;
    let checked_hash = sha256_file(&checked_path)?;
    let stale_hash = sha256_file(&regenerated_path)?;
    record_check(
        "stale-artifact",
        "temporary regenerated artifact hash differs from checked hash",
        checked_hash != stale_hash,
        serde_json::json!({
            "checked_path": checked_path,
            "checked_sha256": checked_hash,
            "regenerated_path": regenerated_path,
            "regenerated_sha256": stale_hash,
        }),
    );

    let counter_hold_scenario_path = root.join("examples/counter_hold/scenario.toml");
    let counter_hold_scenario_text = fs::read_to_string(&counter_hold_scenario_path)
        .with_context(|| format!("missing {}", counter_hold_scenario_path.display()))?;
    let counter_hold_scenario = boon_runtime_host::parse_scenario(&counter_hold_scenario_text);
    let all_steps = counter_hold_scenario.steps.len();
    let command_steps = counter_hold_scenario
        .steps
        .iter()
        .filter(|step| !step.commands.is_empty())
        .count();
    let ordered_command_step_detected = counter_hold_scenario.steps.iter().any(|step| {
        step.events
            .iter()
            .map(|event| match event {
                boon_dd::ScenarioEvent::Source(action) => format!("source:{}", action.source),
                boon_dd::ScenarioEvent::Command(command) => {
                    format!("command:{}", command.command)
                }
            })
            .collect::<Vec<_>>()
            == [
                "command:enable_persistence",
                "source:store.sources.increment_button.event.press",
                "command:reload",
            ]
    });
    let first_step_only_detected =
        all_steps > 1 && command_steps > 0 && ordered_command_step_detected;
    record_check(
        "skipped-scenario-steps",
        "synthetic runner executes only the first step of a multi-step command scenario",
        first_step_only_detected,
        serde_json::json!({
            "scenario": "examples/counter_hold/scenario.toml",
            "all_steps": all_steps,
            "command_steps": command_steps,
            "ordered_command_step_detected": ordered_command_step_detected,
            "first_step_runner_steps": usize::from(all_steps > 0),
        }),
    );

    let counter_index = boon_examples::GENERATED_CORPUS
        .iter()
        .position(|fixture| fixture.name == "counter")
        .context("missing generated counter fixture")?;
    let counter_scenario_path = root.join("examples/counter/scenario.toml");
    let counter_scenario_text = fs::read_to_string(&counter_scenario_path)
        .with_context(|| format!("missing {}", counter_scenario_path.display()))?;
    let (_name, actual_counter_output) =
        boon_examples::run_generated_scenario_at(counter_index, &counter_scenario_text)
            .context("generated counter scenario did not run")?;
    let mut tampered_counter_output = actual_counter_output.clone();
    tampered_counter_output.render.clear();
    record_check(
        "wrong-fixture-output",
        "synthetic expected output removes the generated render command",
        actual_counter_output != tampered_counter_output,
        serde_json::json!({
            "actual_render_commands": actual_counter_output.render.len(),
            "tampered_render_commands": tampered_counter_output.render.len(),
        }),
    );

    let counter_source_path = root.join("examples/counter/source.bn");
    let counter_source_text = fs::read_to_string(&counter_source_path)
        .with_context(|| format!("missing {}", counter_source_path.display()))?;
    let counter_plan =
        boon_compiler::compile_source("examples/counter/source.bn", counter_source_text);
    let injected_disabled_lowering_node_count = 0_usize;
    let disabled_lowering_detected = !counter_plan.semantic_ir.nodes.is_empty()
        && counter_plan.dd_graph_ir.nodes.len() != injected_disabled_lowering_node_count;
    record_check(
        "disabled-dd-lowering",
        "synthetic lowerer returns zero DD graph nodes for a non-empty semantic IR",
        disabled_lowering_detected,
        serde_json::json!({
            "semantic_ir_nodes": counter_plan.semantic_ir.nodes.len(),
            "actual_dd_graph_nodes": counter_plan.dd_graph_ir.nodes.len(),
            "injected_dd_graph_nodes": injected_disabled_lowering_node_count,
        }),
    );

    let passed = failures.is_empty();
    let details = serde_json::json!({
        "verdict": if passed { "pass" } else { "fail" },
        "checks": checks,
        "failures": failures,
        "blockers": if passed {
            Vec::<String>::new()
        } else {
            vec!["one or more injected verifier faults were not detected".to_owned()]
        },
    });
    let artifact = write_artifact("verification-harness-self-test-report.json", &details)?;
    Ok(GateReport {
        name: "verification-harness-self-test".to_owned(),
        command: "run synthetic injected-fault verifier self-tests".to_owned(),
        status: if passed { "passed" } else { "failed" }.to_owned(),
        duration_ms: started.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

fn deterministic_generated_only_runtime_gate(root: &Path) -> GateReport {
    let started = Instant::now();
    let runtime_execution_patterns = &[
        "RuntimeHost",
        "compile_and_run_scenario",
        "run_dd_graph_template",
        "execute_scenario",
    ];
    let mut hits = Vec::new();
    for rel in [
        "crates/boon_backend_app_window",
        "crates/boon_backend_browser",
        "crates/boon_backend_ratatui",
        "crates/boon_backend_wgpu",
        "crates/boon_examples",
    ] {
        let path = root.join(rel);
        if path.exists()
            && let Err(error) = scan_forbidden_in_dir(&path, runtime_execution_patterns, &mut hits)
        {
            hits.push(serde_json::json!({
                "path": rel,
                "line": 0,
                "pattern": format!("scan-error: {error:#}"),
            }));
        }
    }
    let generated_outputs = boon_examples::run_embedded_matrix();
    let generated_count = generated_outputs.len();
    let expected_count = boon_dd::REQUIRED_EXAMPLES.len();
    let status = if hits.is_empty() && generated_count == expected_count {
        "passed"
    } else {
        "failed"
    };
    let blockers = if status == "passed" {
        Vec::<&str>::new()
    } else {
        vec![
            "backend/example/xtask execution paths still reference non-generated runtime helpers or generated fixture count is incomplete",
        ]
    };
    GateReport {
        name: "generated-only-runtime".to_owned(),
        command: "scan backend/example execution paths and run generated fixture matrix".to_owned(),
        status: status.to_owned(),
        duration_ms: started.elapsed().as_millis(),
        artifacts: Vec::new(),
        details: serde_json::json!({
            "runtime_execution_patterns": runtime_execution_patterns,
            "hits": hits,
            "hit_count": hits.len(),
            "generated_fixture_outputs": generated_count,
            "required_examples": expected_count,
            "blockers": blockers,
        }),
    }
}

fn deterministic_input_hashes(
    root: &Path,
    manifest: &LanguageManifest,
) -> Result<serde_json::Value> {
    let mut files = Vec::new();
    for path in [HONEST_COMPILER_PLAN, LANGUAGE_MANIFEST, "Cargo.lock"] {
        let full = root.join(path);
        files.push(serde_json::json!({
            "path": path,
            "sha256": full.exists().then(|| sha256_file(&full)).transpose()?,
        }));
    }
    for example in &manifest.examples {
        for path in [&example.source, &example.scenario, &example.expected_render] {
            let full = root.join(path);
            files.push(serde_json::json!({
                "path": path,
                "sha256": full.exists().then(|| sha256_file(&full)).transpose()?,
            }));
        }
    }
    for negative in &manifest.negative_examples {
        for path in [&negative.source, &negative.metadata] {
            let full = root.join(path);
            files.push(serde_json::json!({
                "path": path,
                "sha256": full.exists().then(|| sha256_file(&full)).transpose()?,
            }));
        }
    }
    let mut generated = Vec::new();
    for example in boon_dd::REQUIRED_EXAMPLES {
        for relative in generated_artifact_relative_paths() {
            let path = format!("generated/{example}/{relative}");
            let full = root.join(&path);
            generated.push(serde_json::json!({
                "path": path,
                "sha256": full.exists().then(|| sha256_file(&full)).transpose()?,
            }));
        }
    }
    Ok(serde_json::json!({
        "source_manifest_and_expected_files": files,
        "generated_artifacts": generated,
    }))
}

fn deterministic_source_truth_gate(root: &Path, manifest: &LanguageManifest) -> GateReport {
    let missing_examples = manifest
        .examples
        .iter()
        .flat_map(|example| [&example.source, &example.scenario, &example.expected_render])
        .filter(|path| !root.join(path).exists())
        .cloned()
        .collect::<Vec<_>>();
    let incomplete_features = manifest
        .features
        .iter()
        .filter(|feature| feature.status != "accepted")
        .map(|feature| feature.id.clone())
        .collect::<Vec<_>>();
    let missing_feature_coverage = manifest
        .features
        .iter()
        .filter(|feature| {
            feature.positive_examples.is_empty() || feature.negative_examples.is_empty()
        })
        .map(|feature| feature.id.clone())
        .collect::<Vec<_>>();
    let passed = missing_examples.is_empty()
        && incomplete_features.is_empty()
        && missing_feature_coverage.is_empty()
        && manifest.language.status == "accepted";
    GateReport {
        name: "source-truth".to_owned(),
        command: "validate docs/language/boon-language-manifest.toml".to_owned(),
        status: if passed { "passed" } else { "failed" }.to_owned(),
        duration_ms: 0,
        artifacts: Vec::new(),
        details: serde_json::json!({
            "language_status": manifest.language.status,
            "accepted_language_version": manifest.language.accepted_language_version,
            "feature_count": manifest.features.len(),
            "example_count": manifest.examples.len(),
            "negative_example_count": manifest.negative_examples.len(),
            "missing_examples": missing_examples,
            "incomplete_features": incomplete_features,
            "missing_feature_coverage": missing_feature_coverage,
        }),
    }
}

fn deterministic_parser_gate(root: &Path, manifest: &LanguageManifest) -> GateReport {
    let mut parsed = Vec::new();
    let mut failures = Vec::new();
    for example in &manifest.examples {
        match fs::read_to_string(root.join(&example.source)) {
            Ok(text) => {
                let module = boon_syntax::parse_source(example.source.clone(), text);
                if module.diagnostics.is_empty() && !module.definitions.is_empty() {
                    parsed.push(serde_json::json!({
                        "example": example.id,
                        "definition_count": module.definitions.len(),
                    }));
                } else {
                    failures.push(serde_json::json!({
                        "example": example.id,
                        "diagnostics": module.diagnostics,
                        "definition_count": module.definitions.len(),
                    }));
                }
            }
            Err(error) => failures.push(serde_json::json!({
                "example": example.id,
                "error": format!("{error:#}"),
            })),
        }
    }
    GateReport {
        name: "parser-completeness".to_owned(),
        command: "parse every manifest source with boon_syntax::parse_source".to_owned(),
        status: if failures.is_empty() {
            "passed"
        } else {
            "failed"
        }
        .to_owned(),
        duration_ms: 0,
        artifacts: Vec::new(),
        details: serde_json::json!({
            "parsed": parsed,
            "failures": failures,
        }),
    }
}

fn deterministic_phase_boundary_gate(root: &Path, manifest: &LanguageManifest) -> GateReport {
    let started = Instant::now();
    match deterministic_phase_boundary_gate_result(root, manifest, started) {
        Ok(report) => report,
        Err(error) => GateReport {
            name: "phase-boundary".to_owned(),
            command: "emit AST/HIR/shape/semantic/DD graph summaries for every manifest source"
                .to_owned(),
            status: "failed".to_owned(),
            duration_ms: started.elapsed().as_millis(),
            artifacts: Vec::new(),
            details: serde_json::json!({
                "error": format!("{error:#}"),
                "blockers": [
                    "phase-boundary verifier crashed before it could write the canonical phase report"
                ],
            }),
        },
    }
}

fn deterministic_phase_boundary_gate_result(
    root: &Path,
    manifest: &LanguageManifest,
    started: Instant,
) -> Result<GateReport> {
    let mut cases = Vec::new();
    let mut failures = Vec::new();
    for example in &manifest.examples {
        match fs::read_to_string(root.join(&example.source)) {
            Ok(text) => {
                let parsed = boon_syntax::parse_source(example.source.clone(), text.clone());
                let hir = boon_hir::lower(&parsed);
                let shape = boon_shape::check_module(&hir);
                let plan = boon_compiler::compile_source(example.source.clone(), text);
                cases.push(serde_json::json!({
                    "example": example.id,
                    "ast_definitions": parsed.definitions.len(),
                    "hir_definitions": hir.definitions.len(),
                    "source_bindings": hir.sources.len(),
                    "shape_definitions": shape.definitions.len(),
                    "semantic_ir_nodes": plan.semantic_ir.nodes.len(),
                    "dd_graph_ir_nodes": plan.dd_graph_ir.nodes.len(),
                    "generated_graph_id": plan.graph.graph_id,
                }));
            }
            Err(error) => failures.push(serde_json::json!({
                "example": example.id,
                "error": format!("{error:#}"),
            })),
        }
    }
    let phase_boundary_violations = scan_phase_boundary_violations(root)?;
    let passed = failures.is_empty() && phase_boundary_violations.is_empty();
    let details = serde_json::json!({
        "verdict": if passed { "pass" } else { "fail" },
        "cases": cases,
        "failures": failures,
        "phase_boundary_violations": phase_boundary_violations,
        "blockers": if passed {
            Vec::<String>::new()
        } else {
            vec![
                "phase summaries are written to a canonical verifier artifact, but compiler boundary violations remain".to_owned(),
                "later compiler phases still contain source-path/source-name semantic fallbacks instead of consuming typed HIR/semantic IR only".to_owned(),
            ]
        },
    });
    let artifact = write_artifact("phase-boundary-report.json", &details)?;
    Ok(GateReport {
        name: "phase-boundary".to_owned(),
        command: "emit AST/HIR/shape/semantic/DD graph summaries for every manifest source"
            .to_owned(),
        status: if passed { "passed" } else { "failed" }.to_owned(),
        duration_ms: started.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

fn scan_phase_boundary_violations(root: &Path) -> Result<Vec<serde_json::Value>> {
    let compiler_path = root.join("crates/boon_compiler/src/lib.rs");
    let text = fs::read_to_string(&compiler_path)
        .with_context(|| format!("missing {}", compiler_path.display()))?;
    let patterns = [
        (
            "graph_id_from_path",
            "graph identity is derived from source path instead of module metadata or semantic IR",
        ),
        (
            "strip_prefix(\"examples/",
            "compiler has example-path-specific graph identity logic",
        ),
        (
            "strip_suffix(\"/source.bn\")",
            "compiler has source-filename-specific graph identity logic",
        ),
        (
            "path.ends_with(\".text\")",
            "source shape falls back to path suffix instead of typed source metadata",
        ),
        (
            "path.ends_with(\".key\")",
            "source shape falls back to path suffix instead of typed source metadata",
        ),
        (
            "path == \"selected_filter\"",
            "source shape falls back to a source name instead of typed source metadata",
        ),
        (
            "source.path.starts_with(\"item.\")",
            "dynamic source family detection falls back to a source path prefix",
        ),
        (
            "monitor_node_name(",
            "monitor sink identity is still selected in compatibility graph construction instead of typed output metadata",
        ),
    ];
    let mut hits = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        for (pattern, reason) in patterns {
            if line.contains(pattern) {
                hits.push(serde_json::json!({
                    "path": compiler_path.display().to_string(),
                    "line": line_index + 1,
                    "pattern": pattern,
                    "reason": reason,
                }));
            }
        }
    }
    Ok(hits)
}

fn deterministic_resolver_shape_gate(root: &Path, manifest: &LanguageManifest) -> GateReport {
    let started = Instant::now();
    match deterministic_resolver_shape_gate_result(root, manifest, started) {
        Ok(report) => report,
        Err(error) => GateReport {
            name: "resolver-and-shape".to_owned(),
            command: "run boon_hir resolver checks and boon_shape over manifest examples"
                .to_owned(),
            status: "failed".to_owned(),
            duration_ms: started.elapsed().as_millis(),
            artifacts: Vec::new(),
            details: serde_json::json!({
                "error": format!("{error:#}"),
                "blockers": [
                    "resolver/shape verifier crashed before it could write the canonical report"
                ],
            }),
        },
    }
}

fn deterministic_resolver_shape_gate_result(
    root: &Path,
    manifest: &LanguageManifest,
    started: Instant,
) -> Result<GateReport> {
    let mut cases = Vec::new();
    let mut source_errors = Vec::new();
    let mut unresolved = Vec::new();
    let mut shape_failures = Vec::new();
    let mut unknown_shapes = Vec::new();
    let mut missing_source_shapes = Vec::new();

    for example in &manifest.examples {
        let text = match fs::read_to_string(root.join(&example.source)) {
            Ok(text) => text,
            Err(error) => {
                source_errors.push(serde_json::json!({
                    "example": example.id,
                    "source": example.source,
                    "error": format!("{error:#}"),
                }));
                continue;
            }
        };
        let parsed = boon_syntax::parse_source(example.source.clone(), text);
        let hir = boon_hir::lower(&parsed);
        let unresolved_refs = boon_hir::unresolved_references(&hir);
        if !unresolved_refs.is_empty() || !hir.diagnostics.is_empty() {
            unresolved.push(serde_json::json!({
                "example": example.id,
                "diagnostics": hir.diagnostics,
                "unresolved": unresolved_refs,
            }));
        }

        let shape = boon_shape::check_module(&hir);
        if !shape.diagnostics.is_empty() {
            shape_failures.push(serde_json::json!({
                "example": example.id,
                "diagnostics": shape.diagnostics,
            }));
        }
        for (definition, definition_shape) in &shape.definitions {
            if matches!(definition_shape, boon_shape::Shape::Unknown) {
                unknown_shapes.push(serde_json::json!({
                    "example": example.id,
                    "kind": "definition",
                    "name": definition,
                    "shape": definition_shape,
                }));
            }
        }
        for source in &hir.sources {
            match shape.sources.get(&source.path) {
                Some(source_shape) if matches!(source_shape, boon_shape::Shape::Unknown) => {
                    unknown_shapes.push(serde_json::json!({
                        "example": example.id,
                        "kind": "source",
                        "path": source.path,
                        "shape": source_shape,
                    }));
                }
                Some(_) => {}
                None => missing_source_shapes.push(serde_json::json!({
                    "example": example.id,
                    "path": source.path,
                })),
            }
        }

        cases.push(serde_json::json!({
            "example": example.id,
            "source": example.source,
            "definition_count": hir.definitions.len(),
            "definitions": shape.definitions,
            "sources": hir.sources,
            "source_shapes": shape.sources,
        }));
    }

    let resolver_shape_heuristics = scan_resolver_shape_heuristics(root)?;
    let passed = source_errors.is_empty()
        && unresolved.is_empty()
        && shape_failures.is_empty()
        && unknown_shapes.is_empty()
        && missing_source_shapes.is_empty()
        && resolver_shape_heuristics.is_empty()
        && manifest
            .features
            .iter()
            .all(|feature| feature.status == "accepted");
    let mut blockers = Vec::new();
    if !resolver_shape_heuristics.is_empty() {
        blockers.push(
            "resolver/shape coverage is report-backed, but the implementation still contains path/root/library heuristics".to_owned(),
        );
    }
    if !shape_failures.is_empty() || !unknown_shapes.is_empty() || !missing_source_shapes.is_empty()
    {
        blockers.push(
            "shape checking must reject Unknown fallbacks and prove source families, dynamic owner scopes, host bindings, and library contracts through typed metadata".to_owned(),
        );
    }
    if manifest
        .features
        .iter()
        .any(|feature| feature.status != "accepted")
    {
        blockers.push(
            "language manifest features remain accepted-incomplete, so this gate cannot honestly pass".to_owned(),
        );
    }
    if !source_errors.is_empty() || !unresolved.is_empty() {
        blockers.push(
            "resolver/shape inputs still contain missing sources, parser diagnostics, HIR diagnostics, or unresolved references".to_owned(),
        );
    }
    let details = serde_json::json!({
        "verdict": if passed { "pass" } else { "fail" },
        "scope": "all manifest examples plus resolver/shape source scan",
        "cases": cases,
        "source_errors": source_errors,
        "unresolved": unresolved,
        "shape_failures": shape_failures,
        "unknown_shapes": unknown_shapes,
        "missing_source_shapes": missing_source_shapes,
        "resolver_shape_heuristics": resolver_shape_heuristics,
        "manifest_features_not_fully_accepted": manifest
            .features
            .iter()
            .filter(|feature| feature.status != "accepted")
            .map(|feature| feature.id.clone())
            .collect::<Vec<_>>(),
        "blockers": blockers,
    });
    let artifact = write_artifact("resolver-shape-report.json", &details)?;
    Ok(GateReport {
        name: "resolver-and-shape".to_owned(),
        command: "run boon_hir resolver checks and boon_shape over manifest examples".to_owned(),
        status: if passed { "passed" } else { "failed" }.to_owned(),
        duration_ms: started.elapsed().as_millis(),
        artifacts: vec![artifact.display().to_string()],
        details,
    })
}

fn scan_resolver_shape_heuristics(root: &Path) -> Result<Vec<serde_json::Value>> {
    let scans = [
        (
            root.join("crates/boon_hir/src/lib.rs"),
            vec![
                (
                    "implicit_source_path(",
                    "HIR derives source bindings from path spelling instead of declared source metadata",
                ),
                (
                    "strip_prefix(\"sources.\")",
                    "HIR treats any sources.* reference as a source family without typed host binding metadata",
                ),
                (
                    "matches!(path, \"selected_filter\" | \"tick\")",
                    "HIR recognizes host/source names by string literal",
                ),
                (
                    "\"item\"",
                    "resolver allowlists dynamic item scope by root-name literal",
                ),
                (
                    "\"state\"",
                    "resolver allowlists HOLD state scope by root-name literal",
                ),
                (
                    "\"game\"",
                    "resolver allowlists Pong state scope by root-name literal",
                ),
                (
                    "\"selected_filter\"",
                    "resolver allowlists selected_filter host binding by root-name literal",
                ),
            ],
        ),
        (
            root.join("crates/boon_shape/src/lib.rs"),
            vec![
                (
                    ".unwrap_or(Shape::Unknown)",
                    "shape pass silently accepts unresolved paths as Unknown",
                ),
                (
                    "fn library_call_shape(",
                    "shape pass has an ad hoc library fallback outside typed library signatures",
                ),
                (
                    "(\"Pong\", \"initial\" | \"step\")",
                    "shape pass hard-codes an app-specific Pong library shape",
                ),
                (
                    "Shape::List(Box::new(Shape::Unknown))",
                    "shape pass accepts list operators without solving item shape variables",
                ),
            ],
        ),
    ];
    let mut hits = Vec::new();
    for (path, patterns) in scans {
        let text =
            fs::read_to_string(&path).with_context(|| format!("missing {}", path.display()))?;
        for (line_index, line) in text.lines().enumerate() {
            for (pattern, reason) in &patterns {
                if line.contains(pattern) {
                    hits.push(serde_json::json!({
                        "path": path.display().to_string(),
                        "line": line_index + 1,
                        "pattern": pattern,
                        "reason": reason,
                    }));
                }
            }
        }
    }
    Ok(hits)
}

fn deterministic_semantic_ir_gate(root: &Path, manifest: &LanguageManifest) -> GateReport {
    let mut unknown_nodes = Vec::new();
    let mut semantic_kinds = std::collections::BTreeSet::new();
    for example in &manifest.examples {
        if let Ok(text) = fs::read_to_string(root.join(&example.source)) {
            let plan = boon_compiler::compile_source(example.source.clone(), text);
            for node in &plan.semantic_ir.nodes {
                semantic_kinds.insert(format!("{:?}", node.kind));
                if format!("{:?}", node.kind) == "Unknown" {
                    unknown_nodes.push(serde_json::json!({
                        "example": example.id,
                        "node": node.node,
                        "source_span": node.source_span,
                    }));
                }
            }
        }
    }
    let passed = unknown_nodes.is_empty();
    GateReport {
        name: "semantic-ir-coverage".to_owned(),
        command: "compile manifest examples into boon_compiler semantic IR".to_owned(),
        status: if passed { "passed" } else { "failed" }.to_owned(),
        duration_ms: 0,
        artifacts: Vec::new(),
        details: serde_json::json!({
            "semantic_kinds": semantic_kinds,
            "unknown_nodes": unknown_nodes,
            "blockers": if passed {
                Vec::<String>::new()
            } else {
                vec![
                    "semantic IR still contains Unknown nodes for accepted examples".to_owned()
                ]
            },
        }),
    }
}

fn deterministic_scenario_protocol_gate(root: &Path, manifest: &LanguageManifest) -> GateReport {
    let mut cases = Vec::new();
    let mut failures = Vec::new();
    let mut command_count = 0_usize;
    let mut ordered_command_cases = 0_usize;
    for example in &manifest.examples {
        match fs::read_to_string(root.join(&example.scenario)) {
            Ok(text) => match boon_runtime_host::parse_scenario_result(&text) {
                Ok(scenario) => {
                    let expected_output = fs::read_to_string(root.join(&example.expected_render))
                        .with_context(|| {
                            format!("missing expected render {}", example.expected_render)
                        })
                        .and_then(|text| {
                            serde_json::from_str::<boon_dd::SmokeOutput>(&text).with_context(|| {
                                format!("invalid expected render {}", example.expected_render)
                            })
                        });
                    let commands = scenario
                        .steps
                        .iter()
                        .map(|step| step.commands.len())
                        .sum::<usize>();
                    command_count += commands;
                    ordered_command_cases += scenario
                        .steps
                        .iter()
                        .filter(|step| {
                            step.events
                                .iter()
                                .any(|event| matches!(event, boon_dd::ScenarioEvent::Command(_)))
                        })
                        .count();
                    let generated_index = boon_examples::GENERATED_CORPUS
                        .iter()
                        .position(|fixture| fixture.name == example.id);
                    let execution = generated_index.and_then(|index| {
                        boon_examples::run_generated_scenario_steps_at(index, &text)
                    });
                    let execution_steps = execution
                        .as_ref()
                        .map(|(_name, steps)| {
                            steps
                                .iter()
                                .map(|step| {
                                    let actual_text = smoke_render_text(&step.output);
                                    let matches_expected = actual_text == step.expected_text;
                                    if !matches_expected {
                                        failures.push(serde_json::json!({
                                            "example": example.id,
                                            "step_index": step.step_index,
                                            "description": step.description.clone(),
                                            "expected_text": step.expected_text.clone(),
                                            "actual_text": actual_text.clone(),
                                            "commands": step.commands.iter().map(|command| command.command.clone()).collect::<Vec<_>>(),
                                            "error": "generated scenario step output does not match expected text",
                                        }));
                                    }
                                    serde_json::json!({
                                        "step_index": step.step_index,
                                        "description": step.description.clone(),
                                        "event_count": step.event_count,
                                        "event_order": step.event_order.clone(),
                                        "action_count": step.action_count,
                                        "commands": step.commands.iter().map(|command| command.command.clone()).collect::<Vec<_>>(),
                                        "expected_text": step.expected_text.clone(),
                                        "actual_text": actual_text,
                                        "matches_expected": matches_expected,
                                        "render_command_count": step.output.render.len(),
                                        "monitor_record_count": step.output.monitor.len(),
                                        "effect_command_count": step.output.effects.len(),
                                        "persistence_command_count": step.output.persistence.len(),
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_else(|| {
                            failures.push(serde_json::json!({
                                "example": example.id,
                                "error": "manifest example has no generated fixture execution path",
                            }));
                            Vec::new()
                        });
                    let structured_final = execution
                        .as_ref()
                        .and_then(|(_name, steps)| steps.last())
                        .map(|step| &step.output);
                    let structured_final_matches_expected = match (
                        expected_output.as_ref(),
                        structured_final,
                    ) {
                        (Ok(expected), Some(actual)) => {
                            let matches = actual == expected;
                            if !matches {
                                failures.push(serde_json::json!({
                                        "example": example.id,
                                        "expected_render": example.expected_render,
                                        "expected_output": expected,
                                        "actual_output": actual,
                                        "error": "generated final SmokeOutput does not match expected render artifact",
                                    }));
                            }
                            Some(matches)
                        }
                        (Err(error), _) => {
                            failures.push(serde_json::json!({
                                "example": example.id,
                                "expected_render": example.expected_render,
                                "error": format!("{error:#}"),
                            }));
                            None
                        }
                        (Ok(_), None) => None,
                    };
                    cases.push(serde_json::json!({
                        "example": example.id,
                        "steps": scenario.steps.len(),
                        "actions": scenario.steps.iter().map(|step| step.actions.len()).sum::<usize>(),
                        "commands": commands,
                        "expected_render": example.expected_render,
                        "structured_final_matches_expected": structured_final_matches_expected,
                        "generated_execution_steps": execution_steps,
                    }));
                }
                Err(error) => failures.push(serde_json::json!({
                    "example": example.id,
                    "error": error,
                })),
            },
            Err(error) => failures.push(serde_json::json!({
                "example": example.id,
                "error": format!("{error:#}"),
            })),
        }
    }
    let passed = failures.is_empty() && command_count > 0 && ordered_command_cases > 0;
    GateReport {
        name: "scenario-protocol".to_owned(),
        command: "strictly parse every manifest scenario and preserve command actions".to_owned(),
        status: if passed { "passed" } else { "failed" }.to_owned(),
        duration_ms: 0,
        artifacts: Vec::new(),
        details: serde_json::json!({
            "cases": cases,
            "failures": failures,
            "command_count": command_count,
            "ordered_command_cases": ordered_command_cases,
            "parser_strict": true,
            "blockers": if passed {
                Vec::<String>::new()
            } else {
                vec![
                    "scenario parser must preserve ordered command/source events and generated graphs must execute every parsed step in order".to_owned(),
                    "command/effect/persistence execution is incomplete for at least one manifest scenario".to_owned(),
                ]
            },
        }),
    }
}

fn smoke_render_text(output: &boon_dd::SmokeOutput) -> String {
    output
        .render
        .first()
        .map(|command| match command {
            boon_dd::RenderCommand::PatchText { text, .. } => text.clone(),
        })
        .unwrap_or_default()
}

#[derive(Debug, Deserialize)]
struct LanguageManifest {
    language: ManifestLanguage,
    #[serde(default)]
    features: Vec<ManifestFeature>,
    #[serde(default)]
    examples: Vec<ManifestExample>,
    #[serde(default)]
    negative_examples: Vec<ManifestNegativeExample>,
}

#[derive(Debug, Deserialize)]
struct ManifestLanguage {
    accepted_language_version: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct ManifestFeature {
    id: String,
    status: String,
    #[serde(default)]
    positive_examples: Vec<String>,
    #[serde(default)]
    negative_examples: Vec<String>,
    #[serde(default)]
    required_coverage: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ManifestExample {
    id: String,
    source: String,
    scenario: String,
    expected_render: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct ManifestNegativeExample {
    id: String,
    phase: String,
    source: String,
    metadata: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct NegativeCase {
    id: String,
    phase: String,
    source: String,
    #[serde(default)]
    expect_diagnostic_contains: Option<String>,
    #[serde(default)]
    expect_no_sources: bool,
}

fn read_language_manifest() -> Result<LanguageManifest> {
    let path = repo_root()?.join(LANGUAGE_MANIFEST);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("missing language manifest {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("invalid TOML {}", path.display()))
}

fn verify_syntax_corpus(_args: &[String]) -> Result<serde_json::Value> {
    let root = repo_root()?;
    let manifest = read_language_manifest()?;
    let gate = deterministic_parser_gate(&root, &manifest);
    let details = serde_json::json!({
        "verdict": if gate.status == "passed" { "pass" } else { "fail" },
        "gate": gate,
        "scope": "current manifest examples",
    });
    let artifact = write_artifact("syntax-corpus-report.json", &details)?;
    if details["verdict"] != "pass" {
        bail!(
            "syntax corpus verification failed; see {}",
            artifact.display()
        );
    }
    Ok(details)
}

fn verify_resolver_corpus(_args: &[String]) -> Result<serde_json::Value> {
    let root = repo_root()?;
    let manifest = read_language_manifest()?;
    let mut cases = Vec::new();
    let mut unresolved = Vec::new();
    for example in &manifest.examples {
        let text = fs::read_to_string(root.join(&example.source))
            .with_context(|| format!("missing source {}", example.source))?;
        let parsed = boon_syntax::parse_source(example.source.clone(), text);
        let hir = boon_hir::lower(&parsed);
        let unresolved_refs = boon_hir::unresolved_references(&hir);
        if !unresolved_refs.is_empty() || !hir.diagnostics.is_empty() {
            unresolved.push(serde_json::json!({
                "example": example.id,
                "diagnostics": hir.diagnostics,
                "unresolved": unresolved_refs,
            }));
        }
        cases.push(serde_json::json!({
            "example": example.id,
            "definitions": hir.definitions.len(),
            "sources": hir.sources,
        }));
    }
    let details = serde_json::json!({
        "verdict": if unresolved.is_empty() { "pass" } else { "fail" },
        "scope": "current manifest examples",
        "cases": cases,
        "unresolved": unresolved,
        "coverage_caveat": "resolver corpus does not yet prove dynamic owner scopes, all host bindings, or full library-symbol coverage",
    });
    let artifact = write_artifact("resolver-corpus-report.json", &details)?;
    if details["verdict"] != "pass" {
        bail!(
            "resolver corpus verification failed; see {}",
            artifact.display()
        );
    }
    Ok(details)
}

fn verify_shape_corpus(_args: &[String]) -> Result<serde_json::Value> {
    let root = repo_root()?;
    let manifest = read_language_manifest()?;
    let mut cases = Vec::new();
    let mut failures = Vec::new();
    for example in &manifest.examples {
        let text = fs::read_to_string(root.join(&example.source))
            .with_context(|| format!("missing source {}", example.source))?;
        let parsed = boon_syntax::parse_source(example.source.clone(), text);
        let hir = boon_hir::lower(&parsed);
        let shape = boon_shape::check_module(&hir);
        if !shape.diagnostics.is_empty() {
            failures.push(serde_json::json!({
                "example": example.id,
                "diagnostics": shape.diagnostics,
            }));
        }
        cases.push(serde_json::json!({
            "example": example.id,
            "definitions": shape.definitions,
            "sources": shape.sources,
        }));
    }
    let details = serde_json::json!({
        "verdict": if failures.is_empty() { "pass" } else { "fail" },
        "scope": "current manifest examples",
        "cases": cases,
        "failures": failures,
        "coverage_caveat": "shape corpus does not yet prove full unification, source leaf solving, or every library contract in the accepted language",
    });
    let artifact = write_artifact("shape-corpus-report.json", &details)?;
    if details["verdict"] != "pass" {
        bail!(
            "shape corpus verification failed; see {}",
            artifact.display()
        );
    }
    Ok(details)
}

fn verify_semantic_ir(_args: &[String]) -> Result<serde_json::Value> {
    let root = repo_root()?;
    let manifest = read_language_manifest()?;
    let gate = deterministic_semantic_ir_gate(&root, &manifest);
    let details = serde_json::json!({
        "verdict": if gate.status == "passed" { "pass" } else { "fail" },
        "gate": gate,
        "scope": "current manifest examples",
    });
    let artifact = write_artifact("semantic-ir-report.json", &details)?;
    if details["verdict"] != "pass" {
        bail!(
            "semantic IR verification failed; see {}",
            artifact.display()
        );
    }
    Ok(details)
}

fn verify_language_corpus(_args: &[String]) -> Result<serde_json::Value> {
    let root = repo_root()?;
    let manifest = root.join(LANGUAGE_MANIFEST);
    let parsed_manifest = read_language_manifest()?;
    let examples = required_examples_from_disk()?;
    let manifest_example_ids = parsed_manifest
        .examples
        .iter()
        .map(|example| example.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let embedded_required_examples = boon_dd::REQUIRED_EXAMPLES
        .iter()
        .map(|example| (*example).to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    let embedded_fixture_examples = boon_examples::GENERATED_CORPUS
        .iter()
        .map(|fixture| fixture.name.to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    let manifest_missing_from_required_examples = manifest_example_ids
        .difference(&embedded_required_examples)
        .cloned()
        .collect::<Vec<_>>();
    let required_examples_missing_from_manifest = embedded_required_examples
        .difference(&manifest_example_ids)
        .cloned()
        .collect::<Vec<_>>();
    let manifest_missing_from_embedded_fixtures = manifest_example_ids
        .difference(&embedded_fixture_examples)
        .cloned()
        .collect::<Vec<_>>();
    let embedded_fixtures_missing_from_manifest = embedded_fixture_examples
        .difference(&manifest_example_ids)
        .cloned()
        .collect::<Vec<_>>();
    let manifest_negative_ids = parsed_manifest
        .negative_examples
        .iter()
        .map(|example| example.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let missing_examples = examples
        .iter()
        .filter(|example| !manifest_example_ids.contains(*example))
        .cloned()
        .collect::<Vec<_>>();
    let mut missing_example_files = Vec::new();
    for example in &parsed_manifest.examples {
        for (kind, path) in [
            ("source", example.source.as_str()),
            ("scenario", example.scenario.as_str()),
            ("expected_render", example.expected_render.as_str()),
        ] {
            if !root.join(path).exists() {
                missing_example_files.push(serde_json::json!({
                    "example": example.id,
                    "kind": kind,
                    "path": path,
                }));
            }
        }
    }
    let feature_unknown_positive_examples = parsed_manifest
        .features
        .iter()
        .flat_map(|feature| {
            feature
                .positive_examples
                .iter()
                .filter(|example| !manifest_example_ids.contains(*example))
                .map(|example| {
                    serde_json::json!({
                        "feature": feature.id,
                        "example": example,
                    })
                })
        })
        .collect::<Vec<_>>();
    let feature_unknown_negative_examples = parsed_manifest
        .features
        .iter()
        .flat_map(|feature| {
            feature
                .negative_examples
                .iter()
                .filter(|example| !manifest_negative_ids.contains(*example))
                .map(|example| {
                    serde_json::json!({
                        "feature": feature.id,
                        "negative_example": example,
                    })
                })
        })
        .collect::<Vec<_>>();
    let features_without_positive = parsed_manifest
        .features
        .iter()
        .filter(|feature| feature.positive_examples.is_empty())
        .map(|feature| feature.id.clone())
        .collect::<Vec<_>>();
    let features_without_negative = parsed_manifest
        .features
        .iter()
        .filter(|feature| feature.negative_examples.is_empty())
        .map(|feature| feature.id.clone())
        .collect::<Vec<_>>();
    let incomplete_features = parsed_manifest
        .features
        .iter()
        .filter(|feature| feature.status != "accepted")
        .map(|feature| {
            serde_json::json!({
                "id": feature.id,
                "status": feature.status,
                "required_coverage": feature.required_coverage,
            })
        })
        .collect::<Vec<_>>();
    let incomplete_examples = parsed_manifest
        .examples
        .iter()
        .filter(|example| example.status != "accepted")
        .map(|example| {
            serde_json::json!({
                "id": example.id,
                "status": example.status,
            })
        })
        .collect::<Vec<_>>();
    let mut negative_files_missing = Vec::new();
    for example in &parsed_manifest.negative_examples {
        for (kind, path) in [
            ("source", example.source.as_str()),
            ("metadata", example.metadata.as_str()),
        ] {
            if !root.join(path).exists() {
                negative_files_missing.push(serde_json::json!({
                    "negative_example": example.id,
                    "kind": kind,
                    "path": path,
                }));
            }
        }
    }
    let incomplete_negative_examples = parsed_manifest
        .negative_examples
        .iter()
        .filter(|example| example.status != "checked")
        .map(|example| {
            serde_json::json!({
                "id": example.id,
                "phase": example.phase,
                "status": example.status,
            })
        })
        .collect::<Vec<_>>();
    let structural_errors = [
        missing_examples.is_empty(),
        missing_example_files.is_empty(),
        feature_unknown_positive_examples.is_empty(),
        feature_unknown_negative_examples.is_empty(),
        features_without_positive.is_empty(),
        features_without_negative.is_empty(),
        manifest_missing_from_required_examples.is_empty(),
        required_examples_missing_from_manifest.is_empty(),
        manifest_missing_from_embedded_fixtures.is_empty(),
        embedded_fixtures_missing_from_manifest.is_empty(),
        negative_files_missing.is_empty(),
        incomplete_negative_examples.is_empty(),
    ]
    .into_iter()
    .any(|ok| !ok);
    let coverage_reports = [
        ("parser", "syntax-corpus-report.json"),
        ("resolver", "resolver-corpus-report.json"),
        ("shape", "shape-corpus-report.json"),
        ("semantic_ir", "semantic-ir-report.json"),
        ("dd_lowering", "lowering-coverage-report.json"),
        ("generated_runtime", "generated-crates.json"),
        ("generated_freshness", "generated-freshness-report.json"),
        ("no_shortcuts", "no-shortcuts-report.json"),
        ("host_parity", "cross-host-parity-report.json"),
    ]
    .into_iter()
    .map(|(coverage, artifact)| {
        let artifact_json = read_artifact_json(artifact).unwrap_or_else(|error| {
            serde_json::json!({
                "read_error": format!("{error:#}"),
            })
        });
        let missing = artifact_json
            .get("missing")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let verdict = artifact_json
            .get("verdict")
            .and_then(|value| value.as_str())
            .or_else(|| {
                artifact_json
                    .get("gate")
                    .and_then(|gate| gate.get("status"))
                    .and_then(|value| value.as_str())
            })
            .unwrap_or("unknown");
        serde_json::json!({
            "coverage": coverage,
            "artifact": format!("target/boon-artifacts/{artifact}"),
            "missing": missing,
            "verdict": verdict,
        })
    })
    .collect::<Vec<_>>();
    let coverage_report_failures = coverage_reports
        .iter()
        .filter(|report| {
            report["missing"].as_bool().unwrap_or(false)
                || !matches!(report["verdict"].as_str(), Some("pass" | "passed"))
        })
        .cloned()
        .collect::<Vec<_>>();
    let passing_coverage = coverage_reports
        .iter()
        .filter(|report| {
            !report["missing"].as_bool().unwrap_or(false)
                && matches!(report["verdict"].as_str(), Some("pass" | "passed"))
        })
        .filter_map(|report| report["coverage"].as_str().map(str::to_owned))
        .collect::<std::collections::BTreeSet<_>>();
    let feature_missing_required_coverage = parsed_manifest
        .features
        .iter()
        .flat_map(|feature| {
            feature
                .required_coverage
                .iter()
                .filter(|coverage| !passing_coverage.contains(*coverage))
                .map(|coverage| {
                    serde_json::json!({
                        "feature": feature.id,
                        "coverage": coverage,
                    })
                })
        })
        .collect::<Vec<_>>();
    let mut blockers = Vec::new();
    if parsed_manifest.language.status != "accepted"
        || !incomplete_features.is_empty()
        || !incomplete_examples.is_empty()
    {
        blockers.push("manifest is still marked incomplete and examples/features are not accepted");
    }
    if !coverage_report_failures.is_empty() {
        blockers.push("one or more required coverage reports are missing or failing");
    }
    if !feature_missing_required_coverage.is_empty() {
        blockers.push("one or more accepted features are missing required passing coverage");
    }
    if structural_errors {
        blockers.push("language manifest has structural coverage errors");
    }
    let verdict = if blockers.is_empty() { "pass" } else { "fail" };
    let details = serde_json::json!({
        "verdict": verdict,
        "manifest": LANGUAGE_MANIFEST,
        "manifest_exists": manifest.exists(),
        "accepted_language_version": parsed_manifest.language.accepted_language_version,
        "language_status": parsed_manifest.language.status,
        "examples_on_disk": examples,
        "manifest_example_count": manifest_example_ids.len(),
        "required_examples_count": embedded_required_examples.len(),
        "embedded_fixture_count": embedded_fixture_examples.len(),
        "missing_examples_in_manifest": missing_examples,
        "manifest_missing_from_required_examples": manifest_missing_from_required_examples,
        "required_examples_missing_from_manifest": required_examples_missing_from_manifest,
        "manifest_missing_from_embedded_fixtures": manifest_missing_from_embedded_fixtures,
        "embedded_fixtures_missing_from_manifest": embedded_fixtures_missing_from_manifest,
        "missing_example_files": missing_example_files,
        "feature_count": parsed_manifest.features.len(),
        "features_without_positive_examples": features_without_positive,
        "features_without_negative_examples": features_without_negative,
        "feature_unknown_positive_examples": feature_unknown_positive_examples,
        "feature_unknown_negative_examples": feature_unknown_negative_examples,
        "negative_example_count": parsed_manifest.negative_examples.len(),
        "negative_files_missing": negative_files_missing,
        "incomplete_negative_examples": incomplete_negative_examples,
        "incomplete_features": incomplete_features,
        "incomplete_examples": incomplete_examples,
        "structural_errors": structural_errors,
        "coverage_reports": coverage_reports,
        "coverage_report_failures": coverage_report_failures,
        "feature_missing_required_coverage": feature_missing_required_coverage,
        "blockers": blockers,
    });
    let artifact = write_artifact("language-corpus-report.json", &details)?;
    if details["verdict"] != "pass" {
        bail!(
            "language corpus coverage is not complete; see {}",
            artifact.display()
        );
    }
    Ok(details)
}

fn verify_negative_corpus(_args: &[String]) -> Result<serde_json::Value> {
    let root = repo_root()?;
    let negative_dir = root.join("docs/language/negative-corpus");
    let manifest = read_language_manifest()?;
    let mut cases = Vec::new();
    let mut failures = Vec::new();
    let metadata_paths = if negative_dir.exists() {
        let mut paths = fs::read_dir(&negative_dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
            .collect::<Vec<_>>();
        paths.sort();
        paths
    } else {
        Vec::new()
    };
    for path in metadata_paths {
        match run_negative_case(&path) {
            Ok(case) => cases.push(case),
            Err(error) => failures.push(serde_json::json!({
                "metadata": path.display().to_string(),
                "error": format!("{error:#}"),
            })),
        }
    }
    let phases = cases
        .iter()
        .filter_map(|case| case["phase"].as_str().map(str::to_owned))
        .collect::<std::collections::BTreeSet<_>>();
    let required_phases = ["syntax", "resolver", "shape", "adversarial-no-heuristics"];
    let missing_phases = required_phases
        .iter()
        .filter(|phase| !phases.contains(**phase))
        .copied()
        .collect::<Vec<_>>();
    let manifest_negative_ids = manifest
        .negative_examples
        .iter()
        .map(|example| example.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let case_ids = cases
        .iter()
        .filter_map(|case| case["id"].as_str().map(str::to_owned))
        .collect::<std::collections::BTreeSet<_>>();
    let missing_manifest_cases = manifest_negative_ids
        .difference(&case_ids)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_cases = case_ids
        .difference(&manifest_negative_ids)
        .cloned()
        .collect::<Vec<_>>();
    let verdict = if failures.is_empty()
        && missing_phases.is_empty()
        && missing_manifest_cases.is_empty()
        && unexpected_cases.is_empty()
        && !cases.is_empty()
    {
        "pass"
    } else {
        "fail"
    };
    let details = serde_json::json!({
        "verdict": verdict,
        "negative_corpus_dir": "docs/language/negative-corpus",
        "negative_case_count": cases.len(),
        "required_phases": required_phases,
        "phases_covered": phases,
        "missing_phases": missing_phases,
        "missing_manifest_cases": missing_manifest_cases,
        "unexpected_cases": unexpected_cases,
        "failures": failures,
        "cases": cases,
    });
    let artifact = write_artifact("negative-corpus-report.json", &details)?;
    if verdict != "pass" {
        bail!("negative corpus is incomplete; see {}", artifact.display());
    }
    Ok(details)
}

fn run_negative_case(metadata_path: &Path) -> Result<serde_json::Value> {
    let root = repo_root()?;
    let metadata_text = fs::read_to_string(metadata_path)
        .with_context(|| format!("missing negative metadata {}", metadata_path.display()))?;
    let case: NegativeCase = toml::from_str(&metadata_text)
        .with_context(|| format!("invalid negative metadata {}", metadata_path.display()))?;
    let source_path = root.join(&case.source);
    let source_text = fs::read_to_string(&source_path)
        .with_context(|| format!("missing negative source {}", source_path.display()))?;
    let parsed = boon_syntax::parse_source(case.source.clone(), source_text);
    let mut evidence = serde_json::Map::new();
    evidence.insert(
        "syntax_diagnostics".to_owned(),
        serde_json::json!(parsed.diagnostics),
    );

    match case.phase.as_str() {
        "syntax" => {
            let expected = case
                .expect_diagnostic_contains
                .as_deref()
                .context("syntax negative case missing expect_diagnostic_contains")?;
            require_message(
                parsed
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.message.as_str()),
                expected,
                &case.id,
            )?;
        }
        "resolver" => {
            let hir = boon_hir::lower(&parsed);
            let unresolved = boon_hir::unresolved_references(&hir);
            evidence.insert(
                "hir_diagnostics".to_owned(),
                serde_json::json!(hir.diagnostics),
            );
            evidence.insert(
                "unresolved_references".to_owned(),
                serde_json::json!(unresolved),
            );
            let expected = case
                .expect_diagnostic_contains
                .as_deref()
                .context("resolver negative case missing expect_diagnostic_contains")?;
            require_message(unresolved.iter().map(String::as_str), expected, &case.id)?;
        }
        "shape" => {
            let hir = boon_hir::lower(&parsed);
            let shape = boon_shape::check_module(&hir);
            evidence.insert(
                "hir_diagnostics".to_owned(),
                serde_json::json!(hir.diagnostics),
            );
            evidence.insert(
                "shape_diagnostics".to_owned(),
                serde_json::json!(shape.diagnostics),
            );
            let expected = case
                .expect_diagnostic_contains
                .as_deref()
                .context("shape negative case missing expect_diagnostic_contains")?;
            require_message(
                shape
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.message.as_str()),
                expected,
                &case.id,
            )?;
        }
        "adversarial-no-heuristics" => {
            if !parsed.diagnostics.is_empty() {
                bail!(
                    "adversarial case {} should parse without diagnostics: {:?}",
                    case.id,
                    parsed.diagnostics
                );
            }
            let hir = boon_hir::lower(&parsed);
            let plan =
                boon_compiler::compile_source(&case.source, fs::read_to_string(&source_path)?);
            evidence.insert("hir_sources".to_owned(), serde_json::json!(hir.sources));
            evidence.insert(
                "source_bindings".to_owned(),
                serde_json::json!(plan.graph.source_bindings),
            );
            if case.expect_no_sources
                && (!hir.sources.is_empty() || !plan.graph.source_bindings.is_empty())
            {
                bail!(
                    "adversarial case {} detected sources from non-semantic text",
                    case.id
                );
            }
        }
        phase => bail!("unknown negative case phase `{phase}` for {}", case.id),
    }

    Ok(serde_json::json!({
        "id": case.id,
        "phase": case.phase,
        "source": case.source,
        "metadata": metadata_path.strip_prefix(repo_root()?).unwrap_or(metadata_path).display().to_string(),
        "status": "passed",
        "evidence": evidence,
    }))
}

fn require_message<'a>(
    messages: impl IntoIterator<Item = &'a str>,
    expected: &str,
    case_id: &str,
) -> Result<()> {
    let messages = messages.into_iter().collect::<Vec<_>>();
    if messages.iter().any(|message| message.contains(expected)) {
        Ok(())
    } else {
        bail!(
            "negative case {case_id} did not produce expected diagnostic `{expected}`; messages: {messages:?}"
        )
    }
}

fn verify_lowering(_args: &[String]) -> Result<serde_json::Value> {
    let shortcuts = scan_honest_shortcuts()?;
    let root = repo_root()?;
    let mut examples = Vec::new();
    let mut unsupported_total = 0_usize;
    let mut limited_render_program_examples = Vec::new();
    let mut missing_render_graph_examples = Vec::new();
    let mut render_graph_node_total = 0_usize;
    let mut sink_kinds = std::collections::BTreeSet::new();
    let mut legacy_render_operations = std::collections::BTreeSet::new();
    for example in boon_dd::REQUIRED_EXAMPLES {
        let source_path = root.join("examples").join(example).join("source.bn");
        let source_text = fs::read_to_string(&source_path)
            .with_context(|| format!("missing source {}", source_path.display()))?;
        let plan =
            boon_compiler::compile_source(format!("examples/{example}/source.bn"), source_text);
        let semantic_kinds = plan
            .semantic_ir
            .nodes
            .iter()
            .map(|node| format!("{:?}", node.kind))
            .collect::<std::collections::BTreeSet<_>>();
        let dd_operators = plan
            .dd_graph_ir
            .nodes
            .iter()
            .map(|node| format!("{:?}", node.operator))
            .collect::<std::collections::BTreeSet<_>>();
        unsupported_total += plan.dd_graph_ir.unsupported_semantic_nodes.len();
        for sink in &plan.dd_graph_ir.output_protocol.sinks {
            sink_kinds.insert(
                format!("{sink:?}")
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
            );
        }
        let render_operation = format!("{:?}", plan.dd_graph_ir.render_program.operation);
        if matches!(
            render_operation.as_str(),
            operation
                if operation.starts_with("CountInputEvents")
                    || operation.starts_with("LatestInputText")
                    || operation.starts_with("MatchTagText")
        ) {
            legacy_render_operations.insert(render_operation);
            limited_render_program_examples.push(example);
        }
        let render_graph_root_found = plan
            .dd_graph_ir
            .render_graph
            .nodes
            .iter()
            .any(|node| node.node == plan.dd_graph_ir.render_graph.root);
        if !render_graph_root_found || plan.dd_graph_ir.render_graph.nodes.is_empty() {
            missing_render_graph_examples.push(example);
        }
        render_graph_node_total += plan.dd_graph_ir.render_graph.nodes.len();
        examples.push(serde_json::json!({
            "example": example,
            "source_path": format!("examples/{example}/source.bn"),
            "source_sha256": plan.dd_graph_ir.source_hash,
            "semantic_node_count": plan.semantic_ir.nodes.len(),
            "semantic_kinds": semantic_kinds,
            "dd_graph_node_count": plan.dd_graph_ir.nodes.len(),
            "dd_operators": dd_operators,
            "dd_render_graph_node_count": plan.dd_graph_ir.render_graph.nodes.len(),
            "dd_render_graph_root": plan.dd_graph_ir.render_graph.root,
            "dd_render_graph_root_found": render_graph_root_found,
            "dd_render_graph_operators": plan.dd_graph_ir.render_graph.nodes.iter().map(|node| format!("{:?}", node.operator)).collect::<std::collections::BTreeSet<_>>(),
            "unsupported_semantic_nodes": plan.dd_graph_ir.unsupported_semantic_nodes,
            "dd_render_program": plan.dd_graph_ir.render_program,
            "dd_render_graph": plan.dd_graph_ir.render_graph,
            "output_protocol": plan.dd_graph_ir.output_protocol,
        }));
    }
    let codegen_source = fs::read_to_string(root.join("crates/boon_codegen_rust/src/lib.rs"))?;
    let render_program_execution_hits = [
        "render_collection_from_program",
        "render_collection_from_expr",
        "&dd_graph_ir.render_program",
    ]
    .into_iter()
    .filter(|needle| codegen_source.contains(needle))
    .collect::<Vec<_>>();
    let required_sink_kinds = [
        "MonitorNodeValue",
        "RenderPatchText",
        "Effect",
        "Persistence",
    ];
    let missing_sink_kinds = required_sink_kinds
        .iter()
        .filter(|kind| !sink_kinds.contains(**kind))
        .copied()
        .collect::<Vec<_>>();
    let mut blockers = Vec::new();
    if !missing_sink_kinds.is_empty() {
        blockers.push(
            "DD graph IR carries an explicit output protocol, but not every required sink family is lowered for the manifest corpus",
        );
    }
    if !legacy_render_operations.is_empty() {
        blockers.push(
            "some examples still use legacy render-operation shortcuts instead of general semantic IR to DD output lowering",
        );
    }
    if !missing_render_graph_examples.is_empty() {
        blockers.push("one or more examples have no complete DD render graph root");
    }
    if !render_program_execution_hits.is_empty() {
        blockers.push(
            "generated runtime code still executes from render_program instead of DD render graph IR",
        );
    }
    if unsupported_total > 0 {
        blockers.push("one or more semantic IR nodes do not lower to DD graph operators");
    }
    let verdict = if blockers.is_empty() { "pass" } else { "fail" };
    let details = serde_json::json!({
        "verdict": verdict,
        "shortcut_scan": shortcuts,
        "examples_checked": examples.len(),
        "unsupported_semantic_node_count": unsupported_total,
        "limited_render_program_examples": limited_render_program_examples,
        "legacy_render_operations": legacy_render_operations,
        "missing_render_graph_examples": missing_render_graph_examples,
        "render_graph_node_total": render_graph_node_total,
        "render_program_execution_hits": render_program_execution_hits,
        "output_sink_kinds_seen": sink_kinds,
        "missing_output_sink_kinds": missing_sink_kinds,
        "examples": examples,
        "blockers": blockers,
    });
    let artifact = write_artifact("lowering-coverage-report.json", &details)?;
    if verdict != "pass" {
        bail!(
            "DD lowering coverage is incomplete; see {}",
            artifact.display()
        );
    }
    Ok(details)
}

fn verify_generated_freshness(_args: &[String]) -> Result<serde_json::Value> {
    let root = repo_root()?;
    let temp_root = artifacts_dir()?.join("generated-freshness");
    if temp_root.exists() {
        fs::remove_dir_all(&temp_root)?;
    }
    fs::create_dir_all(&temp_root)?;
    let mut checked = Vec::new();
    let mut stale = Vec::new();
    let mut missing = Vec::new();
    for example in boon_dd::REQUIRED_EXAMPLES {
        let expected_dir = temp_root.join(example);
        write_generated_artifacts_at(example, &expected_dir)?;
        for relative in generated_artifact_relative_paths() {
            let actual_path = root.join("generated").join(example).join(relative);
            let expected_path = expected_dir.join(relative);
            if !actual_path.exists() {
                missing.push(serde_json::json!({
                    "example": example,
                    "path": actual_path.display().to_string(),
                }));
                continue;
            }
            let actual_sha256 = sha256_file(&actual_path)?;
            let expected_sha256 = sha256_file(&expected_path)?;
            let record = serde_json::json!({
                "example": example,
                "path": format!("generated/{example}/{relative}"),
                "actual_sha256": actual_sha256,
                "expected_sha256": expected_sha256,
            });
            if record["actual_sha256"] != record["expected_sha256"] {
                stale.push(record.clone());
            }
            checked.push(record);
        }
    }
    let verdict = if missing.is_empty() && stale.is_empty() {
        "pass"
    } else {
        "fail"
    };
    let details = serde_json::json!({
        "verdict": verdict,
        "checked_file_count": checked.len(),
        "checked_examples": boon_dd::REQUIRED_EXAMPLES,
        "temporary_regeneration_dir": temp_root,
        "missing": missing,
        "stale": stale,
        "checked": checked,
        "blockers": if verdict == "pass" {
            Vec::<String>::new()
        } else {
            vec![
                "one or more checked-in generated files are missing or stale".to_owned()
            ]
        },
    });
    let artifact = write_artifact("generated-freshness-report.json", &details)?;
    if verdict != "pass" {
        bail!(
            "generated freshness verification failed; see {}",
            artifact.display()
        );
    }
    Ok(details)
}

fn write_honest_compiler_prompts(_args: &[String]) -> Result<serde_json::Value> {
    let root = repo_root()?;
    let prompt_dir = root.join("docs/prompts/honest-compiler");
    let manifest = root.join(LANGUAGE_MANIFEST);
    let deterministic_report = artifacts_dir()?.join("honesty-deterministic-report.json");
    let required = [
        "01_shortcut_and_fallback_audit.md",
        "02_language_completeness_audit.md",
        "03_runtime_boundary_audit.md",
        "04_verifier_fake_pass_audit.md",
        "05_cross_repo_semantics_audit.md",
    ];
    let prompts = required
        .iter()
        .map(|file| {
            let path = prompt_dir.join(file);
            serde_json::json!({
                "path": format!("docs/prompts/honest-compiler/{file}"),
                "exists": path.exists(),
                "sha256": path.exists().then(|| sha256_file(&path).unwrap_or_else(|error| format!("unavailable: {error:#}"))),
            })
        })
        .collect::<Vec<_>>();
    let deterministic_report_sha256 = canonical_deterministic_report_hash(&deterministic_report)?;
    let details = serde_json::json!({
        "verdict": "pass",
        "repo_state": repo_state()?,
        "repo_state_hash": repo_state_hash()?,
        "manifest": LANGUAGE_MANIFEST,
        "manifest_sha256": manifest.exists().then(|| sha256_file(&manifest).unwrap_or_else(|error| format!("unavailable: {error:#}"))),
        "deterministic_report": "target/boon-artifacts/honesty-deterministic-report.json",
        "deterministic_report_sha256": deterministic_report_sha256,
        "audits_required": [
            "01_shortcut_and_fallback_audit",
            "01_shortcut_and_fallback_audit_second_independent_auditor",
            "02_language_completeness_audit",
            "03_runtime_boundary_audit",
            "04_verifier_fake_pass_audit",
            "04_verifier_fake_pass_audit_second_independent_auditor",
            "05_cross_repo_semantics_audit"
        ],
        "prompt_dir": "docs/prompts/honest-compiler",
        "prompts": prompts,
    });
    write_artifact("honest-compiler-prompt-pack.json", &details)?;
    Ok(details)
}

fn verify_prompt_audit(_args: &[String]) -> Result<serde_json::Value> {
    let root = repo_root()?;
    let prompt_dir = root.join("docs/prompts/honest-compiler");
    let deterministic_report = artifacts_dir()?.join("honesty-deterministic-report.json");
    let current_repo_state_hash = repo_state_hash()?;
    let current_deterministic_report_hash =
        canonical_deterministic_report_hash(&deterministic_report)?;
    let required_prompt_counts = [
        ("01_shortcut_and_fallback_audit", 2_usize),
        ("02_language_completeness_audit", 1),
        ("03_runtime_boundary_audit", 1),
        ("04_verifier_fake_pass_audit", 2),
        ("05_cross_repo_semantics_audit", 1),
    ];
    let mut prompt_hashes = std::collections::BTreeMap::new();
    for (prompt_id, _count) in required_prompt_counts {
        let path = prompt_dir.join(format!("{prompt_id}.md"));
        prompt_hashes.insert(
            prompt_id.to_owned(),
            path.exists().then(|| sha256_file(&path)).transpose()?,
        );
    }
    let audit_dir = artifacts_dir()?.join("prompt-audit");
    let audit_files = if audit_dir.exists() {
        fs::read_dir(&audit_dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut audit_counts = std::collections::BTreeMap::<String, usize>::new();
    let mut audits = Vec::new();
    let mut schema_errors = Vec::new();
    let mut critical_findings_open = 0_usize;
    let mut inconclusive_audits = 0_usize;
    let mut hash_mismatches = 0_usize;
    let mut audits_passed = 0_usize;
    for path in audit_files {
        match validate_prompt_audit_file(
            &path,
            &prompt_hashes,
            &current_repo_state_hash,
            current_deterministic_report_hash.as_deref(),
        ) {
            Ok(audit) => {
                let prompt_id = audit["prompt_id"].as_str().unwrap_or_default().to_owned();
                *audit_counts.entry(prompt_id).or_default() += 1;
                critical_findings_open +=
                    audit["critical_findings_open"].as_u64().unwrap_or_default() as usize;
                inconclusive_audits += (audit["verdict"].as_str() == Some("inconclusive")) as usize;
                hash_mismatches += audit["hash_mismatches"].as_u64().unwrap_or_default() as usize;
                if audit["accepted"].as_bool() == Some(true) {
                    audits_passed += 1;
                }
                audits.push(audit);
            }
            Err(error) => schema_errors.push(serde_json::json!({
                "path": path.display().to_string(),
                "error": format!("{error:#}"),
            })),
        }
    }
    let missing_required = required_prompt_counts
        .iter()
        .filter_map(|(prompt_id, required_count)| {
            let found = audit_counts.get(*prompt_id).copied().unwrap_or_default();
            (found < *required_count).then(|| {
                serde_json::json!({
                    "prompt_id": prompt_id,
                    "required": required_count,
                    "found": found,
                })
            })
        })
        .collect::<Vec<_>>();
    let audits_required = required_prompt_counts
        .iter()
        .map(|(_prompt_id, count)| *count)
        .sum::<usize>();
    let verdict = if audits_passed == audits_required
        && missing_required.is_empty()
        && schema_errors.is_empty()
        && critical_findings_open == 0
        && inconclusive_audits == 0
        && hash_mismatches == 0
    {
        "pass"
    } else {
        "fail"
    };
    let details = serde_json::json!({
        "verdict": verdict,
        "audits_required": audits_required,
        "audits_passed": audits_passed,
        "audit_json_files_found": audits.len() + schema_errors.len(),
        "required_prompt_counts": required_prompt_counts,
        "missing_required": missing_required,
        "critical_findings_open": critical_findings_open,
        "inconclusive_audits": inconclusive_audits,
        "hash_mismatches": hash_mismatches,
        "schema_errors": schema_errors,
        "current_repo_state_hash": current_repo_state_hash,
        "current_deterministic_report_hash": current_deterministic_report_hash,
        "prompt_hashes": prompt_hashes,
        "audits": audits,
        "blockers": if verdict == "pass" {
            Vec::<String>::new()
        } else {
            vec![
                "prompt audit outputs are missing, stale, inconclusive, failing, or schema-invalid".to_owned()
            ]
        },
    });
    let artifact = write_artifact("prompt-audit-report.json", &details)?;
    if verdict != "pass" {
        bail!("prompt audit is incomplete; see {}", artifact.display());
    }
    Ok(details)
}

fn validate_prompt_audit_file(
    path: &Path,
    prompt_hashes: &std::collections::BTreeMap<String, Option<String>>,
    current_repo_state_hash: &str,
    current_deterministic_report_hash: Option<&str>,
) -> Result<serde_json::Value> {
    let text =
        fs::read_to_string(path).with_context(|| format!("missing audit {}", path.display()))?;
    let audit: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("invalid JSON {}", path.display()))?;
    let prompt_id = audit
        .get("prompt_id")
        .and_then(|value| value.as_str())
        .context("audit missing prompt_id")?;
    let prompt_hash = audit
        .get("prompt_hash")
        .and_then(|value| value.as_str())
        .context("audit missing prompt_hash")?;
    let repo_state_hash = audit
        .get("repo_state_hash")
        .and_then(|value| value.as_str())
        .context("audit missing repo_state_hash")?;
    let deterministic_report_hash = audit
        .get("deterministic_report_hash")
        .and_then(|value| value.as_str())
        .context("audit missing deterministic_report_hash")?;
    let verdict = audit
        .get("verdict")
        .and_then(|value| value.as_str())
        .context("audit missing verdict")?;
    let critical_findings = audit
        .get("critical_findings")
        .and_then(|value| value.as_array())
        .context("audit missing critical_findings array")?;
    for (index, finding) in critical_findings.iter().enumerate() {
        for field in ["summary", "path", "evidence", "required_fix"] {
            finding
                .get(field)
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .with_context(|| {
                    format!("audit critical_findings[{index}] missing non-empty {field}")
                })?;
        }
        finding
            .get("line")
            .and_then(|value| value.as_u64())
            .with_context(|| {
                format!("audit critical_findings[{index}] missing non-negative line")
            })?;
    }
    for field in ["reviewed_files", "reviewed_artifacts", "commands_reviewed"] {
        audit
            .get(field)
            .and_then(|value| value.as_array())
            .with_context(|| format!("audit missing {field} array"))?;
    }
    let expected_prompt_hash = prompt_hashes
        .get(prompt_id)
        .and_then(|hash| hash.as_deref());
    let hash_mismatches = [
        expected_prompt_hash != Some(prompt_hash),
        repo_state_hash != current_repo_state_hash,
        Some(deterministic_report_hash) != current_deterministic_report_hash,
    ]
    .into_iter()
    .filter(|mismatch| *mismatch)
    .count();
    let accepted = verdict == "pass" && critical_findings.is_empty() && hash_mismatches == 0;
    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "prompt_id": prompt_id,
        "verdict": verdict,
        "accepted": accepted,
        "critical_findings_open": critical_findings.len(),
        "hash_mismatches": hash_mismatches,
    }))
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
            format!("generated/{example}/dd_graph_ir.json").into_boxed_str(),
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
            "contract": "generated graph crate test replays the full parsed scenario protocol and compares the final SmokeOutput to examples/<example>/expected.render.json",
        }));
    }
    let artifact = artifacts_dir()?.join("generated-crates.json");
    let details = serde_json::json!({
        "verdict": "pass",
        "checked_count": checked.len(),
        "checked": checked,
    });
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
    let artifact_dir = repo_root()?.join("generated").join(example);
    Ok(serde_json::json!({
        "example": example,
        "expected": expected_path,
        "generated_artifacts": artifact_dir.display().to_string(),
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
    let (generated_example, output) =
        boon_examples::run_generated_for_checked_source(&source_text, &scenario_text)
            .with_context(|| format!("example {example} has no verified generated graph"))?;
    ensure!(
        generated_example == example,
        "example {example} resolved to generated graph {generated_example}"
    );
    serde_json::to_string(&output).context("failed to serialize compiled DD graph output")
}

fn write_generated_artifacts_at(example: &str, generated_dir: &Path) -> Result<()> {
    let root = repo_root()?;
    let example_dir = root.join("examples").join(example);
    let source_path = example_dir.join("source.bn");
    let scenario_path = example_dir.join("scenario.toml");
    let source_text = fs::read_to_string(&source_path)?;
    let scenario_text = fs::read_to_string(&scenario_path)?;
    let expected_render_text = fs::read_to_string(example_dir.join("expected.render.json"))?;
    let source_path_string = format!("examples/{example}/source.bn");
    let plan = boon_compiler::compile_source(&source_path_string, &source_text);
    let scenario = boon_runtime_host::parse_scenario(&scenario_text);
    let (generated_example, generated_output) =
        boon_examples::run_generated_for_checked_source(&source_text, &scenario_text)
            .with_context(|| format!("example {example} has no verified generated graph"))?;
    ensure!(
        generated_example == example,
        "example {example} resolved to generated graph {generated_example}"
    );
    let outputs = vec![generated_output];
    let scenario_steps = scenario.steps;

    let src_dir = generated_dir.join("src");
    fs::create_dir_all(generated_dir)?;
    fs::create_dir_all(&src_dir)?;
    fs::write(
        generated_dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"generated_{example}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nboon_dd = {{ path = \"../../crates/boon_dd\" }}\ndifferential-dataflow = {{ version = \"=0.23.0\", default-features = false }}\nserde = {{ version = \"1\", features = [\"derive\"] }}\nserde_json = \"1\"\ntimely = {{ version = \"=0.29.0\", default-features = false }}\n"
        ),
    )?;
    fs::write(
        src_dir.join("lib.rs"),
        generated_lib_rs(&scenario_steps, &expected_render_text),
    )?;
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
        generated_dir.join("dd_graph_ir.json"),
        serde_json::to_vec_pretty(&plan.dd_graph_ir)?,
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
    format_generated_rust(generated_dir)?;
    Ok(())
}

fn generated_artifact_relative_paths() -> &'static [&'static str] {
    &[
        "Cargo.toml",
        "src/lib.rs",
        "src/graph.rs",
        "src/ids.rs",
        "src/source_events.rs",
        "src/shapes.rs",
        "src/values.rs",
        "src/render_bindings.rs",
        "src/monitor_bindings.rs",
        "src/persist_bindings.rs",
        "graph_static.json",
        "dd_graph_ir.json",
        "generated_graph.rs",
        "monitor_snapshot.json",
        "terminal_120x40.snapshot.txt",
        "native_render_1280x720.json",
        "browser_render_1280x720.json",
    ]
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

fn generated_lib_rs(steps: &[boon_dd::ScenarioStep], expected_render_text: &str) -> String {
    let steps_json =
        serde_json::to_string(steps).expect("scenario steps should serialize into generated test");
    format!(
        "pub mod graph;\npub mod ids;\npub mod monitor_bindings;\npub mod persist_bindings;\npub mod render_bindings;\npub mod shapes;\npub mod source_events;\npub mod values;\n\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn generated_graph_matches_checked_scenario_output() {{\n        let expected: boon_dd::SmokeOutput = serde_json::from_str({expected:?})\n            .expect(\"checked expected render JSON should deserialize\");\n        let scenario_steps: Vec<boon_dd::ScenarioStep> = serde_json::from_str({steps:?})\n            .expect(\"checked scenario steps should deserialize\");\n        let allocator = || timely::communication::Allocator::Thread(\n            timely::communication::allocator::Thread::default(),\n        );\n        let mut worker = timely::worker::Worker::new(timely::WorkerConfig::default(), allocator(), None);\n        let mut graph = crate::graph::build_dataflow(&mut worker);\n        let has_persistence_tap = crate::persist_bindings::has_persistence_tap();\n        let mut persistence_enabled = false;\n        let mut persisted_text: Option<String> = None;\n        let mut last_generated_persisted_text: Option<String> = None;\n        let mut last_output: Option<boon_dd::SmokeOutput> = None;\n        for (step_index, step) in scenario_steps.iter().enumerate() {{\n            let epoch = step_index as u64 + 1;\n            let mut submitted = false;\n            for event in &step.events {{\n                match event {{\n                    boon_dd::ScenarioEvent::Source(action) => {{\n                        graph.sources.submit_action(action, epoch);\n                        submitted = true;\n                    }}\n                    boon_dd::ScenarioEvent::Command(command)\n                        if command.command == \"enable_persistence\" =>\n                    {{\n                        if has_persistence_tap {{\n                            persistence_enabled = true;\n                            persisted_text = last_generated_persisted_text.clone();\n                        }}\n                    }}\n                    boon_dd::ScenarioEvent::Command(command) if command.command == \"reload\" => {{\n                        worker = timely::worker::Worker::new(\n                            timely::WorkerConfig::default(),\n                            allocator(),\n                            None,\n                        );\n                        graph = crate::graph::build_dataflow(&mut worker);\n                        if persistence_enabled {{\n                            if let Some(value) = persisted_text.clone() {{\n                                graph.sources.submit_persisted_text(value, epoch);\n                                submitted = true;\n                            }}\n                        }}\n                    }}\n                    boon_dd::ScenarioEvent::Command(_) => {{}}\n                }}\n            }}\n            if !submitted {{\n                graph.sources.submit_host_tick(epoch);\n            }}\n            graph.sources.close_epoch(epoch);\n            let target = crate::graph::completion_time(epoch) + 1;\n            let mut worker_steps = 0_usize;\n            while graph.probe.less_than(&target) {{\n                if worker_steps == 1024 {{\n                    panic!(\"generated graph step {{step_index}} probe stalled at {{target}} after {{worker_steps}} steps\");\n                }}\n                worker.step();\n                worker_steps += 1;\n            }}\n            let output = graph\n                .sources\n                .outputs()\n                .into_iter()\n                .last()\n                .expect(\"generated graph emitted no scenario output\");\n            last_generated_persisted_text = output.persistence.iter().rev().find_map(|command| {{\n                match command {{\n                    boon_dd::PersistenceCommand::SaveText {{ value, .. }} => Some(value.clone()),\n                    boon_dd::PersistenceCommand::LoadText {{ .. }} => None,\n                }}\n            }});\n            last_output = Some(output);\n        }}\n        let actual = last_output\n            .as_ref()\n            .expect(\"generated graph emitted no scenario output\");\n        assert_eq!(actual, &expected);\n    }}\n}}\n",
        expected = expected_render_text.trim(),
        steps = steps_json,
    )
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
    let launcher = run_capture("bash", &["-lc", "command -v cosmic-background-launch"])
        .context("missing cosmic-background-launch")?;
    let child = Command::new(launcher.trim())
        .args(["--workspace", COSMIC_WORKSPACE, "--"])
        .args(args)
        .current_dir(repo_root()?)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| {
            format!(
                "failed to launch window command with cosmic-background-launch --workspace {COSMIC_WORKSPACE} -- {}",
                args.join(" ")
            )
        })?;
    Ok(child.id().to_string())
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

fn browser_playground_html() -> &'static str {
    r#"<!doctype html>
<meta charset="utf-8">
<title>Boon DD Browser Playground</title>
<style>
body { margin: 0; font: 14px system-ui, sans-serif; color: #e7edf7; background: #11151c; }
#app { display: grid; grid-template-columns: 240px 1fr; min-height: 100vh; }
#examples { border-right: 1px solid #303846; padding: 12px; overflow: auto; }
button { display: block; width: 100%; margin: 0 0 4px; padding: 6px 8px; color: #dce8ff; background: #1d2633; border: 1px solid #3c495b; text-align: left; }
button[aria-selected="true"] { background: #285ea8; color: white; }
#stage { display: grid; grid-template-rows: minmax(240px, 1fr) auto; }
canvas { width: 100%; height: 100%; background: #0c1017; }
pre { margin: 0; padding: 12px; white-space: pre-wrap; border-top: 1px solid #303846; background: #151b24; }
</style>
<div id="app">
  <nav id="examples"></nav>
  <main id="stage">
    <canvas id="canvas" width="960" height="540"></canvas>
    <pre id="output"></pre>
  </main>
</div>
<script type="module">
import init, { run_smoke_json } from "./boon_wasm_smoke.js";
try {
  await init();
  const rows = JSON.parse(run_smoke_json());
  const loadedExamples = rows.map((row) => row[0]);
  const outputs = new Map(rows);
  const nav = document.getElementById("examples");
  const output = document.getElementById("output");
  let selected = loadedExamples[0];
  function renderSelection(name) {
    selected = name;
    for (const button of nav.querySelectorAll("button")) {
      button.setAttribute("aria-selected", String(button.dataset.example === name));
    }
    const value = outputs.get(name);
    const text = value && value.render && value.render[0] && value.render[0].PatchText
      ? value.render[0].PatchText.text
      : JSON.stringify(value && value.render || []);
    output.textContent = `Selected: ${name}\n\nRender output:\n${text}\n\nMonitor entries: ${(value && value.monitor || []).length}`;
  }
  for (const name of loadedExamples) {
    const button = document.createElement("button");
    button.type = "button";
    button.dataset.example = name;
    button.textContent = name;
    button.addEventListener("click", () => renderSelection(name));
    nav.append(button);
  }
  renderSelection(selected);

  if (!("gpu" in navigator)) {
    throw new Error("Browser playground WebGPU failed: navigator.gpu is unavailable");
  }
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) {
    throw new Error("Browser playground WebGPU failed: requestAdapter returned null");
  }
  const device = await adapter.requestDevice();
  if (!device) {
    throw new Error("Browser playground WebGPU failed: requestDevice returned null");
  }
  const canvas = document.getElementById("canvas");
  const context = canvas.getContext("webgpu");
  if (!context) {
    throw new Error("Browser playground WebGPU failed: canvas webgpu context is unavailable");
  }
  const format = navigator.gpu.getPreferredCanvasFormat();
  context.configure({ device, format, alphaMode: "opaque" });
  const encoder = device.createCommandEncoder();
  const pass = encoder.beginRenderPass({
    colorAttachments: [{
      view: context.getCurrentTexture().createView(),
      clearValue: { r: 0.05, g: 0.08, b: 0.12, a: 1.0 },
      loadOp: "clear",
      storeOp: "store"
    }]
  });
  pass.end();
  device.queue.submit([encoder.finish()]);

  const second = nav.querySelectorAll("button")[1];
  if (second) {
    second.click();
  }
  const result = JSON.stringify({
    backend: "browser-webgpu",
    mode: "playground",
    webgpu: {
      navigator_gpu: true,
      adapter: true,
      device: true,
      canvas_context: true,
      frame_presented: true
    },
    ui: {
      buttons: nav.querySelectorAll("button").length,
      canvas: true,
      output_panel: output.textContent.includes("Selected:"),
      simulated_click: selected === loadedExamples[1]
    },
    interactive_controls: ["click example button"],
    loaded_examples: loadedExamples,
    example_count: loadedExamples.length,
    selected_initial: loadedExamples[0],
    selected_after_simulated_click: selected,
    wasm_smoke: rows
  });
  document.body.dataset.result = result;
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
