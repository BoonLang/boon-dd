use boon_dd::{
    BoonNumber, BoonTime, BoonValue, Diff, EffectCommand, EncodedTime, GeneratedSourceEvent,
    GeneratedSourceEventPayload, MonitorRecord, NodeId, OwnerKey, PersistenceCommand,
    RenderCommand, Scenario, ScenarioCommand, ScenarioEvent, ScenarioStep, SmokeOutput,
    SourceAction, SourceFamilyId, SourceId,
};
use differential_dataflow::collection::VecCollection;
use differential_dataflow::input::InputSession;
use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use timely::dataflow::operators::probe::Handle as ProbeHandle;

pub fn run_compiled_source_scenario(
    source_path: impl Into<String>,
    source_text: impl Into<String>,
    scenario_text: &str,
) -> Result<SmokeOutput, String> {
    Ok(
        run_compiled_source_scenario_steps(source_path, source_text, scenario_text)?
            .into_iter()
            .last()
            .map(|step| step.output)
            .unwrap_or_else(empty_structured_output),
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledScenarioStepOutput {
    pub step_index: usize,
    pub description: String,
    pub event_count: usize,
    pub event_order: Vec<String>,
    pub action_count: usize,
    pub commands: Vec<ScenarioCommand>,
    pub expected_text: String,
    pub output: SmokeOutput,
}

pub fn run_compiled_source_scenario_steps(
    source_path: impl Into<String>,
    source_text: impl Into<String>,
    scenario_text: &str,
) -> Result<Vec<CompiledScenarioStepOutput>, String> {
    let source_path = source_path.into();
    let source_text = source_text.into();
    let scenario = parse_scenario_result(scenario_text)?;
    let mut session = CompiledGraphSession::new(source_path.clone(), source_text.clone())?;
    let _ = session.drain_epoch(0)?;
    let mut persistence_enabled = false;
    let mut persisted_text: Option<String> = None;
    let mut last_generated_persisted_text: Option<String> = None;
    let mut outputs = Vec::new();
    for (step_index, step) in scenario.steps.iter().enumerate() {
        let epoch = step_index as u64 + 1;
        let mut submitted = false;
        for event in &step.events {
            match event {
                ScenarioEvent::Source(action) => {
                    session.submit_action(action, epoch);
                    submitted = true;
                }
                ScenarioEvent::Command(command) if command.command == "enable_persistence" => {
                    if session.has_persistence_tap() {
                        persistence_enabled = true;
                        persisted_text = last_generated_persisted_text.clone();
                    }
                }
                ScenarioEvent::Command(command) if command.command == "reload" => {
                    session = CompiledGraphSession::new(source_path.clone(), source_text.clone())?;
                    if persistence_enabled {
                        if let Some(value) = persisted_text.clone() {
                            session.submit_persisted_text(value, epoch);
                            submitted = true;
                        }
                    }
                }
                ScenarioEvent::Command(_) => {}
            }
        }
        if !submitted {
            session.submit_host_tick(epoch);
        }
        let output = session.drain_epoch(epoch)?;
        last_generated_persisted_text =
            output
                .persistence
                .iter()
                .rev()
                .find_map(|command| match command {
                    PersistenceCommand::SaveText { value, .. } => Some(value.clone()),
                    PersistenceCommand::LoadText { .. } => None,
                });
        outputs.push(CompiledScenarioStepOutput {
            step_index,
            description: step.description.clone(),
            event_count: step.events.len(),
            event_order: step
                .events
                .iter()
                .map(|event| match event {
                    ScenarioEvent::Source(action) => format!("source:{}", action.source),
                    ScenarioEvent::Command(command) => {
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
    Ok(outputs)
}

pub fn run_compiled_source_actions(
    source_path: impl Into<String>,
    source_text: impl Into<String>,
    actions: &[SourceAction],
) -> Result<SmokeOutput, String> {
    let mut session = CompiledGraphSession::new(source_path, source_text)?;
    let epoch = 1_u64;
    if actions.is_empty() {
        session.submit_host_tick(epoch);
    } else {
        for action in actions {
            session.submit_action(action, epoch);
        }
    }
    session.drain_epoch(epoch)
}

pub struct CompiledGraphSession {
    worker: timely::worker::Worker,
    sources: InputSession<EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    probe: ProbeHandle<EncodedTime>,
    outputs: Arc<Mutex<VecDeque<SmokeOutput>>>,
    source_ids_by_path: BTreeMap<String, SourceId>,
    monitor_node: NodeId,
    render_node: NodeId,
    persistence_nodes: Vec<NodeId>,
    next_sequence: u64,
}

#[derive(Clone, Debug)]
pub struct SubmittedEvent {
    sequence: u64,
    event: GeneratedSourceEvent,
}

pub struct ThreadedGraphSession {
    sender: mpsc::Sender<ThreadedRequest>,
    thread: Option<thread::JoinHandle<()>>,
}

enum ThreadedRequest {
    HostTick {
        epoch: u64,
        response: mpsc::Sender<Result<SmokeOutput, String>>,
    },
    SubmitAction {
        action: SourceAction,
        epoch: u64,
        response: mpsc::Sender<Result<SmokeOutput, String>>,
    },
    RetractLast {
        epoch: u64,
        response: mpsc::Sender<Result<SmokeOutput, String>>,
    },
    Shutdown,
}

impl ThreadedGraphSession {
    pub fn new(
        source_path: impl Into<String>,
        source_text: impl Into<String>,
    ) -> Result<Self, String> {
        let source_path = source_path.into();
        let source_text = source_text.into();
        let (sender, receiver) = mpsc::channel::<ThreadedRequest>();
        let (ready_sender, ready_receiver) = mpsc::channel::<Result<(), String>>();
        let thread =
            thread::spawn(
                move || match CompiledGraphSession::new(source_path, source_text) {
                    Ok(mut session) => {
                        let _ = ready_sender.send(Ok(()));
                        let mut submitted_events = Vec::<SubmittedEvent>::new();
                        while let Ok(request) = receiver.recv() {
                            match request {
                                ThreadedRequest::HostTick { epoch, response } => {
                                    session.submit_host_tick(epoch);
                                    let _ = response.send(session.drain_epoch(epoch));
                                }
                                ThreadedRequest::SubmitAction {
                                    action,
                                    epoch,
                                    response,
                                } => {
                                    submitted_events.push(session.submit_action(&action, epoch));
                                    let _ = response.send(session.drain_epoch(epoch));
                                }
                                ThreadedRequest::RetractLast { epoch, response } => {
                                    if let Some(submitted) = submitted_events.pop() {
                                        session.retract_event(&submitted, epoch);
                                    }
                                    let _ = response.send(session.drain_epoch(epoch));
                                }
                                ThreadedRequest::Shutdown => break,
                            }
                        }
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error));
                    }
                },
            );
        ready_receiver
            .recv()
            .map_err(|error| format!("compiled graph worker did not start: {error}"))??;
        Ok(Self {
            sender,
            thread: Some(thread),
        })
    }

    pub fn submit_host_tick_and_drain(&self, epoch: u64) -> Result<SmokeOutput, String> {
        self.request(|response| ThreadedRequest::HostTick { epoch, response })
    }

    pub fn submit_action_and_drain(
        &self,
        action: SourceAction,
        epoch: u64,
    ) -> Result<SmokeOutput, String> {
        self.request(|response| ThreadedRequest::SubmitAction {
            action,
            epoch,
            response,
        })
    }

    pub fn retract_last_and_drain(&self, epoch: u64) -> Result<SmokeOutput, String> {
        self.request(|response| ThreadedRequest::RetractLast { epoch, response })
    }

    fn request(
        &self,
        build: impl FnOnce(mpsc::Sender<Result<SmokeOutput, String>>) -> ThreadedRequest,
    ) -> Result<SmokeOutput, String> {
        let (response, receiver) = mpsc::channel();
        self.sender
            .send(build(response))
            .map_err(|error| format!("compiled graph worker stopped: {error}"))?;
        receiver
            .recv()
            .map_err(|error| format!("compiled graph worker response failed: {error}"))?
    }
}

impl Drop for ThreadedGraphSession {
    fn drop(&mut self) {
        let _ = self.sender.send(ThreadedRequest::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl CompiledGraphSession {
    pub fn new(
        source_path: impl Into<String>,
        source_text: impl Into<String>,
    ) -> Result<Self, String> {
        let plan = boon_compiler::compile_source(source_path.into(), source_text.into());
        let graph = plan.dd_graph_ir.render_graph.clone();
        let effect_requests = plan
            .dd_graph_ir
            .output_protocol
            .sinks
            .iter()
            .filter_map(|sink| match sink {
                boon_dd::DdOutputSink::Effect { node, name, .. } => {
                    Some((node.clone(), name.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let persistence_nodes = plan
            .dd_graph_ir
            .output_protocol
            .sinks
            .iter()
            .filter_map(|sink| match sink {
                boon_dd::DdOutputSink::Persistence { node, .. } => Some(node.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let monitor_node = plan.graph.monitor_node.clone();
        let render_node = plan.graph.render_node.clone();
        let source_ids_by_path = plan
            .graph
            .source_bindings
            .iter()
            .map(|binding| (binding.path.clone(), binding.source_id.clone()))
            .collect::<BTreeMap<_, _>>();
        let bound_source_ids = plan
            .graph
            .source_bindings
            .iter()
            .map(|binding| binding.source_id.0.clone())
            .collect::<Vec<_>>();
        let allocator = timely::communication::Allocator::Thread(
            timely::communication::allocator::Thread::default(),
        );
        let mut worker =
            timely::worker::Worker::new(timely::WorkerConfig::default(), allocator, None);
        let mut sources = InputSession::<EncodedTime, (u64, GeneratedSourceEvent), Diff>::new();
        let mut probe = ProbeHandle::new();
        let outputs = Arc::new(Mutex::new(VecDeque::<SmokeOutput>::new()));
        let output_in_graph = Arc::clone(&outputs);
        let monitor_in_graph = monitor_node.clone();
        let render_in_graph = render_node.clone();
        let effect_requests_in_graph = effect_requests.clone();
        let persistence_nodes_in_graph = persistence_nodes.clone();
        let bound_source_ids_in_graph = bound_source_ids.clone();

        worker.dataflow::<EncodedTime, _, _>(|scope| {
            let events = sources.to_collection(scope);
            let rendered_values =
                runtime_render_collection(&graph, &events, &bound_source_ids_in_graph)
                    .map(|text| ((), text));
            let rendered_owners = if bound_source_ids_in_graph.is_empty() {
                events
                    .clone()
                    .map(|(_sequence, event)| ((), runtime_source_event_owner(&event)))
            } else {
                let bound_source_ids = bound_source_ids_in_graph.clone();
                events
                    .clone()
                    .filter(move |(_sequence, event)| {
                        runtime_event_matches_bound_source(event, &bound_source_ids)
                    })
                    .map(|(_sequence, event)| ((), runtime_source_event_owner(&event)))
            };
            let rendered = rendered_values
                .join(rendered_owners)
                .map(|(_key, (text, owner))| (owner, text));
            rendered
                .inspect(move |((owner, text), time, diff)| {
                    if *diff > 0 {
                        output_in_graph
                            .lock()
                            .expect("compiled runtime output lock poisoned")
                            .push_back(SmokeOutput {
                                monitor: vec![MonitorRecord::NodeValue {
                                    epoch: BoonTime::decode(*time).epoch,
                                    node: monitor_in_graph.clone(),
                                    owner: owner.clone(),
                                    value_preview: text.clone(),
                                }],
                                render: vec![RenderCommand::PatchText {
                                    node: render_in_graph.clone(),
                                    text: text.clone(),
                                }],
                                effects: effect_requests_in_graph
                                    .iter()
                                    .map(|(node, name)| EffectCommand::Requested {
                                        node: node.clone(),
                                        name: name.clone(),
                                    })
                                    .collect(),
                                persistence: persistence_nodes_in_graph
                                    .iter()
                                    .map(|node| PersistenceCommand::SaveText {
                                        node: node.clone(),
                                        value: text.clone(),
                                    })
                                    .collect(),
                            });
                    }
                })
                .probe_with(&mut probe);
        });

        Ok(Self {
            worker,
            sources,
            probe,
            outputs,
            source_ids_by_path,
            monitor_node,
            render_node,
            persistence_nodes,
            next_sequence: 0,
        })
    }

    pub fn submit_action(&mut self, action: &SourceAction, epoch: u64) -> SubmittedEvent {
        let payload = boon_dd::source_action_payload(action);
        let event = match (&action.owner, action.generation) {
            (Some(owner), generation) => GeneratedSourceEvent::Dynamic {
                family_id: SourceFamilyId(
                    self.source_ids_by_path
                        .get(&action.source)
                        .map(|source_id| source_id.0.clone())
                        .unwrap_or_else(|| action.source.clone()),
                ),
                owner_key: owner.clone(),
                generation: generation.expect("dynamic source action must include generation"),
                payload,
            },
            (None, _) => GeneratedSourceEvent::Static {
                source_id: self
                    .source_ids_by_path
                    .get(&action.source)
                    .cloned()
                    .unwrap_or_else(|| SourceId(action.source.clone())),
                payload,
            },
        };
        self.submit_event(event, epoch)
    }

    pub fn submit_host_tick(&mut self, epoch: u64) -> SubmittedEvent {
        self.submit_event(
            GeneratedSourceEvent::Static {
                source_id: SourceId("__host_tick".to_owned()),
                payload: GeneratedSourceEventPayload::EmptyRecord,
            },
            epoch,
        )
    }

    pub fn submit_persisted_text(
        &mut self,
        value: impl Into<String>,
        epoch: u64,
    ) -> SubmittedEvent {
        self.submit_event(
            GeneratedSourceEvent::Static {
                source_id: SourceId("__persisted_text".to_owned()),
                payload: GeneratedSourceEventPayload::Text(value.into()),
            },
            epoch,
        )
    }

    pub fn has_persistence_tap(&self) -> bool {
        !self.persistence_nodes.is_empty()
    }

    pub fn submit_event(&mut self, event: GeneratedSourceEvent, epoch: u64) -> SubmittedEvent {
        self.sources
            .advance_to(BoonTime { epoch, phase: 0 }.encode());
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.sources.insert((sequence, event.clone()));
        SubmittedEvent { sequence, event }
    }

    pub fn retract_event(&mut self, submitted: &SubmittedEvent, epoch: u64) {
        self.sources
            .advance_to(BoonTime { epoch, phase: 0 }.encode());
        self.sources
            .remove((submitted.sequence, submitted.event.clone()));
    }

    pub fn drain_epoch(&mut self, epoch: u64) -> Result<SmokeOutput, String> {
        self.sources.advance_to(completion_time(epoch) + 1);
        self.sources.flush();
        let target = completion_time(epoch) + 1;
        let mut steps = 0_usize;
        while self.probe.less_than(&target) {
            if steps == 1024 {
                return Err(format!(
                    "compiled graph probe stalled at {target} after {steps} steps"
                ));
            }
            self.worker.step();
            steps += 1;
        }
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|_| "compiled runtime output lock poisoned".to_owned())?;
        Ok(outputs.drain(..).last().unwrap_or_else(|| SmokeOutput {
            monitor: vec![MonitorRecord::NodeValue {
                epoch,
                node: self.monitor_node.clone(),
                owner: OwnerKey("Root".to_owned()),
                value_preview: String::new(),
            }],
            render: vec![RenderCommand::PatchText {
                node: self.render_node.clone(),
                text: String::new(),
            }],
            effects: Vec::new(),
            persistence: Vec::new(),
        }))
    }
}

fn completion_time(epoch: u64) -> u64 {
    BoonTime { epoch, phase: 3 }.encode()
}

fn empty_structured_output() -> SmokeOutput {
    SmokeOutput {
        monitor: Vec::new(),
        render: Vec::new(),
        effects: Vec::new(),
        persistence: Vec::new(),
    }
}

fn runtime_render_collection<'scope>(
    graph: &boon_dd::DdRenderGraph,
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    source_ids: &[String],
) -> VecCollection<'scope, EncodedTime, String, Diff> {
    runtime_collection(graph, &graph.root, events, source_ids, &BTreeMap::new()).into_text()
}

enum RuntimeCollection<'scope> {
    Text(VecCollection<'scope, EncodedTime, String, Diff>),
    Number(VecCollection<'scope, EncodedTime, i64, Diff>),
    Bool(VecCollection<'scope, EncodedTime, bool, Diff>),
}

impl<'scope> RuntimeCollection<'scope> {
    fn into_text(self) -> VecCollection<'scope, EncodedTime, String, Diff> {
        match self {
            RuntimeCollection::Text(values) => values,
            RuntimeCollection::Number(values) => values.map(|value| value.to_string()),
            RuntimeCollection::Bool(values) => {
                values.map(|value| if value { "True" } else { "False" }.to_owned())
            }
        }
    }

    fn into_bool(self) -> VecCollection<'scope, EncodedTime, bool, Diff> {
        match self {
            RuntimeCollection::Bool(values) => values,
            RuntimeCollection::Text(values) => {
                values.map(|value| !value.is_empty() && value != "False" && value != "false")
            }
            RuntimeCollection::Number(values) => values.map(|value| value != 0),
        }
    }

    fn into_unit(self) -> VecCollection<'scope, EncodedTime, (), Diff> {
        match self {
            RuntimeCollection::Text(values) => values.map(|_| ()),
            RuntimeCollection::Number(values) => values.map(|_| ()),
            RuntimeCollection::Bool(values) => values.map(|_| ()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ConstValue {
    Empty,
    Text(String),
    Number(i64),
    Bool(bool),
    Tag(String),
    List(Vec<ConstValue>),
    Record(BTreeMap<String, ConstValue>),
}

impl ConstValue {
    fn text(self) -> String {
        match self {
            ConstValue::Empty => String::new(),
            ConstValue::Text(value) | ConstValue::Tag(value) => value,
            ConstValue::Number(value) => value.to_string(),
            ConstValue::Bool(value) => if value { "True" } else { "False" }.to_owned(),
            ConstValue::List(values) => values
                .into_iter()
                .map(ConstValue::text)
                .collect::<Vec<_>>()
                .join(","),
            ConstValue::Record(_) => String::new(),
        }
    }

    fn number(self) -> i64 {
        match self {
            ConstValue::Number(value) => value,
            ConstValue::Text(value) | ConstValue::Tag(value) => {
                value.parse::<i64>().unwrap_or_default()
            }
            ConstValue::Bool(value) => i64::from(value),
            ConstValue::List(values) => values.len() as i64,
            ConstValue::Record(fields) => fields.len() as i64,
            ConstValue::Empty => 0,
        }
    }

    fn bool(self) -> bool {
        match self {
            ConstValue::Bool(value) => value,
            ConstValue::Tag(value) => matches!(value.as_str(), "True" | "true" | "Some"),
            ConstValue::Text(value) => !value.is_empty() && value != "False" && value != "false",
            ConstValue::Number(value) => value != 0,
            ConstValue::List(values) => !values.is_empty(),
            ConstValue::Record(fields) => !fields.is_empty(),
            ConstValue::Empty => false,
        }
    }

    fn record_member(self, name: &str) -> Option<ConstValue> {
        match self {
            ConstValue::Record(fields) => fields.get(name).cloned(),
            _ => None,
        }
    }
}

fn runtime_collection<'scope>(
    graph: &boon_dd::DdRenderGraph,
    node: &NodeId,
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    source_ids: &[String],
    env: &BTreeMap<String, ConstValue>,
) -> RuntimeCollection<'scope> {
    if let Some(value) = const_value(graph, node, None, env) {
        return const_collection(events, value);
    }
    let Some(node) = graph.nodes.iter().find(|candidate| &candidate.node == node) else {
        return RuntimeCollection::Text(events.clone().map(|_| String::new()));
    };
    match &node.operation {
        boon_dd::DdRenderGraphOperation::Source | boon_dd::DdRenderGraphOperation::Path(_) => {
            RuntimeCollection::Text(source_text_collection(events, source_ids))
        }
        boon_dd::DdRenderGraphOperation::Pipe { input, stage } => {
            let input = runtime_collection(graph, input, events, source_ids, env);
            runtime_stage_collection(graph, stage, input, events, source_ids, env)
        }
        boon_dd::DdRenderGraphOperation::Then { body } => {
            then_collection(graph, body, events.clone().map(|_| ()), env)
        }
        boon_dd::DdRenderGraphOperation::Hold { body, binder } => {
            hold_collection(graph, events.clone().map(|_| ()), body, binder, env)
        }
        boon_dd::DdRenderGraphOperation::Latest(_) => {
            RuntimeCollection::Text(latest_text_collection(events, source_ids))
        }
        boon_dd::DdRenderGraphOperation::Match { arms, .. } => {
            match_text_collection(graph, source_text_collection(events, source_ids), arms, env)
        }
        _ => RuntimeCollection::Text(events.clone().map(|_| String::new())),
    }
}

fn runtime_stage_collection<'scope>(
    graph: &boon_dd::DdRenderGraph,
    stage: &NodeId,
    input: RuntimeCollection<'scope>,
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    source_ids: &[String],
    env: &BTreeMap<String, ConstValue>,
) -> RuntimeCollection<'scope> {
    let Some(stage_node) = graph
        .nodes
        .iter()
        .find(|candidate| &candidate.node == stage)
    else {
        return RuntimeCollection::Text(input.into_unit().map(|_| String::new()));
    };
    match &stage_node.operation {
        boon_dd::DdRenderGraphOperation::Call { callee, args } => {
            runtime_call_collection(graph, callee, input, args, env)
        }
        boon_dd::DdRenderGraphOperation::Then { body } => {
            then_collection(graph, body, input.into_unit(), env)
        }
        boon_dd::DdRenderGraphOperation::Hold { body, binder } => {
            hold_collection(graph, input.into_unit(), body, binder, env)
        }
        boon_dd::DdRenderGraphOperation::Latest(_) => {
            RuntimeCollection::Text(latest_text_collection(events, source_ids))
        }
        boon_dd::DdRenderGraphOperation::Match { arms, .. } => {
            match_text_collection(graph, input.into_text(), arms, env)
        }
        boon_dd::DdRenderGraphOperation::SourceAt { .. }
        | boon_dd::DdRenderGraphOperation::Link { .. } => input,
        _ => runtime_collection(graph, &stage_node.node, events, source_ids, env),
    }
}

fn runtime_call_collection<'scope>(
    graph: &boon_dd::DdRenderGraph,
    callee: &str,
    input: RuntimeCollection<'scope>,
    args: &[boon_dd::DdRenderGraphArg],
    env: &BTreeMap<String, ConstValue>,
) -> RuntimeCollection<'scope> {
    match canonical_runtime_call(callee) {
        "Math/sum" => {
            RuntimeCollection::Number(input.into_unit().count().map(|(_key, count)| count as i64))
        }
        "Text/from_number" | "Document/new" | "Scene/new" => {
            RuntimeCollection::Text(input.into_text())
        }
        "Text/append" => {
            let suffix = first_runtime_arg(args)
                .and_then(|node| const_value(graph, node, None, env))
                .map(ConstValue::text)
                .unwrap_or_default();
            RuntimeCollection::Text(input.into_text().map(move |text| format!("{text}{suffix}")))
        }
        "Text/uppercase" => {
            RuntimeCollection::Text(input.into_text().map(|text| text.to_uppercase()))
        }
        "Text/is_empty" => RuntimeCollection::Bool(input.into_text().map(|text| text.is_empty())),
        "Bool/not" => RuntimeCollection::Bool(input.into_bool().map(|value| !value)),
        "Timer/interval" | "Window/animation_frame" => input,
        callee if callee.starts_with("Element/") => RuntimeCollection::Text(input.into_text()),
        _ => RuntimeCollection::Text(input.into_text()),
    }
}

fn then_collection<'scope>(
    graph: &boon_dd::DdRenderGraph,
    body: &[NodeId],
    trigger: VecCollection<'scope, EncodedTime, (), Diff>,
    env: &BTreeMap<String, ConstValue>,
) -> RuntimeCollection<'scope> {
    let value = body
        .last()
        .and_then(|node| const_value(graph, node, None, env))
        .unwrap_or(ConstValue::Empty);
    const_collection_from_trigger(trigger, value)
}

fn hold_collection<'scope>(
    graph: &boon_dd::DdRenderGraph,
    trigger: VecCollection<'scope, EncodedTime, (), Diff>,
    body: &[NodeId],
    binder: &str,
    env: &BTreeMap<String, ConstValue>,
) -> RuntimeCollection<'scope> {
    let graph = graph.clone();
    let body = body.to_vec();
    let binder = binder.to_owned();
    let env = env.clone();
    RuntimeCollection::Number(trigger.count().map(move |(_key, count)| {
        let mut env = env.clone();
        env.insert(
            binder.clone(),
            ConstValue::Number(count.saturating_sub(1) as i64),
        );
        body.last()
            .and_then(|node| const_value(&graph, node, None, &env))
            .map(ConstValue::number)
            .unwrap_or(count as i64)
    }))
}

fn match_text_collection<'scope>(
    graph: &boon_dd::DdRenderGraph,
    input: VecCollection<'scope, EncodedTime, String, Diff>,
    arms: &[boon_dd::DdRenderGraphMatchArm],
    env: &BTreeMap<String, ConstValue>,
) -> RuntimeCollection<'scope> {
    let graph = graph.clone();
    let arms = arms.to_vec();
    let env = env.clone();
    RuntimeCollection::Text(input.flat_map(move |matched| {
        arms.iter()
            .find(|arm| arm.pattern == "__" || runtime_text_pattern_matches(&matched, &arm.pattern))
            .and_then(|arm| {
                const_value(
                    &graph,
                    &arm.value,
                    Some(ConstValue::Text(matched.clone())),
                    &env,
                )
            })
            .and_then(|value| match value {
                ConstValue::Empty => None,
                value => Some(value.text()),
            })
    }))
}

fn const_collection<'scope>(
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    value: ConstValue,
) -> RuntimeCollection<'scope> {
    const_collection_from_trigger(events.clone().map(|_| ()), value)
}

fn const_collection_from_trigger<'scope>(
    trigger: VecCollection<'scope, EncodedTime, (), Diff>,
    value: ConstValue,
) -> RuntimeCollection<'scope> {
    match value {
        ConstValue::Number(number) => RuntimeCollection::Number(trigger.map(move |_| number)),
        ConstValue::Bool(value) => RuntimeCollection::Bool(trigger.map(move |_| value)),
        value => {
            let text = value.text();
            RuntimeCollection::Text(trigger.map(move |_| text.clone()))
        }
    }
}

fn source_text_collection<'scope>(
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    source_ids: &[String],
) -> VecCollection<'scope, EncodedTime, String, Diff> {
    let source_ids = source_ids.to_vec();
    events
        .clone()
        .filter(move |(_sequence, event)| runtime_event_matches_bound_source(event, &source_ids))
        .map(|(_sequence, event)| runtime_source_event_text(&event))
}

