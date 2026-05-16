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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum RuntimeValue {
    Empty,
    Text(String),
    Number(i64),
    Tag(String),
    List(Vec<RuntimeValue>),
    Record(Vec<(String, RuntimeValue)>),
}

impl RuntimeValue {
    fn text(self) -> String {
        match self {
            RuntimeValue::Text(text) => text,
            RuntimeValue::Number(number) => number.to_string(),
            RuntimeValue::Tag(tag) => tag,
            RuntimeValue::List(values) => values
                .into_iter()
                .map(RuntimeValue::text)
                .collect::<Vec<_>>()
                .join(","),
            RuntimeValue::Empty => String::new(),
            RuntimeValue::Record(_) => String::new(),
        }
    }

    fn number(self) -> i64 {
        match self {
            RuntimeValue::Number(number) => number,
            RuntimeValue::Text(text) => text.parse::<i64>().unwrap_or_default(),
            RuntimeValue::Tag(tag) => tag.parse::<i64>().unwrap_or_default(),
            RuntimeValue::List(values) => values.len() as i64,
            RuntimeValue::Record(fields) => fields.len() as i64,
            RuntimeValue::Empty => 0,
        }
    }

    fn field(self, name: &str) -> RuntimeValue {
        match self {
            RuntimeValue::Record(fields) => fields
                .into_iter()
                .find(|(field, _)| field == name)
                .map(|(_, value)| value)
                .unwrap_or(RuntimeValue::Empty),
            _ => RuntimeValue::Empty,
        }
    }

    fn truthy(self) -> bool {
        match self {
            RuntimeValue::Tag(tag) => matches!(tag.as_str(), "True" | "true" | "Some"),
            RuntimeValue::Text(text) => !text.is_empty() && text != "False" && text != "false",
            RuntimeValue::Number(number) => number != 0,
            RuntimeValue::List(values) => !values.is_empty(),
            RuntimeValue::Record(fields) => !fields.is_empty(),
            RuntimeValue::Empty => false,
        }
    }
}

pub fn run_compiled_source_scenario(
    source_path: impl Into<String>,
    source_text: impl Into<String>,
    scenario_text: &str,
) -> Result<SmokeOutput, String> {
    let source_path = source_path.into();
    let source_text = source_text.into();
    let scenario = parse_scenario_result(scenario_text)?;
    let mut session = CompiledGraphSession::new(source_path.clone(), source_text.clone())?;
    let mut last = session.drain_epoch(0)?;
    let mut persistence_enabled = false;
    let mut persisted_text: Option<String> = None;
    let mut last_generated_persisted_text: Option<String> = None;
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
        last = session.drain_epoch(epoch)?;
        last_generated_persisted_text =
            last.persistence
                .iter()
                .rev()
                .find_map(|command| match command {
                    PersistenceCommand::SaveText { value, .. } => Some(value.clone()),
                    PersistenceCommand::LoadText { .. } => None,
                });
    }
    Ok(last)
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
            let rendered_values = runtime_render_collection(&graph, &events)
                .map(RuntimeValue::text)
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

fn runtime_render_collection<'scope>(
    graph: &boon_dd::DdRenderGraph,
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
) -> VecCollection<'scope, EncodedTime, RuntimeValue, Diff> {
    runtime_value_collection(graph, &graph.root, events, &BTreeMap::new())
}

