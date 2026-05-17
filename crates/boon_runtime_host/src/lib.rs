use boon_dd::{
    BoonNumber, BoonTime, BoonValue, Diff, EffectCommand, EncodedTime, GeneratedSourceEvent,
    GeneratedSourceEventPayload, MonitorRecord, NodeId, OwnerKey, PersistenceCommand,
    RenderCommand, Scenario, ScenarioCommand, ScenarioEvent, ScenarioStep, SmokeOutput,
    SourceAction, SourceFamilyId, SourceId,
};
use differential_dataflow::collection::VecCollection;
use differential_dataflow::input::InputSession;
use std::collections::BTreeMap;
use std::sync::mpsc;
use std::thread;
use timely::dataflow::operators::probe::Handle as ProbeHandle;

pub fn run_dd_graph_scenario(
    source_path: impl Into<String>,
    source_text: impl Into<String>,
    scenario_text: &str,
) -> Result<SmokeOutput, String> {
    Ok(
        run_dd_graph_scenario_steps(source_path, source_text, scenario_text)?
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

pub fn run_dd_graph_scenario_steps(
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
        if !submitted || !session.has_bound_sources() {
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

pub fn run_dd_graph_actions(
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
    outputs: mpsc::Receiver<SmokeOutput>,
    source_ids_by_path: BTreeMap<String, SourceId>,
    monitor_node: NodeId,
    render_node: NodeId,
    persistence_nodes: Vec<NodeId>,
    has_bound_sources: bool,
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
        let effect_requests: Vec<_> = plan
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
            .collect();
        let persistence_nodes: Vec<_> = plan
            .dd_graph_ir
            .output_protocol
            .sinks
            .iter()
            .filter_map(|sink| match sink {
                boon_dd::DdOutputSink::Persistence { node, .. } => Some(node.clone()),
                _ => None,
            })
            .collect();
        let monitor_node = plan.graph.monitor_node.clone();
        let render_node = plan.graph.render_node.clone();
        let source_ids_by_path = plan
            .graph
            .source_bindings
            .iter()
            .map(|binding| (binding.path.clone(), binding.source_id.clone()))
            .collect::<BTreeMap<_, _>>();
        let bound_source_ids: Vec<_> = plan
            .graph
            .source_bindings
            .iter()
            .map(|binding| binding.source_id.0.clone())
            .collect();
        let allocator = timely::communication::Allocator::Thread(
            timely::communication::allocator::Thread::default(),
        );
        let mut worker =
            timely::worker::Worker::new(timely::WorkerConfig::default(), allocator, None);
        let mut sources = InputSession::<EncodedTime, (u64, GeneratedSourceEvent), Diff>::new();
        let mut probe = ProbeHandle::new();
        let (output_in_graph, outputs) = mpsc::channel::<SmokeOutput>();
        let monitor_in_graph = monitor_node.clone();
        let render_in_graph = render_node.clone();
        let effect_requests_in_graph = effect_requests.clone();
        let persistence_nodes_in_graph = persistence_nodes.clone();
        let bound_source_ids_in_graph = bound_source_ids.clone();

        worker.dataflow::<EncodedTime, _, _>(|scope| {
            let events = sources.to_collection(scope);
            let render_events = if bound_source_ids_in_graph.first().is_some() {
                let bound_source_ids = bound_source_ids_in_graph.clone();
                events.clone().filter(move |(_sequence, event)| {
                    runtime_event_matches_bound_source(event, &bound_source_ids)
                })
            } else {
                events
                    .clone()
                    .filter(|(_sequence, event)| runtime_event_is_host_tick(event))
            };
            let rendered_values =
                lower_render_text_collection(&graph, &render_events, &bound_source_ids_in_graph)
                    .map(|text| ((), text));
            let rendered_owners = render_events
                .clone()
                .map(|(_sequence, event)| ((), runtime_source_event_owner(&event)));
            let rendered = rendered_values
                .join(rendered_owners)
                .map(|(_key, (text, owner))| (owner, text));
            rendered
                .inspect(move |((owner, text), time, diff)| {
                    if *diff > 0 {
                        let _ = output_in_graph.send(SmokeOutput {
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
            has_bound_sources: bound_source_ids.first().is_some(),
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

    pub fn has_bound_sources(&self) -> bool {
        self.has_bound_sources
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
        let mut latest_output = None;
        while let Ok(output) = self.outputs.try_recv() {
            latest_output = Some(output);
        }
        Ok(latest_output.unwrap_or_else(|| SmokeOutput {
            monitor: vec![MonitorRecord::NodeValue {
                epoch,
                node: self.monitor_node.clone(),
                owner: root_owner_key(),
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

fn lower_render_text_collection<'scope>(
    graph: &boon_dd::DdRenderGraph,
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    source_ids: &[String],
) -> VecCollection<'scope, EncodedTime, String, Diff> {
    lower_node_stream(graph, &graph.root, events, source_ids, &BTreeMap::new()).to_text()
}

enum RenderStream<'scope> {
    Text(VecCollection<'scope, EncodedTime, String, Diff>),
    Number(VecCollection<'scope, EncodedTime, i64, Diff>),
    Flag(VecCollection<'scope, EncodedTime, bool, Diff>),
}

impl<'scope> RenderStream<'scope> {
    fn to_text(self) -> VecCollection<'scope, EncodedTime, String, Diff> {
        match self {
            RenderStream::Text(values) => values,
            RenderStream::Number(values) => values.map(|value| value.to_string()),
            RenderStream::Flag(values) => {
                values.map(|value| if value { "True" } else { "False" }.to_owned())
            }
        }
    }

    fn to_flag(self) -> VecCollection<'scope, EncodedTime, bool, Diff> {
        match self {
            RenderStream::Flag(values) => values,
            RenderStream::Text(values) => {
                values.map(|value| !value.is_empty() && value != "False" && value != "false")
            }
            RenderStream::Number(values) => values.map(|value| value != 0),
        }
    }

    fn to_unit(self) -> VecCollection<'scope, EncodedTime, (), Diff> {
        match self {
            RenderStream::Text(values) => values.map(|_| ()),
            RenderStream::Number(values) => values.map(|_| ()),
            RenderStream::Flag(values) => values.map(|_| ()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Literal {
    Empty,
    Text(String),
    Number(i64),
    Flag(bool),
    Tag(String),
    Items(Vec<Literal>),
    Fields(BTreeMap<String, Literal>),
}

impl Literal {
    fn as_text(self) -> String {
        match self {
            Literal::Empty => String::new(),
            Literal::Text(value) | Literal::Tag(value) => value,
            Literal::Number(value) => value.to_string(),
            Literal::Flag(value) => if value { "True" } else { "False" }.to_owned(),
            Literal::Items(values) => {
                let mut out = String::new();
                for (index, value) in values.into_iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    out.push_str(&value.as_text());
                }
                out
            }
            Literal::Fields(_) => String::new(),
        }
    }

    fn as_number(self) -> i64 {
        match self {
            Literal::Number(value) => value,
            Literal::Text(value) | Literal::Tag(value) => value.parse::<i64>().unwrap_or_default(),
            Literal::Flag(value) => i64::from(value),
            Literal::Items(values) => i64::try_from(values.len()).unwrap_or(i64::MAX),
            Literal::Fields(fields) => i64::try_from(fields.len()).unwrap_or(i64::MAX),
            Literal::Empty => 0,
        }
    }

    fn as_flag(self) -> bool {
        match self {
            Literal::Flag(value) => value,
            Literal::Tag(value) => matches!(value.as_str(), "True" | "true" | "Some"),
            Literal::Text(value) => !value.is_empty() && value != "False" && value != "false",
            Literal::Number(value) => value != 0,
            Literal::Items(values) => !values.is_empty(),
            Literal::Fields(fields) => !fields.is_empty(),
            Literal::Empty => false,
        }
    }

    fn member(self, name: &str) -> Option<Literal> {
        match self {
            Literal::Fields(fields) => fields.get(name).cloned(),
            _ => None,
        }
    }
}

type LiteralEnv = BTreeMap<String, Literal>;

fn lower_node_stream<'scope>(
    graph: &boon_dd::DdRenderGraph,
    node: &NodeId,
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    source_ids: &[String],
    env: &LiteralEnv,
) -> RenderStream<'scope> {
    if let Some(literal) = fold_literal(graph, node, None, env) {
        return literal_stream(events.clone().map(|_| ()), literal);
    }
    let Some(node) = graph.nodes.iter().find(|candidate| &candidate.node == node) else {
        return RenderStream::Text(events.clone().map(|_| String::new()));
    };
    match &node.operation {
        boon_dd::DdRenderGraphOperation::Source | boon_dd::DdRenderGraphOperation::Path(_) => {
            RenderStream::Text(source_event_text_stream(events, source_ids))
        }
        boon_dd::DdRenderGraphOperation::Pipe { input, stage } => {
            let input = lower_node_stream(graph, input, events, source_ids, env);
            lower_stage_stream(graph, stage, input, events, source_ids, env)
        }
        boon_dd::DdRenderGraphOperation::Then { body } => {
            then_stream(graph, body, events.clone().map(|_| ()), env)
        }
        boon_dd::DdRenderGraphOperation::Hold { body, binder } => {
            hold_stream(graph, events.clone().map(|_| ()), body, binder, env)
        }
        boon_dd::DdRenderGraphOperation::Latest(_) => {
            RenderStream::Text(latest_event_text_stream(events, source_ids))
        }
        boon_dd::DdRenderGraphOperation::Match { arms, .. } => match_text_stream(
            graph,
            source_event_text_stream(events, source_ids),
            arms,
            env,
        ),
        _ => RenderStream::Text(events.clone().map(|_| String::new())),
    }
}

fn lower_stage_stream<'scope>(
    graph: &boon_dd::DdRenderGraph,
    stage: &NodeId,
    input: RenderStream<'scope>,
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    source_ids: &[String],
    env: &LiteralEnv,
) -> RenderStream<'scope> {
    let Some(stage_node) = graph
        .nodes
        .iter()
        .find(|candidate| &candidate.node == stage)
    else {
        return RenderStream::Text(input.to_unit().map(|_| String::new()));
    };
    match &stage_node.operation {
        boon_dd::DdRenderGraphOperation::Call { callee, args } => {
            lower_call_stream(graph, callee, input, args, env)
        }
        boon_dd::DdRenderGraphOperation::Then { body } => {
            then_stream(graph, body, input.to_unit(), env)
        }
        boon_dd::DdRenderGraphOperation::Hold { body, binder } => {
            hold_stream(graph, input.to_unit(), body, binder, env)
        }
        boon_dd::DdRenderGraphOperation::Latest(_) => {
            RenderStream::Text(latest_event_text_stream(events, source_ids))
        }
        boon_dd::DdRenderGraphOperation::Match { arms, .. } => {
            match_text_stream(graph, input.to_text(), arms, env)
        }
        boon_dd::DdRenderGraphOperation::SourceAt { .. }
        | boon_dd::DdRenderGraphOperation::Link { .. } => input,
        _ => lower_node_stream(graph, &stage_node.node, events, source_ids, env),
    }
}

fn lower_call_stream<'scope>(
    graph: &boon_dd::DdRenderGraph,
    callee: &str,
    input: RenderStream<'scope>,
    args: &[boon_dd::DdRenderGraphArg],
    env: &LiteralEnv,
) -> RenderStream<'scope> {
    match canonical_library_call(callee) {
        "Math/sum" => RenderStream::Number(
            input
                .to_unit()
                .count()
                .map(|(_key, count)| i64::try_from(count).unwrap_or(i64::MAX)),
        ),
        "Text/from_number" | "Document/new" | "Scene/new" => RenderStream::Text(input.to_text()),
        "Text/append" => {
            let suffix = first_arg(args)
                .and_then(|node| fold_literal(graph, node, None, env))
                .map(Literal::as_text)
                .unwrap_or_default();
            RenderStream::Text(input.to_text().map(move |text| format!("{text}{suffix}")))
        }
        "Text/uppercase" => RenderStream::Text(input.to_text().map(|text| text.to_uppercase())),
        "Text/is_empty" => RenderStream::Flag(input.to_text().map(|text| text.is_empty())),
        "Bool/not" => RenderStream::Flag(input.to_flag().map(|value| !value)),
        "Timer/interval" | "Window/animation_frame" => input,
        callee if callee.starts_with("Element/") => RenderStream::Text(input.to_text()),
        _ => RenderStream::Text(input.to_text()),
    }
}

fn then_stream<'scope>(
    graph: &boon_dd::DdRenderGraph,
    body: &[NodeId],
    trigger: VecCollection<'scope, EncodedTime, (), Diff>,
    env: &LiteralEnv,
) -> RenderStream<'scope> {
    let literal = body
        .last()
        .and_then(|node| fold_literal(graph, node, None, env))
        .unwrap_or(Literal::Empty);
    literal_stream(trigger, literal)
}

fn hold_stream<'scope>(
    graph: &boon_dd::DdRenderGraph,
    trigger: VecCollection<'scope, EncodedTime, (), Diff>,
    body: &[NodeId],
    binder: &str,
    env: &LiteralEnv,
) -> RenderStream<'scope> {
    let graph = graph.clone();
    let body = body.to_vec();
    let binder = binder.to_owned();
    let env = env.clone();
    RenderStream::Number(trigger.count().map(move |(_key, count)| {
        let mut env = env.clone();
        env.insert(
            binder.clone(),
            Literal::Number(i64::try_from(count.saturating_sub(1)).unwrap_or(i64::MAX)),
        );
        body.last()
            .and_then(|node| fold_literal(&graph, node, None, &env))
            .map(Literal::as_number)
            .unwrap_or_else(|| i64::try_from(count).unwrap_or(i64::MAX))
    }))
}

fn match_text_stream<'scope>(
    graph: &boon_dd::DdRenderGraph,
    input: VecCollection<'scope, EncodedTime, String, Diff>,
    arms: &[boon_dd::DdRenderGraphMatchArm],
    env: &LiteralEnv,
) -> RenderStream<'scope> {
    let graph = graph.clone();
    let arms = arms.to_vec();
    let env = env.clone();
    RenderStream::Text(input.flat_map(move |matched| {
        arms.iter()
            .find(|arm| text_pattern_matches(&matched, &arm.pattern))
            .and_then(|arm| {
                fold_literal(
                    &graph,
                    &arm.value,
                    Some(Literal::Text(matched.clone())),
                    &env,
                )
            })
            .and_then(|literal| match literal {
                Literal::Empty => None,
                literal => Some(literal.as_text()),
            })
    }))
}

fn literal_stream<'scope>(
    trigger: VecCollection<'scope, EncodedTime, (), Diff>,
    literal: Literal,
) -> RenderStream<'scope> {
    match literal {
        Literal::Number(number) => RenderStream::Number(trigger.map(move |_| number)),
        Literal::Flag(value) => RenderStream::Flag(trigger.map(move |_| value)),
        literal => {
            let text = literal.as_text();
            RenderStream::Text(trigger.map(move |_| text.clone()))
        }
    }
}

fn source_event_text_stream<'scope>(
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    source_ids: &[String],
) -> VecCollection<'scope, EncodedTime, String, Diff> {
    let source_ids = source_ids.to_vec();
    events
        .clone()
        .filter(move |(_sequence, event)| runtime_event_matches_bound_source(event, &source_ids))
        .map(|(_sequence, event)| typed_event_text(&event))
}

fn latest_event_text_stream<'scope>(
    events: &VecCollection<'scope, EncodedTime, (u64, GeneratedSourceEvent), Diff>,
    source_ids: &[String],
) -> VecCollection<'scope, EncodedTime, String, Diff> {
    let source_ids = source_ids.to_vec();
    events
        .clone()
        .filter(move |(_sequence, event)| runtime_event_matches_bound_source(event, &source_ids))
        .map(|(sequence, event)| ((), (sequence, typed_event_text(&event))))
        .reduce(|_, inputs, output| {
            if let Some(((_sequence, text), _diff)) =
                inputs.iter().max_by_key(|((sequence, _), _)| *sequence)
            {
                output.push((text.clone(), 1));
            }
        })
        .map(|(_key, text)| text)
}

