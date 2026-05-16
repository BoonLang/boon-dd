use crate::app::{FrameEvidence, PlaygroundState, evaluate_frame, show_frame};
use crate::{DEFAULT_FONT, evidence};
use ply_engine::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct NativeHumanOptions {
    example: String,
    output_dir: PathBuf,
    actions_path: Option<PathBuf>,
    source_hash: String,
    control_manifest_hash: String,
    deterministic_ply_report_sha256: String,
    matrix_run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HumanAction {
    kind: String,
    name: Option<String>,
    value: Option<String>,
    semantic: Option<String>,
    field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanStateSnapshot {
    pub selected_example: String,
    pub selected_output_text: String,
    pub selected_action_count: usize,
    pub selected_monitor_count: usize,
    pub input_buffer: String,
    pub last_submitted_text: String,
    pub interaction_log: Vec<String>,
}

pub fn state_snapshot(state: &PlaygroundState) -> HumanStateSnapshot {
    HumanStateSnapshot {
        selected_example: state.selected_name().to_owned(),
        selected_output_text: state.selected_output_text(),
        selected_action_count: state.selected_action_count(),
        selected_monitor_count: state.selected_monitor_count(),
        input_buffer: state.input_buffer.clone(),
        last_submitted_text: state.last_submitted_text.clone(),
        interaction_log: state.interaction_log.clone(),
    }
}

pub async fn run_native_human_surface(args: &[String]) -> ! {
    match run_native_human_surface_inner(args).await {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            eprintln!("native Ply human-surface run failed: {error}");
            std::process::exit(1);
        }
    }
}

async fn run_native_human_surface_inner(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_native_human_options(args)?;
    std::fs::create_dir_all(&options.output_dir)?;

    let mut state = PlaygroundState::new();
    state.select_by_name(&options.example);
    state
        .interaction_log
        .push(format!("human_surface_select:{}", options.example));
    let mut ply = Ply::<()>::new(&DEFAULT_FONT).await;
    ply.set_debug_mode(false);

    let actions = read_human_actions(options.actions_path.as_deref())?;
    let mut events = Vec::new();
    let mut selected_frame = None;
    let mut after_action_frame = None;
    let mut before = None;
    let mut after_action = None;
    let mut extra_screenshots = Vec::new();
    for action in &actions {
        match action.kind.as_str() {
            "select_example" => {
                if let Some(value) = action.value.as_deref() {
                    state.select_by_name(value);
                }
                events.push(action_event(action));
            }
            "type_text" => {
                state.type_text(action.value.as_deref().unwrap_or_default());
                events.push(action_event(action));
            }
            "press_key" => {
                state.press_key_label(action.value.as_deref().unwrap_or_default());
                events.push(action_event(action));
            }
            "activate" => {
                if action.semantic.as_deref() == Some("decrement") {
                    state.press_key_label("-");
                } else {
                    state.press_key_label("Enter");
                }
                events.push(action_event(action));
            }
            "wait_frames" | "wait_millis" => {
                events.push(action_event(action));
            }
            "screenshot" => {
                let name = action.name.as_deref().unwrap_or("screenshot");
                let path = options.output_dir.join(format!("{name}.png"));
                let frame = present_and_capture(&mut ply, &state, &path).await?;
                let snapshot = state_snapshot(&state);
                if name == "selected" {
                    before = Some(snapshot);
                    selected_frame = Some(frame);
                } else if name == "after-action" {
                    after_action = Some(snapshot);
                    after_action_frame = Some(frame);
                } else {
                    extra_screenshots.push(json!({
                        "name": name,
                        "path": path,
                        "frame": frame,
                        "state": snapshot,
                    }));
                }
                events.push(action_event(action));
            }
            kind if kind.starts_with("assert_") => {
                events.push(action_event(action));
            }
            _ => events.push(action_event(action)),
        }
    }

    let mut screenshot_hashes = Vec::new();
    for entry in std::fs::read_dir(&options.output_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("png") {
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("screenshot")
                .to_owned();
            screenshot_hashes.push(json!({
                "name": name,
                "path": path,
                "sha256": hash_file(&path)?,
            }));
        }
    }
    screenshot_hashes.sort_by(|left, right| {
        left.get("name")
            .and_then(|value| value.as_str())
            .cmp(&right.get("name").and_then(|value| value.as_str()))
    });

    let telemetry = json!({
        "backend": "ply-engine",
        "target": "native",
        "surface": "native-ply-window",
        "renderer": evidence::renderer_object("native-human-surface"),
        "example": options.example,
        "source_hash": options.source_hash,
        "control_manifest_hash": options.control_manifest_hash,
        "deterministic_ply_report_sha256": options.deterministic_ply_report_sha256,
        "matrix_run_id": options.matrix_run_id,
        "process_id": std::process::id(),
        "before": before.ok_or("manifest did not capture selected screenshot")?,
        "after_action": after_action.ok_or("manifest did not capture after-action screenshot")?,
        "selected_frame": selected_frame.ok_or("manifest did not capture selected frame")?,
        "after_action_frame": after_action_frame.ok_or("manifest did not capture after-action frame")?,
        "extra_screenshots": extra_screenshots,
        "screenshot_hashes": screenshot_hashes,
        "presented_ply_frames": true,
        "captured_from_macroquad_framebuffer": true,
    });
    write_json(&options.output_dir.join("telemetry.json"), &telemetry)?;

    let trace = json!({
        "target": "native",
        "example": options.example,
        "control_source": "native Ply window test mode",
        "events": events,
        "matrix_run_id": options.matrix_run_id,
        "source_hash": options.source_hash,
        "control_manifest_hash": options.control_manifest_hash,
        "deterministic_ply_report_sha256": options.deterministic_ply_report_sha256,
    });
    write_json(&options.output_dir.join("trace.json"), &trace)?;
    Ok(())
}

async fn present_and_capture(
    ply: &mut Ply<()>,
    state: &PlaygroundState,
    path: &Path,
) -> Result<FrameEvidence, Box<dyn std::error::Error>> {
    clear_background(MacroquadColor::from_rgba(12, 16, 23, 255));
    let frame = evaluate_frame(ply, state);
    show_frame(ply, state).await;
    let image = get_screen_data();
    image.export_png(path.to_str().ok_or("screenshot path is not UTF-8")?);
    next_frame().await;
    if !path.exists() {
        return Err(format!("screenshot was not written: {}", path.display()).into());
    }
    Ok(frame)
}

fn parse_native_human_options(
    args: &[String],
) -> Result<NativeHumanOptions, Box<dyn std::error::Error>> {
    let mut example = None;
    let mut output_dir = None;
    let mut actions_path = None;
    let mut source_hash = None;
    let mut control_manifest_hash = None;
    let mut deterministic_ply_report_sha256 = None;
    let mut matrix_run_id = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--example" => {
                index += 1;
                example = args.get(index).cloned();
            }
            "--output-dir" => {
                index += 1;
                output_dir = args.get(index).map(PathBuf::from);
            }
            "--actions" => {
                index += 1;
                actions_path = args.get(index).map(PathBuf::from);
            }
            "--source-hash" => {
                index += 1;
                source_hash = args.get(index).cloned();
            }
            "--control-manifest-hash" => {
                index += 1;
                control_manifest_hash = args.get(index).cloned();
            }
            "--deterministic-ply-report-sha256" => {
                index += 1;
                deterministic_ply_report_sha256 = args.get(index).cloned();
            }
            "--matrix-run-id" => {
                index += 1;
                matrix_run_id = args.get(index).cloned();
            }
            other => return Err(format!("unknown --human-surface argument: {other}").into()),
        }
        index += 1;
    }
    Ok(NativeHumanOptions {
        example: example.ok_or("missing --example")?,
        output_dir: output_dir.ok_or("missing --output-dir")?,
        actions_path,
        source_hash: source_hash.ok_or("missing --source-hash")?,
        control_manifest_hash: control_manifest_hash.ok_or("missing --control-manifest-hash")?,
        deterministic_ply_report_sha256: deterministic_ply_report_sha256
            .ok_or("missing --deterministic-ply-report-sha256")?,
        matrix_run_id: matrix_run_id.ok_or("missing --matrix-run-id")?,
    })
}

fn read_human_actions(path: Option<&Path>) -> Result<Vec<HumanAction>, Box<dyn std::error::Error>> {
    let Some(path) = path else {
        return Ok(vec![
            HumanAction {
                kind: "select_example".to_owned(),
                name: None,
                value: None,
                semantic: None,
                field: None,
            },
            HumanAction {
                kind: "screenshot".to_owned(),
                name: Some("selected".to_owned()),
                value: None,
                semantic: None,
                field: None,
            },
            HumanAction {
                kind: "activate".to_owned(),
                name: None,
                value: None,
                semantic: Some("primary".to_owned()),
                field: None,
            },
            HumanAction {
                kind: "screenshot".to_owned(),
                name: Some("after-action".to_owned()),
                value: None,
                semantic: None,
                field: None,
            },
        ]);
    };
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn action_event(action: &HumanAction) -> serde_json::Value {
    json!({
        "kind": action.kind,
        "name": action.name,
        "value": action.value,
        "semantic": action.semantic,
        "field": action.field,
    })
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, std::io::Error> {
    let mut hasher = Sha256::new();
    hasher.update(std::fs::read(path)?);
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
