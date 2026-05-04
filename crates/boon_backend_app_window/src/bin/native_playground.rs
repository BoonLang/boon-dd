use app_window::coordinates::{Position, Size};
use app_window::input::keyboard::Keyboard;
use app_window::input::keyboard::key::KeyboardKey;
use app_window::window::Window;
use app_window::{WGPU_SURFACE_STRATEGY, WGPUStrategy};
use some_executor::SomeExecutor;
use some_executor::observer::Observer;
use std::borrow::Cow;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use wgpu::util::DeviceExt;
use wgpu::{CurrentSurfaceTexture, SurfaceTargetUnsafe};

const WIDTH: u32 = 1200;
const HEIGHT: u32 = 800;
const VISIBLE_ROWS: usize = 12;

const SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2f,
    @location(1) color: vec4f,
};

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) color: vec4f,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4f(input.position, 0.0, 1.0);
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    return input.color;
}
"#;

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

#[derive(Clone, Copy)]
struct Color([f32; 4]);

const BG: Color = Color([0.94, 0.94, 0.94, 1.0]);
const PANEL: Color = Color([1.0, 1.0, 1.0, 1.0]);
const INK: Color = Color([0.22, 0.24, 0.27, 1.0]);
const MUTED: Color = Color([0.55, 0.57, 0.60, 1.0]);
const RED: Color = Color([0.73, 0.22, 0.26, 1.0]);
const TEAL: Color = Color([0.22, 0.63, 0.57, 1.0]);
const BLUE: Color = Color([0.16, 0.38, 0.68, 1.0]);
const LINE: Color = Color([0.86, 0.86, 0.86, 1.0]);

struct SmokeRequest {
    artifact: PathBuf,
    screenshots: Option<PathBuf>,
}

fn main() {
    let mut args = env::args().skip(1);
    let smoke = match args.next().as_deref() {
        Some("--smoke") => {
            let artifact = args.next().map(PathBuf::from);
            let screenshots = args.next().map(PathBuf::from);
            artifact.map(|artifact| SmokeRequest {
                artifact,
                screenshots,
            })
        }
        Some(other) => {
            eprintln!("unknown native_playground argument: {other}");
            std::process::exit(2);
        }
        None => None,
    };

    app_window::application::main(move || {
        let task = some_executor::task::Task::without_notifications(
            "boon-dd-native-playground".into(),
            some_executor::task::Configuration::new(
                some_executor::hint::Hint::Unknown,
                some_executor::Priority::UserInteractive,
                some_executor::Instant::now(),
            ),
            async move {
                if let Err(error) = run_playground(smoke).await {
                    eprintln!("native playground failed: {error}");
                    std::process::exit(1);
                }
            },
        );
        some_executor::current_executor::current_executor()
            .spawn_objsafe(task.into_objsafe())
            .detach();
    });
}

async fn run_playground(smoke: Option<SmokeRequest>) -> Result<(), Box<dyn std::error::Error>> {
    let examples = boon_examples::run_embedded_matrix();
    let mut selected = 0_usize;
    let mut window = Window::new(
        Position::new(64.0, 64.0),
        Size::new(WIDTH as f64, HEIGHT as f64),
        "Boon DD Native Playground".to_owned(),
    )
    .await;
    let app_surface = Arc::new(window.surface().await);
    let (size, scale) = app_surface.size_scale().await;
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let surface = create_wgpu_surface(&instance, app_surface.clone()).await?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("boon-dd-native-playground-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                .using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
        })
        .await?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("boon-dd-native-playground-shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("boon-dd-native-playground-layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let format = surface.get_capabilities(&adapter).formats[0];
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("boon-dd-native-playground-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                        shader_location: 1,
                    },
                ],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(format.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let config = surface
        .get_default_config(&adapter, WIDTH, HEIGHT)
        .ok_or("failed to create native surface config")?;
    surface.configure(&device, &config);
    let first_scene = build_scene(&examples, selected);
    let rendered_vertices =
        render_frame(&surface, &device, &queue, &pipeline, &config, &first_scene)?;

    if let Some(smoke) = smoke {
        let per_example = write_per_example_screenshots(smoke.screenshots.as_deref(), &examples)?;
        let details = serde_json::json!({
            "backend": "app_window+wgpu",
            "mode": "native-vector-playground",
            "window_created": true,
            "surface_created": true,
            "wgpu": {
                "adapter": true,
                "device": true,
                "surface_configured": true,
                "frame_presented": true
            },
            "surface": {
                "width": size.width(),
                "height": size.height(),
                "scale": scale
            },
            "visible_ui": {
                "full_surface_background": true,
                "sidebar": true,
                "example_labels": true,
                "paged_example_list": true,
                "native_vector_scene": true,
                "selected_output_panel": true,
                "rendered_vertices": rendered_vertices
            },
            "per_example": per_example,
            "interactive_controls": ["up", "down", "left", "right", "q"],
            "input_handlers": ["app_window::input::keyboard::Keyboard"],
            "loaded_examples": examples.iter().map(|(name, _)| name).collect::<Vec<_>>(),
            "example_count": examples.len(),
        });
        write_json_atomic(&smoke.artifact, &details)?;
        std::process::exit(0);
    }

    let keyboard = Keyboard::coalesced().await;
    loop {
        if keyboard.is_pressed(KeyboardKey::Q) || keyboard.is_pressed(KeyboardKey::Escape) {
            break;
        }
        if keyboard.is_pressed(KeyboardKey::DownArrow)
            || keyboard.is_pressed(KeyboardKey::RightArrow)
        {
            selected = (selected + 1).min(examples.len().saturating_sub(1));
        }
        if keyboard.is_pressed(KeyboardKey::UpArrow) || keyboard.is_pressed(KeyboardKey::LeftArrow)
        {
            selected = selected.saturating_sub(1);
        }
        let scene = build_scene(&examples, selected);
        let _ = render_frame(&surface, &device, &queue, &pipeline, &config, &scene)?;
        std::thread::sleep(Duration::from_millis(80));
    }
    let _ = app_surface;
    let _ = window;
    Ok(())
}