fn latest_text_collection<'scope>(
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    source_ids: &[String],
) -> VecCollection<'scope, EncodedTime, String, Diff> {
    let source_ids = source_ids.to_vec();
    events
        .clone()
        .filter(move |(_sequence, event)| runtime_event_matches_bound_source(event, &source_ids))
        .map(|(sequence, event)| ((), (sequence, runtime_source_event_text(&event))))
        .reduce(|_, inputs, output| {
            if let Some(((_sequence, value), _diff)) =
                inputs.iter().max_by_key(|((sequence, _), _)| *sequence)
            {
                output.push((value.clone(), 1));
            }
        })
        .map(|(_key, value)| value)
}

fn const_value(
    graph: &boon_dd::DdRenderGraph,
    node: &NodeId,
    pipe_input: Option<ConstValue>,
    env: &BTreeMap<String, ConstValue>,
) -> Option<ConstValue> {
    let node = graph
        .nodes
        .iter()
        .find(|candidate| &candidate.node == node)?;
    match &node.operation {
        boon_dd::DdRenderGraphOperation::Missing
        | boon_dd::DdRenderGraphOperation::Source
        | boon_dd::DdRenderGraphOperation::SourceAt { .. }
        | boon_dd::DdRenderGraphOperation::Link { .. }
        | boon_dd::DdRenderGraphOperation::Skip => None,
        boon_dd::DdRenderGraphOperation::Path(path) => const_path_value(path, pipe_input, env),
        boon_dd::DdRenderGraphOperation::Number(number) => {
            Some(ConstValue::Number(number.parse::<i64>().unwrap_or_else(
                |_| number.parse::<f64>().unwrap_or_default() as i64,
            )))
        }
        boon_dd::DdRenderGraphOperation::Text(text) => Some(ConstValue::Text(text.clone())),
        boon_dd::DdRenderGraphOperation::Tag(tag) => Some(match tag.as_str() {
            "True" => ConstValue::Bool(true),
            "False" => ConstValue::Bool(false),
            _ => ConstValue::Tag(tag.clone()),
        }),
        boon_dd::DdRenderGraphOperation::Record(fields) => fields
            .iter()
            .map(|field| {
                const_value(graph, &field.value, pipe_input.clone(), env)
                    .map(|value| (field.name.clone(), value))
            })
            .collect::<Option<BTreeMap<_, _>>>()
            .map(ConstValue::Record),
        boon_dd::DdRenderGraphOperation::List(values) => values
            .iter()
            .map(|value| const_value(graph, value, pipe_input.clone(), env))
            .collect::<Option<Vec<_>>>()
            .map(ConstValue::List),
        boon_dd::DdRenderGraphOperation::Block(values)
        | boon_dd::DdRenderGraphOperation::Latest(values)
        | boon_dd::DdRenderGraphOperation::Then { body: values }
        | boon_dd::DdRenderGraphOperation::Hold { body: values, .. } => values
            .last()
            .and_then(|value| const_value(graph, value, pipe_input, env)),
        boon_dd::DdRenderGraphOperation::Constructor { callee, fields } => {
            if callee == "Text" && fields.len() == 1 {
                const_value(graph, &fields[0].value, pipe_input, env)
            } else {
                fields
                    .iter()
                    .map(|field| {
                        const_value(graph, &field.value, pipe_input.clone(), env)
                            .map(|value| (field.name.clone(), value))
                    })
                    .collect::<Option<BTreeMap<_, _>>>()
                    .map(ConstValue::Record)
            }
        }
        boon_dd::DdRenderGraphOperation::FieldAccess { base, field } => {
            const_value(graph, base, pipe_input, env).and_then(|value| value.record_member(field))
        }
        boon_dd::DdRenderGraphOperation::BinaryAdd { left, right } => Some(ConstValue::Number(
            const_value(graph, left, pipe_input.clone(), env)?.number()
                + const_value(graph, right, pipe_input, env)?.number(),
        )),
        boon_dd::DdRenderGraphOperation::BinarySubtract { left, right } => {
            Some(ConstValue::Number(
                const_value(graph, left, pipe_input.clone(), env)?.number()
                    - const_value(graph, right, pipe_input, env)?.number(),
            ))
        }
        boon_dd::DdRenderGraphOperation::BinaryEqual { left, right } => Some(ConstValue::Bool(
            const_value(graph, left, pipe_input.clone(), env)?
                == const_value(graph, right, pipe_input, env)?,
        )),
        boon_dd::DdRenderGraphOperation::Pipe { input, stage } => {
            let input = const_value(graph, input, pipe_input, env)?;
            const_stage_value(graph, stage, input, env)
        }
        boon_dd::DdRenderGraphOperation::Call { callee, args } => {
            const_call_value(graph, callee, pipe_input, args, env)
        }
        boon_dd::DdRenderGraphOperation::Match { arms, .. } => {
            let matched = pipe_input?;
            let matched_text = matched.clone().text();
            arms.iter()
                .find(|arm| {
                    arm.pattern == "__" || runtime_text_pattern_matches(&matched_text, &arm.pattern)
                })
                .and_then(|arm| const_value(graph, &arm.value, Some(matched), env))
        }
    }
}

