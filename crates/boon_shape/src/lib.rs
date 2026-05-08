use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    diagnostics: Vec<ShapeDiagnostic>,
}

impl ShapeContext {
    fn shape_expr(&mut self, expression: &boon_syntax::Expr) -> Shape {
        match expression {
            boon_syntax::Expr::Missing => Shape::Unknown,
            boon_syntax::Expr::Path(path) => self.path_shape(path),
            boon_syntax::Expr::Number(_) => Shape::Number,
            boon_syntax::Expr::Source => Shape::SourceMarker,
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
                    boon_syntax::BinaryOp::Add
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
                }
            }
            boon_syntax::Expr::Hold { body, .. } => self.last_shape(body),
            boon_syntax::Expr::Match { arms, .. } => {
                let values = arms.iter().map(|arm| arm.value.clone()).collect::<Vec<_>>();
                self.common_shape(&values)
            }
        }
    }

    fn path_shape(&self, path: &str) -> Shape {
        let root = path.split('.').next().unwrap_or(path);
        self.definitions
            .get(root)
            .cloned()
            .unwrap_or(Shape::Unknown)
    }

    fn pipe_shape(&mut self, input: Shape, stage: &boon_syntax::Expr) -> Shape {
        match stage {
            boon_syntax::Expr::Then { body } => self.last_shape(body),
            boon_syntax::Expr::Hold { .. } => input,
            boon_syntax::Expr::Match { arms, .. } => {
                let values = arms.iter().map(|arm| arm.value.clone()).collect::<Vec<_>>();
                self.common_shape(&values)
            }
            boon_syntax::Expr::Call { callee, args } => self.call_shape(callee, args),
            _ => self.shape_expr(stage),
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
            "Element/button" => Shape::Element,
            "Scene/new" => Shape::Scene,
            "Text/from_number" | "Text/join" | "Text/append" | "Text/uppercase" => Shape::Text,
            "Math/sum" | "List/count" | "Temperature/c_to_f" => Shape::Number,
            "Timer/interval" | "Window/animation_frame" => Shape::SourceMarker,
            "Bool/not" => Shape::TagSet(vec!["True".to_owned(), "False".to_owned()]),
            "List/append" | "List/remove" | "List/map" | "List/retain" => {
                Shape::List(Box::new(Shape::Unknown))
            }
            _ => {
                if let Some(shape) = library_call_shape(callee) {
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
        values
            .iter()
            .map(|value| self.shape_expr(value))
            .last()
            .unwrap_or(Shape::Unknown)
    }

    fn common_shape(&mut self, values: &[boon_syntax::Expr]) -> Shape {
        let mut shapes = values.iter().map(|value| self.shape_expr(value));
        let Some(first) = shapes.next() else {
            return Shape::Unknown;
        };
        for shape in shapes {
            if shape != first && shape != Shape::Skip && first != Shape::Skip {
                return Shape::Unknown;
            }
        }
        first
    }
}

fn library_call_shape(callee: &str) -> Option<Shape> {
    let (library, function) = callee.split_once('/')?;
    match (library, function) {
        ("Pong", "initial" | "step") => Some(Shape::Record(BTreeMap::new())),
        _ => None,
    }
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
}
