use wasm_bindgen::prelude::*;

macro_rules! run_generated {
    ($name:literal, $crate_name:ident) => {{
        let allocator = timely::communication::Allocator::Thread(
            timely::communication::allocator::Thread::default(),
        );
        let mut worker =
            timely::worker::Worker::new(timely::WorkerConfig::default(), allocator, None);
        let mut graph = $crate_name::graph::build_dataflow(&mut worker);
        let outputs = graph
            .submit_text_and_drain(&mut worker, $crate_name::graph::smoke_input_text(), 1, 1024)
            .expect("generated browser WASM graph should drain");
        (
            $name,
            outputs
                .into_iter()
                .last()
                .expect("generated browser WASM graph emitted no output"),
        )
    }};
}

#[wasm_bindgen]
pub fn run_smoke_json() -> Result<String, JsValue> {
    serde_json::to_string(&run_generated_matrix())
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

fn run_generated_matrix() -> Vec<(&'static str, boon_dd::SmokeOutput)> {
    vec![
        run_generated!("counter", generated_counter),
        run_generated!("counter_hold", generated_counter_hold),
        run_generated!("interval", generated_interval),
        run_generated!("interval_hold", generated_interval_hold),
        run_generated!("latest", generated_latest),
        run_generated!("when", generated_when),
        run_generated!("while", generated_while),
        run_generated!("then", generated_then),
        run_generated!("list_map_block", generated_list_map_block),
        run_generated!("list_map_external_dep", generated_list_map_external_dep),
        run_generated!("list_object_state", generated_list_object_state),
        run_generated!("list_retain_count", generated_list_retain_count),
        run_generated!("list_retain_reactive", generated_list_retain_reactive),
        run_generated!("list_retain_remove", generated_list_retain_remove),
        run_generated!("shopping_list", generated_shopping_list),
        run_generated!("todo_mvc", generated_todo_mvc),
        run_generated!("crud", generated_crud),
        run_generated!("flight_booker", generated_flight_booker),
        run_generated!("temperature_converter", generated_temperature_converter),
        run_generated!("pong", generated_pong),
        run_generated!("cells", generated_cells),
        run_generated!("todo_mvc_physical", generated_todo_mvc_physical),
    ]
}
