use std::path::Path;

use serde_json::{Map, Value, json};

use super::identity::is_navori_command;
use crate::action::Harness;
use crate::state::State;

pub(crate) type Event = &'static str;

const CLAUDE_AND_CODEX_EVENTS: [Event; 3] = ["UserPromptSubmit", "Stop", "SessionEnd"];
const GROK_EVENTS: [Event; 5] = [
    "UserPromptSubmit",
    "Stop",
    "StopCancelled",
    "StopFailure",
    "SessionEnd",
];

const SESSION_END: Event = "SessionEnd";

pub(crate) fn required_events(harness: Harness) -> &'static [Event] {
    match harness {
        Harness::Claude | Harness::Codex => &CLAUDE_AND_CODEX_EVENTS,
        Harness::Grok => &GROK_EVENTS,
    }
}

/// One hook the way Navori writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Handler {
    pub(crate) command: String,
    pub(crate) timeout: u32,
}

impl Handler {
    pub(crate) fn to_value(&self) -> Value {
        json!({ "type": "command", "command": self.command, "timeout": self.timeout })
    }
}

fn command_for(dev: bool, helper: &Path) -> String {
    let quoted = helper.display().to_string().replace('\'', "'\\''");
    if dev {
        format!("'{quoted}' --profile dev hook")
    } else {
        format!("'{quoted}' hook")
    }
}

pub(crate) fn desired(harness: Harness, dev: bool, helper: &Path) -> Vec<(Event, Handler)> {
    let command = command_for(dev, helper);
    required_events(harness)
        .iter()
        .map(|&event| {
            let timeout = if event == SESSION_END { 1 } else { 2 };
            (
                event,
                Handler {
                    command: command.clone(),
                    timeout,
                },
            )
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub state: State,
    /// How many of the running profile's handlers the file holds.
    pub ours: usize,
    /// Why the file cannot be judged, when `state` is `Unreadable`.
    pub reason: Option<String>,
}

fn unreadable(reason: String) -> Classification {
    Classification {
        state: State::Unreadable,
        ours: 0,
        reason: Some(reason),
    }
}

/// What a parsed Hook file says about the running profile's hooks. Never `NotFound`: whether
/// the Harness exists is decided before a file is read.
pub fn classify(doc: &Value, harness: Harness, dev: bool, helper: &Path) -> Classification {
    let Some(root) = doc.as_object() else {
        return unreadable("the top level is not a JSON object".to_owned());
    };
    let empty = Map::new();
    let events = match root.get("hooks") {
        None => &empty,
        Some(Value::Object(events)) => events,
        Some(_) => return unreadable("\"hooks\" is not a JSON object".to_owned()),
    };

    let wanted = desired(harness, dev, helper);
    let mut ours = 0;
    let mut correct = true;
    for (event, groups) in events {
        let Some(groups) = groups.as_array() else {
            return unreadable(format!("\"hooks.{event}\" is not a list"));
        };
        let want = wanted.iter().find(|(name, _)| name == event);
        let mut found_here = 0;
        for group in groups {
            let Some(group) = group.as_object() else {
                return unreadable(format!("an entry under \"hooks.{event}\" is not an object"));
            };
            let handlers = match group.get("hooks") {
                None => continue,
                Some(Value::Array(handlers)) => handlers,
                Some(_) => {
                    return unreadable(format!(
                        "\"hooks\" inside an entry of \"hooks.{event}\" is not a list"
                    ));
                }
            };
            for handler in handlers {
                if !is_ours(handler, dev) {
                    continue;
                }
                ours += 1;
                found_here += 1;
                let canonical = group.len() == 1
                    && handlers.len() == 1
                    && want.is_some_and(|(_, h)| *handler == h.to_value());
                correct &= canonical;
            }
        }
        correct &= found_here == usize::from(want.is_some());
    }

    let state = if ours == 0 {
        State::NotInstalled
    } else if correct && ours == wanted.len() {
        State::Installed
    } else {
        State::NeedsRepair
    };
    Classification {
        state,
        ours,
        reason: None,
    }
}

fn is_ours(handler: &Value, dev: bool) -> bool {
    handler
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| is_navori_command(command, dev))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desired_lists_the_events_of_each_harness_with_their_timeouts() {
        let helper = Path::new("/A/Navori.app/Contents/Helpers/navori");
        let claude = desired(Harness::Claude, false, helper);
        let names: Vec<_> = claude.iter().map(|(e, h)| (*e, h.timeout)).collect();
        assert_eq!(
            names,
            [("UserPromptSubmit", 2), ("Stop", 2), ("SessionEnd", 1)]
        );
        assert_eq!(desired(Harness::Grok, false, helper).len(), 5);
    }

    #[test]
    fn a_quote_in_the_helper_path_is_escaped() {
        let helper = Path::new("/A/It's/Navori.app/Contents/Helpers/navori");
        let wanted = desired(Harness::Claude, true, helper);
        assert_eq!(
            wanted[0].1.command,
            "'/A/It'\\''s/Navori.app/Contents/Helpers/navori' --profile dev hook"
        );
    }
}
