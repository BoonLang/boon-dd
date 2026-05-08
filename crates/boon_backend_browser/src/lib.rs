pub fn browser_wasm_output(
    source_path: &str,
    source_text: &str,
    scenario_text: &str,
) -> Option<boon_dd::SmokeOutput> {
    let scenario = boon_runtime_host::parse_scenario(scenario_text);
    boon_runtime_host::RuntimeHost
        .compile_and_run_scenario(source_path, source_text, &scenario)
        .into_iter()
        .next()
}

pub fn browser_wasm_matrix_len() -> usize {
    boon_dd::REQUIRED_EXAMPLES.len()
}
