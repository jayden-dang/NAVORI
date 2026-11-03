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

/// Whether `converge` altered the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Changed {
    Yes,
    No,
}

/// Brings the running profile's hooks in `doc` to exactly the desired set and touches nothing
/// else. `doc` must already have passed `classify` without being unreadable.
pub(crate) fn converge(doc: &mut Value, harness: Harness, dev: bool, helper: &Path) -> Changed {
    let before = doc.clone();
    let wanted = desired(harness, dev, helper);
    if let Some(root) = doc.as_object_mut() {
        let hooks = root
            .entry("hooks")
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(events) = hooks.as_object_mut() {
            let present: Vec<String> = events.keys().cloned().collect();
            for event in present {
                let want = wanted
                    .iter()
                    .find(|(name, _)| *name == event)
                    .map(|(_, h)| h);
                let Some(slot) = events.get_mut(&event) else {
                    continue;
                };
                let Some(groups) = slot.as_array_mut() else {
                    continue;
                };
                let removed = settle_event(groups, want, dev);
                if removed && groups.is_empty() {
                    events.shift_remove(&event);
                }
            }
            for (event, handler) in &wanted {
                if !events.contains_key(*event) {
                    events.insert((*event).to_owned(), json!([canonical_group(handler)]));
                }
            }
        }
    }
    if *doc == before {
        Changed::No
    } else {
        Changed::Yes
    }
}

/// Takes the running profile's handlers out of `doc` and nothing else, then drops each group,
/// event array and `hooks` object that this removal itself left empty. `doc` must already have
/// passed `classify` without being unreadable.
pub(crate) fn strip(doc: &mut Value, dev: bool) -> Changed {
    let Some(root) = doc.as_object_mut() else {
        return Changed::No;
    };
    let Some(events) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Changed::No;
    };
    let mut changed = false;
    let names: Vec<String> = events.keys().cloned().collect();
    for name in names {
        let Some(groups) = events.get_mut(&name).and_then(Value::as_array_mut) else {
            continue;
        };
        let mut event_touched = false;
        groups.retain_mut(|group| {
            let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let count = handlers.len();
            handlers.retain(|handler| !is_ours(handler, dev));
            let touched = handlers.len() != count;
            event_touched |= touched;
            !(touched && handlers.is_empty())
        });
        if event_touched {
            changed = true;
            if groups.is_empty() {
                events.shift_remove(&name);
            }
        }
    }
    if changed && events.is_empty() {
        root.shift_remove("hooks");
    }
    if changed { Changed::Yes } else { Changed::No }
}

fn canonical_group(handler: &Handler) -> Value {
    json!({ "hooks": [handler.to_value()] })
}

/// Keeps the first canonical group of ours (rewritten to `want`), takes ours out of every other
/// place, appends a canonical group when none was kept. Returns whether anything of ours was
/// taken out of a group.
fn settle_event(groups: &mut Vec<Value>, want: Option<&Handler>, dev: bool) -> bool {
    let mut kept = false;
    let mut removed = false;
    let mut settled = Vec::with_capacity(groups.len());
    for mut group in std::mem::take(groups) {
        if let (Some(want), false) = (want, kept)
            && is_canonical_ours(&group, dev)
        {
            kept = true;
            group["hooks"][0] = want.to_value();
            settled.push(group);
            continue;
        }
        if let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) {
            let count = handlers.len();
            handlers.retain(|handler| !is_ours(handler, dev));
            if handlers.len() != count {
                removed = true;
                if handlers.is_empty() {
                    continue;
                }
            }
        }
        settled.push(group);
    }
    if let (Some(want), false) = (want, kept) {
        settled.push(canonical_group(want));
    }
    *groups = settled;
    removed
}

fn is_canonical_ours(group: &Value, dev: bool) -> bool {
    group.as_object().is_some_and(|object| {
        object.len() == 1
            && object
                .get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|handlers| handlers.len() == 1 && is_ours(&handlers[0], dev))
    })
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

    fn ours() -> Value {
        json!({ "type": "command", "command": "'/A/Navori.app/Contents/Helpers/navori' hook", "timeout": 2 })
    }

    fn helper() -> &'static Path {
        Path::new("/A/Navori.app/Contents/Helpers/navori")
    }

    #[test]
    fn converge_takes_ours_out_of_an_unwanted_event_without_moving_other_keys() {
        let foreign = json!({ "hooks": [{ "type": "command", "command": "x" }] });
        let mut doc = json!({ "hooks": {
            "A": [foreign.clone()],
            "StopFailure": [{ "hooks": [ours()] }],
            "B": [foreign.clone()],
            "C": [foreign]
        }});
        assert_eq!(
            converge(&mut doc, Harness::Claude, false, helper()),
            Changed::Yes
        );
        let keys: Vec<_> = doc["hooks"].as_object().unwrap().keys().cloned().collect();
        assert_eq!(
            keys,
            ["A", "B", "C", "UserPromptSubmit", "Stop", "SessionEnd"]
        );
    }

    #[test]
    fn converge_keeps_the_foreign_handler_of_a_shared_group_and_one_copy_of_ours() {
        let foreign = json!({ "type": "command", "command": "x" });
        let mut doc = json!({ "hooks": { "Stop": [
            { "matcher": "m", "hooks": [ours(), foreign.clone()] },
            { "hooks": [ours()] },
            { "hooks": [ours()] }
        ]}});
        converge(&mut doc, Harness::Claude, false, helper());
        let stop = doc["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0], json!({ "matcher": "m", "hooks": [foreign] }));
        assert_eq!(stop[1], json!({ "hooks": [ours()] }));
    }

    #[test]
    fn converge_on_a_correct_file_changes_nothing() {
        let mut doc = json!({});
        converge(&mut doc, Harness::Grok, true, helper());
        assert_eq!(
            converge(&mut doc, Harness::Grok, true, helper()),
            Changed::No
        );
    }
}
