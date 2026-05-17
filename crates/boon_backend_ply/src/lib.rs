pub mod app;
pub mod evidence;
pub mod human_surface;

use ply_engine::prelude::*;
use std::env;
use std::path::PathBuf;

pub static DEFAULT_FONT: FontAsset = FontAsset::Bytes {
    file_name: "FiraSans-Regular.otf",
    data: include_bytes!("/usr/share/fonts/opentype/fira/FiraSans-Regular.otf"),
};

pub fn window_conf() -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: miniquad::conf::Conf {
            window_title: "Boon DD Ply Playground".to_owned(),
            window_width: 1200,
            window_height: 800,
            high_dpi: true,
            sample_count: 4,
            platform: miniquad::conf::Platform {
                linux_backend: miniquad::conf::LinuxBackend::WaylandOnly,
                linux_wm_class: "boon-dd-ply-playground",
                webgl_version: miniquad::conf::WebGLVersion::WebGL2,
                ..Default::default()
            },
            ..Default::default()
        },
        draw_call_vertex_capacity: 200_000,
        draw_call_index_capacity: 200_000,
        ..Default::default()
    }
}

pub async fn run_macroquad_app() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("--human-surface") {
        human_surface::run_native_human_surface(&args[1..]).await;
    }
    let mut smoke_artifact = None;
    let mut selected_name = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--smoke" => {
                index += 1;
                smoke_artifact = args.get(index).map(PathBuf::from);
            }
            "--example" => {
                index += 1;
                selected_name = args.get(index).cloned();
            }
            other => {
                eprintln!("unknown boon_backend_ply argument: {other}");
                std::process::exit(2);
            }
        }
        index += 1;
    }

    let mut state = app::PlaygroundState::new();
    if let Some(name) = selected_name {
        state.select_by_name(&name);
    }
    let mut ply = Ply::<()>::new(&DEFAULT_FONT).await;
    ply.set_debug_mode(false);

    if let Some(artifact) = smoke_artifact {
        clear_background(BLACK);
        let frame = app::evaluate_frame(&mut ply, &state);
        app::show_frame(&mut ply, &state).await;
        next_frame().await;
        let report = evidence::native_smoke_report(&state, &frame);
        if let Err(error) = evidence::write_json_atomic(&artifact, &report) {
            eprintln!("failed to write native Ply smoke artifact: {error}");
            std::process::exit(1);
        }
        std::process::exit(0);
    }

    let mut browser_smoke_interaction_applied = false;
    loop {
        clear_background(MacroquadColor::from_rgba(12, 16, 23, 255));
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Q) {
            break;
        }
        app::handle_keyboard(&mut state);
        state.update_auto_tick(get_time());
        apply_browser_smoke_interaction(&mut state, &mut browser_smoke_interaction_applied);
        let mut frame = app::evaluate_frame(&mut ply, &state);
        if app::handle_pointer(&mut state, &ply) {
            frame = app::evaluate_frame(&mut ply, &state);
        }
        app::show_frame(&mut ply, &state).await;
        next_frame().await;
        publish_browser_smoke(&state, &frame);
    }
}

#[cfg(target_arch = "wasm32")]
fn apply_browser_smoke_interaction(state: &mut app::PlaygroundState, applied: &mut bool) {
    let _ = state;
    *applied = true;
}

#[cfg(not(target_arch = "wasm32"))]
fn apply_browser_smoke_interaction(_state: &mut app::PlaygroundState, _applied: &mut bool) {}

#[cfg(target_arch = "wasm32")]
fn publish_browser_smoke(state: &app::PlaygroundState, frame: &app::FrameEvidence) {
    let value = evidence::browser_runtime_telemetry(state, frame);
    if let Ok(text) = serde_json::to_string(&value) {
        ply_engine::net::post("boon-ply-browser-smoke", "/result", |request| {
            request
                .header("Content-Type", "application/json")
                .body(&text)
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn publish_browser_smoke(_state: &app::PlaygroundState, _frame: &app::FrameEvidence) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_ply_matrix_covers_required_examples_and_interactions() {
        let report = evidence::headless_matrix_report();
        assert!(report.success);
        assert_eq!(report.examples.len(), boon_dd::REQUIRED_EXAMPLES.len());
        assert!(report.interaction.selection_changed);
        assert!(report.interaction.counter_state_changed);
        assert!(report.interaction.counter_advanced_twice);
        assert!(report.interaction.generic_state_changed_or_noop_documented);
    }
}
