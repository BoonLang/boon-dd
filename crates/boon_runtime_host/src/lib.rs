use boon_dd::{
    BoonNumber, BoonValue, NodeId, OwnerKey, Scenario, ScenarioCommand, ScenarioStep, SourceAction,
};
use std::collections::BTreeMap;

pub fn parse_scenario(text: &str) -> Scenario {
    parse_scenario_result(text).expect("scenario TOML must be structurally valid")
}

pub fn parse_scenario_result(text: &str) -> Result<Scenario, String> {
    let root = text
        .parse::<toml::Value>()
        .map_err(|error| format!("invalid scenario TOML: {error}"))?;
    let root_table = root
        .as_table()
        .ok_or_else(|| "scenario root must be a TOML table".to_owned())?;
    reject_unknown_keys(root_table, &["initial", "step"], "scenario root")?;
    if let Some(initial) = root.get("initial").and_then(toml::Value::as_table) {
        reject_unknown_keys(initial, &["expect_text"], "initial")?;
    }
    let initial_expect_text = root
        .get("initial")
        .and_then(|initial| initial.get("expect_text"))
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let steps = root
        .get("step")
        .and_then(toml::Value::as_array)
        .map(|steps| steps.iter().map(parse_step).collect::<Result<Vec<_>, _>>())
        .transpose()?
        .unwrap_or_default();
    Ok(Scenario {
        initial_expect_text,
        steps,
    })
}

fn parse_step(value: &toml::Value) -> Result<ScenarioStep, String> {
    let table = value
        .as_table()
        .ok_or_else(|| format!("scenario step must be a table: {value:?}"))?;
    reject_unknown_keys(
        table,
        &[
            "description",
            "actions",
            "expect_text",
            "expect_monitor_changed",
        ],
        "step",
    )?;
    let description = value
        .get("description")
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let expect_text = value
        .get("expect_text")
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let expect_monitor_changed = value
        .get("expect_monitor_changed")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(|node| NodeId(node.to_owned()))
        .collect();
    let mut actions = Vec::new();
    let mut commands = Vec::new();
    for action in value
        .get("actions")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(command) = action.get("command").and_then(toml::Value::as_str) {
            commands.push(ScenarioCommand {
                command: command.to_owned(),
            });
        } else {
            actions.push(parse_action(action)?);
        }
    }
    Ok(ScenarioStep {
        description,
        actions,
        commands,
        expect_text,
        expect_monitor_changed,
    })
}

fn parse_action(value: &toml::Value) -> Result<SourceAction, String> {
    let table = value
        .as_table()
        .ok_or_else(|| format!("scenario action must be a table: {value:?}"))?;
    reject_unknown_keys(
        table,
        &["source", "owner", "generation", "value", "command"],
        "action",
    )?;
    let source = value
        .get("source")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("scenario action missing source: {value:?}"))?
        .to_owned();
    let owner = value
        .get("owner")
        .and_then(toml::Value::as_str)
        .map(|owner| OwnerKey(owner.to_owned()));
    let generation = value
        .get("generation")
        .and_then(toml::Value::as_integer)
        .and_then(|generation| u32::try_from(generation).ok());
    let payload = value
        .get("value")
        .map(parse_value)
        .unwrap_or(BoonValue::EmptyRecord);
    Ok(SourceAction {
        source,
        owner,
        generation,
        value: payload,
    })
}

fn reject_unknown_keys(
    table: &toml::map::Map<String, toml::Value>,
    allowed: &[&str],
    context: &str,
) -> Result<(), String> {
    for key in table.keys() {
        if !allowed.iter().any(|allowed| *allowed == key) {
            return Err(format!("unknown {context} key `{key}`"));
        }
    }
    Ok(())
}

fn parse_value(value: &toml::Value) -> BoonValue {
    match value {
        toml::Value::String(text) => text_to_value(text),
        toml::Value::Integer(number) => BoonValue::Number(BoonNumber::Int(*number)),
        toml::Value::Float(number) => BoonValue::Number(BoonNumber::Float(*number)),
        toml::Value::Boolean(value) => BoonValue::Tag {
            name: boon_dd::TagName(if *value { "True" } else { "False" }.to_owned()),
            payload: None,
        },
        toml::Value::Array(values) if values.is_empty() => BoonValue::EmptyRecord,
        toml::Value::Array(values) => {
            BoonValue::List(values.iter().map(parse_value).collect::<Vec<_>>())
        }
        toml::Value::Table(table) => BoonValue::Record(
            table
                .iter()
                .map(|(key, value)| (key.clone(), parse_value(value)))
                .collect::<BTreeMap<_, _>>(),
        ),
        toml::Value::Datetime(value) => BoonValue::Text(value.to_string()),
    }
}

fn text_to_value(text: &str) -> BoonValue {
    match text {
        "True" | "False" | "Enter" | "Escape" | "Active" | "Completed" => BoonValue::Tag {
            name: boon_dd::TagName(text.to_owned()),
            payload: None,
        },
        _ => BoonValue::Text(text.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands_without_dropping_them() {
        let scenario = parse_scenario(include_str!("../../../examples/counter_hold/scenario.toml"));
        assert!(
            scenario
                .steps
                .iter()
                .flat_map(|step| step.commands.iter())
                .any(|command| command.command == "enable_persistence")
        );
    }

    #[test]
    fn parses_source_actions_structurally() {
        let scenario = parse_scenario(include_str!("../../../examples/when/scenario.toml"));
        let action = &scenario.steps[0].actions[0];
        assert_eq!(action.source, "key_down.key");
        assert!(matches!(
            action.value,
            BoonValue::Tag {
                ref name,
                payload: None
            } if name.0 == "Enter"
        ));
    }

    #[test]
    fn invalid_scenario_toml_is_not_silently_emptied() {
        let error = parse_scenario_result("[[step]\n").expect_err("invalid TOML must fail");
        assert!(error.contains("invalid scenario TOML"));
    }

    #[test]
    fn unknown_scenario_keys_fail() {
        let error = parse_scenario_result(
            r#"
            [initial]
            expect_text = "0"

            [[step]]
            description = "bad"
            typo = true
            actions = []
            expect_text = "0"
            "#,
        )
        .expect_err("unknown step key must fail");
        assert!(error.contains("unknown step key `typo`"));
    }
}
