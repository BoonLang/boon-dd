pub struct ExampleFixture {
    pub name: &'static str,
    pub source: &'static str,
    pub scenario: &'static str,
}

macro_rules! fixture {
    ($name:literal) => {
        ExampleFixture {
            name: $name,
            source: include_str!(concat!("../../../examples/", $name, "/source.bn")),
            scenario: include_str!(concat!("../../../examples/", $name, "/scenario.toml")),
        }
    };
}

pub const REQUIRED_FIXTURES: &[ExampleFixture] = &[
    fixture!("counter"),
    fixture!("counter_hold"),
    fixture!("interval"),
    fixture!("interval_hold"),
    fixture!("latest"),
    fixture!("when"),
    fixture!("while"),
    fixture!("then"),
    fixture!("list_map_block"),
    fixture!("list_map_external_dep"),
    fixture!("list_object_state"),
    fixture!("list_retain_count"),
    fixture!("list_retain_reactive"),
    fixture!("list_retain_remove"),
    fixture!("shopping_list"),
    fixture!("todo_mvc"),
    fixture!("crud"),
    fixture!("flight_booker"),
    fixture!("temperature_converter"),
    fixture!("pong"),
    fixture!("cells"),
    fixture!("todo_mvc_physical"),
];

pub fn required_examples() -> &'static [&'static str] {
    boon_dd::REQUIRED_EXAMPLES
}

pub fn run_embedded_matrix() -> Vec<(String, boon_dd::SmokeOutput)> {
    REQUIRED_FIXTURES
        .iter()
        .filter_map(|fixture| {
            let scenario = boon_runtime_host::parse_scenario(fixture.scenario);
            boon_runtime_host::RuntimeHost
                .compile_and_run_step(
                    &format!("examples/{}/source.bn", fixture.name),
                    fixture.source,
                    &scenario,
                )
                .map(|output| (fixture.name.to_owned(), output))
        })
        .collect()
}

pub fn run_embedded_matrix_json() -> Result<String, serde_json::Error> {
    serde_json::to_string(&run_embedded_matrix())
}
