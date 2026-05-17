pub fn browser_wasm_output(
    source_path: &str,
    source_text: &str,
    scenario_text: &str,
) -> Option<boon_dd::SmokeOutput> {
    boon_runtime_host::run_dd_graph_scenario(source_path, source_text, scenario_text).ok()
}

pub fn browser_wasm_matrix_len() -> usize {
    boon_dd::REQUIRED_EXAMPLES.len()
}
