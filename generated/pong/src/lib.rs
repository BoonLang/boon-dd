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
        let expected: boon_dd::SmokeOutput = serde_json::from_str("{\n  \"monitor\": [\n    {\n      \"NodeValue\": {\n        \"epoch\": 1,\n        \"node\": \"DocumentOutput\",\n        \"owner\": \"Root\",\n        \"value_preview\": \"frame 1\"\n      }\n    }\n  ],\n  \"render\": [\n    {\n      \"PatchText\": {\n        \"node\": \"DocumentText\",\n        \"text\": \"frame 1\"\n      }\n    }\n  ],\n  \"effects\": [\n    {\n      \"Requested\": {\n        \"node\": \"EffectSink\",\n        \"name\": \"Window/animation_frame\"\n      }\n    }\n  ],\n  \"persistence\": [\n    {\n      \"SaveText\": {\n        \"node\": \"PersistTap\",\n        \"value\": \"frame 1\"\n      }\n    }\n  ]\n}")
            .expect("checked expected render JSON should deserialize");
        let actions: Vec<boon_dd::SourceAction> = serde_json::from_str("[{\"source\":\"store.sources.frame\",\"owner\":null,\"generation\":null,\"value\":{\"Record\":{\"delta\":{\"Number\":{\"Int\":16}},\"now\":{\"Number\":{\"Int\":16}}}}}]")
            .expect("checked scenario actions should deserialize");
        let allocator = timely::communication::Allocator::Thread(
            timely::communication::allocator::Thread::default(),
        );
        let mut worker =
            timely::worker::Worker::new(timely::WorkerConfig::default(), allocator, None);
        let mut graph = crate::graph::build_dataflow(&mut worker);
        let epoch = 1_u64;
        if actions.is_empty() {
            graph.sources.submit_host_tick(epoch);
        } else {
            for action in &actions {
                graph.sources.submit_action(action, epoch);
            }
        }
        graph.sources.close_epoch(epoch);
        let target = crate::graph::completion_time(epoch) + 1;
        let mut steps = 0_usize;
        while graph.probe.less_than(&target) {
            if steps == 1024 {
                panic!("generated graph probe stalled at {target} after {steps} steps");
            }
            worker.step();
            steps += 1;
        }
        let outputs = graph.sources.outputs();
        let actual = outputs
            .last()
            .expect("generated graph emitted no scenario output");
        assert_eq!(actual, &expected);
    }
}