fn runtime_value_collection<'scope>(
    graph: &boon_dd::DdRenderGraph,
    node: &NodeId,
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    env: &BTreeMap<String, RuntimeValue>,
) -> VecCollection<'scope, EncodedTime, RuntimeValue, Diff> {
    let Some(node) = graph.nodes.iter().find(|candidate| &candidate.node == node) else {
        return events.clone().map(|_| RuntimeValue::Empty);
    };
    match &node.operation {
        boon_dd::DdRenderGraphOperation::Source | boon_dd::DdRenderGraphOperation::Path(_) => {
            events
                .clone()
                .filter(|(_sequence, event)| !runtime_event_is_host_tick(event))
                .map(|(_sequence, event)| runtime_source_event_value(&event))
        }
        boon_dd::DdRenderGraphOperation::Pipe { input, stage } => {
            let input = runtime_value_collection(graph, input, events, env);
            runtime_stage_collection(graph, stage, &input, events, env)
        }
        boon_dd::DdRenderGraphOperation::Then { body } => {
            let graph = graph.clone();
            let body = body.clone();
            let env = env.clone();
            events.clone().map(move |_| {
                body.last()
                    .map(|node| runtime_value(&graph, node, None, &env))
                    .unwrap_or(RuntimeValue::Empty)
            })
        }
        boon_dd::DdRenderGraphOperation::Hold { body, binder } => {
            let graph = graph.clone();
            let body = body.clone();
            let binder = binder.clone();
            let env = env.clone();
            events
                .clone()
                .map(|_| ())
                .count()
                .map(move |(_key, count)| {
                    let mut env = env.clone();
                    env.insert(
                        binder.clone(),
                        RuntimeValue::Number(count.saturating_sub(1) as i64),
                    );
                    body.last()
                        .map(|node| runtime_value(&graph, node, None, &env))
                        .unwrap_or(RuntimeValue::Number(count as i64))
                })
        }
        boon_dd::DdRenderGraphOperation::Latest(_values) => events
            .clone()
            .filter(|(_sequence, event)| !runtime_event_is_host_tick(event))
            .map(|(_sequence, event)| runtime_source_event_value(&event)),
        _ => {
            let graph = graph.clone();
            let node_id = node.node.clone();
            let env = env.clone();
            events
                .clone()
                .map(move |_| runtime_value(&graph, &node_id, None, &env))
        }
    }
}

fn runtime_stage_collection<'scope>(
    graph: &boon_dd::DdRenderGraph,
    stage: &NodeId,
    input: &VecCollection<'scope, EncodedTime, RuntimeValue, Diff>,
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    env: &BTreeMap<String, RuntimeValue>,
) -> VecCollection<'scope, EncodedTime, RuntimeValue, Diff> {
    let Some(stage_node) = graph
        .nodes
        .iter()
        .find(|candidate| &candidate.node == stage)
    else {
        return input.clone().map(|_| RuntimeValue::Empty);
    };
    match &stage_node.operation {
        boon_dd::DdRenderGraphOperation::Call { callee, args } if callee == "Math/sum" => input
            .clone()
            .map(|_| ())
            .count()
            .map(|(_key, count)| RuntimeValue::Number(count as i64)),
        boon_dd::DdRenderGraphOperation::Then { body } => {
            let graph = graph.clone();
            let body = body.clone();
            let env = env.clone();
            input.clone().map(move |_| {
                body.last()
                    .map(|node| runtime_value(&graph, node, None, &env))
                    .unwrap_or(RuntimeValue::Empty)
            })
        }
        boon_dd::DdRenderGraphOperation::Hold { body, binder } => {
            let graph = graph.clone();
            let body = body.clone();
            let binder = binder.clone();
            let env = env.clone();
            input.clone().map(|_| ()).count().map(move |(_key, count)| {
                let mut env = env.clone();
                env.insert(
                    binder.clone(),
                    RuntimeValue::Number(count.saturating_sub(1) as i64),
                );
                body.last()
                    .map(|node| runtime_value(&graph, node, None, &env))
                    .unwrap_or(RuntimeValue::Number(count as i64))
            })
        }
        boon_dd::DdRenderGraphOperation::Latest(_) => events
            .clone()
            .filter(|(_sequence, event)| !runtime_event_is_host_tick(event))
            .map(|(_sequence, event)| runtime_source_event_value(&event)),
        _ => {
            let graph = graph.clone();
            let stage = stage.clone();
            let env = env.clone();
            input
                .clone()
                .map(move |pipe_input| runtime_value(&graph, &stage, Some(pipe_input), &env))
        }
    }
}

