pub fn browser_wasm_output(
    _source_path: &str,
    _source_text: &str,
    _scenario_text: &str,
) -> Option<boon_dd::SmokeOutput> {
    None
}

pub fn browser_wasm_matrix_len() -> usize {
    boon_dd::REQUIRED_EXAMPLES.len()
}