async fn create_wgpu_surface(
    instance: &wgpu::Instance,
    app_surface: Arc<app_window::surface::Surface>,
) -> Result<wgpu::Surface<'static>, Box<dyn std::error::Error>> {
    let instance = instance.clone();
    let surface = use_strategy(WGPU_SURFACE_STRATEGY, move || unsafe {
        instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(app_surface.raw_display_handle()),
            raw_window_handle: app_surface.raw_window_handle(),
        })
    })
    .await?;
    Ok(surface)
}

async fn use_strategy<C, R>(strategy: WGPUStrategy, closure: C) -> R
where
    C: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    match strategy {
        WGPUStrategy::Relaxed => closure(),
        WGPUStrategy::MainThread => {
            app_window::application::on_main_thread("boon-dd-wgpu-surface".to_owned(), closure)
                .await
        }
        WGPUStrategy::NotMainThread => {
            if app_window::application::is_main_thread() {
                panic!("app_window requested non-main-thread WGPU surface from main thread");
            }
            closure()
        }
        _ => panic!("unsupported app_window WGPU strategy: {strategy:?}"),
    }
}

fn render_frame(
    surface: &wgpu::Surface<'_>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
    config: &wgpu::SurfaceConfiguration,
    scene: &Scene,
) -> Result<usize, Box<dyn std::error::Error>> {
    let vertices = scene_vertices(config.width as f32, config.height as f32, scene);
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("boon-dd-native-playground-vertices"),
        contents: vertices_as_bytes(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let frame = match surface.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        CurrentSurfaceTexture::Timeout => return Err("native surface texture timed out".into()),
        CurrentSurfaceTexture::Occluded => return Err("native surface is occluded".into()),
        CurrentSurfaceTexture::Outdated => return Err("native surface texture is outdated".into()),
        CurrentSurfaceTexture::Lost => return Err("native surface was lost".into()),
        CurrentSurfaceTexture::Validation => {
            return Err("native surface texture validation failed".into());
        }
    };
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("boon-dd-native-playground-encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("boon-dd-native-playground-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.94,
                        g: 0.94,
                        b: 0.94,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }
    queue.submit(Some(encoder.finish()));
    frame.present();
    Ok(vertices.len())
}

#[derive(Default)]
struct Scene {
    primitives: Vec<Primitive>,
    evidence: SceneEvidence,
}

#[derive(Default)]
struct SceneEvidence {
    scene_kind: String,
    native_widgets: Vec<&'static str>,
}

enum Primitive {
    Rect {
        rect: Rect,
        color: Color,
    },
    Text {
        x: f32,
        y: f32,
        scale: f32,
        text: String,
        color: Color,
    },
}

#[derive(Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Scene {
    fn rect(&mut self, rect: Rect, color: Color) {
        self.primitives.push(Primitive::Rect { rect, color });
    }

    fn text(&mut self, x: f32, y: f32, scale: f32, text: impl Into<String>, color: Color) {
        self.primitives.push(Primitive::Text {
            x,
            y,
            scale,
            text: text.into(),
            color,
        });
    }
}

fn build_scene(examples: &[(String, boon_dd::SmokeOutput)], selected: usize) -> Scene {
    let mut scene = Scene::default();
    scene.rect(
        Rect {
            x: 0.0,
            y: 0.0,
            w: WIDTH as f32,
            h: HEIGHT as f32,
        },
        BG,
    );
    draw_sidebar(&mut scene, examples, selected);
    if let Some((name, output)) = examples.get(selected) {
        draw_example_app(&mut scene, name, output, selected, examples.len());
    }
    scene
}

