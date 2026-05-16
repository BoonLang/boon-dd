pub fn render_commands(
    source_path: &str,
    source_text: &str,
    scenario_text: &str,
) -> Option<boon_dd::SmokeOutput> {
    boon_runtime_host::run_compiled_source_scenario(source_path, source_text, scenario_text).ok()
}
