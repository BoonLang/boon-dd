use wasm_bindgen::prelude::*;

macro_rules! run_manifest_source {
    ($name:literal) => {{
        let output = boon_runtime_host::run_dd_graph_scenario(
            concat!("examples/", $name, "/source.bn"),
            include_str!(concat!("../../../examples/", $name, "/source.bn")),
            include_str!(concat!("../../../examples/", $name, "/scenario.toml")),
        )
        .map_err(|error| {
            JsValue::from_str(&format!(
                "compiled manifest scenario {name}: {error}",
                name = $name
            ))
        })?;
        Ok::<_, JsValue>(($name, output))
    }};
}

#[wasm_bindgen]
pub fn run_compiled_manifest_json() -> Result<String, JsValue> {
    serde_json::to_string(&run_compiled_manifest()?)
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

fn run_compiled_manifest() -> Result<Vec<(&'static str, boon_dd::SmokeOutput)>, JsValue> {
    Ok(vec![
        run_manifest_source!("counter")?,
        run_manifest_source!("counter_hold")?,
        run_manifest_source!("interval")?,
        run_manifest_source!("interval_hold")?,
        run_manifest_source!("latest")?,
        run_manifest_source!("when")?,
        run_manifest_source!("while")?,
        run_manifest_source!("then")?,
        run_manifest_source!("list_map_block")?,
        run_manifest_source!("list_map_external_dep")?,
        run_manifest_source!("list_object_state")?,
        run_manifest_source!("list_retain_count")?,
        run_manifest_source!("list_retain_reactive")?,
        run_manifest_source!("list_retain_remove")?,
        run_manifest_source!("shopping_list")?,
        run_manifest_source!("todo_mvc")?,
        run_manifest_source!("crud")?,
        run_manifest_source!("flight_booker")?,
        run_manifest_source!("temperature_converter")?,
        run_manifest_source!("pong")?,
        run_manifest_source!("cells")?,
        run_manifest_source!("todo_mvc_physical")?,
    ])
}