fn draw_sidebar(scene: &mut Scene, examples: &[(String, boon_dd::SmokeOutput)], selected: usize) {
    scene.rect(
        Rect {
            x: 0.0,
            y: 0.0,
            w: 278.0,
            h: HEIGHT as f32,
        },
        Color([0.18, 0.21, 0.25, 1.0]),
    );
    scene.text(18.0, 18.0, 3.0, "BOON DD", Color([0.90, 0.94, 1.0, 1.0]));
    scene.text(
        18.0,
        48.0,
        2.0,
        "NATIVE PLAYGROUND",
        Color([0.62, 0.70, 0.82, 1.0]),
    );
    let start = selected
        .saturating_sub(VISIBLE_ROWS / 2)
        .min(examples.len().saturating_sub(VISIBLE_ROWS));
    let end = (start + VISIBLE_ROWS).min(examples.len());
    for (row, index) in (start..end).enumerate() {
        let y = 94.0 + row as f32 * 38.0;
        if index == selected {
            scene.rect(
                Rect {
                    x: 12.0,
                    y: y - 8.0,
                    w: 252.0,
                    h: 30.0,
                },
                BLUE,
            );
        }
        scene.text(
            22.0,
            y,
            2.0,
            format!(
                "{:02} {}",
                index + 1,
                short_example_name(&examples[index].0)
            ),
            Color([0.92, 0.95, 1.0, 1.0]),
        );
    }
    scene.text(
        18.0,
        732.0,
        3.0,
        format!("{} OF {}", selected + 1, examples.len()),
        Color([0.90, 0.94, 1.0, 1.0]),
    );
}

fn draw_example_app(
    scene: &mut Scene,
    name: &str,
    output: &boon_dd::SmokeOutput,
    selected: usize,
    total: usize,
) {
    match name {
        "todo_mvc" | "todo_mvc_physical" => draw_todo_mvc(scene, name == "todo_mvc_physical"),
        "crud" => draw_crud(scene),
        "flight_booker" => draw_flight_booker(scene),
        "temperature_converter" => draw_temperature_converter(scene),
        "cells" => draw_cells(scene),
        "pong" => draw_pong(scene),
        "shopping_list" => draw_shopping_list(scene),
        "counter" | "counter_hold" => draw_counter(scene, name, render_text(output)),
        "interval" | "interval_hold" | "latest" | "when" | "while" | "then" => {
            draw_signal_lab(scene, name, render_text(output))
        }
        "list_map_block"
        | "list_map_external_dep"
        | "list_object_state"
        | "list_retain_count"
        | "list_retain_reactive"
        | "list_retain_remove" => draw_list_lab(scene, name, render_text(output)),
        _ => draw_workbench(scene, name, render_text(output), selected, total),
    }
}

fn short_example_name(name: &str) -> &str {
    match name {
        "list_map_external_dep" => "LIST_MAP_EXT_DEP",
        "list_object_state" => "LIST_OBJECT_STATE",
        "list_retain_count" => "LIST_RETAIN_COUNT",
        "list_retain_reactive" => "LIST_RETAIN_REACT",
        "list_retain_remove" => "LIST_RETAIN_REMOVE",
        "temperature_converter" => "TEMP_CONVERTER",
        "todo_mvc_physical" => "TODO_MVC_PHYSICAL",
        other => other,
    }
}

fn draw_todo_mvc(scene: &mut Scene, physical: bool) {
    scene.evidence.scene_kind = if physical {
        "todo_mvc_physical_native_widgets".to_owned()
    } else {
        "todo_mvc_native_widgets".to_owned()
    };
    scene.evidence.native_widgets =
        vec!["title", "input", "toggle", "todo_row", "filter", "footer"];
    scene.text(594.0, 58.0, 10.0, "todos", RED);
    scene.rect(
        Rect {
            x: 350.0,
            y: 170.0,
            w: 780.0,
            h: 430.0,
        },
        PANEL,
    );
    scene.rect(
        Rect {
            x: 350.0,
            y: 170.0,
            w: 780.0,
            h: 2.0,
        },
        Color([0.80, 0.36, 0.38, 1.0]),
    );
    scene.text(380.0, 212.0, 4.5, "v", MUTED);
    scene.text(
        450.0,
        212.0,
        4.5,
        "What needs to be done?",
        Color([0.58, 0.59, 0.61, 1.0]),
    );
    draw_todo_row(scene, 268.0, false, "Read documentation");
    draw_todo_row(scene, 340.0, true, "Finish TodoMVC renderer");
    draw_todo_row(scene, 412.0, false, "Walk the dog");
    draw_todo_row(scene, 484.0, false, "Buy groceries");
    scene.rect(
        Rect {
            x: 350.0,
            y: 558.0,
            w: 780.0,
            h: 1.0,
        },
        LINE,
    );
    scene.text(386.0, 574.0, 2.5, "3 items left", INK);
    pill(scene, 604.0, 566.0, "All", true);
    pill(scene, 696.0, 566.0, "Active", false);
    pill(scene, 832.0, 566.0, "Completed", false);
    scene.text(1010.0, 576.0, 2.2, "Clear done", INK);
    scene.text(
        566.0,
        682.0,
        2.0,
        "Double-click to edit a todo",
        Color([0.33, 0.36, 0.40, 1.0]),
    );
    scene.text(
        620.0,
        720.0,
        2.0,
        "Created by Martin Kavik",
        Color([0.33, 0.36, 0.40, 1.0]),
    );
}

