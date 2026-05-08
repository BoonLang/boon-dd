use boon_dd::{
    DdRenderOperation, DdRenderProgram, DdRenderProgramSource, GraphNode, GraphOperator,
    GraphOperatorKind, NodeId, SourceBinding, SourceId, StaticGraph,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompilePlan {
    pub source_path: String,
    pub semantic_ir: SemanticIr,
    pub dd_graph_ir: DdGraphIr,
    pub graph: StaticGraph,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticIr {
    pub source_path: String,
    pub nodes: Vec<SemanticNode>,
    pub outputs: SemanticOutputs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticNode {
    pub node: NodeId,
    pub kind: SemanticNodeKind,
    pub shape: String,
    pub source_span: String,
    pub dependencies: Vec<NodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SemanticNodeKind {
    SourceLeaf,
    PathReference,
    Skip,
    ConstantText,
    ConstantNumber,
    Tag,
    Record,
    List,
    BinaryAdd,
    Pipe,
    Then,
    Hold,
    Latest,
    When,
    While,
    LibraryCall,
    RenderSink,
    MonitorTap,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticOutputs {
    pub monitor_node: NodeId,
    pub render_node: NodeId,
    pub physical_scene: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdGraphIr {
    pub graph_id: String,
    pub source_hash: String,
    pub nodes: Vec<DdGraphNode>,
    pub unsupported_semantic_nodes: Vec<NodeId>,
    pub render_program: DdRenderProgram,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdGraphNode {
    pub node: NodeId,
    pub operator: GraphOperatorKind,
    pub inputs: Vec<NodeId>,
    pub output: NodeId,
    pub order: u32,
}

pub fn compile_source(path: impl Into<String>, text: impl Into<String>) -> CompilePlan {
    let path = path.into();
    let text = text.into();
    let parsed = boon_syntax::parse_source(path.clone(), text);
    let hir = boon_hir::lower(&parsed);
    let shapes = boon_shape::check_module(&hir);
    let graph = build_static_graph(&parsed, &hir, &shapes);
    let semantic_ir = build_semantic_ir(&hir, &shapes, &graph);
    let dd_graph_ir = lower_semantic_to_dd(&semantic_ir, &graph, &hir);
    CompilePlan {
        source_path: hir.source_path,
        semantic_ir,
        dd_graph_ir,
        graph,
    }
}

fn build_semantic_ir(
    hir: &boon_hir::HirModule,
    shapes: &boon_shape::ShapeReport,
    graph: &StaticGraph,
) -> SemanticIr {
    let mut builder = SemanticBuilder {
        hir,
        shapes,
        nodes: Vec::new(),
        next_id: 0,
    };
    for definition in &hir.definitions {
        builder.visit_definition(definition);
    }
    builder.nodes.push(SemanticNode {
        node: graph.render_node.clone(),
        kind: SemanticNodeKind::RenderSink,
        shape: "Document".to_owned(),
        source_span: format!("{}:document", hir.source_path),
        dependencies: Vec::new(),
    });
    builder.nodes.push(SemanticNode {
        node: graph.monitor_node.clone(),
        kind: SemanticNodeKind::MonitorTap,
        shape: "Text".to_owned(),
        source_span: format!("{}:document", hir.source_path),
        dependencies: Vec::new(),
    });
    SemanticIr {
        source_path: hir.source_path.clone(),
        nodes: builder.nodes,
        outputs: SemanticOutputs {
            monitor_node: graph.monitor_node.clone(),
            render_node: graph.render_node.clone(),
            physical_scene: graph.physical_scene,
        },
    }
}

struct SemanticBuilder<'a> {
    hir: &'a boon_hir::HirModule,
    shapes: &'a boon_shape::ShapeReport,
    nodes: Vec<SemanticNode>,
    next_id: usize,
}

impl SemanticBuilder<'_> {
    fn visit_definition(&mut self, definition: &boon_hir::HirDefinition) -> NodeId {
        self.visit_expr(
            &definition.expression,
            &definition.name,
            format!(
                "{}:{}..{}",
                self.hir.source_path, definition.span.start, definition.span.end
            ),
        )
    }

    fn visit_expr(
        &mut self,
        expression: &boon_syntax::Expr,
        label: &str,
        source_span: String,
    ) -> NodeId {
        let child_span = source_span.clone();
        let mut dependencies = Vec::new();
        match expression {
            boon_syntax::Expr::Record(fields) | boon_syntax::Expr::Constructor { fields, .. } => {
                for field in fields {
                    dependencies.push(self.visit_expr(
                        &field.value,
                        &format!("{label}.{}", field.name),
                        child_span.clone(),
                    ));
                }
            }
            boon_syntax::Expr::List(values)
            | boon_syntax::Expr::Block(values)
            | boon_syntax::Expr::Latest(values)
            | boon_syntax::Expr::Then { body: values } => {
                for (index, value) in values.iter().enumerate() {
                    dependencies.push(self.visit_expr(
                        value,
                        &format!("{label}.{index}"),
                        child_span.clone(),
                    ));
                }
            }
            boon_syntax::Expr::Call { args, .. } => {
                for (index, arg) in args.iter().enumerate() {
                    let (arg_label, value) = match arg {
                        boon_syntax::CallArg::Positional(value) => (index.to_string(), value),
                        boon_syntax::CallArg::Named { name, value } => (name.clone(), value),
                    };
                    dependencies.push(self.visit_expr(
                        value,
                        &format!("{label}.{arg_label}"),
                        child_span.clone(),
                    ));
                }
            }
            boon_syntax::Expr::Pipe { input, stage } => {
                dependencies.push(self.visit_expr(
                    input,
                    &format!("{label}.input"),
                    child_span.clone(),
                ));
                dependencies.push(self.visit_expr(
                    stage,
                    &format!("{label}.stage"),
                    child_span.clone(),
                ));
            }
            boon_syntax::Expr::Binary { left, right, .. } => {
                dependencies.push(self.visit_expr(
                    left,
                    &format!("{label}.left"),
                    child_span.clone(),
                ));
                dependencies.push(self.visit_expr(
                    right,
                    &format!("{label}.right"),
                    child_span.clone(),
                ));
            }
            boon_syntax::Expr::Hold { body, .. } => {
                for (index, value) in body.iter().enumerate() {
                    dependencies.push(self.visit_expr(
                        value,
                        &format!("{label}.hold.{index}"),
                        child_span.clone(),
                    ));
                }
            }
            boon_syntax::Expr::Match { arms, .. } => {
                for arm in arms {
                    dependencies.push(self.visit_expr(
                        &arm.value,
                        &format!("{label}.arm.{}", arm.pattern),
                        child_span.clone(),
                    ));
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

        let node = self.next_node(label);
        self.nodes.push(SemanticNode {
            node: node.clone(),
            kind: semantic_kind(expression),
            shape: self.shape_for(label),
            source_span,
            dependencies,
        });
        node
    }

    fn next_node(&mut self, label: &str) -> NodeId {
        self.next_id += 1;
        NodeId(format!(
            "Semantic{}{}",
            self.next_id,
            to_pascal_identifier(label)
        ))
    }

    fn shape_for(&self, label: &str) -> String {
        let definition = label.split('.').next().unwrap_or(label);
        self.shapes
            .definitions
            .get(definition)
            .map(|shape| format!("{shape:?}"))
            .unwrap_or_else(|| "Unknown".to_owned())
    }
}

fn semantic_kind(expression: &boon_syntax::Expr) -> SemanticNodeKind {
    match expression {
        boon_syntax::Expr::Missing => SemanticNodeKind::Unknown,
        boon_syntax::Expr::Path(_) => SemanticNodeKind::PathReference,
        boon_syntax::Expr::Skip => SemanticNodeKind::Skip,
        boon_syntax::Expr::Number(_) => SemanticNodeKind::ConstantNumber,
        boon_syntax::Expr::Source => SemanticNodeKind::SourceLeaf,
        boon_syntax::Expr::Tag(_) => SemanticNodeKind::Tag,
        boon_syntax::Expr::Text(_) => SemanticNodeKind::ConstantText,
        boon_syntax::Expr::Record(_) | boon_syntax::Expr::Constructor { .. } => {
            SemanticNodeKind::Record
        }
        boon_syntax::Expr::List(_) | boon_syntax::Expr::Block(_) => SemanticNodeKind::List,
        boon_syntax::Expr::Latest(_) => SemanticNodeKind::Latest,
        boon_syntax::Expr::Call { .. } => SemanticNodeKind::LibraryCall,
        boon_syntax::Expr::Pipe { .. } => SemanticNodeKind::Pipe,
        boon_syntax::Expr::Binary { .. } => SemanticNodeKind::BinaryAdd,
        boon_syntax::Expr::Then { .. } => SemanticNodeKind::Then,
        boon_syntax::Expr::Hold { .. } => SemanticNodeKind::Hold,
        boon_syntax::Expr::Match {
            kind: boon_syntax::MatchKind::When,
            ..
        } => SemanticNodeKind::When,
        boon_syntax::Expr::Match {
            kind: boon_syntax::MatchKind::While,
            ..
        } => SemanticNodeKind::While,
    }
}

fn lower_semantic_to_dd(
    semantic_ir: &SemanticIr,
    graph: &StaticGraph,
    hir: &boon_hir::HirModule,
) -> DdGraphIr {
    let lowered_kinds = graph
        .operators
        .iter()
        .filter_map(|operator| semantic_kind_for_operator(&operator.kind))
        .collect::<std::collections::BTreeSet<_>>();
    let unsupported_semantic_nodes = semantic_ir
        .nodes
        .iter()
        .filter(|node| {
            !matches!(
                node.kind,
                SemanticNodeKind::ConstantText
                    | SemanticNodeKind::ConstantNumber
                    | SemanticNodeKind::PathReference
                    | SemanticNodeKind::Pipe
                    | SemanticNodeKind::Skip
                    | SemanticNodeKind::Tag
                    | SemanticNodeKind::Record
                    | SemanticNodeKind::List
            ) && !lowered_kinds.contains(&node.kind)
        })
        .map(|node| node.node.clone())
        .collect();
    DdGraphIr {
        graph_id: graph.graph_id.clone(),
        source_hash: graph.source_hash.clone(),
        nodes: graph
            .operators
            .iter()
            .map(|operator| DdGraphNode {
                node: operator.node.clone(),
                operator: operator.kind.clone(),
                inputs: operator.inputs.clone(),
                output: operator.output.clone(),
                order: operator.order,
            })
            .collect(),
        unsupported_semantic_nodes,
        render_program: dd_render_program_from_hir(hir, graph),
    }
}

fn dd_render_program_from_hir(hir: &boon_hir::HirModule, graph: &StaticGraph) -> DdRenderProgram {
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
    let source = DdRenderProgramSource {
        semantic_path: target_path.clone(),
        output_node: graph.render_node.clone(),
    };
    let operation = if let Some(text) = document_expr.and_then(|expr| constant_text(expr, hir)) {
        DdRenderOperation::ConstantText(text)
    } else if let Some(expr) = semantic_expr {
        dd_render_operation_from_expr(expr, hir, &graph.initial_text)
    } else {
        DdRenderOperation::ConstantText(String::new())
    };
    DdRenderProgram { source, operation }
}

fn dd_render_operation_from_expr(
    expr: &boon_syntax::Expr,
    hir: &boon_hir::HirModule,
    initial_text: &str,
) -> DdRenderOperation {
    if expression_has_latest(expr) {
        DdRenderOperation::LatestInputText
    } else if let Some((tag, text)) = match_branch_text(expr, boon_syntax::MatchKind::When) {
        DdRenderOperation::MatchTagText { tag, text }
    } else if let Some((tag, text)) = match_branch_text(expr, boon_syntax::MatchKind::While) {
        DdRenderOperation::MatchTagText { tag, text }
    } else if expression_has_call(expr, "Math/sum")
        || expression_has_hold(expr)
        || expression_has_then(expr)
    {
        DdRenderOperation::CountInputEvents {
            initial: initial_text.parse().unwrap_or(0),
        }
    } else {
        DdRenderOperation::ConstantText(constant_text(expr, hir).unwrap_or_default())
    }
}

fn semantic_kind_for_operator(kind: &GraphOperatorKind) -> Option<SemanticNodeKind> {
    Some(match kind {
        GraphOperatorKind::SourceLeaf => SemanticNodeKind::SourceLeaf,
        GraphOperatorKind::Then | GraphOperatorKind::ThenConst => SemanticNodeKind::Then,
        GraphOperatorKind::When => SemanticNodeKind::When,
        GraphOperatorKind::WhileSwitch => SemanticNodeKind::While,
        GraphOperatorKind::Latest => SemanticNodeKind::Latest,
        GraphOperatorKind::Hold | GraphOperatorKind::KeyedHold => SemanticNodeKind::Hold,
        GraphOperatorKind::ListAppend
        | GraphOperatorKind::ListRemove
        | GraphOperatorKind::ListMap
        | GraphOperatorKind::ListRetain => SemanticNodeKind::LibraryCall,
        GraphOperatorKind::RenderSink => SemanticNodeKind::RenderSink,
        GraphOperatorKind::EffectSink | GraphOperatorKind::PersistTap => return None,
        GraphOperatorKind::MonitorTap => SemanticNodeKind::MonitorTap,
        GraphOperatorKind::LibraryCall => SemanticNodeKind::LibraryCall,
        GraphOperatorKind::BinaryAdd => SemanticNodeKind::BinaryAdd,
    })
}

fn build_static_graph(
    parsed: &boon_syntax::ParsedModule,
    hir: &boon_hir::HirModule,
    shapes: &boon_shape::ShapeReport,
) -> StaticGraph {
    let graph_id = graph_id_from_source_hash(&parsed.source.text);
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
    let monitor_node = NodeId(document_output_node_name(target_path.as_deref()));
    let render_node = NodeId("DocumentText".to_owned());
    let initial_text = initial_text(document_expr, semantic_expr, hir);
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
        source_hash: sha256_text(&parsed.source.text),
        source_bindings,
        nodes,
        operators,
        monitor_node,
        render_node,
        initial_text,
        physical_scene: hir
            .definitions
            .iter()
            .any(|definition| expression_has_call(&definition.expression, "Scene/new")),
    }
}

fn graph_id_from_source_hash(source: &str) -> String {
    format!("module_{}", &sha256_text(source)[..16])
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
                self.add(GraphOperatorKind::BinaryAdd);
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
            (GraphOperatorKind::BinaryAdd, "BinaryAdd", 80),
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
            dynamic: false,
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
    "Unknown".to_owned()
}

fn document_output_node_name(target_path: Option<&str>) -> String {
    target_path
        .and_then(|path| path.rsplit('.').next())
        .map(to_pascal_identifier)
        .unwrap_or_else(|| "DocumentOutput".to_owned())
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

fn sha256_text(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
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
        assert!(plan.graph.graph_id.starts_with("module_"));
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
