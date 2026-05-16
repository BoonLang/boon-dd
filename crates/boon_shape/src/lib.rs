use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    Unknown,
    EmptyRecord,
    Record(BTreeMap<String, Shape>),
    List(Box<Shape>),
    Text,
    Number,
    TagSet(Vec<String>),
    SourceMarker,
    Skip,
    Document,
    Element,
    Scene,
    Effect,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapeReport {
    pub definitions: BTreeMap<String, Shape>,
    pub sources: BTreeMap<String, Shape>,
    pub diagnostics: Vec<ShapeDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapeDiagnostic {
    pub message: String,
}

pub fn check_module(module: &boon_hir::HirModule) -> ShapeReport {
    let mut context = ShapeContext {
        definitions: BTreeMap::new(),
        source_paths: module
            .sources
            .iter()
            .map(|source| source.path.clone())
            .collect(),
        scopes: Vec::new(),
        diagnostics: Vec::new(),
    };
    for definition in &module.definitions {
        let shape = context.shape_expr(&definition.expression);
        context.definitions.insert(definition.name.clone(), shape);
    }
    let sources = module
        .sources
        .iter()
        .map(|source| (source.path.clone(), Shape::SourceMarker))
        .collect();
    ShapeReport {
        definitions: context.definitions,
        sources,
        diagnostics: context.diagnostics,
    }
}

struct ShapeContext {
    definitions: BTreeMap<String, Shape>,
    source_paths: BTreeSet<String>,
    scopes: Vec<BTreeMap<String, Shape>>,
    diagnostics: Vec<ShapeDiagnostic>,
}

impl ShapeContext {
    fn shape_expr(&mut self, expression: &boon_syntax::Expr) -> Shape {
        match expression {
            boon_syntax::Expr::Missing => Shape::Unknown,
            boon_syntax::Expr::Path(path) => self.path_shape(path),
            boon_syntax::Expr::Number(_) => Shape::Number,
            boon_syntax::Expr::Source => Shape::SourceMarker,
            boon_syntax::Expr::SourceAt { target } => {
                self.shape_expr(target);
                Shape::SourceMarker
            }
            boon_syntax::Expr::Link { target } => {
                if let Some(target) = target {
                    self.shape_expr(target);
                }
                Shape::SourceMarker
            }
            boon_syntax::Expr::Skip => Shape::Skip,
            boon_syntax::Expr::Tag(name) => Shape::TagSet(vec![name.clone()]),
            boon_syntax::Expr::Text(_) => Shape::Text,
            boon_syntax::Expr::Record(fields) | boon_syntax::Expr::Constructor { fields, .. } => {
                Shape::Record(
                    fields
                        .iter()
                        .map(|field| (field.name.clone(), self.shape_expr(&field.value)))
                        .collect(),
                )
            }
            boon_syntax::Expr::List(values) => Shape::List(Box::new(self.common_shape(values))),
            boon_syntax::Expr::Block(values)
            | boon_syntax::Expr::Latest(values)
            | boon_syntax::Expr::Then { body: values } => self.last_shape(values),
            boon_syntax::Expr::Call { callee, args } => self.call_shape(callee, args),
            boon_syntax::Expr::Pipe { input, stage } => {
                let input_shape = self.shape_expr(input);
                self.pipe_shape(input_shape, stage)
            }
            boon_syntax::Expr::Binary { op, left, right } => {
                let left = self.shape_expr(left);
                let right = self.shape_expr(right);
                match op {
                    boon_syntax::BinaryOp::Add | boon_syntax::BinaryOp::Subtract
                        if left == Shape::Number && right == Shape::Number =>
                    {
                        Shape::Number
                    }
                    boon_syntax::BinaryOp::Add => {
                        self.diagnostics.push(ShapeDiagnostic {
                            message: format!("cannot add shapes {left:?} and {right:?}"),
                        });
                        Shape::Unknown
                    }
                    boon_syntax::BinaryOp::Subtract => {
                        self.diagnostics.push(ShapeDiagnostic {
                            message: format!("cannot subtract shapes {left:?} and {right:?}"),
                        });
                        Shape::Unknown
                    }
                    boon_syntax::BinaryOp::Equal => {
                        Shape::TagSet(vec!["True".to_owned(), "False".to_owned()])
                    }
                }
            }
            boon_syntax::Expr::Hold { binder, body } => self
                .with_scope([(binder.clone(), Shape::Unknown)], |context| {
                    context.last_shape(body)
                }),
            boon_syntax::Expr::Match { arms, .. } => {
                let values = arms.iter().map(|arm| arm.value.clone()).collect::<Vec<_>>();
                self.common_shape(&values)
            }
        }
    }

    fn path_shape(&mut self, path: &str) -> Shape {
        if self.source_paths.contains(path)
            || path
                .strip_prefix("sources.")
                .is_some_and(|source_path| self.source_paths.contains(source_path))
        {
            return Shape::SourceMarker;
        }
        let mut parts = path.split('.');
        let root = parts.next().unwrap_or(path);
        let Some(mut shape) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(root).cloned())
            .or_else(|| self.definitions.get(root).cloned())
        else {
            self.diagnostics.push(ShapeDiagnostic {
                message: format!("unresolved path `{path}`"),
            });
            return Shape::Unknown;
        };
        for part in parts {
            if part == "sources" {
                return Shape::SourceMarker;
            }
            match shape {
                Shape::Record(fields) => {
                    let Some(field_shape) = fields.get(part).cloned() else {
                        self.diagnostics.push(ShapeDiagnostic {
                            message: format!("unknown field `{part}` in path `{path}`"),
                        });
                        return Shape::Unknown;
                    };
                    shape = field_shape;
                }
                other => {
                    self.diagnostics.push(ShapeDiagnostic {
                        message: format!("cannot access field `{part}` on shape {other:?}"),
                    });
                    return Shape::Unknown;
                }
            }
        }
        shape
    }

    fn pipe_shape(&mut self, input: Shape, stage: &boon_syntax::Expr) -> Shape {
        match stage {
            boon_syntax::Expr::SourceAt { target } => {
                self.shape_expr(target);
                input
            }
            boon_syntax::Expr::Link { target } => {
                if let Some(target) = target {
                    self.shape_expr(target);
                }
                input
            }
            boon_syntax::Expr::Then { body } => self.last_shape(body),
            boon_syntax::Expr::Hold { binder, body } => self
                .with_scope([(binder.clone(), input)], |context| {
                    context.last_shape(body)
                }),
            boon_syntax::Expr::Match { arms, .. } => {
                let values = arms.iter().map(|arm| arm.value.clone()).collect::<Vec<_>>();
                self.common_shape(&values)
            }
            boon_syntax::Expr::Call { callee, args } => self.pipe_call_shape(input, callee, args),
            _ => self.shape_expr(stage),
        }
    }

    fn pipe_call_shape(
        &mut self,
        input: Shape,
        callee: &str,
        args: &[boon_syntax::CallArg],
    ) -> Shape {
        match callee {
            "Document/new" => Shape::Document,
            callee if is_element_call(callee) => Shape::Element,
            "Text/empty" | "Text/find" | "Text/from_number" | "Text/join" | "Text/join_lines"
            | "Text/append" | "Text/length" | "Text/repeat" | "Text/space" | "Text/substring"
            | "Text/trim" | "Text/uppercase" => text_call_shape(callee),
            "List/range" => Shape::List(Box::new(Shape::Number)),
            "Math/min" | "Math/round" | "Math/sum" | "Temperature/c_to_f" => Shape::Number,
            "Bool/and" | "Bool/not" | "Bool/or" | "Bool/xor" => {
                Shape::TagSet(vec!["True".to_owned(), "False".to_owned()])
            }
            "List/append" => {
                let item_shape =
                    match named_arg(args, "item").or_else(|| first_positional_arg(args)) {
                        Some(expr) => self.shape_expr(expr),
                        None => {
                            self.diagnostics.push(ShapeDiagnostic {
                                message: "List/append requires an item argument".to_owned(),
                            });
                            Shape::Unknown
                        }
                    };
                match input {
                    Shape::List(existing) if *existing == item_shape => Shape::List(existing),
                    Shape::List(existing) if matches!(*existing, Shape::Unknown) => {
                        Shape::List(Box::new(item_shape))
                    }
                    Shape::List(existing) => {
                        self.diagnostics.push(ShapeDiagnostic {
                            message: format!(
                                "cannot append item shape {item_shape:?} to list item shape {existing:?}"
                            ),
                        });
                        Shape::Unknown
                    }
                    other => {
                        self.diagnostics.push(ShapeDiagnostic {
                            message: format!("List/append expected List input, got {other:?}"),
                        });
                        Shape::Unknown
                    }
                }
            }
            "List/map" => match input {
                Shape::List(item_shape) => {
                    let binder = list_binder(args);
                    let mapped = named_arg(args, "new")
                        .map(|expr| {
                            self.with_scope([(binder, (*item_shape).clone())], |context| {
                                context.shape_expr(expr)
                            })
                        })
                        .unwrap_or((*item_shape).clone());
                    Shape::List(Box::new(mapped))
                }
                other => {
                    self.diagnostics.push(ShapeDiagnostic {
                        message: format!("List/map expected List input, got {other:?}"),
                    });
                    Shape::Unknown
                }
            },
            "List/retain" | "List/remove" => match input {
                Shape::List(item_shape) => {
                    let binder = list_binder(args);
                    for arg_name in ["if", "on"] {
                        if let Some(expr) = named_arg(args, arg_name) {
                            self.with_scope([(binder.clone(), (*item_shape).clone())], |context| {
                                context.shape_expr(expr)
                            });
                        }
                    }
                    Shape::List(item_shape)
                }
                other => {
                    self.diagnostics.push(ShapeDiagnostic {
                        message: format!("{callee} expected List input, got {other:?}"),
                    });
                    Shape::Unknown
                }
            },
            "List/count" => {
                if let Shape::List(item_shape) = input {
                    if let Some(expr) = named_arg(args, "if") {
                        let binder = list_binder(args);
                        self.with_scope([(binder, (*item_shape).clone())], |context| {
                            context.shape_expr(expr)
                        });
                    }
                }
                Shape::Number
            }
            "List/get" | "List/latest" => match input {
                Shape::List(item_shape) => *item_shape,
                other => {
                    self.diagnostics.push(ShapeDiagnostic {
                        message: format!("{callee} expected List input, got {other:?}"),
                    });
                    Shape::Unknown
                }
            },
            "List/is_empty" => Shape::TagSet(vec!["True".to_owned(), "False".to_owned()]),
            "List/sum" => Shape::Number,
            "List/any" | "List/every" => match input {
                Shape::List(item_shape) => {
                    let binder = list_binder(args);
                    if let Some(expr) = named_arg(args, "if") {
                        self.with_scope([(binder, (*item_shape).clone())], |context| {
                            context.shape_expr(expr)
                        });
                    }
                    Shape::TagSet(vec!["True".to_owned(), "False".to_owned()])
                }
                other => {
                    self.diagnostics.push(ShapeDiagnostic {
                        message: format!("{callee} expected List input, got {other:?}"),
                    });
                    Shape::Unknown
                }
            },
            _ => self.call_shape(callee, args),
        }
    }

    fn call_shape(&mut self, callee: &str, args: &[boon_syntax::CallArg]) -> Shape {
        for arg in args {
            match arg {
                boon_syntax::CallArg::Positional(value)
                | boon_syntax::CallArg::Named { value, .. } => {
                    self.shape_expr(value);
                }
            }
        }
        match callee {
            "Document/new" => Shape::Document,
            callee if is_element_call(callee) => Shape::Element,
            "Scene/new" => Shape::Scene,
            "Text/empty" | "Text/find" | "Text/from_number" | "Text/join" | "Text/join_lines"
            | "Text/append" | "Text/length" | "Text/repeat" | "Text/space" | "Text/starts_with"
            | "Text/substring" | "Text/trim" | "Text/uppercase" => text_call_shape(callee),
            "Math/min"
            | "Math/round"
            | "Math/sum"
            | "List/count"
            | "List/sum"
            | "Temperature/c_to_f"
            | "Number/abs"
            | "Number/neg_abs"
            | "Number/max"
            | "Number/clamp"
            | "Number/percent_of_range"
            | "Number/scale_percent" => Shape::Number,
            "Timer/interval" | "Window/animation_frame" => Shape::SourceMarker,
            "Bool/and"
            | "Bool/not"
            | "Bool/or"
            | "Bool/xor"
            | "Number/less_than"
            | "Number/greater_than"
            | "Geometry/intersects"
            | "Text/is_empty"
            | "Text/is_not_empty" => Shape::TagSet(vec!["True".to_owned(), "False".to_owned()]),
            "List/range" => Shape::List(Box::new(Shape::Number)),
            "List/append" | "List/any" | "List/every" | "List/get" | "List/is_empty"
            | "List/latest" | "List/map" | "List/remove" | "List/retain" => Shape::Unknown,
            _ => {
                if let Some(shape) = typed_library_signature(callee) {
                    return shape;
                }
                self.diagnostics.push(ShapeDiagnostic {
                    message: format!("unknown call `{callee}`"),
                });
                Shape::Unknown
            }
        }
    }

    fn last_shape(&mut self, values: &[boon_syntax::Expr]) -> Shape {
        let mut last = None;
        for value in values {
            last = Some(self.shape_expr(value));
        }
        match last {
            Some(shape) => shape,
            None => {
                self.diagnostics.push(ShapeDiagnostic {
                    message: "empty expression block has no shape".to_owned(),
                });
                Shape::Unknown
            }
        }
    }

    fn common_shape(&mut self, values: &[boon_syntax::Expr]) -> Shape {
        let mut shapes = values.iter().map(|value| self.shape_expr(value));
        let Some(first) = shapes.next() else {
            return Shape::Unknown;
        };
        let mut common = first;
        for shape in shapes {
            common = merge_shapes(common, shape);
        }
        common
    }

    fn with_scope<const N: usize, F>(&mut self, bindings: [(String, Shape); N], f: F) -> Shape
    where
        F: FnOnce(&mut ShapeContext) -> Shape,
    {
        self.scopes.push(bindings.into_iter().collect());
        let result = f(self);
        self.scopes.pop();
        result
    }
}

