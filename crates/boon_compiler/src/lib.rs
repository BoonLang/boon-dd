use boon_dd::{
    DdOutputProtocol, DdOutputSink, DdRenderArg, DdRenderExpr, DdRenderField, DdRenderMatchArm,
    DdRenderMatchKind, DdRenderOperation, DdRenderProgram, DdRenderProgramSource, GraphNode,
    GraphOperator, GraphOperatorKind, NodeId, SourceBinding, SourceId, StaticGraph,
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
    pub output_protocol: DdOutputProtocol,
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
    let render_program = dd_render_program_from_hir(hir, graph);
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
        output_protocol: dd_output_protocol(graph, &render_program),
        render_program,
    }
}

fn dd_output_protocol(graph: &StaticGraph, render_program: &DdRenderProgram) -> DdOutputProtocol {
    let render_source = render_program.source.clone();
    let mut sinks = vec![
        DdOutputSink::MonitorNodeValue {
            node: graph.monitor_node.clone(),
            source: render_source.clone(),
        },
        DdOutputSink::RenderPatchText {
            node: graph.render_node.clone(),
            source: render_source.clone(),
        },
    ];
    if graph
        .operators
        .iter()
        .any(|operator| operator.kind == GraphOperatorKind::EffectSink)
    {
        sinks.push(DdOutputSink::Effect {
            node: NodeId("EffectSink".to_owned()),
            source: render_source.clone(),
        });
    }
    if graph
        .operators
        .iter()
        .any(|operator| operator.kind == GraphOperatorKind::PersistTap)
    {
        sinks.push(DdOutputSink::Persistence {
            node: NodeId("PersistTap".to_owned()),
            source: render_source,
        });
    }
    DdOutputProtocol {
        schema_version: "boon-dd-output-v1".to_owned(),
        sinks,
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
    let operation = semantic_expr
        .map(|expr| render_operation_from_syntax(expr, hir))
        .unwrap_or_else(|| DdRenderOperation::StaticText {
            expr: DdRenderExpr::Text(String::new()),
        });
    DdRenderProgram { source, operation }
}

fn render_operation_from_syntax(
    expr: &boon_syntax::Expr,
    hir: &boon_hir::HirModule,
) -> DdRenderOperation {
    if tree_has_latest(expr) {
        DdRenderOperation::LatestInputText
    } else if let Some((tag, value)) = first_match_branch(expr, boon_syntax::MatchKind::When, hir) {
        DdRenderOperation::MatchTagText { tag, expr: value }
    } else if let Some((tag, value)) = first_match_branch(expr, boon_syntax::MatchKind::While, hir)
    {
        DdRenderOperation::MatchTagText { tag, expr: value }
    } else if tree_has_callee(expr, "Math/sum") || tree_has_hold(expr) || tree_has_then(expr) {
        DdRenderOperation::CountInputEvents {
            initial: counter_seed(expr),
        }
    } else {
        DdRenderOperation::StaticText {
            expr: render_expr_from_syntax(expr, hir),
        }
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
    let operators = graph_operators(hir);
    let source_bindings = source_bindings(hir, shapes);
    let monitor_node = NodeId(document_output_node_name(target_path.as_deref()));
    let render_node = NodeId("DocumentText".to_owned());
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
        initial_text: String::new(),
        physical_scene: hir
            .definitions
            .iter()
            .any(|definition| tree_has_callee(&definition.expression, "Scene/new")),
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

fn render_expr_from_syntax(
    expression: &boon_syntax::Expr,
    hir: &boon_hir::HirModule,
) -> DdRenderExpr {
    match expression {
        boon_syntax::Expr::Missing => DdRenderExpr::Missing,
        boon_syntax::Expr::Path(path) => target_expression(path, hir)
            .map(|expr| render_expr_from_syntax(expr, hir))
            .unwrap_or_else(|| DdRenderExpr::Path(path.clone())),
        boon_syntax::Expr::Number(number) => DdRenderExpr::Number(number.clone()),
        boon_syntax::Expr::Source => DdRenderExpr::Source,
        boon_syntax::Expr::Skip => DdRenderExpr::Skip,
        boon_syntax::Expr::Tag(tag) => DdRenderExpr::Tag(tag.clone()),
        boon_syntax::Expr::Text(text) => DdRenderExpr::Text(text.clone()),
        boon_syntax::Expr::Record(fields) => DdRenderExpr::Record(render_fields(fields, hir)),
        boon_syntax::Expr::List(values) => DdRenderExpr::List(render_exprs(values, hir)),
        boon_syntax::Expr::Block(values) => DdRenderExpr::Block(render_exprs(values, hir)),
        boon_syntax::Expr::Latest(values) => DdRenderExpr::Latest(render_exprs(values, hir)),
        boon_syntax::Expr::Call { callee, args } => DdRenderExpr::Call {
            callee: callee.clone(),
            args: render_args(args, hir),
        },
        boon_syntax::Expr::Constructor { callee, fields } => DdRenderExpr::Constructor {
            callee: callee.clone(),
            fields: render_fields(fields, hir),
        },
        boon_syntax::Expr::Pipe { input, stage } => DdRenderExpr::Pipe {
            input: Box::new(render_expr_from_syntax(input, hir)),
            stage: Box::new(render_expr_from_syntax(stage, hir)),
        },
        boon_syntax::Expr::Binary { op, left, right } => match op {
            boon_syntax::BinaryOp::Add => DdRenderExpr::BinaryAdd {
                left: Box::new(render_expr_from_syntax(left, hir)),
                right: Box::new(render_expr_from_syntax(right, hir)),
            },
        },
        boon_syntax::Expr::Then { body } => DdRenderExpr::Then {
            body: render_exprs(body, hir),
        },
        boon_syntax::Expr::Hold { binder, body } => DdRenderExpr::Hold {
            binder: binder.clone(),
            body: render_exprs(body, hir),
        },
        boon_syntax::Expr::Match { kind, arms } => DdRenderExpr::Match {
            kind: match kind {
                boon_syntax::MatchKind::When => DdRenderMatchKind::When,
                boon_syntax::MatchKind::While => DdRenderMatchKind::While,
            },
            arms: arms
                .iter()
                .map(|arm| DdRenderMatchArm {
                    pattern: arm.pattern.clone(),
                    value: render_expr_from_syntax(&arm.value, hir),
                })
                .collect(),
        },
    }
}

fn render_exprs(expressions: &[boon_syntax::Expr], hir: &boon_hir::HirModule) -> Vec<DdRenderExpr> {
    expressions
        .iter()
        .map(|expr| render_expr_from_syntax(expr, hir))
        .collect()
}

fn render_fields(
    fields: &[boon_syntax::RecordField],
    hir: &boon_hir::HirModule,
) -> Vec<DdRenderField> {
    fields
        .iter()
        .map(|field| DdRenderField {
            name: field.name.clone(),
            value: render_expr_from_syntax(&field.value, hir),
        })
        .collect()
}

fn render_args(args: &[boon_syntax::CallArg], hir: &boon_hir::HirModule) -> Vec<DdRenderArg> {
    args.iter()
        .map(|arg| match arg {
            boon_syntax::CallArg::Positional(value) => {
                DdRenderArg::Positional(render_expr_from_syntax(value, hir))
            }
            boon_syntax::CallArg::Named { name, value } => DdRenderArg::Named {
                name: name.clone(),
                value: render_expr_from_syntax(value, hir),
            },
        })
        .collect()
}

fn tree_has_callee(expression: &boon_syntax::Expr, callee: &str) -> bool {
    walk_any(
        expression,
        &mut |expr| matches!(expr, boon_syntax::Expr::Call { callee: found, .. } if found == callee),
    )
}

fn tree_has_then(expression: &boon_syntax::Expr) -> bool {
    walk_any(expression, &mut |expr| {
        matches!(expr, boon_syntax::Expr::Then { .. })
    })
}

fn tree_has_hold(expression: &boon_syntax::Expr) -> bool {
    walk_any(expression, &mut |expr| {
        matches!(expr, boon_syntax::Expr::Hold { .. })
    })
}

fn tree_has_latest(expression: &boon_syntax::Expr) -> bool {
    walk_any(expression, &mut |expr| {
        matches!(expr, boon_syntax::Expr::Latest(_))
    })
}

fn counter_seed(expression: &boon_syntax::Expr) -> i64 {
    match expression {
        boon_syntax::Expr::Pipe { input, stage }
            if matches!(stage.as_ref(), boon_syntax::Expr::Hold { .. }) =>
        {
            if let boon_syntax::Expr::Number(number) = input.as_ref() {
                return number.parse().unwrap_or(0);
            }
            0
        }
        _ => 0,
    }
}

fn first_match_branch(
    expression: &boon_syntax::Expr,
    kind: boon_syntax::MatchKind,
    hir: &boon_hir::HirModule,
) -> Option<(String, DdRenderExpr)> {
    let mut result = None;
    walk(expression, &mut |expr| {
        if result.is_some() {
            return;
        }
        if let boon_syntax::Expr::Match { kind: found, arms } = expr {
            if *found == kind {
                result = arms.iter().find(|arm| arm.pattern != "__").map(|arm| {
                    (
                        arm.pattern.clone(),
                        render_expr_from_syntax(&arm.value, hir),
                    )
                });
            }
        }
    });
    result
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
    fn todo_document_render_is_lowered_as_expression_ir() {
        let plan = compile_source(
            "examples/todo_mvc/source.bn",
            include_str!("../../../examples/todo_mvc/source.bn"),
        );
        assert!(matches!(
            plan.dd_graph_ir.render_program.operation,
            DdRenderOperation::StaticText { .. }
        ));
    }
}
