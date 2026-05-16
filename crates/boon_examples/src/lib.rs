pub fn required_examples() -> &'static [&'static str] {
    boon_dd::REQUIRED_EXAMPLES
}

pub fn scenario_source_actions_for_text(scenario_text: &str) -> Vec<boon_dd::SourceAction> {
    boon_runtime_host::parse_scenario(scenario_text)
        .steps
        .into_iter()
        .flat_map(|step| {
            step.events.into_iter().filter_map(|event| match event {
                boon_dd::ScenarioEvent::Source(action) => Some(action),
                boon_dd::ScenarioEvent::Command(_) => None,
            })
        })
        .collect()
}

pub fn scenario_steps_for_text(scenario_text: &str) -> Vec<boon_dd::ScenarioStep> {
    boon_runtime_host::parse_scenario(scenario_text).steps
}
