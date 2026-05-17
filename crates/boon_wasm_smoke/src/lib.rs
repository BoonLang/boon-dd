use wasm_bindgen::prelude::*;

macro_rules! run_manifest_source {
    ($name:literal, $crate_name:ident) => {{ ($name, $crate_name::run_checked_scenario()) }};
}

macro_rules! run_manifest_steps {
    ($name:literal, $crate_name:ident) => {{ ($name, $crate_name::run_checked_scenario_step_outputs()) }};
}

#[wasm_bindgen]
pub fn run_generated_manifest_json() -> Result<String, JsValue> {
    serde_json::to_string(&run_generated_manifest())
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

pub fn run_generated_manifest() -> Vec<(&'static str, boon_dd::SmokeOutput)> {
    vec![
        run_manifest_source!("counter", generated_counter),
        run_manifest_source!("counter_hold", generated_counter_hold),
        run_manifest_source!("interval", generated_interval),
        run_manifest_source!("interval_hold", generated_interval_hold),
        run_manifest_source!("latest", generated_latest),
        run_manifest_source!("when", generated_when),
        run_manifest_source!("while", generated_while),
        run_manifest_source!("then", generated_then),
        run_manifest_source!("list_map_block", generated_list_map_block),
        run_manifest_source!("list_map_external_dep", generated_list_map_external_dep),
        run_manifest_source!("list_object_state", generated_list_object_state),
        run_manifest_source!("list_retain_count", generated_list_retain_count),
        run_manifest_source!("list_retain_reactive", generated_list_retain_reactive),
        run_manifest_source!("list_retain_remove", generated_list_retain_remove),
        run_manifest_source!("shopping_list", generated_shopping_list),
        run_manifest_source!("todo_mvc", generated_todo_mvc),
        run_manifest_source!("crud", generated_crud),
        run_manifest_source!("flight_booker", generated_flight_booker),
        run_manifest_source!("temperature_converter", generated_temperature_converter),
        run_manifest_source!("pong", generated_pong),
        run_manifest_source!("cells", generated_cells),
        run_manifest_source!("todo_mvc_physical", generated_todo_mvc_physical),
    ]
}

pub fn run_generated_manifest_step_outputs() -> Vec<(&'static str, Vec<boon_dd::SmokeOutput>)> {
    vec![
        run_manifest_steps!("counter", generated_counter),
        run_manifest_steps!("counter_hold", generated_counter_hold),
        run_manifest_steps!("interval", generated_interval),
        run_manifest_steps!("interval_hold", generated_interval_hold),
        run_manifest_steps!("latest", generated_latest),
        run_manifest_steps!("when", generated_when),
        run_manifest_steps!("while", generated_while),
        run_manifest_steps!("then", generated_then),
        run_manifest_steps!("list_map_block", generated_list_map_block),
        run_manifest_steps!("list_map_external_dep", generated_list_map_external_dep),
        run_manifest_steps!("list_object_state", generated_list_object_state),
        run_manifest_steps!("list_retain_count", generated_list_retain_count),
        run_manifest_steps!("list_retain_reactive", generated_list_retain_reactive),
        run_manifest_steps!("list_retain_remove", generated_list_retain_remove),
        run_manifest_steps!("shopping_list", generated_shopping_list),
        run_manifest_steps!("todo_mvc", generated_todo_mvc),
        run_manifest_steps!("crud", generated_crud),
        run_manifest_steps!("flight_booker", generated_flight_booker),
        run_manifest_steps!("temperature_converter", generated_temperature_converter),
        run_manifest_steps!("pong", generated_pong),
        run_manifest_steps!("cells", generated_cells),
        run_manifest_steps!("todo_mvc_physical", generated_todo_mvc_physical),
    ]
}