fn const_stage_value(
    graph: &boon_dd::DdRenderGraph,
    stage: &NodeId,
    input: ConstValue,
    env: &BTreeMap<String, ConstValue>,
) -> Option<ConstValue> {
    let stage = graph
        .nodes
        .iter()
        .find(|candidate| &candidate.node == stage)?;
    match &stage.operation {
        boon_dd::DdRenderGraphOperation::Call { callee, args } => {
            const_call_value(graph, callee, Some(input), args, env)
        }
        boon_dd::DdRenderGraphOperation::Match { arms, .. } => {
            let matched_text = input.clone().text();
            arms.iter()
                .find(|arm| {
                    arm.pattern == "__" || runtime_text_pattern_matches(&matched_text, &arm.pattern)
                })
                .and_then(|arm| const_value(graph, &arm.value, Some(input), env))
        }
        boon_dd::DdRenderGraphOperation::SourceAt { .. }
        | boon_dd::DdRenderGraphOperation::Link { .. } => Some(input),
        _ => const_value(graph, &stage.node, Some(input), env),
    }
}

fn const_call_value(
    graph: &boon_dd::DdRenderGraph,
    callee: &str,
    pipe_input: Option<ConstValue>,
    args: &[boon_dd::DdRenderGraphArg],
    env: &BTreeMap<String, ConstValue>,
) -> Option<ConstValue> {
    match canonical_runtime_call(callee) {
        "Document/new" | "Scene/new" => pipe_input
            .clone()
            .or_else(|| const_named_arg_value(graph, args, "root", &pipe_input, env))
            .or_else(|| const_first_arg_value(graph, args, &pipe_input, env)),
        "Text/from_number" => pipe_input
            .clone()
            .or_else(|| const_first_arg_value(graph, args, &pipe_input, env))
            .map(|value| ConstValue::Text(value.number().to_string())),
        "Text/append" => {
            let input = pipe_input
                .clone()
                .or_else(|| const_first_arg_value(graph, args, &pipe_input, env))?;
            let suffix = first_runtime_arg(args)
                .and_then(|node| const_value(graph, node, None, env))
                .unwrap_or(ConstValue::Text(String::new()));
            Some(ConstValue::Text(format!(
                "{}{}",
                input.text(),
                suffix.text()
            )))
        }
        "Text/join" | "Text/join_lines" => {
            let input = pipe_input.clone()?;
            let separator = const_named_arg_value(graph, args, "separator", &pipe_input, env)
                .or_else(|| const_first_arg_value(graph, args, &pipe_input, env))
                .map(ConstValue::text)
                .unwrap_or_else(|| {
                    if callee == "Text/join_lines" {
                        "\n"
                    } else {
                        ","
                    }
                    .to_owned()
                });
            Some(ConstValue::Text(match input {
                ConstValue::List(values) => values
                    .into_iter()
                    .map(ConstValue::text)
                    .collect::<Vec<_>>()
                    .join(&separator),
                value => value.text(),
            }))
        }
        "Text/uppercase" => pipe_input
            .clone()
            .or_else(|| const_first_arg_value(graph, args, &pipe_input, env))
            .map(|value| ConstValue::Text(value.text().to_uppercase())),
        "Text/is_empty" => pipe_input
            .clone()
            .or_else(|| const_first_arg_value(graph, args, &pipe_input, env))
            .map(|value| ConstValue::Bool(value.text().is_empty())),
        "Bool/not" => pipe_input
            .clone()
            .or_else(|| const_first_arg_value(graph, args, &pipe_input, env))
            .map(|value| ConstValue::Bool(!value.bool())),
        "List/append" => {
            let input = pipe_input.clone()?;
            let item = const_named_arg_value(graph, args, "item", &pipe_input, env)
                .or_else(|| const_first_arg_value(graph, args, &pipe_input, env))
                .unwrap_or(ConstValue::Empty);
            match input {
                ConstValue::List(mut values) => {
                    values.push(item);
                    Some(ConstValue::List(values))
                }
                value => Some(value),
            }
        }
        "List/map" => {
            let input = pipe_input.clone()?;
            let new_expr = named_runtime_arg(args, "new")?;
            match input {
                ConstValue::List(values) => values
                    .into_iter()
                    .map(|item| {
                        let mut env = env.clone();
                        env.insert("item".to_owned(), item);
                        const_value(graph, new_expr, None, &env)
                    })
                    .collect::<Option<Vec<_>>>()
                    .map(ConstValue::List),
                value => Some(value),
            }
        }
        "List/retain" => {
            let input = pipe_input.clone()?;
            let predicate = named_runtime_arg(args, "if");
            match input {
                ConstValue::List(values) => Some(ConstValue::List(
                    values
                        .into_iter()
                        .filter(|item| {
                            let Some(predicate) = predicate else {
                                return true;
                            };
                            let mut env = env.clone();
                            env.insert("item".to_owned(), item.clone());
                            const_value(graph, predicate, None, &env)
                                .map(ConstValue::bool)
                                .unwrap_or(false)
                        })
                        .collect(),
                )),
                value => Some(value),
            }
        }
        "List/count" => {
            let input = pipe_input.clone()?;
            let predicate = named_runtime_arg(args, "if");
            match input {
                ConstValue::List(values) => Some(ConstValue::Number(
                    values
                        .into_iter()
                        .filter(|item| {
                            let Some(predicate) = predicate else {
                                return true;
                            };
                            let mut env = env.clone();
                            env.insert("item".to_owned(), item.clone());
                            const_value(graph, predicate, None, &env)
                                .map(ConstValue::bool)
                                .unwrap_or(false)
                        })
                        .count() as i64,
                )),
                ConstValue::Empty => Some(ConstValue::Number(0)),
                _ => Some(ConstValue::Number(1)),
            }
        }
        "List/latest" => match pipe_input.clone()? {
            ConstValue::List(values) => {
                Some(values.into_iter().last().unwrap_or(ConstValue::Empty))
            }
            value => Some(value),
        },
        "Math/sum" => None,
        "Temperature/c_to_f" => pipe_input
            .clone()
            .or_else(|| const_first_arg_value(graph, args, &pipe_input, env))
            .map(|value| ConstValue::Number(value.number() * 9 / 5 + 32)),
        callee if callee.starts_with("Element/") => pipe_input
            .clone()
            .or_else(|| const_named_arg_value(graph, args, "label", &pipe_input, env))
            .or_else(|| const_named_arg_value(graph, args, "text", &pipe_input, env))
            .or_else(|| const_first_arg_value(graph, args, &pipe_input, env)),
        _ => pipe_input
            .clone()
            .or_else(|| const_first_arg_value(graph, args, &pipe_input, env)),
    }
}

