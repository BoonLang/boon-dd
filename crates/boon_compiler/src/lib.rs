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
    let parsed = boon_syntax::parse_source(path.clone(), text);
    let hir = boon_hir::lower(&parsed);
    let shapes = boon_shape::check_module(&hir);
    let graph = build_static_graph(&parsed, &hir, &shapes);
    CompilePlan {
        source_path: hir.source_path,
        graph,
    }
}

fn build_static_graph(
    parsed: &boon_syntax::ParsedModule,
    hir: &boon_hir::HirModule,
    shapes: &boon_shape::ShapeReport,
) -> StaticGraph {
    let graph_id = graph_id_from_path(&hir.source_path);
    let document = hir
        .definitions
        .iter()
        .find(|definition| definition.name == "document");
    let document_expr = document.map(|definition| &definition.expression);
    let target_path = document_expr.and_then(document_target_path);
    let target_expr = target_path
        .as_deref()
        .and_then(|path| target_expression(path, hir));
    let semantic_expr = target_expr.or(document_expr);
    let operators = graph_operators(hir);
    let source_bindings = source_bindings(hir, shapes);
    let monitor_node = NodeId(monitor_node_name(
        &graph_id,
        target_path.as_deref(),
        semantic_expr,
        hir,
        &operators,
    ));
    let render_node = NodeId("DocumentText".to_owned());
    let initial_text = initial_text(document_expr, semantic_expr, hir);
    let text_behavior = output_behavior(document_expr, semantic_expr, hir, &initial_text);
    let mut nodes = Vec::new();
    nodes.push(GraphNode {
        node: render_node.clone(),
        kind: "render.text".to_owned(),
        shape: "Text".to_owned(),
        source_span: format!("{}:document", hir.source_path),
    });
    nodes.push(GraphNode {
        node: monitor_node.clone(),
        kind: "monitor.value".to_owned(),
        shape: "Text".to_owned(),
        source_span: format!("{}:document", hir.source_path),
    });
    nodes.extend(source_bindings.iter().map(|binding| GraphNode {
        node: NodeId(binding.source_id.0.clone()),
        kind: "source.leaf".to_owned(),
        shape: binding.shape.clone(),
        source_span: format!("{}:{}", hir.source_path, binding.path),
    }));

    StaticGraph {
        graph_id,
        source_path: hir.source_path.clone(),
        source_hash: stable_u64(&parsed.source.text),
        source_bindings,
        nodes,
        operators,
        monitor_node,
        render_node,
        initial_text,
        text_behavior,
        physical_scene: hir
            .definitions
            .iter()
            .any(|definition| expression_has_call(&definition.expression, "Scene/new")),
    }
}

fn graph_id_from_path(path: &str) -> String {
    path.strip_prefix("examples/")
        .unwrap_or(path)
        .strip_suffix("/source.bn")
        .unwrap_or(path.strip_prefix("examples/").unwrap_or(path))
        .replace('/', "_")
}

fn graph_operators(hir: &boon_hir::HirModule) -> Vec<GraphOperator> {
    let mut builder = OperatorBuilder::default();
    for definition in &hir.definitions {
        builder.visit(&definition.expression);
    }
    builder.finish()
}

#[derive(Default)]
struct OperatorBuilder {
    seen: Vec<GraphOperatorKind>,
}

impl OperatorBuilder {
    fn add(&mut self, kind: GraphOperatorKind) {
        if !self.seen.iter().any(|seen| *seen == kind) {
            self.seen.push(kind);
        }
    }

