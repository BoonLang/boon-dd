use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedModule {
    pub source: SourceFile,
}

pub fn parse_source(path: impl Into<String>, text: impl Into<String>) -> ParsedModule {
    ParsedModule {
        source: SourceFile {
            path: path.into(),
            text: text.into(),
        },
    }
}