fn runtime_value(
    graph: &boon_dd::DdRenderGraph,
    node: &NodeId,
    pipe_input: Option<RuntimeValue>,
    env: &BTreeMap<String, RuntimeValue>,
) -> RuntimeValue {
    let Some(node) = graph.nodes.iter().find(|candidate| &candidate.node == node) else {
        return RuntimeValue::Empty;
    };
    match &node.operation {
        boon_dd::DdRenderGraphOperation::Missing
        | boon_dd::DdRenderGraphOperation::Source
        | boon_dd::DdRenderGraphOperation::SourceAt { .. }
        | boon_dd::DdRenderGraphOperation::Link { .. }
        | boon_dd::DdRenderGraphOperation::Skip => RuntimeValue::Empty,
        boon_dd::DdRenderGraphOperation::Path(path) => {
            if path == "pipe_input" {
                pipe_input.unwrap_or(RuntimeValue::Empty)
            } else if let Some((root, fields)) = path.split_once('.') {
                fields.split('.').fold(
                    env.get(root).cloned().unwrap_or(RuntimeValue::Empty),
                    |value, field| value.field(field),
                )
            } else {
                env.get(path).cloned().unwrap_or(RuntimeValue::Empty)
            }
        }
        boon_dd::DdRenderGraphOperation::Number(number) => RuntimeValue::Number(
            number
                .parse::<i64>()
                .unwrap_or_else(|_| number.parse::<f64>().unwrap_or_default() as i64),
        ),
        boon_dd::DdRenderGraphOperation::Text(text) => RuntimeValue::Text(text.clone()),
        boon_dd::DdRenderGraphOperation::Tag(tag) => RuntimeValue::Tag(tag.clone()),
        boon_dd::DdRenderGraphOperation::Record(fields) => RuntimeValue::Record(
            fields
                .iter()
                .map(|field| {
                    (
                        field.name.clone(),
                        runtime_value(graph, &field.value, pipe_input.clone(), env),
                    )
                })
                .collect(),
        ),
        boon_dd::DdRenderGraphOperation::List(values)
        | boon_dd::DdRenderGraphOperation::Block(values)
        | boon_dd::DdRenderGraphOperation::Latest(values) => RuntimeValue::List(
            values
                .iter()
                .map(|value| runtime_value(graph, value, pipe_input.clone(), env))
                .collect(),
        ),
        boon_dd::DdRenderGraphOperation::Constructor { callee, fields } => {
            if callee == "Text" && fields.len() == 1 {
                runtime_value(graph, &fields[0].value, pipe_input, env)
            } else {
                RuntimeValue::Record(
                    fields
                        .iter()
                        .map(|field| {
                            (
                                field.name.clone(),
                                runtime_value(graph, &field.value, pipe_input.clone(), env),
                            )
                        })
                        .collect(),
                )
            }
        }
        boon_dd::DdRenderGraphOperation::FieldAccess { base, field } => {
            runtime_value(graph, base, pipe_input, env).field(field)
        }
        boon_dd::DdRenderGraphOperation::BinaryAdd { left, right } => RuntimeValue::Number(
            runtime_value(graph, left, pipe_input.clone(), env).number()
                + runtime_value(graph, right, pipe_input, env).number(),
        ),
        boon_dd::DdRenderGraphOperation::BinarySubtract { left, right } => RuntimeValue::Number(
            runtime_value(graph, left, pipe_input.clone(), env).number()
                - runtime_value(graph, right, pipe_input, env).number(),
        ),
        boon_dd::DdRenderGraphOperation::BinaryEqual { left, right } => RuntimeValue::Tag(
            if runtime_value(graph, left, pipe_input.clone(), env)
                == runtime_value(graph, right, pipe_input, env)
            {
                "True"
            } else {
                "False"
            }
            .to_owned(),
        ),
        boon_dd::DdRenderGraphOperation::Pipe { input, stage } => {
            let input = runtime_value(graph, input, pipe_input, env);
            runtime_value(graph, stage, Some(input), env)
        }
        boon_dd::DdRenderGraphOperation::Then { body } => body
            .last()
            .map(|node| runtime_value(graph, node, pipe_input, env))
            .unwrap_or(RuntimeValue::Empty),
        boon_dd::DdRenderGraphOperation::Hold { body, .. } => body
            .last()
            .map(|node| runtime_value(graph, node, pipe_input, env))
            .unwrap_or(RuntimeValue::Empty),
        boon_dd::DdRenderGraphOperation::Call { callee, args } => {
            runtime_call_value(graph, callee, pipe_input, args, env)
        }
        boon_dd::DdRenderGraphOperation::Match { arms, .. } => {
            let matched = pipe_input.unwrap_or(RuntimeValue::Empty);
            arms.iter()
                .find(|arm| arm.pattern == "__" || runtime_pattern_matches(&matched, &arm.pattern))
                .map(|arm| runtime_value(graph, &arm.value, Some(matched.clone()), env))
                .unwrap_or(RuntimeValue::Empty)
        }
    }
}