fn fold_literal(
    graph: &boon_dd::DdRenderGraph,
    node: &NodeId,
    pipe_input: Option<Literal>,
    env: &LiteralEnv,
) -> Option<Literal> {
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
        boon_dd::DdRenderGraphOperation::Path(path) => literal_path(path, pipe_input, env),
        boon_dd::DdRenderGraphOperation::Number(number) => number
            .parse::<i64>()
            .ok()
            .or_else(|| number.parse::<f64>().ok().map(|value| value as i64))
            .map(Literal::Number),
        boon_dd::DdRenderGraphOperation::Text(text) => Some(Literal::Text(text.clone())),
        boon_dd::DdRenderGraphOperation::Tag(tag) => Some(match tag.as_str() {
            "True" => Literal::Flag(true),
            "False" => Literal::Flag(false),
            _ => Literal::Tag(tag.clone()),
        }),
        boon_dd::DdRenderGraphOperation::Record(fields) => {
            let mut record = BTreeMap::new();
            for item in fields {
                record.insert(
                    item.name.clone(),
                    fold_literal(graph, &item.value, pipe_input.clone(), env)?,
                );
            }
            Some(Literal::Fields(record))
        }
        boon_dd::DdRenderGraphOperation::List(values) => {
            let mut items = Vec::new();
            for value in values {
                items.push(fold_literal(graph, value, pipe_input.clone(), env)?);
            }
            Some(Literal::Items(items))
        }
        boon_dd::DdRenderGraphOperation::Block(values)
        | boon_dd::DdRenderGraphOperation::Latest(values)
        | boon_dd::DdRenderGraphOperation::Then { body: values }
        | boon_dd::DdRenderGraphOperation::Hold { body: values, .. } => values
            .last()
            .and_then(|value| fold_literal(graph, value, pipe_input, env)),
        boon_dd::DdRenderGraphOperation::Constructor { callee, fields } => {
            if callee == "Text" && fields.len() == 1 {
                fold_literal(graph, &fields[0].value, pipe_input, env)
            } else {
                let mut record = BTreeMap::new();
                for item in fields {
                    record.insert(
                        item.name.clone(),
                        fold_literal(graph, &item.value, pipe_input.clone(), env)?,
                    );
                }
                Some(Literal::Fields(record))
            }
        }
        boon_dd::DdRenderGraphOperation::FieldAccess { base, field } => {
            fold_literal(graph, base, pipe_input, env).and_then(|literal| literal.member(field))
        }
        boon_dd::DdRenderGraphOperation::BinaryAdd { left, right } => Some(Literal::Number(
            fold_literal(graph, left, pipe_input.clone(), env)?.as_number()
                + fold_literal(graph, right, pipe_input, env)?.as_number(),
        )),
        boon_dd::DdRenderGraphOperation::BinarySubtract { left, right } => Some(Literal::Number(
            fold_literal(graph, left, pipe_input.clone(), env)?.as_number()
                - fold_literal(graph, right, pipe_input, env)?.as_number(),
        )),
        boon_dd::DdRenderGraphOperation::BinaryEqual { left, right } => Some(Literal::Flag(
            fold_literal(graph, left, pipe_input.clone(), env)?.as_text()
                == fold_literal(graph, right, pipe_input, env)?.as_text(),
        )),
        boon_dd::DdRenderGraphOperation::Pipe { input, stage } => {
            let input = fold_literal(graph, input, pipe_input, env)?;
            fold_stage_literal(graph, stage, input, env)
        }
        boon_dd::DdRenderGraphOperation::Call { callee, args } => {
            fold_call_literal(graph, callee, pipe_input, args, env)
        }
        boon_dd::DdRenderGraphOperation::Match { arms, .. } => {
            let matched = pipe_input?;
            let text = matched.clone().as_text();
            arms.iter()
                .find(|arm| text_pattern_matches(&text, &arm.pattern))
                .and_then(|arm| fold_literal(graph, &arm.value, Some(matched), env))
        }
    }
}

