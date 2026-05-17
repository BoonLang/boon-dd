use differential_dataflow::collection::VecCollection;
use differential_dataflow::input::InputSession;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DdValue {
    Empty,
    Text(String),
    Number(i64),
    Tag(String),
    List(Vec<DdValue>),
    Record(Vec<(String, DdValue)>),
}

impl DdValue {
    pub fn text(self) -> String {
        match self {
            DdValue::Text(text) => text,
            DdValue::Number(number) => number.to_string(),
            DdValue::Tag(tag) => tag,
            DdValue::List(values) => values
                .into_iter()
                .map(DdValue::text)
                .collect::<Vec<_>>()
                .join(","),
            DdValue::Empty => panic!("DD render tried to coerce Empty to Text"),
            DdValue::Record(_) => panic!("DD render tried to coerce Record to Text"),
        }
    }

    pub fn render_text(self) -> String {
        match self {
            DdValue::Text(text) => text,
            DdValue::Number(number) => number.to_string(),
            DdValue::Tag(tag) => tag,
            DdValue::List(values) => values
                .into_iter()
                .map(DdValue::render_text)
                .collect::<Vec<_>>()
                .join(""),
            DdValue::Empty => String::new(),
            DdValue::Record(_) => String::new(),
        }
    }

    pub fn number(self) -> i64 {
        match self {
            DdValue::Number(number) => number,
            DdValue::Text(text) => text.parse::<i64>().unwrap_or_else(|error| {
                panic!("DD render failed to parse Text as Number: {error}")
            }),
            DdValue::Empty => panic!("DD render tried to coerce Empty to Number"),
            DdValue::Tag(_) => panic!("DD render tried to coerce Tag to Number"),
            DdValue::List(_) => panic!("DD render tried to coerce List to Number"),
            DdValue::Record(_) => panic!("DD render tried to coerce Record to Number"),
        }
    }

    pub fn field(self, name: &str) -> DdValue {
        match self {
            DdValue::Record(fields) => fields
                .into_iter()
                .find(|(field, _)| field == name)
                .map(|(_, value)| value)
                .unwrap_or_else(|| panic!("DD render missing record field {name}")),
            other => {
                panic!("DD render tried to read field {name} from non-record value: {other:?}")
            }
        }
    }

    pub fn truthy(self) -> bool {
        match self {
            DdValue::Tag(tag) => tag == "True",
            DdValue::Text(text) => !text.is_empty(),
            DdValue::Number(number) => number != 0,
            DdValue::List(values) => !values.is_empty(),
            DdValue::Record(fields) => !fields.is_empty(),
            DdValue::Empty => panic!("DD render tried to coerce Empty to Bool"),
        }
    }
}

pub fn dd_pattern_matches(value: &DdValue, pattern: &str) -> bool {
    match value {
        DdValue::Tag(tag) => tag == pattern,
        DdValue::Text(text) => text == pattern,
        DdValue::Number(number) => number.to_string() == pattern,
        DdValue::Empty => pattern.is_empty(),
        DdValue::List(_) | DdValue::Record(_) => false,
    }
}

pub fn dd_value_from_source_payload(payload: &GeneratedSourceEventPayload) -> DdValue {
    match payload {
        GeneratedSourceEventPayload::EmptyRecord => DdValue::Empty,
        GeneratedSourceEventPayload::Text(text) => DdValue::Text(text.clone()),
        GeneratedSourceEventPayload::Number(BoonNumber::Int(number)) => DdValue::Number(*number),
        GeneratedSourceEventPayload::Number(BoonNumber::Float(number)) => {
            DdValue::Text(number.to_string())
        }
        GeneratedSourceEventPayload::Tag { name, payload } => DdValue::Tag(match payload {
            Some(payload) => format!("{}({})", name.0, value_to_text(payload)),
            None => name.0.clone(),
        }),
        GeneratedSourceEventPayload::Record(record) => DdValue::Record(
            record
                .iter()
                .map(|(name, value)| (name.clone(), dd_value_from_boon_value(value)))
                .collect(),
        ),
        GeneratedSourceEventPayload::List(values) => {
            DdValue::List(values.iter().map(dd_value_from_boon_value).collect())
        }
    }
}