fn runtime_call_value(
    graph: &boon_dd::DdRenderGraph,
    callee: &str,
    pipe_input: Option<RuntimeValue>,
    args: &[boon_dd::DdRenderGraphArg],
    env: &BTreeMap<String, RuntimeValue>,
) -> RuntimeValue {
    let pipe_for_args = pipe_input.clone();
    let arg = |index: usize| -> RuntimeValue {
        args.get(index)
            .map(|arg| match arg {
                boon_dd::DdRenderGraphArg::Positional(node) => {
                    runtime_value(graph, node, pipe_for_args.clone(), env)
                }
                boon_dd::DdRenderGraphArg::Named { value, .. } => {
                    runtime_value(graph, value, pipe_for_args.clone(), env)
                }
            })
            .unwrap_or(RuntimeValue::Empty)
    };
    match callee {
        "Text/from_number" => RuntimeValue::Text(
            pipe_input
                .clone()
                .unwrap_or_else(|| arg(0))
                .number()
                .to_string(),
        ),
        "Text/append" => RuntimeValue::Text(format!(
            "{}{}",
            pipe_input.clone().unwrap_or_default_text().text(),
            arg(0).text()
        )),
        "Text/join" | "Text/join_lines" => {
            let separator = arg(0).text();
            RuntimeValue::Text(match pipe_input.unwrap_or(RuntimeValue::Empty) {
                RuntimeValue::List(values) => values
                    .into_iter()
                    .map(RuntimeValue::text)
                    .collect::<Vec<_>>()
                    .join(&separator),
                other => other.text(),
            })
        }
        "Text/is_empty" => RuntimeValue::Tag(
            if pipe_input.unwrap_or_default_text().text().is_empty() {
                "True"
            } else {
                "False"
            }
            .to_owned(),
        ),
        "Text/uppercase" => RuntimeValue::Text(
            pipe_input
                .clone()
                .unwrap_or_else(|| arg(0))
                .text()
                .to_uppercase(),
        ),
        "Bool/not" => RuntimeValue::Tag(
            if pipe_input.clone().unwrap_or_else(|| arg(0)).truthy() {
                "False"
            } else {
                "True"
            }
            .to_owned(),
        ),
        "List/count" => RuntimeValue::Number(match pipe_input.unwrap_or(RuntimeValue::Empty) {
            RuntimeValue::List(values) => values.len() as i64,
            RuntimeValue::Empty => 0,
            _ => 1,
        }),
        "List/map" => match pipe_input.unwrap_or(RuntimeValue::Empty) {
            RuntimeValue::List(values) => {
                if let Some(new_expr) = runtime_named_arg(args, "new") {
                    RuntimeValue::List(
                        values
                            .into_iter()
                            .map(|item| {
                                let mut item_env = env.clone();
                                item_env.insert("item".to_owned(), item);
                                runtime_value(graph, new_expr, None, &item_env)
                            })
                            .collect(),
                    )
                } else {
                    RuntimeValue::List(values)
                }
            }
            other => other,
        },
        "List/retain" => match pipe_input.unwrap_or(RuntimeValue::Empty) {
            RuntimeValue::List(values) => {
                if let Some(predicate_expr) = runtime_named_arg(args, "if") {
                    RuntimeValue::List(
                        values
                            .into_iter()
                            .filter(|item| {
                                let mut item_env = env.clone();
                                item_env.insert("item".to_owned(), item.clone());
                                runtime_value(graph, predicate_expr, None, &item_env).truthy()
                            })
                            .collect(),
                    )
                } else {
                    RuntimeValue::List(values)
                }
            }
            other => other,
        },
        "List/latest" => match pipe_input.unwrap_or(RuntimeValue::Empty) {
            RuntimeValue::List(values) => values.into_iter().last().unwrap_or(RuntimeValue::Empty),
            other => other,
        },
        "List/append" => match pipe_input.unwrap_or(RuntimeValue::Empty) {
            RuntimeValue::List(mut values) => {
                values.push(arg(0));
                RuntimeValue::List(values)
            }
            other => other,
        },
        "Document/new" | "Scene/new" => pipe_input.clone().unwrap_or_else(|| arg(0)),
        callee if callee.starts_with("Element/") => pipe_input.clone().unwrap_or_else(|| arg(0)),
        _ => pipe_input.clone().unwrap_or_else(|| arg(0)),
    }
}