fn fold_stage_literal(
    graph: &boon_dd::DdRenderGraph,
    stage: &NodeId,
    input: Literal,
    env: &LiteralEnv,
) -> Option<Literal> {
    let stage = graph
        .nodes
        .iter()
        .find(|candidate| &candidate.node == stage)?;
    match &stage.operation {
        boon_dd::DdRenderGraphOperation::Call { callee, args } => {
            fold_call_literal(graph, callee, Some(input), args, env)
        }
        boon_dd::DdRenderGraphOperation::Match { arms, .. } => {
            let text = input.clone().as_text();
            arms.iter()
                .find(|arm| text_pattern_matches(&text, &arm.pattern))
                .and_then(|arm| fold_literal(graph, &arm.value, Some(input), env))
        }
        boon_dd::DdRenderGraphOperation::SourceAt { .. }
        | boon_dd::DdRenderGraphOperation::Link { .. } => Some(input),
        _ => fold_literal(graph, &stage.node, Some(input), env),
    }
}

fn fold_call_literal(
    graph: &boon_dd::DdRenderGraph,
    callee: &str,
    pipe_input: Option<Literal>,
    args: &[boon_dd::DdRenderGraphArg],
    env: &LiteralEnv,
) -> Option<Literal> {
    match canonical_library_call(callee) {
        "Document/new" | "Scene/new" => pipe_input
            .clone()
            .or_else(|| named_arg_literal(graph, args, "root", &pipe_input, env))
            .or_else(|| first_arg_literal(graph, args, &pipe_input, env)),
        "Text/from_number" => pipe_input
            .clone()
            .or_else(|| first_arg_literal(graph, args, &pipe_input, env))
            .map(|literal| Literal::Text(literal.as_number().to_string())),
        "Text/append" => {
            let input = pipe_input
                .clone()
                .or_else(|| first_arg_literal(graph, args, &pipe_input, env))?;
            let suffix = first_arg(args)
                .and_then(|node| fold_literal(graph, node, None, env))
                .unwrap_or(Literal::Text(String::new()));
            Some(Literal::Text(format!(
                "{}{}",
                input.as_text(),
                suffix.as_text()
            )))
        }
        "Text/join" | "Text/join_lines" => {
            let input = pipe_input.clone()?;
            let separator = named_arg_literal(graph, args, "separator", &pipe_input, env)
                .or_else(|| first_arg_literal(graph, args, &pipe_input, env))
                .map(Literal::as_text)
                .unwrap_or_else(|| {
                    if callee == "Text/join_lines" {
                        "\n"
                    } else {
                        ","
                    }
                    .to_owned()
                });
            Some(Literal::Text(match input {
                Literal::Items(values) => {
                    let mut out = String::new();
                    for (index, item) in values.into_iter().enumerate() {
                        if index > 0 {
                            out.push_str(&separator);
                        }
                        out.push_str(&item.as_text());
                    }
                    out
                }
                literal => literal.as_text(),
            }))
        }
        "Text/uppercase" => pipe_input
            .clone()
            .or_else(|| first_arg_literal(graph, args, &pipe_input, env))
            .map(|literal| Literal::Text(literal.as_text().to_uppercase())),
        "Text/is_empty" => pipe_input
            .clone()
            .or_else(|| first_arg_literal(graph, args, &pipe_input, env))
            .map(|literal| Literal::Flag(literal.as_text().is_empty())),
        "Bool/not" => pipe_input
            .clone()
            .or_else(|| first_arg_literal(graph, args, &pipe_input, env))
            .map(|literal| Literal::Flag(!literal.as_flag())),
        "List/append" => {
            let input = pipe_input.clone()?;
            let item = named_arg_literal(graph, args, "item", &pipe_input, env)
                .or_else(|| first_arg_literal(graph, args, &pipe_input, env))
                .unwrap_or(Literal::Empty);
            match input {
                Literal::Items(mut values) => {
                    values.push(item);
                    Some(Literal::Items(values))
                }
                literal => Some(literal),
            }
        }
        "List/map" => {
            let input = pipe_input.clone()?;
            let new_expr = named_arg(args, "new")?;
            match input {
                Literal::Items(values) => {
                    let mut mapped = Vec::new();
                    for item in values {
                        let mut env = env.clone();
                        env.insert("item".to_owned(), item);
                        mapped.push(fold_literal(graph, new_expr, None, &env)?);
                    }
                    Some(Literal::Items(mapped))
                }
                literal => Some(literal),
            }
        }
        "List/retain" => {
            let input = pipe_input.clone()?;
            let predicate = named_arg(args, "if");
            match input {
                Literal::Items(values) => {
                    let mut retained = Vec::new();
                    for item in values {
                        let keep = if let Some(predicate) = predicate {
                            let mut env = env.clone();
                            env.insert("item".to_owned(), item.clone());
                            fold_literal(graph, predicate, None, &env)
                                .map(Literal::as_flag)
                                .unwrap_or(false)
                        } else {
                            true
                        };
                        if keep {
                            retained.push(item);
                        }
                    }
                    Some(Literal::Items(retained))
                }
                literal => Some(literal),
            }
        }
        "List/count" => {
            let input = pipe_input.clone()?;
            let predicate = named_arg(args, "if");
            match input {
                Literal::Items(values) => {
                    let mut count = 0_i64;
                    for item in values {
                        let keep = if let Some(predicate) = predicate {
                            let mut env = env.clone();
                            env.insert("item".to_owned(), item);
                            fold_literal(graph, predicate, None, &env)
                                .map(Literal::as_flag)
                                .unwrap_or(false)
                        } else {
                            true
                        };
                        if keep {
                            count = count.saturating_add(1);
                        }
                    }
                    Some(Literal::Number(count))
                }
                Literal::Empty => Some(Literal::Number(0)),
                _ => Some(Literal::Number(1)),
            }
        }
        "List/latest" => match pipe_input.clone()? {
            Literal::Items(values) => Some(values.into_iter().last().unwrap_or(Literal::Empty)),
            literal => Some(literal),
        },
        "Math/sum" => None,
        "Temperature/c_to_f" => pipe_input
            .clone()
            .or_else(|| first_arg_literal(graph, args, &pipe_input, env))
            .map(|literal| Literal::Number(literal.as_number() * 9 / 5 + 32)),
        callee if callee.starts_with("Element/") => pipe_input
            .clone()
            .or_else(|| named_arg_literal(graph, args, "label", &pipe_input, env))
            .or_else(|| named_arg_literal(graph, args, "text", &pipe_input, env))
            .or_else(|| first_arg_literal(graph, args, &pipe_input, env)),
        _ => pipe_input
            .clone()
            .or_else(|| first_arg_literal(graph, args, &pipe_input, env)),
    }
}