fn const_first_arg_value(
    graph: &boon_dd::DdRenderGraph,
    args: &[boon_dd::DdRenderGraphArg],
    pipe_input: &Option<ConstValue>,
    env: &BTreeMap<String, ConstValue>,
) -> Option<ConstValue> {
    first_runtime_arg(args).and_then(|node| const_value(graph, node, pipe_input.clone(), env))
}

fn const_named_arg_value(
    graph: &boon_dd::DdRenderGraph,
    args: &[boon_dd::DdRenderGraphArg],
    name: &str,
    pipe_input: &Option<ConstValue>,
    env: &BTreeMap<String, ConstValue>,
) -> Option<ConstValue> {
    named_runtime_arg(args, name).and_then(|node| const_value(graph, node, pipe_input.clone(), env))
}

fn const_path_value(
    path: &str,
    pipe_input: Option<ConstValue>,
    env: &BTreeMap<String, ConstValue>,
) -> Option<ConstValue> {
    if path == "pipe_input" {
        return pipe_input;
    }
    let mut parts = path.split('.');
    let root = parts.next()?;
    let mut value = env.get(root).cloned()?;
    for part in parts {
        value = value.record_member(part)?;
    }
    Some(value)
}

fn canonical_runtime_call(callee: &str) -> &str {
    match callee {
        "Scene/Element/block" => "Element/block",
        "Scene/Element/button" => "Element/button",
        "Scene/Element/checkbox" => "Element/checkbox",
        "Scene/Element/container" => "Element/container",
        "Scene/Element/grid" => "Element/grid",
        "Scene/Element/label" => "Element/label",
        "Scene/Element/link" => "Element/link",
        "Scene/Element/panel" => "Element/panel",
        "Scene/Element/paragraph" => "Element/paragraph",
        "Scene/Element/rect" => "Element/rect",
        "Scene/Element/select" => "Element/select",
        "Scene/Element/slider" => "Element/slider",
        "Scene/Element/stack" => "Element/stack",
        "Scene/Element/stripe" => "Element/stripe",
        "Scene/Element/svg" => "Element/svg",
        "Scene/Element/svg_circle" => "Element/svg_circle",
        "Scene/Element/text" => "Element/text",
        "Scene/Element/text_input" => "Element/text_input",
        _ => callee,
    }
}

