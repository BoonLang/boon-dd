pub fn render_terminal(
    source_path: &str,
    source_text: &str,
    scenario_text: &str,
) -> Option<boon_dd::SmokeOutput> {
    let scenario = boon_runtime_host::parse_scenario(scenario_text);
    boon_runtime_host::RuntimeHost.compile_and_run_step(source_path, source_text, &scenario)
}
