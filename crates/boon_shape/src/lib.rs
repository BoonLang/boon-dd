use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    Unknown,
    EmptyRecord,
    Record(BTreeMap<String, Shape>),
    Text,
    Number,
    TagSet(Vec<String>),
    SourceMarker,
    Skip,
}
