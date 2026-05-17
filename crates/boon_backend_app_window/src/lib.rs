pub fn native_noninteractive_smoke(
    source_path: &str,
    source_text: &str,
    scenario_text: &str,
) -> Option<boon_dd::SmokeOutput> {
    boon_runtime_host::run_dd_graph_scenario(source_path, source_text, scenario_text).ok()
}
