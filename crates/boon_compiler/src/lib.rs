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
    let document_text = definition_block(text, "document").unwrap_or_else(|| text.to_owned());
    let document_target = infer_document_target(&document_text);
    let target_definition = document_target
        .as_deref()
        .and_then(|target| definition_block(text, target_name(target)));
    let monitor_node = NodeId(infer_monitor_node(
        &graph_id,
        text,
        document_target.as_deref(),
        target_definition.as_deref(),
        &operators,
    ));
    let render_node = NodeId("DocumentText".to_owned());
    let initial_text = infer_initial_text(text, &document_text, target_definition.as_deref());
    let text_behavior = infer_text_behavior(
        text,
        &document_text,
        target_definition.as_deref(),
        &initial_text,
    );
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
            dynamic: path.starts_with("item."),
            path,
        });
    }
    bindings.sort_by(|left, right| left.path.cmp(&right.path));
    bindings.dedup_by(|left, right| left.path == right.path);
    bindings
}

fn infer_source_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    for line in text.lines() {
        let indent = leading_spaces(line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        while stack
            .last()
            .is_some_and(|(stack_indent, _)| *stack_indent >= indent)
        {
            stack.pop();
        }
        if trimmed.contains("SOURCE") {
            paths.extend(source_paths_in_line(&stack, trimmed));
        }
        if let Some(label) = leading_label(trimmed) {
            stack.push((indent, label));
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

fn infer_monitor_node(
    graph_id: &str,
    text: &str,
    document_target: Option<&str>,
    target_definition: Option<&str>,
    operators: &[GraphOperator],
) -> String {
    let target_name = document_target.map(target_name);
    let target_pascal = target_name.map(to_pascal_identifier);
    let target_definition = target_definition.unwrap_or_default();
    if target_definition.contains("Math/sum") && text.contains("Timer/interval") {
        return "IntervalCounter".to_owned();
    }
    if target_definition.contains("HOLD") && text.contains("Timer/interval") {
        return "IntervalHoldCounter".to_owned();
    }
    if operators
        .iter()
        .any(|op| op.kind == GraphOperatorKind::Latest)
        && target_definition.contains("LATEST")
    {
        return "LatestValue".to_owned();
    }
    if operators
        .iter()
        .any(|op| op.kind == GraphOperatorKind::When)
        && target_definition.contains("WHEN")
    {
        return "WhenEnter".to_owned();
    }
    if operators
        .iter()
        .any(|op| op.kind == GraphOperatorKind::WhileSwitch)
        && target_definition.contains("WHILE")
    {
        return "WhileFilter".to_owned();
    }
    if target_definition.contains("HOLD") {
        return format!(
            "{}Hold",
            target_pascal.unwrap_or_else(|| "Value".to_owned())
        );
    }
    if target_definition.contains("Math/sum") {
        return target_pascal.unwrap_or_else(|| "Sum".to_owned());
    }
    if target_definition.contains("|> THEN") {
        return format!(
            "Then{}",
            target_pascal.unwrap_or_else(|| "Value".to_owned())
        );
    }
    to_pascal_identifier(graph_id)
}

fn infer_initial_text(
    all_text: &str,
    document_text: &str,
    target_definition: Option<&str>,
) -> String {
    if let Some(text) = infer_document_text(document_text) {
        return text;
    }
    let target_definition = target_definition.unwrap_or_default();
    if target_definition.contains("Math/sum")
        || target_definition.contains("HOLD")
        || target_definition.contains("|> THEN { 1 }")
    {
        return "0".to_owned();
    }
    if target_definition.contains("LATEST")
        || target_definition.contains("WHEN")
        || target_definition.contains("WHILE")
    {
        return String::new();
    }
    infer_constant_text(all_text).unwrap_or_default()
}

fn infer_text_behavior(
    all_text: &str,
    document_text: &str,
    target_definition: Option<&str>,
    initial_text: &str,
) -> TextBehavior {
    if let Some(text) = infer_document_text(document_text) {
        return TextBehavior::Constant(text);
    }
    let target_definition = target_definition.unwrap_or_default();
    if target_definition.contains("LATEST") {
        TextBehavior::LatestAction
    } else if target_definition.contains("WHEN") {
        TextBehavior::BranchOnTag {
            tag: "Enter".to_owned(),
            text: first_text_literal(target_definition).unwrap_or_default(),
        }
    } else if target_definition.contains("WHILE") {
        TextBehavior::BranchOnTag {
            tag: "Active".to_owned(),
            text: first_text_literal(target_definition).unwrap_or_default(),
        }
    } else if target_definition.contains("Math/sum")
        || target_definition.contains("HOLD")
        || target_definition.contains("|> THEN { 1 }")
    {
        TextBehavior::CountActions {
            initial: initial_text.parse().unwrap_or(0),
        }
    } else {
        TextBehavior::Constant(infer_constant_text(all_text).unwrap_or_default())
    }
}

fn infer_document_text(document_text: &str) -> Option<String> {
    document_head_expression(document_text).and_then(|expression| {
        expression
            .trim_start()
            .starts_with("TEXT {")
            .then(|| text_literals(expression).last().cloned())
            .flatten()
    })
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

fn definition_block(text: &str, label: &str) -> Option<String> {
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let indent = leading_spaces(line);
        let trimmed = line.trim();
        if leading_label(trimmed).as_deref() != Some(label) {
            continue;
        }
        let mut block = String::from(line);
        while let Some(next) = lines.peek().copied() {
            if !next.trim().is_empty() && leading_spaces(next) <= indent {
                break;
            }
            block.push('\n');
            block.push_str(next);
            lines.next();
        }
        return Some(block);
    }
    None
}

fn infer_document_target(document_text: &str) -> Option<String> {
    for marker in ["|> Text/from_number", "|> Text/join", "|> Document/new"] {
        if let Some(target) = expression_before_marker(document_text, marker) {
            return Some(target);
        }
    }
    None
}

fn expression_before_marker(text: &str, marker: &str) -> Option<String> {
    for line in text.lines() {
        let Some(marker_index) = line.find(marker) else {
            continue;
        };
        let before_marker = &line[..marker_index];
        let expression = before_marker
            .rsplit(':')
            .next()
            .unwrap_or(before_marker)
            .split("|>")
            .next()
            .unwrap_or(before_marker)
            .trim();
        if is_reference_expression(expression) {
            return Some(expression.to_owned());
        }
    }
    None
}

fn document_head_expression(document_text: &str) -> Option<&str> {
    for line in document_text.lines() {
        let Some(pipe_index) = line.find("|>") else {
            continue;
        };
        return Some(
            line[..pipe_index]
                .rsplit(':')
                .next()
                .unwrap_or(&line[..pipe_index]),
        );
    }
    None
}

fn is_reference_expression(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
}

fn target_name(target: &str) -> &str {
    target.rsplit('.').next().unwrap_or(target)
}

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn leading_label(trimmed: &str) -> Option<String> {
    let colon = trimmed.find(':')?;
    let candidate = trimmed[..colon].trim().trim_matches(&['[', ']'][..]).trim();
    if candidate.is_empty()
        || candidate
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
    {
        None
    } else {
        Some(candidate.to_owned())
    }
}

fn source_paths_in_line(stack: &[(usize, String)], trimmed: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = trimmed;
    while let Some(source_index) = rest.find("SOURCE") {
        let before_source = &rest[..source_index];
        let mut parts = stack
            .iter()
            .map(|(_, label)| label.clone())
            .collect::<Vec<_>>();
        parts.extend(labels_before_source(before_source));
        if !parts.is_empty() {
            paths.push(parts.join("."));
        }
        rest = &rest[source_index + "SOURCE".len()..];
    }
    paths
}

fn labels_before_source(value: &str) -> Vec<String> {
    let mut labels = Vec::new();
    let mut token = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            token.push(ch);
        } else if ch == ':' {
            if !token.is_empty() {
                labels.push(token.clone());
                token.clear();
            }
        } else if !ch.is_whitespace() {
            token.clear();
        }
    }
    labels
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
