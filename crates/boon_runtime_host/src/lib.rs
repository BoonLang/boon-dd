use boon_dd::{
    BoonNumber, BoonValue, NodeId, OwnerKey, Scenario, ScenarioStep, SmokeOutput, SourceAction,
};

#[derive(Default)]
pub struct RuntimeHost;

impl RuntimeHost {
    pub fn compile_and_run_step(
        &self,
        source_path: &str,
        source_text: &str,
        scenario: &Scenario,
    ) -> Option<SmokeOutput> {
        let graph = boon_compiler::compile_source(source_path, source_text).graph;
        let step = scenario.steps.first()?;
        Some(boon_dd::execute_static_graph(&graph, &step.actions))
    }
}

pub fn parse_scenario(text: &str) -> Scenario {
    let mut initial_expect_text = String::new();
    let mut steps = Vec::new();
    let mut current: Option<ScenarioStep> = None;
    let mut in_actions = false;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line == "[initial]" {
            continue;
        }
        if line == "[[step]]" {
            if let Some(step) = current.take() {
                steps.push(step);
            }
            current = Some(ScenarioStep {
                description: String::new(),
                actions: Vec::new(),
                expect_text: String::new(),
                expect_monitor_changed: Vec::new(),
            });
            in_actions = false;
            continue;
        }
        if let Some(value) = quoted_assignment(line, "expect_text") {
            if let Some(step) = current.as_mut() {
                step.expect_text = value;
            } else {
                initial_expect_text = value;
            }
            continue;
        }
        if let Some(value) = quoted_assignment(line, "description") {
            if let Some(step) = current.as_mut() {
                step.description = value;
            }
            continue;
        }
        if line.starts_with("actions = [") {
            in_actions = true;
            continue;
        }
        if in_actions && line.starts_with(']') {
            in_actions = false;
            continue;
        }
        if in_actions && line.starts_with('{') {
            if let Some(action) = parse_action(line) {
                if let Some(step) = current.as_mut() {
                    step.actions.push(action);
                }
            }
            continue;
        }
        if let Some(values) = array_assignment(line, "expect_monitor_changed") {
            if let Some(step) = current.as_mut() {
                step.expect_monitor_changed = values.into_iter().map(NodeId).collect();
            }
        }
    }

    if let Some(step) = current.take() {
        steps.push(step);
    }

    Scenario {
        initial_expect_text,
        steps,
    }
}

fn quoted_assignment(line: &str, key: &str) -> Option<String> {
    let prefix = format!("{key} = ");
    let value = line.strip_prefix(&prefix)?;
    quoted(value)
}

fn array_assignment(line: &str, key: &str) -> Option<Vec<String>> {
    let prefix = format!("{key} = [");
    let value = line.strip_prefix(&prefix)?.trim_end_matches(']');
    Some(
        value
            .split(',')
            .filter_map(|part| quoted(part.trim()))
            .collect(),
    )
}

fn parse_action(line: &str) -> Option<SourceAction> {
    if line.contains("command =") {
        return None;
    }
    let source = field_value(line, "source").and_then(|value| quoted(&value))?;
    let owner = field_value(line, "owner").and_then(|value| quoted(&value).map(OwnerKey));
    let generation = field_value(line, "generation").and_then(|value| value.parse::<u32>().ok());
    let value = field_value(line, "value")
        .map(|value| parse_value(value.trim().trim_end_matches('}').trim()))
        .unwrap_or(BoonValue::EmptyRecord);
    Some(SourceAction {
        source,
        owner,
        generation,
        value,
    })
}

fn field_value(line: &str, key: &str) -> Option<String> {
    let needle = format!("{key} = ");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let mut value = String::new();
    let mut bracket_depth = 0_i32;
    let mut brace_depth = 0_i32;
    let mut in_string = false;
    for ch in rest.chars() {
        match ch {
            '"' => {
                in_string = !in_string;
                value.push(ch);
            }
            '[' if !in_string => {
                bracket_depth += 1;
                value.push(ch);
            }
            ']' if !in_string => {
                bracket_depth -= 1;
                value.push(ch);
            }
            '{' if !in_string => {
                brace_depth += 1;
                value.push(ch);
            }
            '}' if !in_string && bracket_depth == 0 && brace_depth == 0 => break,
            '}' if !in_string => {
                brace_depth -= 1;
                value.push(ch);
            }
            ',' if !in_string && bracket_depth == 0 && brace_depth == 0 => break,
            _ => value.push(ch),
        }
    }
    Some(value.trim().to_owned())
}

fn quoted(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches(',').trim();
    Some(value.strip_prefix('"')?.strip_suffix('"')?.to_owned())
}

fn parse_value(value: &str) -> BoonValue {
    if value == "[]" {
        BoonValue::EmptyRecord
    } else if let Some(text) = quoted(value) {
        match text.as_str() {
            "True" | "False" | "Enter" | "Escape" | "Active" | "Completed" => BoonValue::Tag {
                name: boon_dd::TagName(text),
                payload: None,
            },
            _ => BoonValue::Text(text),
        }
    } else if value.starts_with('[') {
        let inner = value.trim_start_matches('[').trim_end_matches(']');
        BoonValue::List(
            inner
                .split(',')
                .filter_map(|part| quoted(part.trim()))
                .map(BoonValue::Text)
                .collect(),
        )
    } else if value.starts_with('{') {
        BoonValue::Record(Default::default())
    } else if let Ok(number) = value.parse::<i64>() {
        BoonValue::Number(BoonNumber::Int(number))
    } else {
        BoonValue::Text(value.to_owned())
    }
}
