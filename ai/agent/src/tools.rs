use navori_context::ToolDef;
use navori_protocol::{ActionSpec, Risk};
use std::collections::HashMap;

use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    pub module: String,
    pub action: String,
    pub risk: Risk,
    pub unit: bool,
}

#[derive(Debug, Default)]
pub struct Toolset {
    entries: Vec<(Tool, ToolDef)>,
}

impl Toolset {
    pub fn from_specs(specs: &[ActionSpec]) -> Toolset {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for spec in specs {
            *counts.entry(tool_name(spec)).or_default() += 1;
        }
        let entries = specs
            .iter()
            .filter_map(|spec| {
                let name = tool_name(spec);
                if counts.get(&name).copied().unwrap_or(0) > 1 {
                    tracing::warn!(tool = %name, "two actions share one tool name; neither is offered");
                    return None;
                }
                let Some(cut) = cut(spec) else {
                    tracing::warn!(
                        module = %spec.module,
                        action = %spec.name,
                        "action has no variant in its module schema; not offered as a tool"
                    );
                    return None;
                };
                let (parameters, unit) = match cut {
                    Cut::Fields(parameters) => (parameters, false),
                    Cut::Unit => (empty_object(), true),
                };
                let tool = Tool {
                    module: spec.module.clone(),
                    action: spec.name.clone(),
                    risk: spec.risk,
                    unit,
                };
                let def = ToolDef {
                    name,
                    description: spec.description.clone(),
                    parameters,
                };
                Some((tool, def))
            })
            .collect();
        Toolset { entries }
    }

    pub fn defs(&self) -> Vec<ToolDef> {
        self.entries
            .iter()
            .filter(|(tool, _)| tool.risk == Risk::Read)
            .map(|(_, def)| def.clone())
            .collect()
    }

    pub fn lookup(&self, name: &str) -> Option<&Tool> {
        self.entries
            .iter()
            .find(|(_, def)| def.name == name)
            .map(|(tool, _)| tool)
    }
}

enum Cut {
    Unit,
    Fields(Value),
}

fn tool_name(spec: &ActionSpec) -> String {
    format!("{}_{}", spec.module, spec.name)
}

fn empty_object() -> Value {
    json!({"type":"object","properties":{},"additionalProperties":false})
}

fn cut(spec: &ActionSpec) -> Option<Cut> {
    let schema = &spec.input_schema;
    let single = std::slice::from_ref(schema);
    let entries = schema
        .get("oneOf")
        .and_then(Value::as_array)
        .map_or(single, Vec::as_slice);
    for entry in entries {
        if entry.get("const").and_then(Value::as_str) == Some(spec.name.as_str()) {
            return Some(Cut::Unit);
        }
        let in_enum = entry
            .get("enum")
            .and_then(Value::as_array)
            .is_some_and(|names| names.iter().any(|n| n.as_str() == Some(spec.name.as_str())));
        if in_enum {
            return Some(Cut::Unit);
        }
        if let Some(fields) = entry.get("properties").and_then(|p| p.get(&spec.name)) {
            let mut parameters = fields.clone();
            if let (Some(object), Some(defs)) = (parameters.as_object_mut(), schema.get("$defs")) {
                object.insert("$defs".to_owned(), defs.clone());
            }
            return Some(Cut::Fields(parameters));
        }
    }
    None
}
