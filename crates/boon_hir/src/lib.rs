use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const SOURCE_NAMESPACE: &str = "sources";

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
    pub parameters: Vec<String>,
    pub is_function: bool,
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
        definitions.push(HirDefinition {
            name: definition.name.clone(),
            parameters: definition.parameters.clone(),
            is_function: definition.is_function,
            expression: definition.expression.clone(),
            span: definition.span.clone(),
        });
    }
    for definition in &definitions {
        let mut path_stack = vec![definition.name.clone()];
        collect_sources(&definition.expression, &mut path_stack, &mut sources);
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
        boon_syntax::Expr::SourceAt { target } => {
            collect_sources(target, path_stack, sources);
            collect_path_source_reference(target, sources);
        }
        boon_syntax::Expr::Link { target } => {
            if let Some(target) = target {
                collect_sources(target, path_stack, sources);
                collect_path_source_reference(target, sources);
            }
        }
        boon_syntax::Expr::Path(path) => {
            if let Some(source_path) = source_namespace_reference(path) {
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
        boon_syntax::Expr::List(values) | boon_syntax::Expr::Block(values) => {
            for value in values {
                collect_sources(value, path_stack, sources);
            }
        }
        boon_syntax::Expr::Latest(values) => {
            for value in values {
                collect_sources(value, path_stack, sources);
                collect_path_source_reference(value, sources);
            }
        }
        boon_syntax::Expr::Then { body: values } => {
            for value in values {
                collect_sources(value, path_stack, sources);
            }
        }
        boon_syntax::Expr::Call { callee, args } => {
            if source_call(callee) && !path_stack.is_empty() {
                sources.insert(path_stack.join("."), true);
            }
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
        boon_syntax::Expr::FieldAccess { base, .. } => {
            collect_sources(base, path_stack, sources);
        }
        boon_syntax::Expr::Pipe { input, stage } => {
            collect_sources(input, path_stack, sources);
            collect_sources(stage, path_stack, sources);
            if pipe_stage_consumes_source(stage) {
                collect_path_source_reference(input, sources);
            }
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

fn collect_path_source_reference(
    expression: &boon_syntax::Expr,
    sources: &mut BTreeMap<String, bool>,
) {
    if let boon_syntax::Expr::Path(path) = expression {
        sources.entry(source_reference(path)).or_insert(false);
    }
}

fn source_reference(path: &str) -> String {
    source_namespace_reference(path).unwrap_or_else(|| path.to_owned())
}

fn source_namespace_reference(path: &str) -> Option<String> {
    let parts = path.split('.').collect::<Vec<_>>();
    let source_index = parts.iter().position(|part| *part == SOURCE_NAMESPACE)?;
    if source_index == 0 {
        let rest = parts.get(1..).unwrap_or_default().join(".");
        if rest.is_empty() { None } else { Some(rest) }
    } else {
        Some(path.to_owned())
    }
}

fn source_call(callee: &str) -> bool {
    matches!(callee, "Timer/interval" | "Window/animation_frame")
}

fn pipe_stage_consumes_source(stage: &boon_syntax::Expr) -> bool {
    matches!(
        stage,
        boon_syntax::Expr::Then { .. } | boon_syntax::Expr::Match { .. }
    )
}

pub fn unresolved_references(module: &HirModule) -> Vec<String> {
    let definitions = module
        .definitions
        .iter()
        .map(|definition| definition.name.as_str())
        .collect::<BTreeSet<_>>();
    let sources = module
        .sources
        .iter()
        .map(|source| source.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut refs = BTreeSet::new();
    for definition in &module.definitions {
        let mut scopes = definition
            .parameters
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        if definition.is_function {
            scopes.push("PASSED");
        }
        collect_top_level_refs(
            &definition.expression,
            &definitions,
            &sources,
            &mut scopes,
            &mut refs,
        );
    }
    refs.into_iter().map(str::to_owned).collect()
}

fn collect_top_level_refs<'a>(
    expression: &'a boon_syntax::Expr,
    definitions: &BTreeSet<&str>,
    sources: &BTreeSet<&str>,
    scopes: &mut Vec<&'a str>,
    refs: &mut BTreeSet<&'a str>,
) {
    match expression {
        boon_syntax::Expr::Path(path) => {
            let root = path.split('.').next().unwrap_or(path.as_str());
            let resolved_source = sources.contains(path.as_str())
                || source_namespace_reference(path)
                    .as_deref()
                    .is_some_and(|path| sources.contains(path));
            if !definitions.contains(root)
                && !scopes.contains(&root)
                && root != SOURCE_NAMESPACE
                && !resolved_source
                && !root.contains('/')
            {
                refs.insert(root);
            }
        }
        boon_syntax::Expr::Record(fields) => {
            for field in fields {
                collect_top_level_refs(&field.value, definitions, sources, scopes, refs);
            }
        }
        boon_syntax::Expr::List(values)
        | boon_syntax::Expr::Block(values)
        | boon_syntax::Expr::Latest(values)
        | boon_syntax::Expr::Then { body: values } => {
            for value in values {
                collect_top_level_refs(value, definitions, sources, scopes, refs);
            }
        }
        boon_syntax::Expr::Call { args, .. } => {
            if let Some((binder, rest)) = call_binder(args) {
                scopes.push(binder);
                for arg in rest {
                    match arg {
                        boon_syntax::CallArg::Positional(value)
                        | boon_syntax::CallArg::Named { value, .. } => {
                            collect_top_level_refs(value, definitions, sources, scopes, refs)
                        }
                    }
                }
                scopes.pop();
            } else {
                for arg in args {
                    match arg {
                        boon_syntax::CallArg::Positional(value)
                        | boon_syntax::CallArg::Named { value, .. } => {
                            collect_top_level_refs(value, definitions, sources, scopes, refs)
                        }
                    }
                }
            }
        }
        boon_syntax::Expr::Constructor { fields, .. } => {
            for field in fields {
                collect_top_level_refs(&field.value, definitions, sources, scopes, refs);
            }
        }
        boon_syntax::Expr::FieldAccess { base, .. } => {
            collect_top_level_refs(base, definitions, sources, scopes, refs);
        }
        boon_syntax::Expr::Pipe { input, stage } => {
            collect_top_level_refs(input, definitions, sources, scopes, refs);
            collect_top_level_refs(stage, definitions, sources, scopes, refs);
        }
        boon_syntax::Expr::Binary { left, right, .. } => {
            collect_top_level_refs(left, definitions, sources, scopes, refs);
            collect_top_level_refs(right, definitions, sources, scopes, refs);
        }
        boon_syntax::Expr::Hold { binder, body } => {
            scopes.push(binder);
            for value in body {
                collect_top_level_refs(value, definitions, sources, scopes, refs);
            }
            scopes.pop();
        }
        boon_syntax::Expr::Match { arms, .. } => {
            for arm in arms {
                collect_top_level_refs(&arm.value, definitions, sources, scopes, refs);
            }
        }
        boon_syntax::Expr::SourceAt { target } => {
            collect_top_level_refs(target, definitions, sources, scopes, refs);
        }
        boon_syntax::Expr::Link { target } => {
            if let Some(target) = target {
                collect_top_level_refs(target, definitions, sources, scopes, refs);
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

fn call_binder(args: &[boon_syntax::CallArg]) -> Option<(&str, &[boon_syntax::CallArg])> {
    let (first, rest) = args.split_first()?;
    match first {
        boon_syntax::CallArg::Positional(boon_syntax::Expr::Path(path)) if !rest.is_empty() => {
            Some((path.as_str(), rest))
        }
        _ => None,
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

    #[test]
    fn function_parameters_and_passed_are_resolver_scopes() {
        let parsed = boon_syntax::parse_source(
            "function_scope.bn",
            "FUNCTION row(label) { Element/label(label: label, element: PASSED.target) }\ndocument: row(label: TEXT { OK }, PASS: [target: store.button])\nstore: [button: LINK]\n",
        );
        let module = lower(&parsed);
        assert!(module.diagnostics.is_empty(), "{:#?}", module.diagnostics);
        assert_eq!(unresolved_references(&module), Vec::<String>::new());
    }
}