fn first_arg_literal(
    graph: &boon_dd::DdRenderGraph,
    args: &[boon_dd::DdRenderGraphArg],
    pipe_input: &Option<Literal>,
    env: &LiteralEnv,
) -> Option<Literal> {
    first_arg(args).and_then(|node| fold_literal(graph, node, pipe_input.clone(), env))
}

fn named_arg_literal(
    graph: &boon_dd::DdRenderGraph,
    args: &[boon_dd::DdRenderGraphArg],
    name: &str,
    pipe_input: &Option<Literal>,
    env: &LiteralEnv,
) -> Option<Literal> {
    named_arg(args, name).and_then(|node| fold_literal(graph, node, pipe_input.clone(), env))
}

fn literal_path(path: &str, pipe_input: Option<Literal>, env: &LiteralEnv) -> Option<Literal> {
    if path == "pipe_input" {
        return pipe_input;
    }
    let mut parts = path.split('.');
    let root = parts.next()?;
    let mut literal = env.get(root).cloned()?;
    for part in parts {
        literal = literal.member(part)?;
    }
    Some(literal)
}

fn canonical_library_call(callee: &str) -> &str {
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

fn first_arg(args: &[boon_dd::DdRenderGraphArg]) -> Option<&NodeId> {
    args.iter().find_map(|arg| match arg {
        boon_dd::DdRenderGraphArg::Positional(node) => Some(node),
        boon_dd::DdRenderGraphArg::Named { .. } => None,
    })
}

fn named_arg<'a>(args: &'a [boon_dd::DdRenderGraphArg], requested: &str) -> Option<&'a NodeId> {
    args.iter().find_map(|arg| match arg {
        boon_dd::DdRenderGraphArg::Named { name, value } if name == requested => Some(value),
        _ => None,
    })
}