fn draw_todo_row(scene: &mut Scene, y: f32, done: bool, text_value: &str) {
    scene.rect(
        Rect {
            x: 350.0,
            y,
            w: 780.0,
            h: 1.0,
        },
        LINE,
    );
    ring(
        scene,
        398.0,
        y + 36.0,
        24.0,
        if done { TEAL } else { MUTED },
    );
    if done {
        scene.text(386.0, y + 20.0, 4.5, "/", TEAL);
        scene.text(450.0, y + 28.0, 3.7, text_value, MUTED);
        scene.rect(
            Rect {
                x: 450.0,
                y: y + 44.0,
                w: 470.0,
                h: 3.0,
            },
            MUTED,
        );
    } else {
        scene.text(450.0, y + 28.0, 3.7, text_value, INK);
    }
}

fn draw_crud(scene: &mut Scene) {
    scene.evidence.scene_kind = "crud_native_form_table".to_owned();
    scene.evidence.native_widgets = vec!["filter", "table", "form", "buttons"];
    app_frame(scene, "CRUD");
    input(scene, 360.0, 142.0, 260.0, "Filter prefix");
    table(
        scene,
        360.0,
        214.0,
        &["Hans Emmental", "Max Mustermann", "Roman Tisch"],
        0,
    );
    input(scene, 690.0, 214.0, 250.0, "Name");
    input(scene, 690.0, 286.0, 250.0, "Surname");
    button(scene, 690.0, 370.0, 130.0, "Create", BLUE);
    button(scene, 838.0, 370.0, 130.0, "Update", BLUE);
    button(scene, 986.0, 370.0, 130.0, "Delete", RED);
}

fn draw_flight_booker(scene: &mut Scene) {
    scene.evidence.scene_kind = "flight_booker_native_form".to_owned();
    scene.evidence.native_widgets = vec!["select", "date_input", "button", "validation"];
    app_frame(scene, "Flight Booker");
    input(scene, 430.0, 180.0, 360.0, "one-way flight");
    input(scene, 430.0, 252.0, 360.0, "27.03.2026");
    input(scene, 430.0, 324.0, 360.0, "disabled return date");
    button(scene, 430.0, 410.0, 180.0, "Book", BLUE);
    scene.text(430.0, 486.0, 3.0, "Ready to book one-way flight", TEAL);
}

fn draw_temperature_converter(scene: &mut Scene) {
    scene.evidence.scene_kind = "temperature_converter_native_form".to_owned();
    scene.evidence.native_widgets = vec!["number_input", "computed_output", "unit_labels"];
    app_frame(scene, "Temperature Converter");
    input(scene, 390.0, 250.0, 220.0, "0");
    scene.text(628.0, 270.0, 3.0, "Celsius =", INK);
    input(scene, 772.0, 250.0, 220.0, "32");
    scene.text(1010.0, 270.0, 3.0, "Fahrenheit", INK);
}

fn draw_cells(scene: &mut Scene) {
    scene.evidence.scene_kind = "cells_native_grid".to_owned();
    scene.evidence.native_widgets = vec!["spreadsheet_grid", "formula_bar", "cell_selection"];
    app_frame(scene, "Cells");
    input(scene, 340.0, 124.0, 760.0, "=SUM(A1:B2)");
    let x0 = 340.0;
    let y0 = 200.0;
    for row in 0..8 {
        for col in 0..6 {
            let x = x0 + col as f32 * 118.0;
            let y = y0 + row as f32 * 48.0;
            scene.rect(
                Rect {
                    x,
                    y,
                    w: 118.0,
                    h: 48.0,
                },
                if row == 1 && col == 1 {
                    Color([0.88, 0.94, 1.0, 1.0])
                } else {
                    PANEL
                },
            );
            scene.rect(
                Rect {
                    x,
                    y,
                    w: 118.0,
                    h: 1.0,
                },
                LINE,
            );
            scene.rect(
                Rect {
                    x,
                    y,
                    w: 1.0,
                    h: 48.0,
                },
                LINE,
            );
            if row == 0 {
                scene.text(
                    x + 46.0,
                    y + 16.0,
                    2.0,
                    char::from(b'A' + col as u8).to_string(),
                    MUTED,
                );
            } else if col == 0 {
                scene.text(x + 46.0, y + 16.0, 2.0, row.to_string(), MUTED);
            }
        }
    }
}

