use navori_kernel::{ActionDoc, ModuleAction, Risk};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TodoAction {
    Add {
        title: String,
        #[serde(default)]
        due_on: Option<String>,
    },
}

impl ModuleAction for TodoAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Add { .. } => Risk::Write,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![ActionDoc {
            name: "add",
            description: "Add a To-do with a title and an optional due date (YYYY-MM-DD).",
            risk: Risk::Write,
        }]
    }
}