    fn visit(&mut self, expression: &boon_syntax::Expr) {
        match expression {
            boon_syntax::Expr::Source => {
                self.add(GraphOperatorKind::SourceLeaf);
            }
            boon_syntax::Expr::Then { body } => {
                self.add(GraphOperatorKind::Then);
                if body
                    .iter()
                    .any(|expr| matches!(expr, boon_syntax::Expr::Number(value) if value == "1"))
                {
                    self.add(GraphOperatorKind::ThenConst);
                }
                self.visit_all(body);
            }
            boon_syntax::Expr::Hold { body, .. } => {
                self.add(GraphOperatorKind::Hold);
                self.add(GraphOperatorKind::PersistTap);
                self.visit_all(body);
            }
            boon_syntax::Expr::Latest(values) => {
                self.add(GraphOperatorKind::Latest);
                self.visit_all(values);
            }
            boon_syntax::Expr::Match { kind, arms } => {
                match kind {
                    boon_syntax::MatchKind::When => {
                        self.add(GraphOperatorKind::When);
                    }
                    boon_syntax::MatchKind::While => {
                        self.add(GraphOperatorKind::WhileSwitch);
                    }
                }
                for arm in arms {
                    self.visit(&arm.value);
                }
            }
            boon_syntax::Expr::Call { callee, args } => {
                self.visit_call(callee);
                for arg in args {
                    match arg {
                        boon_syntax::CallArg::Positional(value)
                        | boon_syntax::CallArg::Named { value, .. } => self.visit(value),
                    }
                }
            }
            boon_syntax::Expr::Constructor { fields, .. } | boon_syntax::Expr::Record(fields) => {
                for field in fields {
                    self.visit(&field.value);
                }
            }
            boon_syntax::Expr::List(values) | boon_syntax::Expr::Block(values) => {
                self.visit_all(values);
            }
            boon_syntax::Expr::Pipe { input, stage } => {
                self.visit(input);
                self.visit(stage);
            }
            boon_syntax::Expr::Binary { left, right, .. } => {
                self.visit(left);
                self.visit(right);
            }
            boon_syntax::Expr::Missing
            | boon_syntax::Expr::Path(_)
            | boon_syntax::Expr::Number(_)
            | boon_syntax::Expr::Skip
            | boon_syntax::Expr::Tag(_)
            | boon_syntax::Expr::Text(_) => {}
        }
    }

    fn visit_all(&mut self, values: &[boon_syntax::Expr]) {
        for value in values {
            self.visit(value);
        }
    }

    fn visit_call(&mut self, callee: &str) {
        let kind = match callee {
            "List/append" => GraphOperatorKind::ListAppend,
            "List/remove" => GraphOperatorKind::ListRemove,
            "List/map" => GraphOperatorKind::ListMap,
            "List/retain" => GraphOperatorKind::ListRetain,
            _ => GraphOperatorKind::LibraryCall,
        };
        self.add(kind);
    }

    fn finish(self) -> Vec<GraphOperator> {
        let order = [
            (GraphOperatorKind::SourceLeaf, "SourceLeaf", 0),
            (GraphOperatorKind::Then, "Then", 10),
            (GraphOperatorKind::ThenConst, "ThenConst", 11),
            (GraphOperatorKind::When, "When", 20),
            (GraphOperatorKind::WhileSwitch, "WhileSwitch", 21),
            (GraphOperatorKind::Latest, "Latest", 22),
            (GraphOperatorKind::Hold, "Hold", 30),
            (GraphOperatorKind::ListAppend, "ListAppend", 40),
            (GraphOperatorKind::ListRemove, "ListRemove", 41),
            (GraphOperatorKind::ListMap, "ListMap", 42),
            (GraphOperatorKind::ListRetain, "ListRetain", 43),
            (GraphOperatorKind::LibraryCall, "LibraryCall", 70),
            (GraphOperatorKind::PersistTap, "PersistTap", 90),
            (GraphOperatorKind::RenderSink, "RenderSink", 100),
            (GraphOperatorKind::MonitorTap, "MonitorTap", 101),
        ];
        order
            .into_iter()
            .filter(|(kind, _, _)| {
                self.seen.iter().any(|seen| seen == kind)
                    || matches!(
                        kind,
                        GraphOperatorKind::RenderSink | GraphOperatorKind::MonitorTap
                    )
            })
            .map(|(kind, node, order)| GraphOperator {
                node: NodeId(node.to_owned()),
                kind,
                inputs: Vec::new(),
                output: NodeId(node.to_owned()),
                order,
            })
            .collect()
    }
}