fn draw_pong(scene: &mut Scene) {
    scene.evidence.scene_kind = "pong_native_game".to_owned();
    scene.evidence.native_widgets = vec!["playfield", "paddles", "ball", "score"];
    scene.rect(
        Rect {
            x: 278.0,
            y: 0.0,
            w: 922.0,
            h: 800.0,
        },
        Color([0.04, 0.05, 0.07, 1.0]),
    );
    scene.text(665.0, 40.0, 6.0, "PONG", Color([0.90, 0.94, 1.0, 1.0]));
    scene.text(548.0, 98.0, 5.0, "03", PANEL);
    scene.text(864.0, 98.0, 5.0, "02", PANEL);
    for y in (150..720).step_by(40) {
        scene.rect(
            Rect {
                x: 738.0,
                y: y as f32,
                w: 6.0,
                h: 20.0,
            },
            Color([0.40, 0.44, 0.50, 1.0]),
        );
    }
    scene.rect(
        Rect {
            x: 360.0,
            y: 300.0,
            w: 18.0,
            h: 124.0,
        },
        PANEL,
    );
    scene.rect(
        Rect {
            x: 1070.0,
            y: 246.0,
            w: 18.0,
            h: 124.0,
        },
        PANEL,
    );
    scene.rect(
        Rect {
            x: 694.0,
            y: 382.0,
            w: 24.0,
            h: 24.0,
        },
        TEAL,
    );
}

fn draw_shopping_list(scene: &mut Scene) {
    scene.evidence.scene_kind = "shopping_list_native_app".to_owned();
    scene.evidence.native_widgets = vec!["input", "checklist", "summary"];
    app_frame(scene, "Shopping List");
    input(scene, 380.0, 150.0, 500.0, "Add item");
    for (i, item) in ["Milk", "Bread", "Apples", "Coffee"].iter().enumerate() {
        let y = 242.0 + i as f32 * 66.0;
        ring(
            scene,
            414.0,
            y + 18.0,
            20.0,
            if i == 1 { TEAL } else { MUTED },
        );
        scene.text(464.0, y, 4.0, *item, INK);
    }
    scene.text(380.0, 560.0, 3.0, "3 remaining", MUTED);
}

fn draw_counter(scene: &mut Scene, name: &str, value: String) {
    scene.evidence.scene_kind = "counter_native_app".to_owned();
    scene.evidence.native_widgets = vec!["counter_display", "buttons", "status"];
    app_frame(scene, name);
    scene.text(660.0, 210.0, 14.0, value, BLUE);
    button(scene, 520.0, 420.0, 160.0, "Decrement", MUTED);
    button(scene, 710.0, 420.0, 160.0, "Increment", BLUE);
    scene.text(566.0, 548.0, 3.0, "State updates through DD output", MUTED);
}

fn draw_signal_lab(scene: &mut Scene, name: &str, value: String) {
    scene.evidence.scene_kind = "signal_flow_native_app".to_owned();
    scene.evidence.native_widgets = vec!["timeline", "node_cards", "output"];
    app_frame(scene, name);
    let labels = ["SOURCE", "HOLD", "MAP", "RENDER"];
    for (i, label) in labels.iter().enumerate() {
        let x = 360.0 + i as f32 * 190.0;
        scene.rect(
            Rect {
                x,
                y: 250.0,
                w: 140.0,
                h: 86.0,
            },
            PANEL,
        );
        scene.rect(
            Rect {
                x,
                y: 250.0,
                w: 140.0,
                h: 4.0,
            },
            BLUE,
        );
        scene.text(x + 24.0, 288.0, 3.0, *label, INK);
        if i < labels.len() - 1 {
            scene.rect(
                Rect {
                    x: x + 150.0,
                    y: 292.0,
                    w: 36.0,
                    h: 4.0,
                },
                MUTED,
            );
        }
    }
    scene.text(360.0, 430.0, 4.0, format!("Output: {value}"), INK);
}

fn draw_list_lab(scene: &mut Scene, name: &str, value: String) {
    scene.evidence.scene_kind = "list_transform_native_app".to_owned();
    scene.evidence.native_widgets = vec!["list_rows", "transform_badges", "output"];
    app_frame(scene, name);
    for (i, row) in ["Alpha", "Beta", "Gamma", "Delta"].iter().enumerate() {
        let y = 190.0 + i as f32 * 72.0;
        scene.rect(
            Rect {
                x: 380.0,
                y,
                w: 330.0,
                h: 52.0,
            },
            PANEL,
        );
        scene.text(408.0, y + 16.0, 3.0, *row, INK);
        scene.text(760.0, y + 16.0, 3.0, "->", MUTED);
        scene.rect(
            Rect {
                x: 820.0,
                y,
                w: 220.0,
                h: 52.0,
            },
            Color([0.88, 0.94, 1.0, 1.0]),
        );
        scene.text(846.0, y + 16.0, 3.0, format!("{row} {value}"), BLUE);
    }
}

