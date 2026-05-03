use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn run_smoke_json() -> Result<String, JsValue> {
    let output = boon_dd::run_required_example_matrix_smoke();
    serde_json::to_string(&output).map_err(|error| JsValue::from_str(&error.to_string()))
}
