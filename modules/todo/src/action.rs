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
    Tick {
        id: i64,
    },
    Undo {
        id: i64,
    },
    History,
}

impl ModuleAction for TodoAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Add { .. } | Self::Tick { .. } | Self::Undo { .. } => Risk::Write,
            Self::History => Risk::Read,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        vec![
            ActionDoc {
                name: "add",
                description: "Add a To-do with a title and an optional due date (YYYY-MM-DD).",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "tick",
                description: "Mark a To-do done and move it to History.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "undo",
                description: "Return a ticked To-do from History to the open list.",
                risk: Risk::Write,
            },
            ActionDoc {
                name: "history",
                description: "List done To-dos inside the retention window, newest first.",
                risk: Risk::Read,
            },
        ]
    }
}
