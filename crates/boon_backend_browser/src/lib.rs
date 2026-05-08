pub fn browser_wasm_output(
    _source_path: &str,
    source_text: &str,
    scenario_text: &str,
) -> Option<boon_dd::SmokeOutput> {
    boon_examples::run_generated_for_checked_source(source_text, scenario_text)
        .map(|(_name, output)| output)
}

pub fn browser_wasm_matrix_len() -> usize {
    boon_dd::REQUIRED_EXAMPLES.len()
}