fn merge_shapes(left: Shape, right: Shape) -> Shape {
    match (left, right) {
        (Shape::Skip, shape) | (shape, Shape::Skip) => shape,
        (Shape::Unknown, shape) | (shape, Shape::Unknown) => shape,
        (Shape::TagSet(mut left), Shape::TagSet(right)) => {
            left.extend(right);
            left.sort();
            left.dedup();
            Shape::TagSet(left)
        }
        (Shape::Record(left), Shape::Record(right)) if left.keys().eq(right.keys()) => {
            let fields = left
                .into_iter()
                .map(|(name, left_shape)| {
                    let right_shape = match right.get(&name).cloned() {
                        Some(shape) => shape,
                        None => Shape::Unknown,
                    };
                    (name, merge_shapes(left_shape, right_shape))
                })
                .collect();
            Shape::Record(fields)
        }
        (Shape::List(left), Shape::List(right)) => {
            Shape::List(Box::new(merge_shapes(*left, *right)))
        }
        (left, right) if left == right => left,
        _ => Shape::Unknown,
    }
}

fn typed_library_signature(callee: &str) -> Option<Shape> {
    let (library, function) = callee.split_once('/')?;
    let record_library = [("Pong", "initial"), ("Pong", "step")];
    if record_library
        .iter()
        .any(|(signature_library, signature_function)| {
            library == *signature_library && function == *signature_function
        })
    {
        Some(Shape::Record(BTreeMap::new()))
    } else {
        None
    }
}

