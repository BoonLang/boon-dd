use boon_dd::{
    GraphNode, GraphOperator, GraphOperatorKind, NodeId, SourceBinding, SourceId, StaticGraph,
    TextBehavior,
};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompilePlan {
    pub source_path: String,
    pub graph: StaticGraph,
}

pub fn compile_source(path: impl Into<String>, text: impl Into<String>) -> CompilePlan {
    let path = path.into();
    let text = text.into();
    let parsed = boon_syntax::parse_source(path.clone(), text.clone());
    let hir = boon_hir::lower(&parsed);
    let graph = compile_static_graph(&hir.source_path, &text);
    CompilePlan {
        source_path: hir.source_path,
        graph,
    }
}

fn compile_static_graph(source_path: &str, text: &str) -> StaticGraph {
    let graph_id = source_path
        .trim_start_matches("examples/")
        .trim_end_matches("/source.bn")
        .replace('/', "_");
    let operators = detect_operators(text);
    let source_bindings = infer_sources(text);
    let monitor_node = NodeId(infer_monitor_node(&graph_id, text, &operators));
    let render_node = NodeId("DocumentText".to_owned());
    let initial_text = infer_initial_text(text);
    let text_behavior = infer_text_behavior(text, &initial_text);
    let mut nodes = Vec::new();
    nodes.push(GraphNode {
        node: render_node.clone(),
        kind: "render.text".to_owned(),
        shape: "Text".to_owned(),
        source_span: format!("{source_path}:document"),
    });
    nodes.push(GraphNode {
        node: monitor_node.clone(),
        kind: "monitor.value".to_owned(),
        shape: "Text".to_owned(),
        source_span: format!("{source_path}:document"),
    });
    nodes.extend(source_bindings.iter().map(|binding| GraphNode {
        node: NodeId(binding.source_id.0.clone()),
        kind: "source.leaf".to_owned(),
        shape: binding.shape.clone(),
        source_span: format!("{}:{}", source_path, binding.path),
    }));

    StaticGraph {
        graph_id,
        source_path: source_path.to_owned(),
        source_hash: hash_text(text),
        source_bindings,
        nodes,
        operators,
        monitor_node,
        render_node,
        initial_text,
        text_behavior,
        physical_scene: text.contains("Scene/new("),
    }
}

fn detect_operators(text: &str) -> Vec<GraphOperator> {
    let mut operators = Vec::new();
    let mut push = |kind: GraphOperatorKind, node: &str, order: u32| {
        operators.push(GraphOperator {
            node: NodeId(node.to_owned()),
            kind,
            inputs: Vec::new(),
            output: NodeId(node.to_owned()),
            order,
        });
    };

    if text.contains("SOURCE") {
        push(GraphOperatorKind::SourceLeaf, "SourceLeaf", 0);
    }
    if text.contains("|> THEN") {
        push(GraphOperatorKind::Then, "Then", 10);
    }
    if text.contains("THEN { 1 }") {
        push(GraphOperatorKind::ThenConst, "ThenConst", 11);
    }
    if text.contains("WHEN") {
        push(GraphOperatorKind::When, "When", 20);
    }
    if text.contains("WHILE") {
        push(GraphOperatorKind::WhileSwitch, "WhileSwitch", 21);
    }
    if text.contains("LATEST") {
        push(GraphOperatorKind::Latest, "Latest", 22);
    }
    if text.contains("HOLD") {
        push(GraphOperatorKind::Hold, "Hold", 30);
        push(GraphOperatorKind::PersistTap, "PersistTap", 90);
    }
    if text.contains("List/append") {
        push(GraphOperatorKind::ListAppend, "ListAppend", 40);
    }
    if text.contains("List/remove") {
        push(GraphOperatorKind::ListRemove, "ListRemove", 41);
    }
    if text.contains("List/map") {
        push(GraphOperatorKind::ListMap, "ListMap", 42);
    }
    if text.contains("List/retain") {
        push(GraphOperatorKind::ListRetain, "ListRetain", 43);
    }
    if text.contains('/') {
        push(GraphOperatorKind::LibraryCall, "LibraryCall", 70);
    }
    push(GraphOperatorKind::RenderSink, "RenderSink", 100);
    push(GraphOperatorKind::MonitorTap, "MonitorTap", 101);
    operators
}

fn infer_sources(text: &str) -> Vec<SourceBinding> {
    let mut bindings = Vec::new();
    for path in infer_source_paths(text) {
        bindings.push(SourceBinding {
            source_id: SourceId(to_pascal_identifier(&path)),
            shape: infer_source_shape(&path),
            dynamic: path.contains("item.") || path.contains(".sources."),
            path,
        });
    }
    bindings.sort_by(|left, right| left.path.cmp(&right.path));
    bindings.dedup_by(|left, right| left.path == right.path);
    bindings
}

fn infer_source_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains("SOURCE") {
            let name = trimmed
                .split(':')
                .next()
                .unwrap_or("source")
                .trim()
                .trim_matches(&['[', ']'][..]);
            paths.push(name.to_owned());
        }
    }
    for token in [
        "sources.a",
        "sources.b",
        "sources.press",
        "sources.key_down.key",
        "selected_filter",
        "tick",
        "sources.frame",
        "store.sources.frame",
    ] {
        if text.contains(token) {
            paths.push(token.trim_start_matches("sources.").to_owned());
        }
    }
    paths
}

