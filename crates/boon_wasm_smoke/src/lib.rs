use wasm_bindgen::prelude::*;

macro_rules! run_generated {
    ($name:literal, $crate_name:ident) => {{
        let allocator = || timely::communication::Allocator::Thread(
            timely::communication::allocator::Thread::default(),
        );
        let mut worker =
            timely::worker::Worker::new(timely::WorkerConfig::default(), allocator(), None);
        let mut graph = $crate_name::graph::build_dataflow(&mut worker);
        let scenario = boon_runtime_host::parse_scenario(include_str!(concat!(
            "../../../examples/",
            $name,
            "/scenario.toml"
        )));
        let has_persistence_tap = $crate_name::persist_bindings::has_persistence_tap();
        let mut persistence_enabled = false;
        let mut persisted_text: Option<String> = None;
        let mut last_generated_persisted_text: Option<String> = None;
        let mut last_output = boon_dd::SmokeOutput {
            monitor: Vec::new(),
            render: Vec::new(),
            effects: Vec::new(),
            persistence: Vec::new(),
        };
        for (step_index, step) in scenario.steps.iter().enumerate() {
            let epoch = step_index as u64 + 1;
            let mut submitted = false;
            for event in &step.events {
                match event {
                    boon_dd::ScenarioEvent::Source(action) => {
                        graph.sources.submit_action(action, epoch);
                        submitted = true;
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
                                submitted = true;
                            }
                        }
                    }
                    boon_dd::ScenarioEvent::Command(_) => {}
                }
            }
            if !submitted {
                graph.sources.submit_host_tick(epoch);
            }
            graph.sources.close_epoch(epoch);
            let target = $crate_name::graph::completion_time(epoch) + 1;
            let mut worker_steps = 0_usize;
            while graph.probe.less_than(&target) {
                if worker_steps == 1024 {
                    panic!(
                        "generated browser WASM graph {} step {} probe stalled at {target} after {worker_steps} steps",
                        $name, step_index
                    );
                }
                worker.step();
                worker_steps += 1;
            }
            last_output = graph.sources.outputs().into_iter().last().unwrap_or_else(|| {
                boon_dd::SmokeOutput {
                    monitor: Vec::new(),
                    render: Vec::new(),
                    effects: Vec::new(),
                    persistence: Vec::new(),
                }
            });
            last_generated_persisted_text = last_output.persistence.iter().rev().find_map(|command| {
                match command {
                    boon_dd::PersistenceCommand::SaveText { value, .. } => Some(value.clone()),
                    boon_dd::PersistenceCommand::LoadText { .. } => None,
                }
            });
        }
        ($name, last_output)
    }};
}

#[wasm_bindgen]
pub fn run_smoke_json() -> Result<String, JsValue> {
    serde_json::to_string(&run_generated_matrix())
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

fn run_generated_matrix() -> Vec<(&'static str, boon_dd::SmokeOutput)> {
    vec![
        run_generated!("counter", generated_counter),
        run_generated!("counter_hold", generated_counter_hold),
        run_generated!("interval", generated_interval),
        run_generated!("interval_hold", generated_interval_hold),
        run_generated!("latest", generated_latest),
        run_generated!("when", generated_when),
        run_generated!("while", generated_while),
        run_generated!("then", generated_then),
        run_generated!("list_map_block", generated_list_map_block),
        run_generated!("list_map_external_dep", generated_list_map_external_dep),
        run_generated!("list_object_state", generated_list_object_state),
        run_generated!("list_retain_count", generated_list_retain_count),
        run_generated!("list_retain_reactive", generated_list_retain_reactive),
        run_generated!("list_retain_remove", generated_list_retain_remove),
        run_generated!("shopping_list", generated_shopping_list),
        run_generated!("todo_mvc", generated_todo_mvc),
        run_generated!("crud", generated_crud),
        run_generated!("flight_booker", generated_flight_booker),
        run_generated!("temperature_converter", generated_temperature_converter),
        run_generated!("pong", generated_pong),
        run_generated!("cells", generated_cells),
        run_generated!("todo_mvc_physical", generated_todo_mvc_physical),
    ]
}
