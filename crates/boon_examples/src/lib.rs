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

fn empty_smoke_output() -> boon_dd::SmokeOutput {
    boon_dd::SmokeOutput {
        monitor: Vec::new(),
        render: Vec::new(),
        effects: Vec::new(),
        persistence: Vec::new(),
    }
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
            graph.sources.submit_host_tick(epoch);
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
            .unwrap_or_else(empty_smoke_output);
        ($fixture.name.to_owned(), output)
    }};
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
            let output = graph
                .sources
                .outputs()
                .into_iter()
                .last()
                .unwrap_or_else(empty_smoke_output);
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

pub fn run_generated_actions_at(
    index: usize,
    actions: &[boon_dd::SourceAction],
) -> Option<(String, boon_dd::SmokeOutput)> {
    Some(match index {
        0 => run_generated_fixture_actions!(
            "counter",
            &GENERATED_CORPUS[0],
            generated_counter,
            actions
        ),
        1 => run_generated_fixture_actions!(
            "counter_hold",
            &GENERATED_CORPUS[1],
            generated_counter_hold,
            actions
        ),
        2 => run_generated_fixture_actions!(
            "interval",
            &GENERATED_CORPUS[2],
            generated_interval,
            actions
        ),
        3 => run_generated_fixture_actions!(
            "interval_hold",
            &GENERATED_CORPUS[3],
            generated_interval_hold,
            actions
        ),
        4 => run_generated_fixture_actions!(
            "latest",
            &GENERATED_CORPUS[4],
            generated_latest,
            actions
        ),
        5 => run_generated_fixture_actions!("when", &GENERATED_CORPUS[5], generated_when, actions),
        6 => {
            run_generated_fixture_actions!("while", &GENERATED_CORPUS[6], generated_while, actions)
        }
        7 => run_generated_fixture_actions!("then", &GENERATED_CORPUS[7], generated_then, actions),
        8 => run_generated_fixture_actions!(
            "list_map_block",
            &GENERATED_CORPUS[8],
            generated_list_map_block,
            actions
        ),
        9 => run_generated_fixture_actions!(
            "list_map_external_dep",
            &GENERATED_CORPUS[9],
            generated_list_map_external_dep,
            actions
        ),
        10 => run_generated_fixture_actions!(
            "list_object_state",
            &GENERATED_CORPUS[10],
            generated_list_object_state,
            actions
        ),
        11 => run_generated_fixture_actions!(
            "list_retain_count",
            &GENERATED_CORPUS[11],
            generated_list_retain_count,
            actions
        ),
        12 => run_generated_fixture_actions!(
            "list_retain_reactive",
            &GENERATED_CORPUS[12],
            generated_list_retain_reactive,
            actions
        ),
        13 => run_generated_fixture_actions!(
            "list_retain_remove",
            &GENERATED_CORPUS[13],
            generated_list_retain_remove,
            actions
        ),
        14 => run_generated_fixture_actions!(
            "shopping_list",
            &GENERATED_CORPUS[14],
            generated_shopping_list,
            actions
        ),
        15 => run_generated_fixture_actions!(
            "todo_mvc",
            &GENERATED_CORPUS[15],
            generated_todo_mvc,
            actions
        ),
        16 => {
            run_generated_fixture_actions!("crud", &GENERATED_CORPUS[16], generated_crud, actions)
        }
        17 => run_generated_fixture_actions!(
            "flight_booker",
            &GENERATED_CORPUS[17],
            generated_flight_booker,
            actions
        ),
        18 => run_generated_fixture_actions!(
            "temperature_converter",
            &GENERATED_CORPUS[18],
            generated_temperature_converter,
            actions
        ),
        19 => {
            run_generated_fixture_actions!("pong", &GENERATED_CORPUS[19], generated_pong, actions)
        }
        20 => {
            run_generated_fixture_actions!("cells", &GENERATED_CORPUS[20], generated_cells, actions)
        }
        21 => run_generated_fixture_actions!(
            "todo_mvc_physical",
            &GENERATED_CORPUS[21],
            generated_todo_mvc_physical,
            actions
        ),
        _ => return None,
    })
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
            .unwrap_or_else(empty_smoke_output);
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

pub fn run_generated_for_checked_source(
    source_text: &str,
    scenario_text: &str,
) -> Option<(String, boon_dd::SmokeOutput)> {
    let plan = boon_compiler::compile_source("host/source.bn", source_text);
    generated_index_for_graph_id(&plan.dd_graph_ir.graph_id)
        .and_then(|index| run_generated_scenario_at(index, scenario_text))
}

fn generated_index_for_graph_id(graph_id: &str) -> Option<usize> {
    [
        generated_counter::graph::graph_id(),
        generated_counter_hold::graph::graph_id(),
        generated_interval::graph::graph_id(),
        generated_interval_hold::graph::graph_id(),
        generated_latest::graph::graph_id(),
        generated_when::graph::graph_id(),
        generated_while::graph::graph_id(),
        generated_then::graph::graph_id(),
        generated_list_map_block::graph::graph_id(),
        generated_list_map_external_dep::graph::graph_id(),
        generated_list_object_state::graph::graph_id(),
        generated_list_retain_count::graph::graph_id(),
        generated_list_retain_reactive::graph::graph_id(),
        generated_list_retain_remove::graph::graph_id(),
        generated_shopping_list::graph::graph_id(),
        generated_todo_mvc::graph::graph_id(),
        generated_crud::graph::graph_id(),
        generated_flight_booker::graph::graph_id(),
        generated_temperature_converter::graph::graph_id(),
        generated_pong::graph::graph_id(),
        generated_cells::graph::graph_id(),
        generated_todo_mvc_physical::graph::graph_id(),
    ]
    .iter()
    .position(|candidate| *candidate == graph_id)
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
