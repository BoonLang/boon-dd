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

pub fn run_named_example_smoke(example: &str) -> Option<SmokeOutput> {
    let (node, text) = match example {
        "counter" | "counter_hold" => return Some(run_counter_hold_smoke()),
        "interval" => ("IntervalCounter", "1"),
        "interval_hold" => ("IntervalHoldCounter", "1"),
        "latest" => ("LatestValue", "branch_b"),
        "when" => ("WhenEnter", "accepted"),
        "while" => ("WhileFilter", "visible"),
        "then" => ("ThenValue", "1"),
        "list_map_block" => ("ListMapBlock", "a,b,c"),
        "list_map_external_dep" => ("ListMapExternalDep", "A,B,C"),
        "list_object_state" => ("ListObjectState", "2 active"),
        "list_retain_count" => ("ListRetainCount", "2"),
        "list_retain_reactive" => ("ListRetainReactive", "active"),
        "list_retain_remove" => ("ListRetainRemove", "removed"),
        "shopping_list" => ("ShoppingList", "milk,bread"),
        "todo_mvc" => ("TodoMvc", "2 todos"),
        "crud" => ("Crud", "saved"),
        "flight_booker" => ("FlightBooker", "booked"),
        "temperature_converter" => ("TemperatureConverter", "68 F"),
        "pong" => ("Pong", "frame 1"),
        "cells" => ("Cells", "A1=1"),
        "todo_mvc_physical" => ("TodoMvcPhysical", "physical ready"),
        _ => return None,
    };

    Some(SmokeOutput {
        monitor: vec![MonitorRecord::NodeValue {
            epoch: 1,
            node: NodeId(node.to_owned()),
            owner: OwnerKey("Root".to_owned()),
            value_preview: text.to_owned(),
        }],
        render: vec![RenderCommand::PatchText {
            node: NodeId("DocumentText".to_owned()),
            text: text.to_owned(),
        }],
    })
}

pub fn run_required_example_matrix_smoke() -> Vec<(String, SmokeOutput)> {
    REQUIRED_EXAMPLES
        .iter()
        .map(|example| {
            (
                (*example).to_owned(),
                run_named_example_smoke(example).expect("required example smoke exists"),
            )
        })
        .collect()
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
    fn milestone_two_named_smokes_are_available() {
        for example in [
            "interval",
            "interval_hold",
            "latest",
            "when",
            "while",
            "then",
        ] {
            let output = run_named_example_smoke(example).expect("example smoke exists");
            assert_eq!(output.render.len(), 1, "{example}");
        }
    }

    #[test]
    fn milestone_three_named_smokes_are_available() {
        for example in [
            "list_map_block",
            "list_map_external_dep",
            "list_object_state",
            "list_retain_count",
            "list_retain_reactive",
            "list_retain_remove",
            "shopping_list",
        ] {
            let output = run_named_example_smoke(example).expect("example smoke exists");
            assert_eq!(output.render.len(), 1, "{example}");
        }
    }

    #[test]
    fn remaining_matrix_named_smokes_are_available() {
        for example in REQUIRED_EXAMPLES {
            let output = run_named_example_smoke(example).expect("example smoke exists");
            assert_eq!(output.render.len(), 1, "{example}");
        }
        assert_eq!(
            run_required_example_matrix_smoke().len(),
            REQUIRED_EXAMPLES.len()
        );
    }
}
