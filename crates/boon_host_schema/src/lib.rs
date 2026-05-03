use boon_shape::Shape;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceBindingSchema {
    pub path: String,
    pub shape: Shape,
}

pub fn button_press_binding() -> SourceBindingSchema {
    SourceBindingSchema {
        path: "element.event.press".to_owned(),
        shape: Shape::EmptyRecord,
    }
}