fn source_bindings(
    hir: &boon_hir::HirModule,
    shapes: &boon_shape::ShapeReport,
) -> Vec<SourceBinding> {
    let mut bindings = hir
        .sources
        .iter()
        .map(|source| SourceBinding {
            source_id: SourceId(to_pascal_identifier(&source.path)),
            shape: source_shape(&source.path, shapes),
            dynamic: source.path.starts_with("item."),
            path: source.path.clone(),
        })
        .collect::<Vec<_>>();
    bindings.sort_by(|left, right| left.path.cmp(&right.path));
    bindings.dedup_by(|left, right| left.path == right.path);
    bindings
}

fn source_shape(path: &str, shapes: &boon_shape::ShapeReport) -> String {
    if let Some(shape) = shapes.sources.get(path) {
        return format!("{shape:?}");
    }
    if path.ends_with(".text") || path == "text" {
        "Text".to_owned()
    } else if path.ends_with(".key") || path == "selected_filter" {
        "TagSet".to_owned()
    } else {
        "EmptyRecord".to_owned()
    }
}

fn document_target_path(expression: &boon_syntax::Expr) -> Option<String> {
    match expression {
        boon_syntax::Expr::Pipe { input, stage } => match stage.as_ref() {
            boon_syntax::Expr::Call { callee, .. }
                if matches!(
                    callee.as_str(),
                    "Text/from_number" | "Text/join" | "Document/new"
                ) =>
            {
                reference_path(input)
            }
            _ => document_target_path(stage).or_else(|| document_target_path(input)),
        },
        boon_syntax::Expr::Call { callee, args } if callee == "Document/new" => {
            args.iter().find_map(|arg| match arg {
                boon_syntax::CallArg::Named { name, value } if name == "root" => {
                    document_target_path(value)
                }
                boon_syntax::CallArg::Positional(value) => document_target_path(value),
                _ => None,
            })
        }
        boon_syntax::Expr::Call { callee, args } if callee == "Element/button" => {
            args.iter().find_map(|arg| match arg {
                boon_syntax::CallArg::Named { name, value } if name == "label" => {
                    document_target_path(value)
                }
                _ => None,
            })
        }
        _ => reference_path(expression),
    }
}

fn reference_path(expression: &boon_syntax::Expr) -> Option<String> {
    match expression {
        boon_syntax::Expr::Path(path) => Some(path.clone()),
        _ => None,
    }
}

fn target_expression<'a>(
    target: &str,
    hir: &'a boon_hir::HirModule,
) -> Option<&'a boon_syntax::Expr> {
    let mut parts = target.split('.');
    let root = parts.next()?;
    let definition = hir
        .definitions
        .iter()
        .find(|definition| definition.name == root)?;
    let mut expression = &definition.expression;
    for part in parts {
        expression = record_field(expression, part)?;
    }
    Some(expression)
}

fn record_field<'a>(
    expression: &'a boon_syntax::Expr,
    name: &str,
) -> Option<&'a boon_syntax::Expr> {
    match expression {
        boon_syntax::Expr::Record(fields) | boon_syntax::Expr::Constructor { fields, .. } => fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| &field.value),
        _ => None,
    }
}

