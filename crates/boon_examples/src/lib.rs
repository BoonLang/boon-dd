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

fn sha256_text(text: &str) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(text.as_bytes());
    format!("{digest:x}")
}

pub fn scenario_actions_for_text(scenario_text: &str) -> Vec<boon_dd::SourceAction> {
    boon_runtime_host::parse_scenario(scenario_text)
        .steps
        .first()
        .map(|step| step.actions.clone())
        .unwrap_or_default()
}

macro_rules! run_generated_fixture_actions {
    ($expected_name:literal, $fixture:expr, $crate_name:ident, $actions:expr) => {{
        assert_eq!(
            $fixture.name, $expected_name,
            "generated fixture registry order drifted"
        );
        let allocator = timely::communication::Allocator::Thread(
            timely::communication::allocator::Thread::default(),
        );
        let mut worker =
            timely::worker::Worker::new(timely::WorkerConfig::default(), allocator, None);
        let mut graph = $crate_name::graph::build_dataflow(&mut worker);
        let epoch = 1_u64;
        let mut submitted = false;
        for action in $actions {
            graph.sources.submit_action(action, epoch);
            submitted = true;
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
            .unwrap_or_else(|| boon_dd::SmokeOutput {
                monitor: Vec::new(),
                render: Vec::new(),
            });
        ($fixture.name.to_owned(), output)
    }};
}

pub fn run_generated_actions_at(
    index: usize,
    actions: &[boon_dd::SourceAction],
) -> Option<(String, boon_dd::SmokeOutput)> {
    Some(match index {
        0 => run_generated_fixture_actions!(
            "counter",
            &REQUIRED_FIXTURES[0],
            generated_counter,
            actions
        ),
        1 => run_generated_fixture_actions!(
            "counter_hold",
            &REQUIRED_FIXTURES[1],
            generated_counter_hold,
            actions
        ),
        2 => run_generated_fixture_actions!(
            "interval",
            &REQUIRED_FIXTURES[2],
            generated_interval,
            actions
        ),
        3 => run_generated_fixture_actions!(
            "interval_hold",
            &REQUIRED_FIXTURES[3],
            generated_interval_hold,
            actions
        ),
        4 => run_generated_fixture_actions!(
            "latest",
            &REQUIRED_FIXTURES[4],
            generated_latest,
            actions
        ),
        5 => run_generated_fixture_actions!("when", &REQUIRED_FIXTURES[5], generated_when, actions),
        6 => {
            run_generated_fixture_actions!("while", &REQUIRED_FIXTURES[6], generated_while, actions)
        }
        7 => run_generated_fixture_actions!("then", &REQUIRED_FIXTURES[7], generated_then, actions),
        8 => run_generated_fixture_actions!(
            "list_map_block",
            &REQUIRED_FIXTURES[8],
            generated_list_map_block,
            actions
        ),
        9 => run_generated_fixture_actions!(
            "list_map_external_dep",
            &REQUIRED_FIXTURES[9],
            generated_list_map_external_dep,
            actions
        ),
        10 => run_generated_fixture_actions!(
            "list_object_state",
            &REQUIRED_FIXTURES[10],
            generated_list_object_state,
            actions
        ),
        11 => run_generated_fixture_actions!(
            "list_retain_count",
            &REQUIRED_FIXTURES[11],
            generated_list_retain_count,
            actions
        ),
        12 => run_generated_fixture_actions!(
            "list_retain_reactive",
            &REQUIRED_FIXTURES[12],
            generated_list_retain_reactive,
            actions
        ),
        13 => run_generated_fixture_actions!(
            "list_retain_remove",
            &REQUIRED_FIXTURES[13],
            generated_list_retain_remove,
            actions
        ),
        14 => run_generated_fixture_actions!(
            "shopping_list",
            &REQUIRED_FIXTURES[14],
            generated_shopping_list,
            actions
        ),
        15 => run_generated_fixture_actions!(
            "todo_mvc",
            &REQUIRED_FIXTURES[15],
            generated_todo_mvc,
            actions
        ),
        16 => {
            run_generated_fixture_actions!("crud", &REQUIRED_FIXTURES[16], generated_crud, actions)
        }
        17 => run_generated_fixture_actions!(
            "flight_booker",
            &REQUIRED_FIXTURES[17],
            generated_flight_booker,
            actions
        ),
        18 => run_generated_fixture_actions!(
            "temperature_converter",
            &REQUIRED_FIXTURES[18],
            generated_temperature_converter,
            actions
        ),
        19 => {
            run_generated_fixture_actions!("pong", &REQUIRED_FIXTURES[19], generated_pong, actions)
        }
        20 => run_generated_fixture_actions!(
            "cells",
            &REQUIRED_FIXTURES[20],
            generated_cells,
            actions
        ),
        21 => run_generated_fixture_actions!(
            "todo_mvc_physical",
            &REQUIRED_FIXTURES[21],
            generated_todo_mvc_physical,
            actions
        ),
        _ => return None,
    })
}

pub fn run_generated_scenario_at(
    index: usize,
    scenario_text: &str,
) -> Option<(String, boon_dd::SmokeOutput)> {
    let actions = scenario_actions_for_text(scenario_text);
    run_generated_actions_at(index, &actions)
}

pub fn run_generated_for_source(
    source_text: &str,
    scenario_text: &str,
) -> Option<(String, boon_dd::SmokeOutput)> {
    let source_hash = sha256_text(source_text);
    REQUIRED_FIXTURES
        .iter()
        .position(|fixture| sha256_text(fixture.source) == source_hash)
        .and_then(|index| run_generated_scenario_at(index, scenario_text))
}

pub fn run_embedded_matrix() -> Vec<(String, boon_dd::SmokeOutput)> {
    REQUIRED_FIXTURES
        .iter()
        .enumerate()
        .map(|(index, fixture)| {
            run_generated_scenario_at(index, fixture.scenario)
                .unwrap_or_else(|| panic!("missing generated fixture {}", fixture.name))
        })
        .collect()
}

pub fn run_embedded_matrix_json() -> Result<String, serde_json::Error> {
    serde_json::to_string(&run_embedded_matrix())
}
