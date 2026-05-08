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

macro_rules! run_generated_fixture {
    ($expected_name:literal, $fixture:expr, $crate_name:ident) => {{
        assert_eq!(
            $fixture.name, $expected_name,
            "generated fixture registry order drifted"
        );
        let scenario = boon_runtime_host::parse_scenario($fixture.scenario);
        let allocator = timely::communication::Allocator::Thread(
            timely::communication::allocator::Thread::default(),
        );
        let mut worker =
            timely::worker::Worker::new(timely::WorkerConfig::default(), allocator, None);
        let mut graph = $crate_name::graph::build_dataflow(&mut worker);
        let epoch = 1_u64;
        let mut submitted = false;
        if let Some(step) = scenario.steps.first() {
            for action in &step.actions {
                graph
                    .sources
                    .submit_text(boon_dd::source_action_text(action), epoch);
                submitted = true;
            }
        }
        if !submitted {
            graph.sources.submit_text("", epoch);
        }
        graph.sources.close_epoch(epoch);
        let target = $crate_name::graph::completion_time(epoch) + 1;
        let mut steps = 0_usize;
        while graph.probe.less_than(&target) {
            if steps == 1024 {
                panic!(
                    "generated fixture {} probe stalled at {target} after {steps} steps",
                    $fixture.name
                );
            }
            worker.step();
            steps += 1;
        }
        let output = graph
            .sources
            .outputs()
            .into_iter()
            .last()
            .unwrap_or_else(|| panic!("generated fixture {} emitted no output", $fixture.name));
        ($fixture.name.to_owned(), output)
    }};
}

pub fn run_embedded_matrix() -> Vec<(String, boon_dd::SmokeOutput)> {
    vec![
        run_generated_fixture!("counter", &REQUIRED_FIXTURES[0], generated_counter),
        run_generated_fixture!(
            "counter_hold",
            &REQUIRED_FIXTURES[1],
            generated_counter_hold
        ),
        run_generated_fixture!("interval", &REQUIRED_FIXTURES[2], generated_interval),
        run_generated_fixture!(
            "interval_hold",
            &REQUIRED_FIXTURES[3],
            generated_interval_hold
        ),
        run_generated_fixture!("latest", &REQUIRED_FIXTURES[4], generated_latest),
        run_generated_fixture!("when", &REQUIRED_FIXTURES[5], generated_when),
        run_generated_fixture!("while", &REQUIRED_FIXTURES[6], generated_while),
        run_generated_fixture!("then", &REQUIRED_FIXTURES[7], generated_then),
        run_generated_fixture!(
            "list_map_block",
            &REQUIRED_FIXTURES[8],
            generated_list_map_block
        ),
        run_generated_fixture!(
            "list_map_external_dep",
            &REQUIRED_FIXTURES[9],
            generated_list_map_external_dep
        ),
        run_generated_fixture!(
            "list_object_state",
            &REQUIRED_FIXTURES[10],
            generated_list_object_state
        ),
        run_generated_fixture!(
            "list_retain_count",
            &REQUIRED_FIXTURES[11],
            generated_list_retain_count
        ),
        run_generated_fixture!(
            "list_retain_reactive",
            &REQUIRED_FIXTURES[12],
            generated_list_retain_reactive
        ),
        run_generated_fixture!(
            "list_retain_remove",
            &REQUIRED_FIXTURES[13],
            generated_list_retain_remove
        ),
        run_generated_fixture!(
            "shopping_list",
            &REQUIRED_FIXTURES[14],
            generated_shopping_list
        ),
        run_generated_fixture!("todo_mvc", &REQUIRED_FIXTURES[15], generated_todo_mvc),
        run_generated_fixture!("crud", &REQUIRED_FIXTURES[16], generated_crud),
        run_generated_fixture!(
            "flight_booker",
            &REQUIRED_FIXTURES[17],
            generated_flight_booker
        ),
        run_generated_fixture!(
            "temperature_converter",
            &REQUIRED_FIXTURES[18],
            generated_temperature_converter
        ),
        run_generated_fixture!("pong", &REQUIRED_FIXTURES[19], generated_pong),
        run_generated_fixture!("cells", &REQUIRED_FIXTURES[20], generated_cells),
        run_generated_fixture!(
            "todo_mvc_physical",
            &REQUIRED_FIXTURES[21],
            generated_todo_mvc_physical
        ),
    ]
}

pub fn run_embedded_matrix_json() -> Result<String, serde_json::Error> {
    serde_json::to_string(&run_embedded_matrix())
}
