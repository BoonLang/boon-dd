use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HirModule {
    pub source_path: String,
    pub definitions: Vec<HirDefinition>,
    pub definition_names: BTreeMap<String, usize>,
    pub sources: Vec<HirSource>,
    pub diagnostics: Vec<HirDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HirDefinition {
    pub name: String,
    pub expression: boon_syntax::Expr,
    pub span: boon_syntax::SourceSpan,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HirSource {
    pub path: String,
    pub declared: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HirDiagnostic {
    pub message: String,
    pub span: boon_syntax::SourceSpan,
}

pub fn lower(parsed: &boon_syntax::ParsedModule) -> HirModule {
    let mut diagnostics = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| HirDiagnostic {
            message: diagnostic.message.clone(),
            span: diagnostic.span.clone(),
        })
        .collect::<Vec<_>>();
    let mut definition_names = BTreeMap::new();
    let mut definitions = Vec::new();
    let mut sources = BTreeMap::<String, bool>::new();

    for definition in &parsed.definitions {
        if let Some(previous) = definition_names.insert(definition.name.clone(), definitions.len())
        {
            diagnostics.push(HirDiagnostic {
                message: format!(
                    "duplicate definition `{}`; first declared at index {previous}",
                    definition.name
                ),
                span: definition.span.clone(),
            });
        }
        let mut path_stack = vec![definition.name.clone()];
        collect_sources(&definition.expression, &mut path_stack, &mut sources);
        definitions.push(HirDefinition {
            name: definition.name.clone(),
            expression: definition.expression.clone(),
            span: definition.span.clone(),
        });
    }

    HirModule {
        source_path: parsed.source.path.clone(),
        definitions,
        definition_names,
        sources: sources
            .into_iter()
            .map(|(path, declared)| HirSource { path, declared })
            .collect(),
        diagnostics,
    }
}

fn collect_sources(
    expression: &boon_syntax::Expr,
    path_stack: &mut Vec<String>,
    sources: &mut BTreeMap<String, bool>,
) {
    match expression {
        boon_syntax::Expr::Source => {
            if !path_stack.is_empty() {
                sources.insert(path_stack.join("."), true);
            }
        }
        boon_syntax::Expr::Path(path) => {
            if let Some(source_path) = implicit_source_path(path) {
                sources.entry(source_path).or_insert(false);
            }
        }
        boon_syntax::Expr::Record(fields) => {
            for field in fields {
                path_stack.push(field.name.clone());
                collect_sources(&field.value, path_stack, sources);
                path_stack.pop();
            }
        }
        boon_syntax::Expr::Constructor { fields, .. } => {
            for field in fields {
                path_stack.push(field.name.clone());
                collect_sources(&field.value, path_stack, sources);
                path_stack.pop();
            }
        }
        boon_syntax::Expr::List(values)
        | boon_syntax::Expr::Block(values)
        | boon_syntax::Expr::Latest(values)
        | boon_syntax::Expr::Then { body: values } => {
            for value in values {
                collect_sources(value, path_stack, sources);
            }
        }
        boon_syntax::Expr::Call { args, .. } => {
            for arg in args {
                match arg {
                    boon_syntax::CallArg::Positional(value) => {
                        collect_sources(value, path_stack, sources);
                    }
                    boon_syntax::CallArg::Named { name, value } => {
                        path_stack.push(name.clone());
                        collect_sources(value, path_stack, sources);
                        path_stack.pop();
                    }
                }
            }
        }
        boon_syntax::Expr::Pipe { input, stage } => {
            collect_sources(input, path_stack, sources);
            collect_sources(stage, path_stack, sources);
        }
        boon_syntax::Expr::Binary { left, right, .. } => {
            collect_sources(left, path_stack, sources);
            collect_sources(right, path_stack, sources);
        }
        boon_syntax::Expr::Hold { body, .. } => {
            for value in body {
                collect_sources(value, path_stack, sources);
            }
        }
        boon_syntax::Expr::Match { arms, .. } => {
            for arm in arms {
                collect_sources(&arm.value, path_stack, sources);
            }
        }
        boon_syntax::Expr::Missing
        | boon_syntax::Expr::Number(_)
        | boon_syntax::Expr::Skip
        | boon_syntax::Expr::Tag(_)
        | boon_syntax::Expr::Text(_) => {}
    }
}

fn implicit_source_path(path: &str) -> Option<String> {
    if let Some(rest) = path.strip_prefix("sources.") {
        return Some(rest.to_owned());
    }
    if matches!(path, "selected_filter" | "tick") {
        return Some(path.to_owned());
    }
    None
}

pub fn unresolved_references(module: &HirModule) -> Vec<String> {
    let definitions = module
        .definitions
        .iter()
        .map(|definition| definition.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut refs = BTreeSet::new();
    for definition in &module.definitions {
        collect_top_level_refs(&definition.expression, &definitions, &mut refs);
    }
    refs.into_iter().map(str::to_owned).collect()
}

fn collect_top_level_refs<'a>(
    expression: &'a boon_syntax::Expr,
    definitions: &BTreeSet<&str>,
    refs: &mut BTreeSet<&'a str>,
) {
    match expression {
        boon_syntax::Expr::Path(path) => {
            let root = path.split('.').next().unwrap_or(path.as_str());
            if !definitions.contains(root)
                && !matches!(
                    root,
                    "SOURCE"
                        | "SKIP"
                        | "True"
                        | "False"
                        | "sources"
                        | "item"
                        | "state"
                        | "game"
                        | "selected_filter"
                        | "store"
                )
                && !root.contains('/')
            {
                refs.insert(root);
            }
        }
        boon_syntax::Expr::Record(fields) => {
            for field in fields {
                collect_top_level_refs(&field.value, definitions, refs);
            }
        }
        boon_syntax::Expr::List(values)
        | boon_syntax::Expr::Block(values)
        | boon_syntax::Expr::Latest(values)
        | boon_syntax::Expr::Then { body: values } => {
            for value in values {
                collect_top_level_refs(value, definitions, refs);
            }
        }
        boon_syntax::Expr::Call { args, .. } => {
            for arg in args {
                match arg {
                    boon_syntax::CallArg::Positional(value)
                    | boon_syntax::CallArg::Named { value, .. } => {
                        collect_top_level_refs(value, definitions, refs)
                    }
                }
            }
        }
        boon_syntax::Expr::Constructor { fields, .. } => {
            for field in fields {
                collect_top_level_refs(&field.value, definitions, refs);
            }
        }
        boon_syntax::Expr::Pipe { input, stage } => {
            collect_top_level_refs(input, definitions, refs);
            collect_top_level_refs(stage, definitions, refs);
        }
        boon_syntax::Expr::Binary { left, right, .. } => {
            collect_top_level_refs(left, definitions, refs);
            collect_top_level_refs(right, definitions, refs);
        }
        boon_syntax::Expr::Hold { body, .. } => {
            for value in body {
                collect_top_level_refs(value, definitions, refs);
            }
        }
        boon_syntax::Expr::Match { arms, .. } => {
            for arm in arms {
                collect_top_level_refs(&arm.value, definitions, refs);
            }
        }
        boon_syntax::Expr::Missing
        | boon_syntax::Expr::Number(_)
        | boon_syntax::Expr::Source
        | boon_syntax::Expr::Skip
        | boon_syntax::Expr::Tag(_)
        | boon_syntax::Expr::Text(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_counter_sources_from_ast() {
        let parsed = boon_syntax::parse_source(
            "examples/counter/source.bn",
            include_str!("../../../examples/counter/source.bn"),
        );
        let module = lower(&parsed);
        assert!(module.diagnostics.is_empty());
        assert_eq!(module.definitions.len(), 2);
        assert!(module.sources.iter().any(|source| {
            source.path == "store.sources.increment_button.event.press" && source.declared
        }));
    }

    #[test]
    fn keeps_implicit_sources_as_references_not_text_scans() {
        let parsed = boon_syntax::parse_source(
            "examples/when/source.bn",
            include_str!("../../../examples/when/source.bn"),
        );
        let module = lower(&parsed);
        assert!(
            module
                .sources
                .iter()
                .any(|source| { source.path == "key_down.key" && !source.declared })
        );
    }
}
