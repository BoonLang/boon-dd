use differential_dataflow::collection::VecCollection;
use differential_dataflow::input::InputSession;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use timely::dataflow::operators::probe::Handle as ProbeHandle;

pub type EncodedTime = u64;
pub type Diff = isize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BoonTime {
    pub epoch: u64,
    pub phase: u8,
}

impl BoonTime {
    pub const fn encode(self) -> EncodedTime {
        self.epoch * 4 + self.phase as u64
    }

    pub const fn decode(time: EncodedTime) -> Self {
        Self {
            epoch: time / 4,
            phase: (time % 4) as u8,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OwnerKey(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceFamilyId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TagName(pub String);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BoonNumber {
    Int(i64),
    Float(f64),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BoonValue {
    EmptyRecord,
    Record(BTreeMap<String, BoonValue>),
    List(Vec<BoonValue>),
    Text(String),
    Number(BoonNumber),
    Tag {
        name: TagName,
        payload: Option<Box<BoonValue>>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GeneratedSourceEventPayload {
    EmptyRecord,
    Text(String),
    Tag {
        name: TagName,
        payload: Option<BoonValue>,
    },
    Record(BTreeMap<String, BoonValue>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GeneratedSourceEvent {
    Static {
        source_id: SourceId,
        payload: GeneratedSourceEventPayload,
    },
    Dynamic {
        family_id: SourceFamilyId,
        owner_key: OwnerKey,
        generation: u32,
        payload: GeneratedSourceEventPayload,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonitorRecord {
    NodeValue {
        epoch: u64,
        node: NodeId,
        owner: OwnerKey,
        value_preview: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderCommand {
    PatchText { node: NodeId, text: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmokeOutput {
    pub monitor: Vec<MonitorRecord>,
    pub render: Vec<RenderCommand>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceBinding {
    pub source_id: SourceId,
    pub path: String,
    pub shape: String,
    pub dynamic: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNode {
    pub node: NodeId,
    pub kind: String,
    pub shape: String,
    pub source_span: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphOperatorKind {
    SourceLeaf,
    ThenConst,
    Then,
    When,
    WhileSwitch,
    Latest,
    Hold,
    KeyedHold,
    ListAppend,
    ListRemove,
    ListMap,
    ListRetain,
    RenderSink,
    EffectSink,
    PersistTap,
    MonitorTap,
    LibraryCall,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphOperator {
    pub node: NodeId,
    pub kind: GraphOperatorKind,
    pub inputs: Vec<NodeId>,
    pub output: NodeId,
    pub order: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DdScalarPlan {
    ConstantText(String),
    CountInputEvents { initial: i64 },
    LatestInputText,
    MatchTagText { tag: String, text: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticGraph {
    pub graph_id: String,
    pub source_path: String,
    pub source_hash: u64,
    pub source_bindings: Vec<SourceBinding>,
    pub nodes: Vec<GraphNode>,
    pub operators: Vec<GraphOperator>,
    pub monitor_node: NodeId,
    pub render_node: NodeId,
    pub initial_text: String,
    pub dd_plan: DdScalarPlan,
    pub physical_scene: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceAction {
    pub source: String,
    pub owner: Option<OwnerKey>,
    pub generation: Option<u32>,
    pub value: BoonValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioCommand {
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScenarioStep {
    pub description: String,
    pub actions: Vec<SourceAction>,
    #[serde(default)]
    pub commands: Vec<ScenarioCommand>,
    pub expect_text: String,
    pub expect_monitor_changed: Vec<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub initial_expect_text: String,
    pub steps: Vec<ScenarioStep>,
}

pub fn execute_scenario(graph: &StaticGraph, scenario: &Scenario) -> Vec<SmokeOutput> {
    scenario
        .steps
        .iter()
        .map(|step| run_static_dd_graph(graph, &step.actions))
        .collect()
}

pub fn value_to_text(value: &BoonValue) -> String {
    match value {
        BoonValue::EmptyRecord => String::new(),
        BoonValue::Record(_) => "<record>".to_owned(),
        BoonValue::List(values) => values
            .iter()
            .map(value_to_text)
            .collect::<Vec<_>>()
            .join(","),
        BoonValue::Text(text) => text.clone(),
        BoonValue::Number(BoonNumber::Int(number)) => number.to_string(),
        BoonValue::Number(BoonNumber::Float(number)) => number.to_string(),
        BoonValue::Tag { name, payload } => match payload {
            Some(payload) => format!("{}({})", name.0, value_to_text(payload)),
            None => name.0.clone(),
        },
    }
}

pub fn source_action_text(action: &SourceAction) -> String {
    value_to_text(&action.value)
}

pub fn run_static_dd_graph(graph: &StaticGraph, actions: &[SourceAction]) -> SmokeOutput {
    let output = Arc::new(Mutex::new(SmokeOutput {
        monitor: Vec::new(),
        render: Vec::new(),
    }));

    let graph = graph.clone();
    let output_in_graph = Arc::clone(&output);
    let allocator = timely::communication::Allocator::Thread(
        timely::communication::allocator::Thread::default(),
    );
    let mut worker = timely::worker::Worker::new(timely::WorkerConfig::default(), allocator, None);

    {
        let mut input = InputSession::<EncodedTime, (u64, String), Diff>::new();
        let mut probe = ProbeHandle::new();

        worker.dataflow::<EncodedTime, _, _>(|scope| {
            let events = input.to_collection(scope);
            let rendered = match &graph.dd_plan {
                DdScalarPlan::ConstantText(text) => {
                    let text = text.clone();
                    events
                        .map(|_| ())
                        .count()
                        .filter(|(_key, count)| *count > 0)
                        .map(move |_| text.clone())
                }
                DdScalarPlan::CountInputEvents { initial } => {
                    let initial = *initial;
                    events
                        .map(|_| ())
                        .count()
                        .map(move |(_key, count)| (initial + count as i64).to_string())
                }
                DdScalarPlan::LatestInputText => events
                    .map(|(sequence, value)| ((), (sequence, value)))
                    .reduce(|_, inputs, output| {
                        if let Some(((_sequence, value), _diff)) =
                            inputs.iter().max_by_key(|((sequence, _), _)| *sequence)
                        {
                            output.push((value.clone(), 1));
                        }
                    })
                    .map(|(_key, value)| value),
                DdScalarPlan::MatchTagText { tag, text } => {
                    let tag = tag.clone();
                    let text = text.clone();
                    events
                        .filter(move |(_sequence, value)| value == &tag)
                        .map(move |_| text.clone())
                }
            };
            rendered
                .inspect(move |(value, time, diff)| {
                    if *diff > 0 {
                        let epoch = BoonTime::decode(*time).epoch;
                        let mut output = output_in_graph.lock().expect("output lock poisoned");
                        output.monitor.push(MonitorRecord::NodeValue {
                            epoch,
                            node: graph.monitor_node.clone(),
                            owner: OwnerKey("Root".to_owned()),
                            value_preview: value.clone(),
                        });
                        output.render.push(RenderCommand::PatchText {
                            node: graph.render_node.clone(),
                            text: value.clone(),
                        });
                    }
                })
                .probe_with(&mut probe);
        });

        let command_time = BoonTime { epoch: 1, phase: 3 }.encode();
        input.advance_to(command_time);
        for (sequence, action) in actions.iter().enumerate() {
            input.insert((sequence as u64, source_action_text(action)));
        }
        if actions.is_empty() && matches!(graph.dd_plan, DdScalarPlan::ConstantText(_)) {
            input.insert((0, String::new()));
        }
        input.advance_to(command_time + 1);
        input.flush();

        let target = command_time + 1;
        let mut steps = 0;
        while probe.less_than(&target) {
            worker.step();
            steps += 1;
            assert!(steps <= 1024, "probe did not drain by target timestamp");
        }
    }

    output.lock().expect("output lock poisoned").clone()
}

pub const REQUIRED_EXAMPLES: &[&str] = &[
    "counter",
    "counter_hold",
    "interval",
    "interval_hold",
    "latest",
    "when",
    "while",
    "then",
    "list_map_block",
    "list_map_external_dep",
    "list_object_state",
    "list_retain_count",
    "list_retain_reactive",
    "list_retain_remove",
    "shopping_list",
    "todo_mvc",
    "crud",
    "flight_booker",
    "temperature_converter",
    "pong",
    "cells",
    "todo_mvc_physical",
];

pub fn then_const<'scope>(
    input: VecCollection<'scope, EncodedTime, &'static str, Diff>,
    value: i64,
) -> VecCollection<'scope, EncodedTime, i64, Diff> {
    input.map(move |_| value)
}

pub fn hold_sum(
    input: VecCollection<'_, EncodedTime, i64, Diff>,
) -> VecCollection<'_, EncodedTime, i64, Diff> {
    input.count().map(|(_value, count)| count as i64)
}

pub fn run_counter_hold_smoke() -> SmokeOutput {
    let final_value = Arc::new(Mutex::new(0_i64));

    let final_value_in_graph = Arc::clone(&final_value);
    let allocator = timely::communication::Allocator::Thread(
        timely::communication::allocator::Thread::default(),
    );
    let mut worker = timely::worker::Worker::new(timely::WorkerConfig::default(), allocator, None);

    {
        let mut input = InputSession::<EncodedTime, &'static str, Diff>::new();
        let mut probe = ProbeHandle::new();

        worker.dataflow::<EncodedTime, _, _>(|scope| {
            let presses = input.to_collection(scope);
            let ones = then_const(presses, 1);
            let counter = hold_sum(ones);
            let final_value_in_probe = Arc::clone(&final_value_in_graph);
            counter
                .inspect(move |(value, _time, diff)| {
                    if *diff > 0 {
                        *final_value_in_probe.lock().expect("counter lock poisoned") = *value;
                    }
                })
                .probe_with(&mut probe);
        });

        input.insert("press");
        input.advance_to(BoonTime { epoch: 1, phase: 3 }.encode());
        input.flush();

        let target = BoonTime { epoch: 1, phase: 3 }.encode();
        let mut steps = 0;
        while probe.less_than(&target) {
            worker.step();
            steps += 1;
            assert!(steps <= 1024, "probe did not drain by target timestamp");
        }
    }

    let final_value = *final_value.lock().expect("counter lock poisoned");
    SmokeOutput {
        monitor: vec![MonitorRecord::NodeValue {
            epoch: 1,
            node: NodeId("CounterHold".to_owned()),
            owner: OwnerKey("Root".to_owned()),
            value_preview: final_value.to_string(),
        }],
        render: vec![RenderCommand::PatchText {
            node: NodeId("DocumentText".to_owned()),
            text: final_value.to_string(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boon_time_round_trips() {
        let time = BoonTime {
            epoch: 42,
            phase: 3,
        };
        assert_eq!(BoonTime::decode(time.encode()), time);
    }

    #[test]
    fn counter_hold_smoke_emits_monitor_and_render() {
        let output = run_counter_hold_smoke();
        assert_eq!(output.monitor.len(), 1);
        assert_eq!(output.render.len(), 1);
        assert_eq!(
            output.render[0],
            RenderCommand::PatchText {
                node: NodeId("DocumentText".to_owned()),
                text: "1".to_owned(),
            }
        );
    }

    #[test]
    fn static_graph_executes_from_generic_behavior() {
        let graph = StaticGraph {
            graph_id: "example".to_owned(),
            source_path: "examples/example/source.bn".to_owned(),
            source_hash: 1,
            source_bindings: Vec::new(),
            nodes: Vec::new(),
            operators: Vec::new(),
            monitor_node: NodeId("ThenValue".to_owned()),
            render_node: NodeId("DocumentText".to_owned()),
            initial_text: "0".to_owned(),
            dd_plan: DdScalarPlan::CountInputEvents { initial: 0 },
            physical_scene: false,
        };
        let output = run_static_dd_graph(
            &graph,
            &[SourceAction {
                source: "press".to_owned(),
                owner: None,
                generation: None,
                value: BoonValue::EmptyRecord,
            }],
        );
        assert_eq!(
            output.render[0],
            RenderCommand::PatchText {
                node: NodeId("DocumentText".to_owned()),
                text: "1".to_owned(),
            }
        );
    }
}
