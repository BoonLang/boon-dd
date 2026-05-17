pub mod graph;
pub mod ids;
pub mod monitor_bindings;
pub mod persist_bindings;
pub mod render_bindings;
pub mod shapes;
pub mod source_events;
pub mod values;

pub fn checked_scenario_steps() -> Vec<boon_dd::ScenarioStep> {
    serde_json::from_str("[{\"description\":\"synthetic tick into hold\",\"events\":[{\"Source\":{\"source\":\"tick\",\"owner\":null,\"generation\":null,\"value\":\"EmptyRecord\"}}],\"actions\":[{\"source\":\"tick\",\"owner\":null,\"generation\":null,\"value\":\"EmptyRecord\"}],\"commands\":[],\"expect_text\":\"1\",\"expect_monitor_changed\":[\"TimerInterval\",\"Counter\",\"DocumentText\"]}]").expect("checked scenario steps should deserialize")
}

pub struct GeneratedGraphSession {
    worker: timely::worker::Worker,
    graph: crate::graph::GeneratedGraphHandles,
    latest_output: Option<boon_dd::SmokeOutput>,
    graph_builds: usize,
    drained_outputs: usize,
}

impl GeneratedGraphSession {
    pub fn new() -> Self {
        let allocator = timely::communication::Allocator::Thread(
            timely::communication::allocator::Thread::default(),
        );
        let mut worker =
            timely::worker::Worker::new(timely::WorkerConfig::default(), allocator, None);
        let graph = crate::graph::build_dataflow(&mut worker);
        Self {
            worker,
            graph,
            latest_output: None,
            graph_builds: 1,
            drained_outputs: 0,
        }
    }

    pub fn submit_action(&mut self, action: &boon_dd::SourceAction, epoch: u64) {
        self.graph.sources.submit_action(action, epoch);
    }

    pub fn submit_host_tick(&mut self, epoch: u64) {
        self.graph.sources.submit_host_tick(epoch);
    }

    pub fn submit_persisted_text(&mut self, value: impl Into<String>, epoch: u64) {
        self.graph.sources.submit_persisted_text(value, epoch);
    }

    pub fn submit_host_tick_and_drain(
        &mut self,
        epoch: u64,
    ) -> Result<boon_dd::SmokeOutput, String> {
        self.submit_host_tick(epoch);
        self.drain_epoch(epoch)
    }

    pub fn submit_action_and_drain(
        &mut self,
        action: &boon_dd::SourceAction,
        epoch: u64,
    ) -> Result<boon_dd::SmokeOutput, String> {
        self.submit_action(action, epoch);
        self.drain_epoch(epoch)
    }

    pub fn submit_actions_and_drain(
        &mut self,
        actions: &[boon_dd::SourceAction],
        epoch: u64,
    ) -> Result<boon_dd::SmokeOutput, String> {
        if actions.is_empty() {
            self.submit_host_tick(epoch);
        } else {
            for action in actions {
                self.submit_action(action, epoch);
            }
        }
        self.drain_epoch(epoch)
    }

    pub fn drain_epoch(&mut self, epoch: u64) -> Result<boon_dd::SmokeOutput, String> {
        self.graph.sources.close_epoch(epoch);
        let target = crate::graph::completion_time(epoch) + 1;
        let mut worker_steps = 0_usize;
        while self.graph.probe.less_than(&target) {
            if worker_steps == 4096 {
                return Err(format!(
                    "generated graph probe stalled at {target} after {worker_steps} steps"
                ));
            }
            self.worker.step();
            worker_steps += 1;
        }
        while let Some(output) = self.graph.sources.take_output() {
            self.drained_outputs += 1;
            self.latest_output = Some(output);
        }
        Ok(self.latest_output.clone().unwrap_or_else(empty_output))
    }

    pub fn graph_builds(&self) -> usize {
        self.graph_builds
    }

    pub fn drained_outputs(&self) -> usize {
        self.drained_outputs
    }
}

fn empty_output() -> boon_dd::SmokeOutput {
    boon_dd::SmokeOutput {
        monitor: Vec::new(),
        render: Vec::new(),
        effects: Vec::new(),
        persistence: Vec::new(),
    }
}

pub fn run_checked_scenario() -> boon_dd::SmokeOutput {
    run_checked_scenario_step_outputs()
        .into_iter()
        .last()
        .expect("generated graph emitted no scenario output")
}

pub fn run_checked_scenario_step_outputs() -> Vec<boon_dd::SmokeOutput> {
    let scenario_steps = checked_scenario_steps();
    let mut session = GeneratedGraphSession::new();
    let has_persistence_tap = crate::persist_bindings::has_persistence_tap();
    let mut persistence_enabled = false;
    let mut persisted_text: Option<String> = None;
    let mut last_generated_persisted_text: Option<String> = None;
    let mut outputs = Vec::new();
    for (step_index, step) in scenario_steps.iter().enumerate() {
        let epoch = step_index as u64 + 1;
        let mut submitted = false;
        for event in &step.events {
            match event {
                boon_dd::ScenarioEvent::Source(action) => {
                    session.submit_action(action, epoch);
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
                    session = GeneratedGraphSession::new();
                    if persistence_enabled {
                        if let Some(value) = persisted_text.clone() {
                            session.submit_persisted_text(value, epoch);
                            submitted = true;
                        }
                    }
                }
                boon_dd::ScenarioEvent::Command(_) => {}
            }
        }
        if !submitted || !crate::graph::has_bound_source_ids() {
            session.submit_host_tick(epoch);
        }
        let output = session
            .drain_epoch(epoch)
            .expect("generated graph emitted no scenario output");
        last_generated_persisted_text =
            output
                .persistence
                .iter()
                .rev()
                .find_map(|command| match command {
                    boon_dd::PersistenceCommand::SaveText { value, .. } => Some(value.clone()),
                    boon_dd::PersistenceCommand::LoadText { .. } => None,
                });
        outputs.push(output);
    }
    outputs
}

#[cfg(test)]
mod tests {
    #[test]
    fn generated_graph_matches_checked_scenario_output() {
        let expected: boon_dd::SmokeOutput = serde_json::from_str("{\n  \"monitor\": [\n    {\n      \"NodeValue\": {\n        \"epoch\": 1,\n        \"node\": \"Counter\",\n        \"owner\": \"Root\",\n        \"value_preview\": \"1\"\n      }\n    }\n  ],\n  \"render\": [\n    {\n      \"PatchText\": {\n        \"node\": \"DocumentText\",\n        \"text\": \"1\"\n      }\n    }\n  ],\n  \"effects\": [\n    {\n      \"Requested\": {\n        \"node\": \"EffectSink\",\n        \"name\": \"Timer/interval\"\n      }\n    }\n  ],\n  \"persistence\": [\n    {\n      \"SaveText\": {\n        \"node\": \"PersistTap\",\n        \"value\": \"1\"\n      }\n    }\n  ]\n}")
            .expect("checked expected render JSON should deserialize");
        let actual = crate::run_checked_scenario();
        assert_eq!(&actual, &expected);
    }
}
