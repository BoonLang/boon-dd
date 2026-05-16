pub mod graph;
pub mod ids;
pub mod monitor_bindings;
pub mod persist_bindings;
pub mod render_bindings;
pub mod shapes;
pub mod source_events;
pub mod values;

#[cfg(test)]
mod tests {
    #[test]
    fn generated_graph_matches_checked_scenario_output() {
        let expected: boon_dd::SmokeOutput = serde_json::from_str("{\n  \"monitor\": [\n    {\n      \"NodeValue\": {\n        \"epoch\": 1,\n        \"node\": \"Visible\",\n        \"owner\": \"Root\",\n        \"value_preview\": \"visible\"\n      }\n    }\n  ],\n  \"render\": [\n    {\n      \"PatchText\": {\n        \"node\": \"DocumentText\",\n        \"text\": \"visible\"\n      }\n    }\n  ],\n  \"effects\": [],\n  \"persistence\": []\n}")
            .expect("checked expected render JSON should deserialize");
        let scenario_steps: Vec<boon_dd::ScenarioStep> = serde_json::from_str("[{\"description\":\"selected branch visible\",\"events\":[{\"Source\":{\"source\":\"selected_filter\",\"owner\":null,\"generation\":null,\"value\":{\"Tag\":{\"name\":\"Active\",\"payload\":null}}}}],\"actions\":[{\"source\":\"selected_filter\",\"owner\":null,\"generation\":null,\"value\":{\"Tag\":{\"name\":\"Active\",\"payload\":null}}}],\"commands\":[],\"expect_text\":\"visible\",\"expect_monitor_changed\":[\"Visible\",\"DocumentText\"]}]")
            .expect("checked scenario steps should deserialize");
        let allocator = || {
            timely::communication::Allocator::Thread(
                timely::communication::allocator::Thread::default(),
            )
        };
        let mut worker =
            timely::worker::Worker::new(timely::WorkerConfig::default(), allocator(), None);
        let mut graph = crate::graph::build_dataflow(&mut worker);
        let has_persistence_tap = crate::persist_bindings::has_persistence_tap();
        let mut persistence_enabled = false;
        let mut persisted_text: Option<String> = None;
        let mut last_generated_persisted_text: Option<String> = None;
        let mut last_output: Option<boon_dd::SmokeOutput> = None;
        for (step_index, step) in scenario_steps.iter().enumerate() {
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
                        graph = crate::graph::build_dataflow(&mut worker);
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
            if !submitted || !crate::graph::has_bound_source_ids() {
                graph.sources.submit_host_tick(epoch);
            }
            graph.sources.close_epoch(epoch);
            let target = crate::graph::completion_time(epoch) + 1;
            let mut worker_steps = 0_usize;
            while graph.probe.less_than(&target) {
                if worker_steps == 1024 {
                    panic!("generated graph step {step_index} probe stalled at {target} after {worker_steps} steps");
                }
                worker.step();
                worker_steps += 1;
            }
            let mut step_output = None;
            while let Some(output) = graph.sources.take_output() {
                step_output = Some(output);
            }
            let output = step_output.expect("generated graph emitted no scenario output");
            last_generated_persisted_text =
                output
                    .persistence
                    .iter()
                    .rev()
                    .find_map(|command| match command {
                        boon_dd::PersistenceCommand::SaveText { value, .. } => Some(value.clone()),
                        boon_dd::PersistenceCommand::LoadText { .. } => None,
                    });
            last_output = Some(output);
        }
        let actual = last_output
            .as_ref()
            .expect("generated graph emitted no scenario output");
        assert_eq!(actual, &expected);
    }
}
