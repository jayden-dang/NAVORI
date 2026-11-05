// Ported from dsh (MIT, Copyright (c) 2026 DeepSeek): packages/guard/repeat-tool-reminder. See ai/agent/LICENSE-dsh.

use serde_json::{Map, Value};

const PREVIEW_CHARS: usize = 500;

const FIRST_REMINDER: &str = "You are repeating the exact same tool call with identical arguments. Carefully analyze the previous result before calling again: if the task is not complete, try a different approach or different arguments instead of repeating the call.";

#[derive(Debug, Default)]
pub struct RepeatGuard {
    last: Option<(String, String)>,
    count: u32,
}

impl RepeatGuard {
    pub fn new() -> RepeatGuard {
        RepeatGuard::default()
    }

    pub fn observe(&mut self, name: &str, arguments: &str) -> Option<String> {
        let canonical = canonicalize(arguments);
        let key = (name.to_owned(), canonical.clone());
        if self.last.as_ref() == Some(&key) {
            self.count += 1;
        } else {
            self.last = Some(key);
            self.count = 1;
        }
        match self.count {
            3 => Some(FIRST_REMINDER.to_owned()),
            5 | 8 => Some(detailed_reminder(name, self.count, &preview(&canonical))),
            _ => None,
        }
    }
}

fn detailed_reminder(name: &str, count: u32, arguments: &str) -> String {
    format!(
        "Repeated tool call detected:\n- tool: {name}\n- consecutive_calls: {count}\n- arguments: {arguments}\nThe repeated calls are not making progress. Do not call this tool with these exact arguments again. Inspect the latest result and choose a different action, different arguments, or finish the task if enough evidence has been gathered."
    )
}

fn canonicalize(arguments: &str) -> String {
    match serde_json::from_str::<Value>(arguments) {
        Ok(value) => sort_keys(value).to_string(),
        Err(_) => arguments.to_owned(),
    }
}

fn sort_keys(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(sort_keys).collect()),
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(k, v)| (k, sort_keys(v)))
                    .collect::<Map<_, _>>(),
            )
        }
        other => other,
    }
}

fn preview(canonical: &str) -> String {
    let total = canonical.chars().count();
    if total <= PREVIEW_CHARS {
        return canonical.to_owned();
    }
    let head: String = canonical.chars().take(PREVIEW_CHARS).collect();
    format!("{head}… (+{} more chars)", total - PREVIEW_CHARS)
}