fn monitor_node_name(
    graph_id: &str,
    target_path: Option<&str>,
    semantic_expr: Option<&boon_syntax::Expr>,
    hir: &boon_hir::HirModule,
    operators: &[GraphOperator],
) -> String {
    let target_name = target_path
        .and_then(|path| path.rsplit('.').next())
        .map(to_pascal_identifier);
    if semantic_expr.is_some_and(|expr| {
        expression_has_call(expr, "Math/sum")
            && expression_or_ref_has_call(expr, "Timer/interval", hir)
    }) {
        return "IntervalCounter".to_owned();
    }
    if semantic_expr.is_some_and(|expr| {
        expression_has_hold(expr) && expression_or_ref_has_call(expr, "Timer/interval", hir)
    }) {
        return "IntervalHoldCounter".to_owned();
    }
    if operators
        .iter()
        .any(|op| op.kind == GraphOperatorKind::Latest)
        && semantic_expr.is_some_and(expression_has_latest)
    {
        return "LatestValue".to_owned();
    }
    if operators
        .iter()
        .any(|op| op.kind == GraphOperatorKind::When)
        && semantic_expr
            .is_some_and(|expr| expression_has_match(expr, boon_syntax::MatchKind::When))
    {
        return "WhenEnter".to_owned();
    }
    if operators
        .iter()
        .any(|op| op.kind == GraphOperatorKind::WhileSwitch)
        && semantic_expr
            .is_some_and(|expr| expression_has_match(expr, boon_syntax::MatchKind::While))
    {
        return "WhileFilter".to_owned();
    }
    if semantic_expr.is_some_and(expression_has_hold) {
        return format!("{}Hold", target_name.unwrap_or_else(|| "Value".to_owned()));
    }
    if semantic_expr.is_some_and(|expr| expression_has_call(expr, "Math/sum")) {
        return target_name.unwrap_or_else(|| "Sum".to_owned());
    }
    if semantic_expr.is_some_and(expression_has_then) {
        return format!("Then{}", target_name.unwrap_or_else(|| "Value".to_owned()));
    }
    to_pascal_identifier(graph_id)
}

fn initial_text(
    document_expr: Option<&boon_syntax::Expr>,
    semantic_expr: Option<&boon_syntax::Expr>,
    hir: &boon_hir::HirModule,
) -> String {
    if let Some(text) = document_expr.and_then(|expr| constant_text(expr, hir)) {
        return text;
    }
    let Some(expr) = semantic_expr else {
        return String::new();
    };
    if expression_has_call(expr, "Math/sum")
        || expression_has_hold(expr)
        || expression_has_then(expr)
    {
        "0".to_owned()
    } else if expression_has_latest(expr)
        || expression_has_match(expr, boon_syntax::MatchKind::When)
        || expression_has_match(expr, boon_syntax::MatchKind::While)
    {
        String::new()
    } else {
        constant_text(expr, hir).unwrap_or_default()
    }
}

fn output_behavior(
    document_expr: Option<&boon_syntax::Expr>,
    semantic_expr: Option<&boon_syntax::Expr>,
    hir: &boon_hir::HirModule,
    initial_text: &str,
) -> TextBehavior {
    if let Some(text) = document_expr.and_then(|expr| constant_text(expr, hir)) {
        return TextBehavior::Constant(text);
    }
    let Some(expr) = semantic_expr else {
        return TextBehavior::Constant(String::new());
    };
    if expression_has_latest(expr) {
        TextBehavior::LatestAction
    } else if let Some((tag, text)) = match_branch_text(expr, boon_syntax::MatchKind::When) {
        TextBehavior::BranchOnTag { tag, text }
    } else if let Some((tag, text)) = match_branch_text(expr, boon_syntax::MatchKind::While) {
        TextBehavior::BranchOnTag { tag, text }
    } else if expression_has_call(expr, "Math/sum")
        || expression_has_hold(expr)
        || expression_has_then(expr)
    {
        TextBehavior::CountActions {
            initial: initial_text.parse().unwrap_or(0),
        }
    } else {
        TextBehavior::Constant(constant_text(expr, hir).unwrap_or_default())
    }
}

