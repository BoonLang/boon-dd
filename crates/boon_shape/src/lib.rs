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
        let shape = context.infer(&definition.expression);
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
    fn infer(&mut self, expression: &boon_syntax::Expr) -> Shape {
        match expression {
            boon_syntax::Expr::Missing => Shape::Unknown,
            boon_syntax::Expr::Path(path) => self.infer_path(path),
            boon_syntax::Expr::Number(_) => Shape::Number,
            boon_syntax::Expr::Source => Shape::SourceMarker,
            boon_syntax::Expr::Skip => Shape::Skip,
            boon_syntax::Expr::Tag(name) => Shape::TagSet(vec![name.clone()]),
            boon_syntax::Expr::Text(_) => Shape::Text,
            boon_syntax::Expr::Record(fields) | boon_syntax::Expr::Constructor { fields, .. } => {
                Shape::Record(
                    fields
                        .iter()
                        .map(|field| (field.name.clone(), self.infer(&field.value)))
                        .collect(),
                )
            }
            boon_syntax::Expr::List(values) => Shape::List(Box::new(self.common_shape(values))),
            boon_syntax::Expr::Block(values)
            | boon_syntax::Expr::Latest(values)
            | boon_syntax::Expr::Then { body: values } => self.last_shape(values),
            boon_syntax::Expr::Call { callee, args } => self.infer_call(callee, args),
            boon_syntax::Expr::Pipe { input, stage } => {
                let input_shape = self.infer(input);
                self.infer_pipe(input_shape, stage)
            }
            boon_syntax::Expr::Binary { op, left, right } => {
                let left = self.infer(left);
                let right = self.infer(right);
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

    fn infer_path(&self, path: &str) -> Shape {
        let root = path.split('.').next().unwrap_or(path);
        self.definitions
            .get(root)
            .cloned()
            .unwrap_or(Shape::Unknown)
    }

    fn infer_pipe(&mut self, input: Shape, stage: &boon_syntax::Expr) -> Shape {
        match stage {
            boon_syntax::Expr::Then { body } => self.last_shape(body),
            boon_syntax::Expr::Hold { .. } => input,
            boon_syntax::Expr::Match { arms, .. } => {
                let values = arms.iter().map(|arm| arm.value.clone()).collect::<Vec<_>>();
                self.common_shape(&values)
            }
            boon_syntax::Expr::Call { callee, args } => self.infer_call(callee, args),
            _ => self.infer(stage),
        }
    }

    fn infer_call(&mut self, callee: &str, args: &[boon_syntax::CallArg]) -> Shape {
        for arg in args {
            match arg {
                boon_syntax::CallArg::Positional(value)
                | boon_syntax::CallArg::Named { value, .. } => {
                    self.infer(value);
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
            "Pong/initial" | "Pong/step" => Shape::Record(BTreeMap::new()),
            _ => {
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
            .map(|value| self.infer(value))
            .last()
            .unwrap_or(Shape::Unknown)
    }

    fn common_shape(&mut self, values: &[boon_syntax::Expr]) -> Shape {
        let mut shapes = values.iter().map(|value| self.infer(value));
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
