use app_window::coordinates::{Position, Size};
use app_window::window::Window;
use some_executor::SomeExecutor;
use some_executor::observer::Observer;
use std::env;
use std::path::PathBuf;

fn main() {
    let artifact = env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: native_smoke <artifact-json>");

    app_window::application::main(move || {
        let task = some_executor::task::Task::without_notifications(
            "boon-dd-native-smoke".into(),
            some_executor::task::Configuration::new(
                some_executor::hint::Hint::Unknown,
                some_executor::Priority::UserInteractive,
                some_executor::Instant::now(),
            ),
            async move {
                let mut window = Window::new(
                    Position::new(40.0, 40.0),
                    Size::new(320.0, 180.0),
                    "Boon DD Native Smoke".to_owned(),
                )
                .await;
                let surface = window.surface().await;
                let (size, scale) = surface.size_scale().await;
                std::fs::write(
                    &artifact,
                    serde_json::to_vec_pretty(&serde_json::json!({
                        "backend": "app_window",
                        "window_created": true,
                        "surface_created": true,
                        "surface": {
                            "width": size.width(),
                            "height": size.height(),
                            "scale": scale
                        }
                    }))
                    .expect("native smoke JSON should serialize"),
                )
                .expect("native smoke artifact should be writable");
                let _ = surface;
                let _ = window;
                std::process::exit(0);
            },
        );
        some_executor::current_executor::current_executor()
            .spawn_objsafe(task.into_objsafe())
            .detach();
    });
}
