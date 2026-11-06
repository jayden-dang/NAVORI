#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use navori_agent::Toolset;
use navori_protocol::{ActionSpec, Risk};
use navori_store::Store;
use serde_json::json;

#[tokio::test]
async fn the_system_actions_schema_has_one_entry_per_variant() {
    let kernel = common::kernel(Store::open_in_memory().unwrap());
    let specs = kernel.actions();
    let system = specs.iter().find(|s| s.module == "system").unwrap();
    let one_of = system.input_schema["oneOf"].as_array().unwrap();
    assert!(one_of.iter().any(|e| e["const"] == "ping"));
    let prune = one_of
        .iter()
        .find(|e| e["properties"]["prune_journal"].is_object())
        .unwrap();
    assert!(prune["properties"]["prune_journal"]["properties"]["older_than_days"].is_object());
}

#[tokio::test]
async fn only_read_actions_are_offered_as_tools() {
    let kernel = common::kernel(Store::open_in_memory().unwrap());
    let toolset = Toolset::from_specs(&kernel.actions());

    let names: Vec<String> = toolset.defs().into_iter().map(|d| d.name).collect();
    assert_eq!(names, ["system_ping"]);
    let prune = toolset.lookup("system_prune_journal").unwrap();
    assert_eq!(prune.risk, Risk::Write);
    assert_eq!(prune.module, "system");
    assert_eq!(prune.action, "prune_journal");
}

#[allow(dead_code)]
#[derive(schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Probe {
    /// Documented unit.
    Status,
    Quiet,
    Loud,
    Find {
        query: String,
        scope: Scope,
    },
}

#[allow(dead_code)]
#[derive(schemars::JsonSchema)]
enum Scope {
    A,
    B,
}

fn probe_spec(name: &str, risk: Risk) -> ActionSpec {
    ActionSpec {
        module: "probe".to_owned(),
        name: name.to_owned(),
        description: format!("probe {name}"),
        risk,
        input_schema: serde_json::to_value(schemars::schema_for!(Probe)).unwrap(),
    }
}

fn empty_object() -> serde_json::Value {
    json!({"type":"object","properties":{},"additionalProperties":false})
}

fn parameters_of(toolset: &Toolset, name: &str) -> serde_json::Value {
    toolset
        .defs()
        .into_iter()
        .find(|d| d.name == name)
        .unwrap()
        .parameters
}

#[test]
fn a_struct_action_takes_its_fields_with_defs_attached() {
    let toolset = Toolset::from_specs(&[probe_spec("find", Risk::Read)]);

    assert_eq!(
        parameters_of(&toolset, "probe_find"),
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "scope": {"$ref": "#/$defs/Scope"}
            },
            "required": ["query", "scope"],
            "$defs": {"Scope": {"type": "string", "enum": ["A", "B"]}}
        })
    );
    assert!(!toolset.lookup("probe_find").unwrap().unit);
}

#[test]
fn an_action_whose_variant_is_missing_from_the_schema_is_not_offered() {
    let toolset = Toolset::from_specs(&[
        probe_spec("find", Risk::Read),
        probe_spec("vanished", Risk::Read),
    ]);

    let names: Vec<String> = toolset.defs().into_iter().map(|d| d.name).collect();
    assert_eq!(names, ["probe_find"]);
    assert!(toolset.lookup("probe_vanished").is_none());
}

#[test]
fn a_unit_action_takes_an_empty_object() {
    let toolset = Toolset::from_specs(&[
        probe_spec("status", Risk::Read),
        probe_spec("quiet", Risk::Read),
    ]);

    assert_eq!(parameters_of(&toolset, "probe_status"), empty_object());
    assert_eq!(parameters_of(&toolset, "probe_quiet"), empty_object());
    assert!(toolset.lookup("probe_status").unwrap().unit);
    assert!(toolset.lookup("probe_quiet").unwrap().unit);
}

#[test]
fn two_actions_with_the_same_tool_name_are_both_left_out() {
    let clash_a = ActionSpec {
        module: "a_b".to_owned(),
        ..probe_spec("status", Risk::Read)
    };
    let clash_b = ActionSpec {
        module: "a".to_owned(),
        name: "b_status".to_owned(),
        input_schema: json!({"oneOf": [{"const": "b_status"}]}),
        ..probe_spec("status", Risk::Read)
    };
    let toolset = Toolset::from_specs(&[clash_a, clash_b, probe_spec("quiet", Risk::Read)]);

    let names: Vec<String> = toolset.defs().into_iter().map(|d| d.name).collect();
    assert_eq!(names, ["probe_quiet"]);
    assert!(toolset.lookup("a_b_status").is_none());
}
