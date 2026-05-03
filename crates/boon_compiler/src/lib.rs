use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompilePlan {
    pub source_path: String,
}

pub fn compile_source(path: impl Into<String>, text: impl Into<String>) -> CompilePlan {
    let parsed = boon_syntax::parse_source(path, text);
    let hir = boon_hir::lower(&parsed);
    CompilePlan {
        source_path: hir.source_path,
    }
}