fn text_pattern_matches(text: &str, pattern: &str) -> bool {
    pattern == "__" || text == pattern || (text.is_empty() && pattern.is_empty())
}

fn runtime_event_matches_bound_source(event: &GeneratedSourceEvent, source_ids: &[String]) -> bool {
    match event {
        GeneratedSourceEvent::Static { source_id, .. } if source_id.0 == "__persisted_text" => true,
        GeneratedSourceEvent::Static { source_id, .. } => {
            source_ids.iter().any(|bound| source_id.0 == *bound)
        }
        GeneratedSourceEvent::Dynamic {
            family_id,
            owner_key,
            generation,
            ..
        } => {
            let identity = (family_id.0.as_str(), owner_key.0.as_str(), *generation);
            source_ids.iter().any(|bound| identity.0 == bound.as_str()) && !identity.1.is_empty()
        }
    }
}

fn runtime_event_is_host_tick(event: &GeneratedSourceEvent) -> bool {
    matches!(event, GeneratedSourceEvent::Static { source_id, .. } if source_id.0 == "__host_tick")
}

fn runtime_source_event_owner(event: &GeneratedSourceEvent) -> OwnerKey {
    match event {
        GeneratedSourceEvent::Static { .. } => root_owner_key(),
        GeneratedSourceEvent::Dynamic { owner_key, .. } => owner_key.clone(),
    }
}

