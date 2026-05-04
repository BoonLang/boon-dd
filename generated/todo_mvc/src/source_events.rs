use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GeneratedSourceEvent {
    Event,
    Text { text: String },
}