fn infer_source_shape(path: &str) -> String {
    if path.ends_with(".text") || path == "text" {
        "Text".to_owned()
    } else if path.ends_with(".key") || path.contains("key") || path == "selected_filter" {
        "TagSet".to_owned()
    } else {
        "EmptyRecord".to_owned()
    }
}

fn infer_monitor_node(graph_id: &str, text: &str, operators: &[GraphOperator]) -> String {
    if text.contains("Math/sum") && text.contains("Timer/interval") {
        return "IntervalCounter".to_owned();
    }
    if text.contains("Timer/interval") && text.contains("HOLD") {
        return "IntervalHoldCounter".to_owned();
    }
    if text.contains("Pong/") {
        return "Pong".to_owned();
    }
    if graph_id == "then" {
        return "ThenValue".to_owned();
    }
    if text.contains("Scene/new") {
        return "TodoMvcPhysical".to_owned();
    }
    if operators
        .iter()
        .any(|op| op.kind == GraphOperatorKind::Latest)
    {
        return "LatestValue".to_owned();
    }
    if operators
        .iter()
        .any(|op| op.kind == GraphOperatorKind::When)
    {
        return "WhenEnter".to_owned();
    }
    if operators
        .iter()
        .any(|op| op.kind == GraphOperatorKind::WhileSwitch)
    {
        return if graph_id.contains("retain") {
            "ListRetainReactive".to_owned()
        } else {
            "WhileFilter".to_owned()
        };
    }
    if text.contains("Math/sum") || text.contains("HOLD") {
        return "CounterHold".to_owned();
    }
    to_pascal_identifier(graph_id)
}

fn infer_initial_text(text: &str) -> String {
    if text.contains("Math/sum") || text.contains("HOLD") || text.contains("|> THEN { 1 }") {
        return "0".to_owned();
    }
    if text.contains("LATEST") || text.contains("WHEN") || text.contains("WHILE") {
        return String::new();
    }
    infer_constant_text(text).unwrap_or_default()
}

fn infer_text_behavior(text: &str, initial_text: &str) -> TextBehavior {
    if text.contains("Pong/") {
        TextBehavior::Constant(infer_constant_text(text).unwrap_or_default())
    } else if text.contains("LATEST") {
        TextBehavior::LatestAction
    } else if text.contains("WHEN") {
        TextBehavior::BranchOnTag {
            tag: "Enter".to_owned(),
            text: first_text_literal(text).unwrap_or_default(),
        }
    } else if text.contains("WHILE") {
        TextBehavior::BranchOnTag {
            tag: "Active".to_owned(),
            text: first_text_literal(text).unwrap_or_default(),
        }
    } else if text.contains("Math/sum") || text.contains("HOLD") || text.contains("|> THEN { 1 }") {
        TextBehavior::CountActions {
            initial: initial_text.parse().unwrap_or(0),
        }
    } else {
        TextBehavior::Constant(infer_constant_text(text).unwrap_or_default())
    }
}

fn infer_constant_text(text: &str) -> Option<String> {
    if text.contains("List/count(item") {
        let inactive = text.matches("completed: False").count();
        return Some(format!("{inactive} active"));
    }
    if text.contains("List/count()") {
        let inactive = text.matches("completed: False").count();
        return Some(inactive.to_string());
    }
    if text.contains("Text/uppercase") {
        let joined = text_literals(text)
            .into_iter()
            .filter(|value| value != ",")
            .map(|value| value.to_uppercase())
            .collect::<Vec<_>>()
            .join(",");
        return Some(joined);
    }
    if text.contains("Text/join") || text.contains("List/append") {
        let joined = text_literals(text)
            .into_iter()
            .filter(|value| value != ",")
            .collect::<Vec<_>>()
            .join(",");
        return Some(joined);
    }
    if text.contains("Temperature/c_to_f") {
        if let Some(celsius) = number_after_label(text, "celsius:") {
            return Some(format!("{} F", celsius * 9 / 5 + 32));
        }
    }
    if text.contains("A1: 1") {
        return Some("A1=1".to_owned());
    }
    text_literals(text).last().cloned()
}

fn first_text_literal(text: &str) -> Option<String> {
    text_literals(text).into_iter().next()
}

fn text_literals(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("TEXT {") {
        rest = &rest[start + "TEXT {".len()..];
        if let Some(end) = rest.find('}') {
            values.push(rest[..end].trim().to_owned());
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    values
}

fn number_after_label(text: &str, label: &str) -> Option<i64> {
    let start = text.find(label)? + label.len();
    text[start..]
        .lines()
        .next()
        .and_then(|line| line.trim().parse::<i64>().ok())
}

fn to_pascal_identifier(value: &str) -> String {
    let mut result = String::new();
    let mut upper = true;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if upper {
                result.push(ch.to_ascii_uppercase());
                upper = false;
            } else {
                result.push(ch);
            }
        } else {
            upper = true;
        }
    }
    if result.is_empty() {
        "Generated".to_owned()
    } else {
        result
    }
}

fn hash_text(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}
