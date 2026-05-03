use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderNode {
    pub id: String,
    pub text: String,
}
