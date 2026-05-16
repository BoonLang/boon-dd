use ply_engine::prelude::*;

fn window_conf() -> macroquad::conf::Conf {
    boon_backend_ply::window_conf()
}

#[macroquad::main(window_conf)]
async fn main() {
    boon_backend_ply::run_macroquad_app().await;
}