pub fn dd_value_from_boon_value(value: &BoonValue) -> DdValue {
    match value {
        BoonValue::EmptyRecord => DdValue::Empty,
        BoonValue::Record(record) => DdValue::Record(
            record
                .iter()
                .map(|(name, value)| (name.clone(), dd_value_from_boon_value(value)))
                .collect(),
        ),
        BoonValue::List(values) => {
            DdValue::List(values.iter().map(dd_value_from_boon_value).collect())
        }
        BoonValue::Text(text) => DdValue::Text(text.clone()),
        BoonValue::Number(BoonNumber::Int(number)) => DdValue::Number(*number),
        BoonValue::Number(BoonNumber::Float(number)) => DdValue::Text(number.to_string()),
        BoonValue::Tag { name, payload } => DdValue::Tag(match payload {
            Some(payload) => format!("{}({})", name.0, value_to_text(payload)),
            None => name.0.clone(),
        }),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GeneratedSourceEventPayload {
    EmptyRecord,
    Text(String),
    Number(BoonNumber),
    Tag {
        name: TagName,
        payload: Option<BoonValue>,
    },
    Record(BTreeMap<String, BoonValue>),
    List(Vec<BoonValue>),
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GeneratedEventIdentity {
    pub family_id: SourceFamilyId,
    pub owner_key: OwnerKey,
    pub generation: u32,
}

impl Eq for GeneratedSourceEvent {}

impl PartialOrd for GeneratedSourceEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GeneratedSourceEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        format!("{self:?}").cmp(&format!("{other:?}"))
    }
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
pub enum EffectCommand {
    Requested { node: NodeId, name: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PersistenceCommand {
    SaveText { node: NodeId, value: String },
    LoadText { node: NodeId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmokeOutput {
    #[serde(default)]
    pub monitor: Vec<MonitorRecord>,
    #[serde(default)]
    pub render: Vec<RenderCommand>,
    #[serde(default)]
    pub effects: Vec<EffectCommand>,
    #[serde(default)]
    pub persistence: Vec<PersistenceCommand>,
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
    SourceAt,
    Link,
    PathReference,
    Skip,
    ConstantText,
    ConstantNumber,
    Tag,
    Record,
    List,
    Pipe,
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
    BinaryAdd,
    BinarySubtract,
    BinaryEqual,
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
pub struct DdRenderProgram {
    pub source: DdRenderProgramSource,
    pub operation: DdRenderOperation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdRenderGraph {
    pub source: DdRenderProgramSource,
    pub root: NodeId,
    pub nodes: Vec<DdRenderGraphNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdRenderGraphNode {
    pub node: NodeId,
    pub operator: GraphOperatorKind,
    pub inputs: Vec<NodeId>,
    pub operation: DdRenderGraphOperation,
    pub order: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DdRenderGraphOperation {
    Missing,
    Path(String),
    Number(String),
    Source,
    SourceAt {
        target: NodeId,
    },
    Link {
        target: Option<NodeId>,
    },
    Skip,
    Tag(String),
    Text(String),
    Record(Vec<DdRenderGraphField>),
    List(Vec<NodeId>),
    Block(Vec<NodeId>),
    Latest(Vec<NodeId>),
    Call {
        callee: String,
        args: Vec<DdRenderGraphArg>,
    },
    Constructor {
        callee: String,
        fields: Vec<DdRenderGraphField>,
    },
    FieldAccess {
        base: NodeId,
        field: String,
    },
    Pipe {
        input: NodeId,
        stage: NodeId,
    },
    BinaryAdd {
        left: NodeId,
        right: NodeId,
    },
    BinarySubtract {
        left: NodeId,
        right: NodeId,
    },
    BinaryEqual {
        left: NodeId,
        right: NodeId,
    },
    Then {
        body: Vec<NodeId>,
    },
    Hold {
        binder: String,
        body: Vec<NodeId>,
    },
    Match {
        kind: DdRenderMatchKind,
        arms: Vec<DdRenderGraphMatchArm>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdRenderGraphField {
    pub name: String,
    pub value: NodeId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DdRenderGraphArg {
    Positional(NodeId),
    Named { name: String, value: NodeId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdRenderGraphMatchArm {
    pub pattern: String,
    pub value: NodeId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdRenderProgramSource {
    pub semantic_path: Option<String>,
    pub output_node: NodeId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdOutputProtocol {
    pub schema_version: String,
    pub sinks: Vec<DdOutputSink>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DdOutputSink {
    MonitorNodeValue {
        node: NodeId,
        source: DdRenderProgramSource,
    },
    RenderPatchText {
        node: NodeId,
        source: DdRenderProgramSource,
    },
    Effect {
        node: NodeId,
        name: String,
        source: DdRenderProgramSource,
    },
    Persistence {
        node: NodeId,
        source: DdRenderProgramSource,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DdRenderOperation {
    Text { expr: DdRenderExpr },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DdRenderExpr {
    Missing,
    Path(String),
    Number(String),
    Source,
    SourceAt {
        target: Box<DdRenderExpr>,
    },
    Link {
        target: Option<Box<DdRenderExpr>>,
    },
    Skip,
    Tag(String),
    Text(String),
    Record(Vec<DdRenderField>),
    List(Vec<DdRenderExpr>),
    Block(Vec<DdRenderExpr>),
    Latest(Vec<DdRenderExpr>),
    Call {
        callee: String,
        args: Vec<DdRenderArg>,
    },
    Constructor {
        callee: String,
        fields: Vec<DdRenderField>,
    },
    FieldAccess {
        base: Box<DdRenderExpr>,
        field: String,
    },
    Pipe {
        input: Box<DdRenderExpr>,
        stage: Box<DdRenderExpr>,
    },
    BinaryAdd {
        left: Box<DdRenderExpr>,
        right: Box<DdRenderExpr>,
    },
    BinarySubtract {
        left: Box<DdRenderExpr>,
        right: Box<DdRenderExpr>,
    },
    BinaryEqual {
        left: Box<DdRenderExpr>,
        right: Box<DdRenderExpr>,
    },
    Then {
        body: Vec<DdRenderExpr>,
    },
    Hold {
        binder: String,
        body: Vec<DdRenderExpr>,
    },
    Match {
        kind: DdRenderMatchKind,
        arms: Vec<DdRenderMatchArm>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdRenderField {
    pub name: String,
    pub value: DdRenderExpr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DdRenderArg {
    Positional(DdRenderExpr),
    Named { name: String, value: DdRenderExpr },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DdRenderMatchKind {
    When,
    While,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdRenderMatchArm {
    pub pattern: String,
    pub value: DdRenderExpr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticGraph {
    pub graph_id: String,
    pub source_path: String,
    pub source_hash: String,
    pub source_bindings: Vec<SourceBinding>,
    pub nodes: Vec<GraphNode>,
    pub operators: Vec<GraphOperator>,
    pub monitor_node: NodeId,
    pub render_node: NodeId,
    pub initial_text: String,
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
pub enum ScenarioEvent {
    Source(SourceAction),
    Command(ScenarioCommand),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScenarioStep {
    pub description: String,
    #[serde(default)]
    pub events: Vec<ScenarioEvent>,
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

pub fn source_action_payload(action: &SourceAction) -> GeneratedSourceEventPayload {
    match &action.value {
        BoonValue::EmptyRecord => GeneratedSourceEventPayload::EmptyRecord,
        BoonValue::Text(text) => GeneratedSourceEventPayload::Text(text.clone()),
        BoonValue::Number(number) => GeneratedSourceEventPayload::Number(number.clone()),
        BoonValue::Tag { name, payload } => GeneratedSourceEventPayload::Tag {
            name: name.clone(),
            payload: payload.as_deref().cloned(),
        },
        BoonValue::Record(record) => GeneratedSourceEventPayload::Record(record.clone()),
        BoonValue::List(values) => GeneratedSourceEventPayload::List(values.clone()),
    }
}

pub fn generated_source_event_text(event: &GeneratedSourceEvent) -> String {
    match event {
        GeneratedSourceEvent::Static { payload, .. }
        | GeneratedSourceEvent::Dynamic { payload, .. } => generated_payload_text(payload),
    }
}

pub fn generated_payload_text(payload: &GeneratedSourceEventPayload) -> String {
    match payload {
        GeneratedSourceEventPayload::EmptyRecord => String::new(),
        GeneratedSourceEventPayload::Text(text) => text.clone(),
        GeneratedSourceEventPayload::Number(number) => {
            value_to_text(&BoonValue::Number(number.clone()))
        }
        GeneratedSourceEventPayload::Tag { name, payload } => value_to_text(&BoonValue::Tag {
            name: name.clone(),
            payload: payload.clone().map(Box::new),
        }),
        GeneratedSourceEventPayload::Record(record) => {
            value_to_text(&BoonValue::Record(record.clone()))
        }
        GeneratedSourceEventPayload::List(values) => {
            value_to_text(&BoonValue::List(values.clone()))
        }
    }
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
        effects: Vec::new(),
        persistence: Vec::new(),
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
}