fn first_runtime_arg(args: &[boon_dd::DdRenderGraphArg]) -> Option<&NodeId> {
    args.iter().find_map(|arg| match arg {
        boon_dd::DdRenderGraphArg::Positional(node) => Some(node),
        boon_dd::DdRenderGraphArg::Named { .. } => None,
    })
}

fn named_runtime_arg<'a>(
    args: &'a [boon_dd::DdRenderGraphArg],
    requested: &str,
) -> Option<&'a NodeId> {
    args.iter().find_map(|arg| match arg {
        boon_dd::DdRenderGraphArg::Named { name, value } if name == requested => Some(value),
        _ => None,
    })
}

fn runtime_text_pattern_matches(value: &str, pattern: &str) -> bool {
    value == pattern || (value.is_empty() && pattern.is_empty())
}

fn runtime_event_matches_bound_source(event: &GeneratedSourceEvent, source_ids: &[String]) -> bool {
    match event {
        GeneratedSourceEvent::Static { source_id, .. } if source_id.0 == "__persisted_text" => true,
        GeneratedSourceEvent::Static { source_id, .. } => {
            source_ids.iter().any(|bound| source_id.0 == *bound)
        }
        GeneratedSourceEvent::Dynamic { family_id, .. } => {
            source_ids.iter().any(|bound| family_id.0 == *bound)
        }
    }
}