fn typed_event_text(event: &GeneratedSourceEvent) -> String {
    match event {
        GeneratedSourceEvent::Static { payload, .. } => typed_payload_text(payload),
        GeneratedSourceEvent::Dynamic {
            family_id: _,
            owner_key: _,
            generation: _,
            payload,
        } => typed_payload_text(payload),
    }
}

fn root_owner_key() -> OwnerKey {
    OwnerKey(String::from("Root"))
}

fn typed_payload_text(payload: &GeneratedSourceEventPayload) -> String {
    match payload {
        GeneratedSourceEventPayload::EmptyRecord => String::new(),
        GeneratedSourceEventPayload::Text(text) => text.clone(),
        GeneratedSourceEventPayload::Number(BoonNumber::Int(number)) => number.to_string(),
        GeneratedSourceEventPayload::Number(BoonNumber::Float(number)) => number.to_string(),
        GeneratedSourceEventPayload::Tag { name, payload } => match payload {
            Some(_) => panic!("typed DD source text does not support nested tag payloads"),
            None => name.0.clone(),
        },
        GeneratedSourceEventPayload::Record(_) => {
            panic!("typed DD source text does not support record payloads")
        }
        GeneratedSourceEventPayload::List(_) => {
            panic!("typed DD source text does not support list payloads")
        }
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
            let parsed_values: Vec<_> = values.iter().map(parse_value).collect();
            BoonValue::List(parsed_values)
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
