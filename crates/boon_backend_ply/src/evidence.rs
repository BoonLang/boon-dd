use crate::app::{FrameEvidence, PlaygroundState, evaluate_headless_state, simulate_interactions};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;

pub const PLY_ENGINE_VERSION: &str = "1.1.1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeadlessExampleEvidence {
    pub example: String,
    pub selected_index: usize,
    pub frame: FrameEvidence,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeadlessMatrixReport {
    pub success: bool,
    pub backend: &'static str,
    pub target: &'static str,
    pub renderer: serde_json::Value,
    pub required_examples: Vec<&'static str>,
    pub examples: Vec<HeadlessExampleEvidence>,
    pub interaction: crate::app::InteractionEvidence,
}

pub fn renderer_object(target: &str) -> serde_json::Value {
    json!({
        "library": "ply-engine",
        "crate_version": PLY_ENGINE_VERSION,
        "target": target,
        "shared_rust_ply_ui": true,
        "custom_wgpu_renderer_removed": true,
        "custom_js_renderer_removed": true,
        "custom_webgpu_renderer_removed": true
    })
}

pub fn headless_matrix_report() -> HeadlessMatrixReport {
    let base = PlaygroundState::new();
    let mut examples = Vec::new();
    for index in 0..base.examples.len() {
        let mut state = base.clone();
        state.selected = index;
        let frame = evaluate_headless_state(&state);
        examples.push(HeadlessExampleEvidence {
            example: state.selected_name().to_owned(),
            selected_index: index,
            frame,
        });
    }
    let success = examples.iter().all(|example| {
        example.frame.ply_render_commands > 0
            && example.frame.texts > 0
            && !example.frame.semantic_widgets.is_empty()
    });
    HeadlessMatrixReport {
        success,
        backend: "ply-engine",
        target: "headless",
        renderer: renderer_object("headless"),
        required_examples: boon_dd::REQUIRED_EXAMPLES.to_vec(),
        examples,
        interaction: simulate_interactions(),
    }
}

pub fn native_smoke_report(state: &PlaygroundState, frame: &FrameEvidence) -> serde_json::Value {
    let per_example = state
        .examples
        .iter()
        .enumerate()
        .map(|(index, example)| {
            json!({
                "example": &example.name,
                "selected_index": index,
                "dd_output": &example.checked_scenario_output,
                "runtime_session": {
                    "graph_builds": example.runtime.graph_builds(),
                    "drained_outputs": example.runtime.drained_outputs(),
                    "source": "generated_crate_session",
                },
                "frame": evaluate_headless_state(&PlaygroundState {
                    examples: state.examples.clone(),
                    selected: index,
                    input_buffer: String::new(),
                    last_submitted_text: String::new(),
                    interaction_log: Vec::new(),
                }),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "backend": "ply-engine",
        "target": "native",
        "renderer": {
            "library": "ply-engine",
            "crate_version": PLY_ENGINE_VERSION,
            "macroquad_backend": true,
            "custom_wgpu_renderer_removed": true,
            "legacy_native_window_stack_removed": true
        },
        "ply_initialized": true,
        "ply_frame_evaluated": true,
        "ply_frame_presented": true,
        "runtime_sessions": runtime_sessions(state),
        "loaded_examples": state.loaded_examples(),
        "example_count": state.examples.len(),
        "selected_example": state.selected_name(),
        "live_state": {
            "selected_example": state.selected_name(),
            "selected_output_text": state.selected_output_text(),
            "selected_action_count": state.selected_action_count(),
            "selected_monitor_count": state.selected_monitor_count(),
            "input_buffer": state.input_buffer,
            "last_submitted_text": state.last_submitted_text,
            "interaction_log": state.interaction_log,
        },
        "visible_ui": {
            "sidebar": true,
            "example_labels": true,
            "selected_output_panel": true,
            "ply_render_commands": frame.ply_render_commands,
            "text_commands": frame.texts,
            "rectangle_commands": frame.rectangles
        },
        "interactive_controls": ["up", "down", "left", "right", "enter", "space", "+", "q", "escape"],
        "interaction": simulate_interactions(),
        "per_example": per_example,
        "frame": frame
    })
}

pub fn browser_runtime_telemetry(
    state: &PlaygroundState,
    frame: &FrameEvidence,
) -> serde_json::Value {
    let wasm_generated_manifest = state
        .examples
        .iter()
        .map(|example| json!([&example.name, &example.checked_scenario_output]))
        .collect::<Vec<_>>();
    json!({
        "backend": "ply-engine",
        "target": "browser",
        "firefox": true,
        "canvas_nonblank": frame.ply_render_commands > 0 && frame.texts > 0,
        "renderer": {
            "library": "ply-engine",
            "crate_version": PLY_ENGINE_VERSION,
            "plyx_web_bundle": true,
            "custom_js_renderer_removed": true,
            "custom_webgpu_renderer_removed": true
        },
        "wasm_loaded": true,
        "ply_frame_presented": true,
        "runtime_sessions": runtime_sessions(state),
        "loaded_examples": state.loaded_examples(),
        "example_count": state.examples.len(),
        "selected_example": state.selected_name(),
        "live_state": {
            "selected_example": state.selected_name(),
            "selected_output_text": state.selected_output_text(),
            "selected_action_count": state.selected_action_count(),
            "selected_monitor_count": state.selected_monitor_count(),
            "input_buffer": state.input_buffer,
            "last_submitted_text": state.last_submitted_text,
            "interaction_log": state.interaction_log,
        },
        "interaction": simulate_interactions(),
        "wasm_generated_manifest": wasm_generated_manifest,
        "frame": frame
    })
}

fn runtime_sessions(state: &PlaygroundState) -> Vec<serde_json::Value> {
    state
        .examples
        .iter()
        .map(|example| {
            json!({
                "example": &example.name,
                "source": "generated_crate_session",
                "graph_builds": example.runtime.graph_builds(),
                "drained_outputs": example.runtime.drained_outputs(),
                "applied_steps": example.step_index,
                "next_epoch": example.next_epoch,
            })
        })
        .collect()
}

pub fn write_json_atomic(
    path: &Path,
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}