fn draw_workbench(scene: &mut Scene, name: &str, value: String, selected: usize, total: usize) {
    scene.evidence.scene_kind = "native_workbench_app".to_owned();
    scene.evidence.native_widgets = vec!["title", "output_card", "status_card"];
    app_frame(scene, name);
    scene.rect(
        Rect {
            x: 380.0,
            y: 200.0,
            w: 680.0,
            h: 160.0,
        },
        PANEL,
    );
    scene.text(420.0, 240.0, 3.0, "Render output", BLUE);
    scene.text(420.0, 292.0, 5.0, value, INK);
    scene.rect(
        Rect {
            x: 380.0,
            y: 410.0,
            w: 680.0,
            h: 130.0,
        },
        PANEL,
    );
    scene.text(420.0, 454.0, 3.0, "Loaded from DD matrix", INK);
    scene.text(
        420.0,
        500.0,
        3.0,
        format!("Example {} of {}", selected + 1, total),
        MUTED,
    );
}

fn app_frame(scene: &mut Scene, title: &str) {
    scene.rect(
        Rect {
            x: 278.0,
            y: 0.0,
            w: 922.0,
            h: 800.0,
        },
        BG,
    );
    scene.text(340.0, 62.0, 6.0, title, RED);
}

fn input(scene: &mut Scene, x: f32, y: f32, w: f32, label: &str) {
    scene.rect(Rect { x, y, w, h: 52.0 }, PANEL);
    scene.rect(Rect { x, y, w, h: 2.0 }, Color([0.72, 0.74, 0.78, 1.0]));
    scene.text(x + 18.0, y + 18.0, 3.0, label, MUTED);
}

fn button(scene: &mut Scene, x: f32, y: f32, w: f32, label: &str, color: Color) {
    let scale = 2.6;
    let width = w.max(label.chars().count() as f32 * 6.0 * scale + 36.0);
    scene.rect(
        Rect {
            x,
            y,
            w: width,
            h: 48.0,
        },
        color,
    );
    scene.text(x + 18.0, y + 16.0, scale, label, PANEL);
}

fn pill(scene: &mut Scene, x: f32, y: f32, label: &str, selected: bool) {
    let scale = 2.2;
    let width = label.chars().count() as f32 * 6.0 * scale + 32.0;
    scene.rect(
        Rect {
            x,
            y,
            w: width,
            h: 34.0,
        },
        if selected {
            Color([1.0, 0.95, 0.95, 1.0])
        } else {
            PANEL
        },
    );
    scene.text(x + 16.0, y + 12.0, scale, label, INK);
}

fn table(scene: &mut Scene, x: f32, y: f32, rows: &[&str], selected: usize) {
    scene.rect(
        Rect {
            x,
            y,
            w: 280.0,
            h: 220.0,
        },
        PANEL,
    );
    for (i, row) in rows.iter().enumerate() {
        let row_y = y + i as f32 * 56.0;
        if i == selected {
            scene.rect(
                Rect {
                    x,
                    y: row_y,
                    w: 280.0,
                    h: 56.0,
                },
                Color([0.88, 0.94, 1.0, 1.0]),
            );
        }
        scene.rect(
            Rect {
                x,
                y: row_y + 55.0,
                w: 280.0,
                h: 1.0,
            },
            LINE,
        );
        scene.text(x + 18.0, row_y + 18.0, 3.0, *row, INK);
    }
}

fn ring(scene: &mut Scene, cx: f32, cy: f32, radius: f32, color: Color) {
    for i in 0..32 {
        let angle = i as f32 / 32.0 * std::f32::consts::TAU;
        let x = cx + angle.cos() * radius;
        let y = cy + angle.sin() * radius;
        scene.rect(
            Rect {
                x,
                y,
                w: 4.0,
                h: 4.0,
            },
            color,
        );
    }
}

fn render_text(output: &boon_dd::SmokeOutput) -> String {
    output
        .render
        .first()
        .map(|command| match command {
            boon_dd::RenderCommand::PatchText { text, .. } => text.clone(),
        })
        .unwrap_or_default()
}

fn scene_vertices(width: f32, height: f32, scene: &Scene) -> Vec<Vertex> {
    let mut vertices = Vec::new();
    for primitive in &scene.primitives {
        match primitive {
            Primitive::Rect { rect, color } => {
                rect_vertices(&mut vertices, width, height, *rect, *color)
            }
            Primitive::Text {
                x,
                y,
                scale,
                text,
                color,
            } => {
                text_vertices(&mut vertices, width, height, *x, *y, *scale, text, *color);
            }
        }
    }
    vertices
}

fn rect_vertices(vertices: &mut Vec<Vertex>, width: f32, height: f32, rect: Rect, color: Color) {
    let x0 = x_to_ndc(rect.x, width);
    let x1 = x_to_ndc(rect.x + rect.w, width);
    let y0 = y_to_ndc(rect.y, height);
    let y1 = y_to_ndc(rect.y + rect.h, height);
    vertices.extend_from_slice(&[
        Vertex {
            position: [x0, y0],
            color: color.0,
        },
        Vertex {
            position: [x1, y0],
            color: color.0,
        },
        Vertex {
            position: [x1, y1],
            color: color.0,
        },
        Vertex {
            position: [x0, y0],
            color: color.0,
        },
        Vertex {
            position: [x1, y1],
            color: color.0,
        },
        Vertex {
            position: [x0, y1],
            color: color.0,
        },
    ]);
}

