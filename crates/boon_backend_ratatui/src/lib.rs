pub fn render_terminal(
    _source_path: &str,
    source_text: &str,
    scenario_text: &str,
) -> Option<boon_dd::SmokeOutput> {
    boon_examples::run_generated_for_source(source_text, scenario_text)
        .map(|(_name, output)| output)
}