fn runtime_source_event_owner(event: &GeneratedSourceEvent) -> OwnerKey {
    match event {
        GeneratedSourceEvent::Static { .. } => OwnerKey("Root".to_owned()),
        GeneratedSourceEvent::Dynamic { owner_key, .. } => owner_key.clone(),
    }
}

fn runtime_source_event_text(event: &GeneratedSourceEvent) -> String {
    match event {
        GeneratedSourceEvent::Static { payload, .. }
        | GeneratedSourceEvent::Dynamic { payload, .. } => runtime_payload_text(payload),
    }
}

fn runtime_payload_text(payload: &GeneratedSourceEventPayload) -> String {
    match payload {
        GeneratedSourceEventPayload::EmptyRecord => String::new(),
        GeneratedSourceEventPayload::Text(text) => text.clone(),
        GeneratedSourceEventPayload::Number(BoonNumber::Int(number)) => number.to_string(),
        GeneratedSourceEventPayload::Number(BoonNumber::Float(number)) => number.to_string(),
        GeneratedSourceEventPayload::Tag { name, payload } => match payload {
            Some(payload) => format!("{}({})", name.0, boon_dd::value_to_text(payload)),
            None => name.0.clone(),
        },
        GeneratedSourceEventPayload::Record(record) => record
            .iter()
            .map(|(name, value)| format!("{name}={}", runtime_boon_text(value)))
            .collect::<Vec<_>>()
            .join(","),
        GeneratedSourceEventPayload::List(values) => values
            .iter()
            .map(runtime_boon_text)
            .collect::<Vec<_>>()
            .join(","),
    }
}