fn text_vertices(
    vertices: &mut Vec<Vertex>,
    width: f32,
    height: f32,
    x: f32,
    y: f32,
    scale: f32,
    text: &str,
    color: Color,
) {
    let mut cursor = x;
    for ch in text.chars() {
        for (row, bits) in glyph(ch).iter().enumerate() {
            for (col, bit) in bits.bytes().enumerate() {
                if bit == b'1' {
                    rect_vertices(
                        vertices,
                        width,
                        height,
                        Rect {
                            x: cursor + col as f32 * scale,
                            y: y + row as f32 * scale,
                            w: scale,
                            h: scale,
                        },
                        color,
                    );
                }
            }
        }
        cursor += 6.0 * scale;
    }
}

fn vertices_as_bytes(vertices: &[Vertex]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            vertices.as_ptr().cast::<u8>(),
            std::mem::size_of_val(vertices),
        )
    }
}

fn write_per_example_screenshots(
    screenshots_dir: Option<&std::path::Path>,
    examples: &[(String, boon_dd::SmokeOutput)],
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let Some(screenshots_dir) = screenshots_dir else {
        return Ok(Vec::new());
    };
    if screenshots_dir.exists() {
        std::fs::remove_dir_all(screenshots_dir)?;
    }
    std::fs::create_dir_all(screenshots_dir)?;
    let mut entries = Vec::new();
    for (index, (name, _)) in examples.iter().enumerate() {
        let scene = build_scene(examples, index);
        let vertices = scene_vertices(WIDTH as f32, HEIGHT as f32, &scene);
        let pixels = rasterize_vertices(WIDTH, HEIGHT, &vertices);
        let screenshot = screenshots_dir.join(format!("{:02}-{name}.png", index + 1));
        write_png_rgb(&screenshot, WIDTH, HEIGHT, &pixels)?;
        entries.push(serde_json::json!({
            "example": name,
            "selected_index": index,
            "scene_kind": scene.evidence.scene_kind,
            "native_widgets": scene.evidence.native_widgets,
            "rendered_vertices": vertices.len(),
            "screenshot": screenshot,
        }));
    }
    Ok(entries)
}

