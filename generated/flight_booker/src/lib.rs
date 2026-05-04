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
    fn generated_graph_emits_monitor_and_render_output() {
        let allocator = timely::communication::Allocator::Thread(
            timely::communication::allocator::Thread::default(),
        );
        let mut worker =
            timely::worker::Worker::new(timely::WorkerConfig::default(), allocator, None);
        let mut graph = crate::graph::build_dataflow(&mut worker);
        let outputs = graph
            .submit_text_and_drain(&mut worker, crate::graph::smoke_input_text(), 1, 1024)
            .expect("generated graph should drain");
        assert!(!outputs.is_empty(), "generated graph emitted no output");
        assert!(outputs.iter().any(|output| !output.monitor.is_empty()));
        assert!(outputs.iter().any(|output| !output.render.is_empty()));
    }
}
