pub fn render_terminal(example: &str) -> Option<boon_dd::SmokeOutput> {
    boon_dd::run_named_example_smoke(example)
}
