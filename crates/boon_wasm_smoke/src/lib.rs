use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn run_smoke_json() -> Result<String, JsValue> {
    boon_examples::run_embedded_matrix_json().map_err(|error| JsValue::from_str(&error.to_string()))
}
