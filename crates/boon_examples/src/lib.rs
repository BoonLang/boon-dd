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

pub const GENERATED_CORPUS: &[ExampleFixture] = &[
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

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedScenarioStepOutput {
    pub step_index: usize,
    pub description: String,
    pub event_count: usize,
    pub event_order: Vec<String>,
    pub action_count: usize,
    pub commands: Vec<boon_dd::ScenarioCommand>,
    pub expected_text: String,
    pub output: boon_dd::SmokeOutput,
}

fn empty_output() -> boon_dd::SmokeOutput {
    boon_dd::SmokeOutput {
        monitor: Vec::new(),
        render: Vec::new(),
        effects: Vec::new(),
        persistence: Vec::new(),
    }
}

macro_rules! run_generated_fixture_steps {
    ($expected_name:literal, $fixture:expr, $crate_name:ident, $steps:expr) => {{
        assert_eq!(
            $fixture.name, $expected_name,
            "generated fixture registry order drifted"
        );
        let allocator = || timely::communication::Allocator::Thread(
            timely::communication::allocator::Thread::default(),
        );
        let mut worker = timely::worker::Worker::new(timely::WorkerConfig::default(), allocator(), None);
        let mut graph = $crate_name::graph::build_dataflow(&mut worker);
        let has_persistence_tap = $crate_name::persist_bindings::has_persistence_tap();
        let mut persistence_enabled = false;
        let mut persisted_text: Option<String> = None;
        let mut last_generated_persisted_text: Option<String> = None;
        let mut outputs = Vec::new();
        for (step_index, step) in $steps.iter().enumerate() {
            let epoch = step_index as u64 + 1;
            for event in &step.events {
                match event {
                    boon_dd::ScenarioEvent::Source(action) => {
                        graph.sources.submit_action(action, epoch);
                    }
                    boon_dd::ScenarioEvent::Command(command)
                        if command.command == "enable_persistence" =>
                    {
                        if has_persistence_tap {
                            persistence_enabled = true;
                            persisted_text = last_generated_persisted_text.clone();
                        }
                    }
                    boon_dd::ScenarioEvent::Command(command) if command.command == "reload" => {
                        worker = timely::worker::Worker::new(
                            timely::WorkerConfig::default(),
                            allocator(),
                            None,
                        );
                        graph = $crate_name::graph::build_dataflow(&mut worker);
                        if persistence_enabled {
                            if let Some(value) = persisted_text.clone() {
                                graph.sources.submit_persisted_text(value, epoch);
                            }
                        }
                    }
                    boon_dd::ScenarioEvent::Command(_) => {}
                }
            }
            graph.sources.close_epoch(epoch);
            let target = $crate_name::graph::completion_time(epoch) + 1;
            let mut worker_steps = 0_usize;
            while graph.probe.less_than(&target) {
                if worker_steps == 1024 {
                    panic!(
                        "generated fixture {} step {} probe stalled at {target} after {worker_steps} steps",
                        $fixture.name, step_index
                    );
                }
                worker.step();
                worker_steps += 1;
            }
            let mut drained_outputs = graph.sources.take_outputs();
            let output = drained_outputs.pop().unwrap_or_else(empty_output);
            last_generated_persisted_text = output.persistence.iter().rev().find_map(|command| {
                match command {
                    boon_dd::PersistenceCommand::SaveText { value, .. } => Some(value.clone()),
                    boon_dd::PersistenceCommand::LoadText { .. } => None,
                }
            });
            outputs.push(GeneratedScenarioStepOutput {
                step_index,
                description: step.description.clone(),
                event_count: step.events.len(),
                event_order: step
                    .events
                    .iter()
                    .map(|event| match event {
                        boon_dd::ScenarioEvent::Source(action) => {
                            format!("source:{}", action.source)
                        }
                        boon_dd::ScenarioEvent::Command(command) => {
                            format!("command:{}", command.command)
                        }
                    })
                    .collect(),
                action_count: step.actions.len(),
                commands: step.commands.clone(),
                expected_text: step.expect_text.clone(),
                output,
            });
        }
        ($fixture.name.to_owned(), outputs)
    }};
}

pub fn run_generated_steps_at(
    index: usize,
    steps: &[boon_dd::ScenarioStep],
) -> Option<(String, Vec<GeneratedScenarioStepOutput>)> {
    Some(match index {
        0 => {
            run_generated_fixture_steps!("counter", &GENERATED_CORPUS[0], generated_counter, steps)
        }
        1 => run_generated_fixture_steps!(
            "counter_hold",
            &GENERATED_CORPUS[1],
            generated_counter_hold,
            steps
        ),
        2 => run_generated_fixture_steps!(
            "interval",
            &GENERATED_CORPUS[2],
            generated_interval,
            steps
        ),
        3 => run_generated_fixture_steps!(
            "interval_hold",
            &GENERATED_CORPUS[3],
            generated_interval_hold,
            steps
        ),
        4 => run_generated_fixture_steps!("latest", &GENERATED_CORPUS[4], generated_latest, steps),
        5 => run_generated_fixture_steps!("when", &GENERATED_CORPUS[5], generated_when, steps),
        6 => run_generated_fixture_steps!("while", &GENERATED_CORPUS[6], generated_while, steps),
        7 => run_generated_fixture_steps!("then", &GENERATED_CORPUS[7], generated_then, steps),
        8 => run_generated_fixture_steps!(
            "list_map_block",
            &GENERATED_CORPUS[8],
            generated_list_map_block,
            steps
        ),
        9 => run_generated_fixture_steps!(
            "list_map_external_dep",
            &GENERATED_CORPUS[9],
            generated_list_map_external_dep,
            steps
        ),
        10 => run_generated_fixture_steps!(
            "list_object_state",
            &GENERATED_CORPUS[10],
            generated_list_object_state,
            steps
        ),
        11 => run_generated_fixture_steps!(
            "list_retain_count",
            &GENERATED_CORPUS[11],
            generated_list_retain_count,
            steps
        ),
        12 => run_generated_fixture_steps!(
            "list_retain_reactive",
            &GENERATED_CORPUS[12],
            generated_list_retain_reactive,
            steps
        ),
        13 => run_generated_fixture_steps!(
            "list_retain_remove",
            &GENERATED_CORPUS[13],
            generated_list_retain_remove,
            steps
        ),
        14 => run_generated_fixture_steps!(
            "shopping_list",
            &GENERATED_CORPUS[14],
            generated_shopping_list,
            steps
        ),
        15 => run_generated_fixture_steps!(
            "todo_mvc",
            &GENERATED_CORPUS[15],
            generated_todo_mvc,
            steps
        ),
        16 => run_generated_fixture_steps!("crud", &GENERATED_CORPUS[16], generated_crud, steps),
        17 => run_generated_fixture_steps!(
            "flight_booker",
            &GENERATED_CORPUS[17],
            generated_flight_booker,
            steps
        ),
        18 => run_generated_fixture_steps!(
            "temperature_converter",
            &GENERATED_CORPUS[18],
            generated_temperature_converter,
            steps
        ),
        19 => run_generated_fixture_steps!("pong", &GENERATED_CORPUS[19], generated_pong, steps),
        20 => run_generated_fixture_steps!("cells", &GENERATED_CORPUS[20], generated_cells, steps),
        21 => run_generated_fixture_steps!(
            "todo_mvc_physical",
            &GENERATED_CORPUS[21],
            generated_todo_mvc_physical,
            steps
        ),
        _ => return None,
    })
}

pub fn run_generated_scenario_at(
    index: usize,
    scenario_text: &str,
) -> Option<(String, boon_dd::SmokeOutput)> {
    let steps = scenario_steps_for_text(scenario_text);
    run_generated_steps_at(index, &steps).map(|(name, steps)| {
        let output = steps
            .into_iter()
            .last()
            .map(|step| step.output)
            .unwrap_or_else(empty_output);
        (name, output)
    })
}

pub fn run_generated_scenario_steps_at(
    index: usize,
    scenario_text: &str,
) -> Option<(String, Vec<GeneratedScenarioStepOutput>)> {
    let steps = scenario_steps_for_text(scenario_text);
    run_generated_steps_at(index, &steps)
}

pub fn run_embedded_matrix() -> Vec<(String, boon_dd::SmokeOutput)> {
    GENERATED_CORPUS
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