fn runtime_boon_text(value: &BoonValue) -> String {
    match value {
        BoonValue::EmptyRecord => String::new(),
        BoonValue::Record(record) => record
            .iter()
            .map(|(name, value)| format!("{name}={}", runtime_boon_text(value)))
            .collect::<Vec<_>>()
            .join(","),
        BoonValue::List(values) => values
            .iter()
            .map(runtime_boon_text)
            .collect::<Vec<_>>()
            .join(","),
        BoonValue::Text(text) => text.clone(),
        BoonValue::Number(BoonNumber::Int(number)) => number.to_string(),
        BoonValue::Number(BoonNumber::Float(number)) => number.to_string(),
        BoonValue::Tag { name, payload } => match payload {
            Some(payload) => format!("{}({})", name.0, boon_dd::value_to_text(payload)),
            None => name.0.clone(),
        },
    }
}

pub fn parse_scenario(text: &str) -> Scenario {
    parse_scenario_result(text).expect("scenario TOML must be structurally valid")
}

pub fn parse_scenario_result(text: &str) -> Result<Scenario, String> {
    let root = text
        .parse::<toml::Value>()
        .map_err(|error| format!("invalid scenario TOML: {error}"))?;
    let root_table = root
        .as_table()
        .ok_or_else(|| "scenario root must be a TOML table".to_owned())?;
    reject_unknown_keys(root_table, &["initial", "step"], "scenario root")?;
    if let Some(initial) = root.get("initial").and_then(toml::Value::as_table) {
        reject_unknown_keys(initial, &["expect_text"], "initial")?;
    }
    let initial_expect_text = root
        .get("initial")
        .and_then(|initial| initial.get("expect_text"))
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let steps = root
        .get("step")
        .and_then(toml::Value::as_array)
        .map(|steps| steps.iter().map(parse_step).collect::<Result<Vec<_>, _>>())
        .transpose()?
        .unwrap_or_default();
    Ok(Scenario {
        initial_expect_text,
        steps,
    })
}