fn text_call_shape(callee: &str) -> Shape {
    match callee {
        "Text/find" | "Text/length" => Shape::Number,
        "Text/is_empty" | "Text/is_not_empty" | "Text/starts_with" => {
            Shape::TagSet(vec!["True".to_owned(), "False".to_owned()])
        }
        _ => Shape::Text,
    }
}

fn is_element_call(callee: &str) -> bool {
    let callee = canonical_element_call(callee);
    matches!(
        callee,
        "Element/block"
            | "Element/button"
            | "Element/checkbox"
            | "Element/container"
            | "Element/grid"
            | "Element/label"
            | "Element/link"
            | "Element/panel"
            | "Element/paragraph"
            | "Element/rect"
            | "Element/select"
            | "Element/slider"
            | "Element/stack"
            | "Element/stripe"
            | "Element/svg"
            | "Element/svg_circle"
            | "Element/text"
            | "Element/text_input"
    )
}

fn canonical_element_call(callee: &str) -> &str {
    match callee {
        "Scene/Element/block" => "Element/block",
        "Scene/Element/button" => "Element/button",
        "Scene/Element/checkbox" => "Element/checkbox",
        "Scene/Element/container" => "Element/container",
        "Scene/Element/grid" => "Element/grid",
        "Scene/Element/label" => "Element/label",
        "Scene/Element/link" => "Element/link",
        "Scene/Element/panel" => "Element/panel",
        "Scene/Element/paragraph" => "Element/paragraph",
        "Scene/Element/rect" => "Element/rect",
        "Scene/Element/select" => "Element/select",
        "Scene/Element/slider" => "Element/slider",
        "Scene/Element/stack" => "Element/stack",
        "Scene/Element/stripe" => "Element/stripe",
        "Scene/Element/svg" => "Element/svg",
        "Scene/Element/svg_circle" => "Element/svg_circle",
        "Scene/Element/text" => "Element/text",
        "Scene/Element/text_input" => "Element/text_input",
        _ => callee,
    }
}