fn write_json_atomic(
    path: &std::path::Path,
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

fn rasterize_vertices(width: u32, height: u32, vertices: &[Vertex]) -> Vec<u8> {
    let mut pixels = vec![240_u8; width as usize * height as usize * 3];
    for chunk in vertices.chunks(6) {
        if chunk.len() != 6 {
            continue;
        }
        let color = chunk[0].color;
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        for vertex in chunk {
            let x = ((vertex.position[0] + 1.0) * 0.5 * width as f32).round();
            let y = ((1.0 - vertex.position[1]) * 0.5 * height as f32).round();
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        let min_x = min_x.clamp(0.0, width as f32) as u32;
        let max_x = max_x.clamp(0.0, width as f32) as u32;
        let min_y = min_y.clamp(0.0, height as f32) as u32;
        let max_y = max_y.clamp(0.0, height as f32) as u32;
        let rgb = [
            (color[0].clamp(0.0, 1.0) * 255.0) as u8,
            (color[1].clamp(0.0, 1.0) * 255.0) as u8,
            (color[2].clamp(0.0, 1.0) * 255.0) as u8,
        ];
        for y in min_y..max_y {
            for x in min_x..max_x {
                let offset = (y as usize * width as usize + x as usize) * 3;
                pixels[offset..offset + 3].copy_from_slice(&rgb);
            }
        }
    }
    pixels
}

fn x_to_ndc(x: f32, width: f32) -> f32 {
    (x / width) * 2.0 - 1.0
}

fn y_to_ndc(y: f32, height: f32) -> f32 {
    1.0 - (y / height) * 2.0
}

fn write_png_rgb(
    path: &std::path::Path,
    width: u32,
    height: u32,
    rgb: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let row_len = width as usize * 3;
    let mut raw = Vec::with_capacity((row_len + 1) * height as usize);
    for row in rgb.chunks(row_len) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut zlib = Vec::new();
    zlib.extend_from_slice(&[0x78, 0x01]);
    let mut remaining = raw.as_slice();
    while !remaining.is_empty() {
        let len = remaining.len().min(65_535);
        let final_block = len == remaining.len();
        zlib.push(if final_block { 0x01 } else { 0x00 });
        zlib.extend_from_slice(&(len as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(len as u16)).to_le_bytes());
        zlib.extend_from_slice(&remaining[..len]);
        remaining = &remaining[len..];
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    write_png_chunk(&mut png, b"IHDR", &ihdr);
    write_png_chunk(&mut png, b"IDAT", &zlib);
    write_png_chunk(&mut png, b"IEND", &[]);
    std::fs::write(path, png)?;
    Ok(())
}

fn write_png_chunk(png: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    png.extend_from_slice(name);
    png.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(name.len() + data.len());
    crc_input.extend_from_slice(name);
    crc_input.extend_from_slice(data);
    png.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn adler32(bytes: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let mut a = 1_u32;
    let mut b = 0_u32;
    for byte in bytes {
        a = (a + u32::from(*byte)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn glyph(ch: char) -> [&'static str; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [
            "01110", "10001", "10001", "11111", "10001", "10001", "10001",
        ],
        'B' => [
            "11110", "10001", "10001", "11110", "10001", "10001", "11110",
        ],
        'C' => [
            "01111", "10000", "10000", "10000", "10000", "10000", "01111",
        ],
        'D' => [
            "11110", "10001", "10001", "10001", "10001", "10001", "11110",
        ],
        'E' => [
            "11111", "10000", "10000", "11110", "10000", "10000", "11111",
        ],
        'F' => [
            "11111", "10000", "10000", "11110", "10000", "10000", "10000",
        ],
        'G' => [
            "01111", "10000", "10000", "10011", "10001", "10001", "01110",
        ],
        'H' => [
            "10001", "10001", "10001", "11111", "10001", "10001", "10001",
        ],
        'I' => [
            "11111", "00100", "00100", "00100", "00100", "00100", "11111",
        ],
        'J' => [
            "00111", "00010", "00010", "00010", "10010", "10010", "01100",
        ],
        'K' => [
            "10001", "10010", "10100", "11000", "10100", "10010", "10001",
        ],
        'L' => [
            "10000", "10000", "10000", "10000", "10000", "10000", "11111",
        ],
        'M' => [
            "10001", "11011", "10101", "10101", "10001", "10001", "10001",
        ],
        'N' => [
            "10001", "11001", "10101", "10011", "10001", "10001", "10001",
        ],
        'O' => [
            "01110", "10001", "10001", "10001", "10001", "10001", "01110",
        ],
        'P' => [
            "11110", "10001", "10001", "11110", "10000", "10000", "10000",
        ],
        'Q' => [
            "01110", "10001", "10001", "10001", "10101", "10010", "01101",
        ],
        'R' => [
            "11110", "10001", "10001", "11110", "10100", "10010", "10001",
        ],
        'S' => [
            "01111", "10000", "10000", "01110", "00001", "00001", "11110",
        ],
        'T' => [
            "11111", "00100", "00100", "00100", "00100", "00100", "00100",
        ],
        'U' => [
            "10001", "10001", "10001", "10001", "10001", "10001", "01110",
        ],
        'V' => [
            "10001", "10001", "10001", "10001", "10001", "01010", "00100",
        ],
        'W' => [
            "10001", "10001", "10001", "10101", "10101", "10101", "01010",
        ],
        'X' => [
            "10001", "10001", "01010", "00100", "01010", "10001", "10001",
        ],
        'Y' => [
            "10001", "10001", "01010", "00100", "00100", "00100", "00100",
        ],
        'Z' => [
            "11111", "00001", "00010", "00100", "01000", "10000", "11111",
        ],
        '0' => [
            "01110", "10001", "10011", "10101", "11001", "10001", "01110",
        ],
        '1' => [
            "00100", "01100", "00100", "00100", "00100", "00100", "01110",
        ],
        '2' => [
            "01110", "10001", "00001", "00010", "00100", "01000", "11111",
        ],
        '3' => [
            "11110", "00001", "00001", "01110", "00001", "00001", "11110",
        ],
        '4' => [
            "00010", "00110", "01010", "10010", "11111", "00010", "00010",
        ],
        '5' => [
            "11111", "10000", "10000", "11110", "00001", "00001", "11110",
        ],
        '6' => [
            "01110", "10000", "10000", "11110", "10001", "10001", "01110",
        ],
        '7' => [
            "11111", "00001", "00010", "00100", "01000", "01000", "01000",
        ],
        '8' => [
            "01110", "10001", "10001", "01110", "10001", "10001", "01110",
        ],
        '9' => [
            "01110", "10001", "10001", "01111", "00001", "00001", "01110",
        ],
        '_' => [
            "00000", "00000", "00000", "00000", "00000", "00000", "11111",
        ],
        '-' => [
            "00000", "00000", "00000", "11111", "00000", "00000", "00000",
        ],
        ':' => [
            "00000", "00100", "00100", "00000", "00100", "00100", "00000",
        ],
        '/' => [
            "00001", "00010", "00010", "00100", "01000", "01000", "10000",
        ],
        '>' => [
            "10000", "01000", "00100", "00010", "00100", "01000", "10000",
        ],
        '=' => [
            "00000", "11111", "00000", "11111", "00000", "00000", "00000",
        ],
        ' ' => [
            "00000", "00000", "00000", "00000", "00000", "00000", "00000",
        ],
        _ => [
            "11111", "00001", "00010", "00100", "00000", "00100", "00100",
        ],
    }
}