fn parse_step(value: &toml::Value) -> Result<ScenarioStep, String> {
    let table = value
        .as_table()
        .ok_or_else(|| format!("scenario step must be a table: {value:?}"))?;
    reject_unknown_keys(
        table,
        &[
            "description",
            "actions",
            "expect_text",
            "expect_monitor_changed",
        ],
        "step",
    )?;
    let description = value
        .get("description")
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let expect_text = value
        .get("expect_text")
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let expect_monitor_changed = value
        .get("expect_monitor_changed")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(|node| NodeId(node.to_owned()))
        .collect();
    let mut actions = Vec::new();
    let mut commands = Vec::new();
    let mut events = Vec::new();
    for action in value
        .get("actions")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(command) = action.get("command").and_then(toml::Value::as_str) {
            let command = ScenarioCommand {
                command: command.to_owned(),
            };
            events.push(ScenarioEvent::Command(command.clone()));
            commands.push(command);
        } else {
            let action = parse_action(action)?;
            events.push(ScenarioEvent::Source(action.clone()));
            actions.push(action);
        }
    }
    Ok(ScenarioStep {
        description,
        events,
        actions,
        commands,
        expect_text,
        expect_monitor_changed,
    })
}

fn parse_action(value: &toml::Value) -> Result<SourceAction, String> {
    let table = value
        .as_table()
        .ok_or_else(|| format!("scenario action must be a table: {value:?}"))?;
    reject_unknown_keys(
        table,
        &["source", "owner", "generation", "value", "command"],
        "action",
    )?;
    let source = value
        .get("source")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("scenario action missing source: {value:?}"))?
        .to_owned();
    let owner = value
        .get("owner")
        .and_then(toml::Value::as_str)
        .map(|owner| OwnerKey(owner.to_owned()));
    let generation = value
        .get("generation")
        .and_then(toml::Value::as_integer)
        .and_then(|generation| u32::try_from(generation).ok());
    let payload = value
        .get("value")
        .map(parse_value)
        .unwrap_or(BoonValue::EmptyRecord);
    Ok(SourceAction {
        source,
        owner,
        generation,
        value: payload,
    })
}

fn reject_unknown_keys(
    table: &toml::map::Map<String, toml::Value>,
    allowed: &[&str],
    context: &str,
) -> Result<(), String> {
    for key in table.keys() {
        if !allowed.iter().any(|allowed| *allowed == key) {
            return Err(format!("unknown {context} key `{key}`"));
        }
    }
    Ok(())
}

fn parse_value(value: &toml::Value) -> BoonValue {
    match value {
        toml::Value::String(text) => text_to_value(text),
        toml::Value::Integer(number) => BoonValue::Number(BoonNumber::Int(*number)),
        toml::Value::Float(number) => BoonValue::Number(BoonNumber::Float(*number)),
        toml::Value::Boolean(value) => BoonValue::Tag {
            name: boon_dd::TagName(if *value { "True" } else { "False" }.to_owned()),
            payload: None,
        },
        toml::Value::Array(values) if values.is_empty() => BoonValue::EmptyRecord,
        toml::Value::Array(values) => {
            BoonValue::List(values.iter().map(parse_value).collect::<Vec<_>>())
        }
        toml::Value::Table(table) => BoonValue::Record(
            table
                .iter()
                .map(|(key, value)| (key.clone(), parse_value(value)))
                .collect::<BTreeMap<_, _>>(),
        ),
        toml::Value::Datetime(value) => BoonValue::Text(value.to_string()),
    }
}

fn text_to_value(text: &str) -> BoonValue {
    match text {
        "True" | "False" | "Enter" | "Escape" | "Active" | "Completed" => BoonValue::Tag {
            name: boon_dd::TagName(text.to_owned()),
            payload: None,
        },
        _ => BoonValue::Text(text.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands_without_dropping_them() {
        let scenario = parse_scenario(include_str!("../../../examples/counter_hold/scenario.toml"));
        let events = &scenario.steps[1].events;
        assert!(matches!(
            &events[0],
            ScenarioEvent::Command(command) if command.command == "enable_persistence"
        ));
        assert!(
            matches!(&events[1], ScenarioEvent::Source(action) if action.source == "store.sources.increment_button.event.press")
        );
        assert!(matches!(
            &events[2],
            ScenarioEvent::Command(command) if command.command == "reload"
        ));
    }

    #[test]
    fn parses_source_actions_structurally() {
        let scenario = parse_scenario(include_str!("../../../examples/when/scenario.toml"));
        let action = &scenario.steps[0].actions[0];
        assert_eq!(action.source, "key_down.key");
        assert!(matches!(
            action.value,
            BoonValue::Tag {
                ref name,
                payload: None
            } if name.0 == "Enter"
        ));
    }

    #[test]
    fn invalid_scenario_toml_is_not_silently_emptied() {
        let error = parse_scenario_result("[[step]\n").expect_err("invalid TOML must fail");
        assert!(error.contains("invalid scenario TOML"));
    }

    #[test]
    fn unknown_scenario_keys_fail() {
        let error = parse_scenario_result(
            r#"
            [initial]
            expect_text = "0"

            [[step]]
            description = "bad"
            typo = true
            actions = []
            expect_text = "0"
            "#,
        )
        .expect_err("unknown step key must fail");
        assert!(error.contains("unknown step key `typo`"));
    }
}