fn named_arg<'a>(args: &'a [boon_syntax::CallArg], name: &str) -> Option<&'a boon_syntax::Expr> {
    args.iter().find_map(|arg| match arg {
        boon_syntax::CallArg::Named {
            name: arg_name,
            value,
        } if arg_name == name => Some(value),
        _ => None,
    })
}

fn first_positional_arg(args: &[boon_syntax::CallArg]) -> Option<&boon_syntax::Expr> {
    args.iter().find_map(|arg| match arg {
        boon_syntax::CallArg::Positional(value) => Some(value),
        boon_syntax::CallArg::Named { .. } => None,
    })
}

fn list_binder(args: &[boon_syntax::CallArg]) -> String {
    first_positional_arg(args)
        .and_then(|expr| match expr {
            boon_syntax::Expr::Path(path) => Some(path.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "item".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_counter_document_shape() {
        let parsed = boon_syntax::parse_source(
            "examples/counter/source.bn",
            include_str!("../../../examples/counter/source.bn"),
        );
        let hir = boon_hir::lower(&parsed);
        let report = check_module(&hir);
        assert_eq!(report.definitions.get("document"), Some(&Shape::Document));
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn records_declared_source_shapes() {
        let parsed = boon_syntax::parse_source(
            "examples/counter/source.bn",
            include_str!("../../../examples/counter/source.bn"),
        );
        let hir = boon_hir::lower(&parsed);
        let report = check_module(&hir);
        assert_eq!(
            report
                .sources
                .get("store.sources.increment_button.event.press"),
            Some(&Shape::SourceMarker)
        );
    }

    #[test]
    fn checks_numeric_and_geometry_library_shapes() {
        let parsed = boon_syntax::parse_source(
            "numeric_geometry.bn",
            "clamped: Number/clamp(value: -1, min: 0, max: 10)\nvisible: Number/less_than(left: 1, right: 2)\nhit: Geometry/intersects(ax: 0, ay: 0, aw: 10, ah: 10, bx: 5, by: 5, bw: 10, bh: 10)\n",
        );
        let hir = boon_hir::lower(&parsed);
        let report = check_module(&hir);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
        assert_eq!(report.definitions.get("clamped"), Some(&Shape::Number));
        assert_eq!(
            report.definitions.get("visible"),
            Some(&Shape::TagSet(vec!["True".to_owned(), "False".to_owned()]))
        );
        assert_eq!(
            report.definitions.get("hit"),
            Some(&Shape::TagSet(vec!["True".to_owned(), "False".to_owned()]))
        );
    }

    #[test]
    fn source_target_pipe_preserves_input_shape() {
        let parsed = boon_syntax::parse_source(
            "source_pipe.bn",
            "store: [button: SOURCE]\nbutton: Element/button(label: TEXT { A }) |> SOURCE { store.button }\n",
        );
        let hir = boon_hir::lower(&parsed);
        let report = check_module(&hir);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
        assert_eq!(report.definitions.get("button"), Some(&Shape::Element));
    }

    #[test]
    fn checks_common_element_and_text_helper_shapes() {
        let parsed = boon_syntax::parse_source(
            "elements.bn",
            "document: Document/new(root: Element/stripe(items: LIST { Element/label(label: TEXT { A }) Element/container(child: Element/text(text: Text/space())) Element/text_input(text: Text/empty()) Element/svg(children: LIST { Element/svg_circle(cx: 1, cy: 2, r: 3) }) }))\n",
        );
        let hir = boon_hir::lower(&parsed);
        let report = check_module(&hir);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
        assert_eq!(report.definitions.get("document"), Some(&Shape::Document));
    }

    #[test]
    fn checks_text_library_shapes() {
        let parsed = boon_syntax::parse_source(
            "text_helpers.bn",
            "trimmed: TEXT {  A  } |> Text/trim()\npart: trimmed |> Text/substring(start: 0, length: 1)\nlength: part |> Text/length()\nfound: part |> Text/find(search: TEXT { A })\nstarts: part |> Text/starts_with(prefix: TEXT { A })\nempty: Text/empty() |> Text/is_empty()\nnot_empty: Text/space() |> Text/is_not_empty()\nlines: LIST { TEXT { A } TEXT { B } } |> Text/join_lines()\nrepeated: TEXT { A } |> Text/repeat(count: 2)\n",
        );
        let hir = boon_hir::lower(&parsed);
        let report = check_module(&hir);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
        assert_eq!(report.definitions.get("trimmed"), Some(&Shape::Text));
        assert_eq!(report.definitions.get("part"), Some(&Shape::Text));
        assert_eq!(report.definitions.get("length"), Some(&Shape::Number));
        assert_eq!(report.definitions.get("found"), Some(&Shape::Number));
        assert_eq!(
            report.definitions.get("starts"),
            Some(&Shape::TagSet(vec!["True".to_owned(), "False".to_owned()]))
        );
        assert_eq!(
            report.definitions.get("empty"),
            Some(&Shape::TagSet(vec!["True".to_owned(), "False".to_owned()]))
        );
        assert_eq!(
            report.definitions.get("not_empty"),
            Some(&Shape::TagSet(vec!["True".to_owned(), "False".to_owned()]))
        );
        assert_eq!(report.definitions.get("lines"), Some(&Shape::Text));
        assert_eq!(report.definitions.get("repeated"), Some(&Shape::Text));
    }

    #[test]
    fn checks_bool_combinator_shapes() {
        let parsed = boon_syntax::parse_source(
            "bool_helpers.bn",
            "both: True |> Bool/and(that: False)\neither: False |> Bool/or(that: True)\nonly_one: True |> Bool/xor(that: False)\n",
        );
        let hir = boon_hir::lower(&parsed);
        let report = check_module(&hir);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
        let bool_shape = Shape::TagSet(vec!["True".to_owned(), "False".to_owned()]);
        assert_eq!(report.definitions.get("both"), Some(&bool_shape));
        assert_eq!(report.definitions.get("either"), Some(&bool_shape));
        assert_eq!(report.definitions.get("only_one"), Some(&bool_shape));
    }

    #[test]
    fn checks_list_helper_shapes() {
        let parsed = boon_syntax::parse_source(
            "list_helpers.bn",
            "items: List/range(from: 1, to: 3)\nfirst: items |> List/get(index: 1)\nlatest: items |> List/latest()\nempty: items |> List/is_empty()\nsum: items |> List/sum()\nany_big: items |> List/any(item, if: Number/greater_than(left: item, right: 2))\nevery_big: items |> List/every(item, if: Number/greater_than(left: item, right: 0))\n",
        );
        let hir = boon_hir::lower(&parsed);
        let report = check_module(&hir);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
        assert_eq!(
            report.definitions.get("items"),
            Some(&Shape::List(Box::new(Shape::Number)))
        );
        assert_eq!(report.definitions.get("first"), Some(&Shape::Number));
        assert_eq!(report.definitions.get("latest"), Some(&Shape::Number));
        assert_eq!(report.definitions.get("sum"), Some(&Shape::Number));
        let bool_shape = Shape::TagSet(vec!["True".to_owned(), "False".to_owned()]);
        assert_eq!(report.definitions.get("empty"), Some(&bool_shape));
        assert_eq!(report.definitions.get("any_big"), Some(&bool_shape));
        assert_eq!(report.definitions.get("every_big"), Some(&bool_shape));
    }

    #[test]
    fn checks_math_helper_shapes() {
        let parsed = boon_syntax::parse_source(
            "math_helpers.bn",
            "minimum: 7 |> Math/min(b: 3)\nrounded: minimum |> Math/round()\n",
        );
        let hir = boon_hir::lower(&parsed);
        let report = check_module(&hir);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
        assert_eq!(report.definitions.get("minimum"), Some(&Shape::Number));
        assert_eq!(report.definitions.get("rounded"), Some(&Shape::Number));
    }

    #[test]
    fn checks_scene_element_alias_shapes() {
        let parsed = boon_syntax::parse_source(
            "scene_elements.bn",
            "scene: Scene/new(root: Scene/Element/stripe(items: LIST { Scene/Element/text(text: TEXT { A }) Scene/Element/button(label: TEXT { B }) }))\n",
        );
        let hir = boon_hir::lower(&parsed);
        let report = check_module(&hir);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
        assert_eq!(report.definitions.get("scene"), Some(&Shape::Scene));
    }
}