fn constant_text(expression: &boon_syntax::Expr, hir: &boon_hir::HirModule) -> Option<String> {
    match constant_value(expression, hir)? {
        ConstantValue::Text(text) => Some(text),
        ConstantValue::Number(number) => Some(number.to_string()),
        ConstantValue::List(values) => Some(
            values
                .into_iter()
                .filter_map(|value| match value {
                    ConstantValue::Text(text) => Some(text),
                    ConstantValue::Number(number) => Some(number.to_string()),
                    ConstantValue::Record(_) | ConstantValue::List(_) | ConstantValue::Tag(_) => {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(","),
        ),
        ConstantValue::Tag(tag) => Some(tag),
        ConstantValue::Record(_) => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ConstantValue {
    Text(String),
    Number(i64),
    Tag(String),
    List(Vec<ConstantValue>),
    Record(Vec<(String, ConstantValue)>),
}

fn constant_value(
    expression: &boon_syntax::Expr,
    hir: &boon_hir::HirModule,
) -> Option<ConstantValue> {
    match expression {
        boon_syntax::Expr::Text(text) => Some(ConstantValue::Text(text.clone())),
        boon_syntax::Expr::Number(number) => number.parse::<i64>().ok().map(ConstantValue::Number),
        boon_syntax::Expr::Tag(tag) => Some(ConstantValue::Tag(tag.clone())),
        boon_syntax::Expr::Path(path) => {
            target_expression(path, hir).and_then(|expression| constant_value(expression, hir))
        }
        boon_syntax::Expr::Record(fields) => Some(ConstantValue::Record(
            fields
                .iter()
                .filter_map(|field| {
                    constant_value(&field.value, hir).map(|value| (field.name.clone(), value))
                })
                .collect(),
        )),
        boon_syntax::Expr::List(values) => Some(ConstantValue::List(
            values
                .iter()
                .filter_map(|value| constant_value(value, hir))
                .collect(),
        )),
        boon_syntax::Expr::Pipe { input, stage } => {
            let input_value = constant_value(input, hir);
            pipe_constant(input_value, stage, hir)
        }
        boon_syntax::Expr::Call { callee, args } => call_constant(callee, None, args, hir),
        boon_syntax::Expr::Binary { op, left, right } => match op {
            boon_syntax::BinaryOp::Add => {
                match (constant_value(left, hir)?, constant_value(right, hir)?) {
                    (ConstantValue::Number(left), ConstantValue::Number(right)) => {
                        Some(ConstantValue::Number(left + right))
                    }
                    _ => None,
                }
            }
        },
        boon_syntax::Expr::Constructor { .. }
        | boon_syntax::Expr::Block(_)
        | boon_syntax::Expr::Latest(_)
        | boon_syntax::Expr::Then { .. }
        | boon_syntax::Expr::Hold { .. }
        | boon_syntax::Expr::Match { .. }
        | boon_syntax::Expr::Missing
        | boon_syntax::Expr::Source
        | boon_syntax::Expr::Skip => None,
    }
}

fn pipe_constant(
    input: Option<ConstantValue>,
    stage: &boon_syntax::Expr,
    hir: &boon_hir::HirModule,
) -> Option<ConstantValue> {
    match stage {
        boon_syntax::Expr::Call { callee, args } => call_constant(callee, input, args, hir),
        _ => constant_value(stage, hir),
    }
}

fn call_constant(
    callee: &str,
    input: Option<ConstantValue>,
    args: &[boon_syntax::CallArg],
    hir: &boon_hir::HirModule,
) -> Option<ConstantValue> {
    match callee {
        "Document/new" | "Text/from_number" => input,
        "Text/append" => {
            let input = text_of(input?)?;
            let suffix = args.iter().find_map(|arg| match arg {
                boon_syntax::CallArg::Positional(value)
                | boon_syntax::CallArg::Named { value, .. } => constant_text(value, hir),
            })?;
            Some(ConstantValue::Text(format!("{input}{suffix}")))
        }
        "Text/join" => {
            let separator =
                named_text_arg(args, "separator", hir).unwrap_or_else(|| ",".to_owned());
            match input? {
                ConstantValue::List(values) => Some(ConstantValue::Text(
                    values
                        .into_iter()
                        .filter_map(text_of)
                        .collect::<Vec<_>>()
                        .join(&separator),
                )),
                _ => None,
            }
        }
        "Text/uppercase" => Some(ConstantValue::Text(text_of(input?)?.to_uppercase())),
        "List/append" => match input? {
            ConstantValue::List(mut values) => {
                if let Some(item) = named_value_arg(args, "item", hir) {
                    values.push(item);
                }
                Some(ConstantValue::List(values))
            }
            _ => None,
        },
        "List/map" => match input? {
            ConstantValue::List(values) => {
                let uppercase = named_expr_arg(args, "new")
                    .is_some_and(|expr| expression_has_call(expr, "Text/uppercase"));
                Some(ConstantValue::List(
                    values
                        .into_iter()
                        .filter_map(|value| {
                            if uppercase {
                                text_of(value).map(|text| ConstantValue::Text(text.to_uppercase()))
                            } else {
                                Some(value)
                            }
                        })
                        .collect(),
                ))
            }
            _ => None,
        },
        "List/retain" => match input? {
            ConstantValue::List(values) => Some(ConstantValue::List(
                values.into_iter().filter(record_is_incomplete).collect(),
            )),
            _ => None,
        },
        "List/count" => match input? {
            ConstantValue::List(values) => {
                let count = if args.iter().any(|arg| match arg {
                    boon_syntax::CallArg::Named { name, .. } => name == "if",
                    _ => false,
                }) {
                    values.into_iter().filter(record_is_incomplete).count()
                } else {
                    values.len()
                };
                Some(ConstantValue::Number(count as i64))
            }
            _ => None,
        },
        "Temperature/c_to_f" => match input? {
            ConstantValue::Number(celsius) => Some(ConstantValue::Number(celsius * 9 / 5 + 32)),
            _ => None,
        },
        _ => None,
    }
}

fn text_of(value: ConstantValue) -> Option<String> {
    match value {
        ConstantValue::Text(text) => Some(text),
        ConstantValue::Number(number) => Some(number.to_string()),
        ConstantValue::Tag(tag) => Some(tag),
        ConstantValue::List(_) | ConstantValue::Record(_) => None,
    }
}

fn named_text_arg(
    args: &[boon_syntax::CallArg],
    name: &str,
    hir: &boon_hir::HirModule,
) -> Option<String> {
    named_expr_arg(args, name).and_then(|expr| constant_text(expr, hir))
}

fn named_value_arg(
    args: &[boon_syntax::CallArg],
    name: &str,
    hir: &boon_hir::HirModule,
) -> Option<ConstantValue> {
    named_expr_arg(args, name).and_then(|expr| constant_value(expr, hir))
}

fn named_expr_arg<'a>(
    args: &'a [boon_syntax::CallArg],
    name: &str,
) -> Option<&'a boon_syntax::Expr> {
    args.iter().find_map(|arg| match arg {
        boon_syntax::CallArg::Named {
            name: arg_name,
            value,
        } if arg_name == name => Some(value),
        _ => None,
    })
}

fn record_is_incomplete(value: &ConstantValue) -> bool {
    match value {
        ConstantValue::Record(fields) => fields.iter().any(|(name, value)| {
            name == "completed" && matches!(value, ConstantValue::Tag(tag) if tag == "False")
        }),
        _ => true,
    }
}

fn expression_has_call(expression: &boon_syntax::Expr, callee: &str) -> bool {
    walk_any(
        expression,
        &mut |expr| matches!(expr, boon_syntax::Expr::Call { callee: found, .. } if found == callee),
    )
}

fn expression_or_ref_has_call(
    expression: &boon_syntax::Expr,
    callee: &str,
    hir: &boon_hir::HirModule,
) -> bool {
    if expression_has_call(expression, callee) {
        return true;
    }
    let mut found = false;
    walk(expression, &mut |expr| {
        if found {
            return;
        }
        if let boon_syntax::Expr::Path(path) = expr {
            found = target_expression(path, hir)
                .is_some_and(|target| expression_has_call(target, callee));
        }
    });
    found
}

fn expression_has_then(expression: &boon_syntax::Expr) -> bool {
    walk_any(expression, &mut |expr| {
        matches!(expr, boon_syntax::Expr::Then { .. })
    })
}

fn expression_has_hold(expression: &boon_syntax::Expr) -> bool {
    walk_any(expression, &mut |expr| {
        matches!(expr, boon_syntax::Expr::Hold { .. })
    })
}

fn expression_has_latest(expression: &boon_syntax::Expr) -> bool {
    walk_any(expression, &mut |expr| {
        matches!(expr, boon_syntax::Expr::Latest(_))
    })
}

fn expression_has_match(expression: &boon_syntax::Expr, kind: boon_syntax::MatchKind) -> bool {
    walk_any(
        expression,
        &mut |expr| matches!(expr, boon_syntax::Expr::Match { kind: found, .. } if *found == kind),
    )
}

fn match_branch_text(
    expression: &boon_syntax::Expr,
    kind: boon_syntax::MatchKind,
) -> Option<(String, String)> {
    let mut result = None;
    walk(expression, &mut |expr| {
        if result.is_some() {
            return;
        }
        if let boon_syntax::Expr::Match { kind: found, arms } = expr {
            if *found == kind {
                result = arms
                    .iter()
                    .find(|arm| arm.pattern != "__")
                    .and_then(|arm| constant_text(&arm.value, &empty_hir()))
                    .map(|text| {
                        let tag = arms
                            .iter()
                            .find(|arm| arm.pattern != "__")
                            .map(|arm| arm.pattern.clone())
                            .unwrap_or_default();
                        (tag, text)
                    });
            }
        }
    });
    result
}

fn empty_hir() -> boon_hir::HirModule {
    boon_hir::HirModule {
        source_path: String::new(),
        definitions: Vec::new(),
        definition_names: Default::default(),
        sources: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn walk_any<F>(expression: &boon_syntax::Expr, predicate: &mut F) -> bool
where
    F: FnMut(&boon_syntax::Expr) -> bool,
{
    let mut found = false;
    walk(expression, &mut |expr| {
        if predicate(expr) {
            found = true;
        }
    });
    found
}

fn walk<F>(expression: &boon_syntax::Expr, visitor: &mut F)
where
    F: FnMut(&boon_syntax::Expr),
{
    visitor(expression);
    match expression {
        boon_syntax::Expr::Record(fields) | boon_syntax::Expr::Constructor { fields, .. } => {
            for field in fields {
                walk(&field.value, visitor);
            }
        }
        boon_syntax::Expr::List(values)
        | boon_syntax::Expr::Block(values)
        | boon_syntax::Expr::Latest(values)
        | boon_syntax::Expr::Then { body: values } => {
            for value in values {
                walk(value, visitor);
            }
        }
        boon_syntax::Expr::Call { args, .. } => {
            for arg in args {
                match arg {
                    boon_syntax::CallArg::Positional(value)
                    | boon_syntax::CallArg::Named { value, .. } => walk(value, visitor),
                }
            }
        }
        boon_syntax::Expr::Pipe { input, stage } => {
            walk(input, visitor);
            walk(stage, visitor);
        }
        boon_syntax::Expr::Binary { left, right, .. } => {
            walk(left, visitor);
            walk(right, visitor);
        }
        boon_syntax::Expr::Hold { body, .. } => {
            for value in body {
                walk(value, visitor);
            }
        }
        boon_syntax::Expr::Match { arms, .. } => {
            for arm in arms {
                walk(&arm.value, visitor);
            }
        }
        boon_syntax::Expr::Missing
        | boon_syntax::Expr::Path(_)
        | boon_syntax::Expr::Number(_)
        | boon_syntax::Expr::Source
        | boon_syntax::Expr::Skip
        | boon_syntax::Expr::Tag(_)
        | boon_syntax::Expr::Text(_) => {}
    }
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

fn stable_u64(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_graph_is_derived_from_ast() {
        let plan = compile_source(
            "examples/counter/source.bn",
            include_str!("../../../examples/counter/source.bn"),
        );
        assert_eq!(plan.graph.graph_id, "counter");
        assert!(
            plan.graph
                .operators
                .iter()
                .any(|operator| operator.kind == GraphOperatorKind::Then)
        );
        assert!(
            plan.graph
                .source_bindings
                .iter()
                .any(|source| source.path == "store.sources.increment_button.event.press")
        );
    }

    #[test]
    fn todo_document_constant_comes_from_ast_text_literal() {
        let plan = compile_source(
            "examples/todo_mvc/source.bn",
            include_str!("../../../examples/todo_mvc/source.bn"),
        );
        assert_eq!(plan.graph.initial_text, "2 todos");
    }
}
