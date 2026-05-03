use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HirModule {
    pub source_path: String,
}

pub fn lower(parsed: &boon_syntax::ParsedModule) -> HirModule {
    HirModule {
        source_path: parsed.source.path.clone(),
    }
}
