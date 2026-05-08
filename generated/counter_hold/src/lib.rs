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
        let expected: boon_dd::SmokeOutput = serde_json::from_str("{\"monitor\":[{\"NodeValue\":{\"epoch\":1,\"node\":\"CounterHold\",\"owner\":\"Root\",\"value_preview\":\"1\"}}],\"render\":[{\"PatchText\":{\"node\":\"DocumentText\",\"text\":\"1\"}}]}")
            .expect("checked expected render JSON should deserialize");
        let allocator = timely::communication::Allocator::Thread(
            timely::communication::allocator::Thread::default(),
        );
        let mut worker =
            timely::worker::Worker::new(timely::WorkerConfig::default(), allocator, None);
        let mut graph = crate::graph::build_dataflow(&mut worker);
        let epoch = 1_u64;
        for value in [""] {
            graph.sources.submit_text(value, epoch);
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
