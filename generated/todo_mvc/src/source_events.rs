use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum GeneratedSourceEvent {
    StoreSourcesNewTodoInputEventKeyDownKey { tag: String },
    StoreSourcesNewTodoInputText { text: String },
}
