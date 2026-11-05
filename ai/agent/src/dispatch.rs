use std::time::Duration;

use navori_context::ToolCall;
use navori_kernel::{DispatchError, Kernel};
use navori_protocol::{Envelope, Risk};
use serde_json::{Value, json};

use crate::Toolset;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutcome {
    pub text: String,
    pub is_error: bool,
}

#[derive(Debug, Clone)]
pub struct Dispatcher {
    action_timeout: Duration,
}

impl Dispatcher {
    pub fn new(action_timeout: Duration) -> Dispatcher {
        Dispatcher { action_timeout }
    }

    pub async fn call(&self, kernel: &Kernel, toolset: &Toolset, call: &ToolCall) -> ToolOutcome {
        let Some(tool) = toolset.lookup(&call.name) else {
            return failure(format!("no Action named {}.", call.name));
        };
        if tool.risk != Risk::Read {
            return failure(format!(
                "Navori AI cannot run {} yet: it changes things and needs your approval, which is not available.",
                call.name
            ));
        }
        let arguments: Value = if tool.unit && call.arguments.trim().is_empty() {
            Value::Null
        } else {
            match serde_json::from_str(&call.arguments) {
                Ok(arguments) => arguments,
                Err(e) => return failure(format!("the arguments are not valid JSON: {e}.")),
            }
        };
        let no_arguments =
            arguments.is_null() || arguments.as_object().is_some_and(|o| o.is_empty());
        if tool.unit && !no_arguments {
            return failure(format!(
                "the arguments do not fit {}: it takes no arguments.",
                call.name
            ));
        }
        let action = if tool.unit {
            Value::String(tool.action.clone())
        } else {
            json!({ tool.action.clone(): arguments })
        };
        let envelope = Envelope {
            module: tool.module.clone(),
            action,
        };
        let reply = tokio::time::timeout(self.action_timeout, kernel.dispatch(envelope)).await;
        match reply {
            Ok(Ok(value)) => ToolOutcome {
                text: value.to_string(),
                is_error: false,
            },
            Ok(Err(DispatchError::InvalidAction { message, .. })) => failure(format!(
                "the arguments do not fit {}: {message}.",
                call.name
            )),
            Ok(Err(e)) => failure(format!("{e}.")),
            Err(_) => failure(format!(
                "{} did not answer within {} s.",
                call.name,
                self.action_timeout.as_secs_f64()
            )),
        }
    }
}

fn failure(message: String) -> ToolOutcome {
    ToolOutcome {
        text: format!("Error: {message}"),
        is_error: true,
    }
}