fn runtime_named_arg<'a>(
    args: &'a [boon_dd::DdRenderGraphArg],
    requested: &str,
) -> Option<&'a NodeId> {
    args.iter().find_map(|arg| match arg {
        boon_dd::DdRenderGraphArg::Named { name, value } if name == requested => Some(value),
        _ => None,
    })
}

trait RuntimeValueDefaultText {
    fn unwrap_or_default_text(self) -> RuntimeValue;
}

impl RuntimeValueDefaultText for Option<RuntimeValue> {
    fn unwrap_or_default_text(self) -> RuntimeValue {
        self.unwrap_or_else(|| RuntimeValue::Text(String::new()))
    }
}

fn runtime_pattern_matches(value: &RuntimeValue, pattern: &str) -> bool {
    match value {
        RuntimeValue::Tag(tag) => tag == pattern,
        RuntimeValue::Text(text) => text == pattern,
        RuntimeValue::Number(number) => number.to_string() == pattern,
        RuntimeValue::Empty => pattern.is_empty(),
        RuntimeValue::List(_) | RuntimeValue::Record(_) => false,
    }
}

fn runtime_event_is_host_tick(event: &GeneratedSourceEvent) -> bool {
    matches!(event, GeneratedSourceEvent::Static { source_id, .. } if source_id.0 == "__host_tick")
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

fn runtime_source_event_value(event: &GeneratedSourceEvent) -> RuntimeValue {
    match event {
        GeneratedSourceEvent::Static { payload, .. }
        | GeneratedSourceEvent::Dynamic { payload, .. } => runtime_payload_value(payload),
    }
}

fn runtime_payload_value(payload: &GeneratedSourceEventPayload) -> RuntimeValue {
    match payload {
        GeneratedSourceEventPayload::EmptyRecord => RuntimeValue::Empty,
        GeneratedSourceEventPayload::Text(text) => RuntimeValue::Text(text.clone()),
        GeneratedSourceEventPayload::Number(BoonNumber::Int(number)) => {
            RuntimeValue::Number(*number)
        }
        GeneratedSourceEventPayload::Number(BoonNumber::Float(number)) => {
            RuntimeValue::Number(*number as i64)
        }
        GeneratedSourceEventPayload::Tag { name, payload } => RuntimeValue::Tag(match payload {
            Some(payload) => format!("{}({})", name.0, boon_dd::value_to_text(payload)),
            None => name.0.clone(),
        }),
        GeneratedSourceEventPayload::Record(record) => RuntimeValue::Record(
            record
                .iter()
                .map(|(name, value)| (name.clone(), runtime_boon_value(value)))
                .collect(),
        ),
        GeneratedSourceEventPayload::List(values) => {
            RuntimeValue::List(values.iter().map(runtime_boon_value).collect())
        }
    }
}

fn runtime_boon_value(value: &BoonValue) -> RuntimeValue {
    match value {
        BoonValue::EmptyRecord => RuntimeValue::Empty,
        BoonValue::Record(record) => RuntimeValue::Record(
            record
                .iter()
                .map(|(name, value)| (name.clone(), runtime_boon_value(value)))
                .collect(),
        ),
        BoonValue::List(values) => {
            RuntimeValue::List(values.iter().map(runtime_boon_value).collect())
        }
        BoonValue::Text(text) => RuntimeValue::Text(text.clone()),
        BoonValue::Number(BoonNumber::Int(number)) => RuntimeValue::Number(*number),
        BoonValue::Number(BoonNumber::Float(number)) => RuntimeValue::Number(*number as i64),
        BoonValue::Tag { name, payload } => RuntimeValue::Tag(match payload {
            Some(payload) => format!("{}({})", name.0, boon_dd::value_to_text(payload)),
            None => name.0.clone(),
        }),
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
