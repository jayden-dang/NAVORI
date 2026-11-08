//! Pure join of Needs you and To-do Module State into NeedsYouView.
#![allow(clippy::unwrap_used)]

use navori_ffi::needs_you::{DecideState, TodoStatus, build};
use navori_mod_needs_you::{
    ChoiceState, DecisionState, NeedsYouState, OpenDecision, UnviewedUpdate,
};
use navori_mod_todo::{OpenTodo, TodoState, TodoStatus as OpenStatus};
use serde_json::json;

fn choice(
    label: &str,
    risk: Option<&str>,
    action: Option<&str>,
    module: Option<&str>,
) -> ChoiceState {
    ChoiceState {
        label: label.to_owned(),
        risk: risk.map(str::to_owned),
        action: action.map(str::to_owned),
        module: module.map(str::to_owned),
    }
}

fn decision(id: i64, title: &str, raised_at_ms: i64, expires_at_ms: Option<i64>) -> OpenDecision {
    OpenDecision {
        id,
        key: format!("k{id}"),
        revision: 1,
        title: title.to_owned(),
        reason: "because".into(),
        raised_at_ms,
        expires_at_ms,
        primary: choice("Yes", Some("write"), Some("poke"), Some("target")),
        secondary: choice("No", None, None, None),
        state: DecisionState::Open,
    }
}

fn todo(id: i64, title: &str, due_on: Option<&str>, status: OpenStatus) -> OpenTodo {
    OpenTodo {
        id,
        title: title.to_owned(),
        due_on: due_on.map(str::to_owned),
        status,
    }
}

fn update(id: i64, title: &str, created_at_ms: i64) -> UnviewedUpdate {
    UnviewedUpdate {
        id,
        kind: "decision_expired".into(),
        title: title.to_owned(),
        created_at_ms,
    }
}

#[test]
fn decisions_order_by_expiry_then_age() {
    let needs = NeedsYouState {
        decisions: vec![
            decision(1, "no expiry newer raise", 100, None),
            decision(2, "expires later", 50, Some(500)),
            decision(3, "expires sooner", 200, Some(300)),
            decision(4, "no expiry older raise", 50, None),
            decision(5, "same expiry older raise", 10, Some(400)),
            decision(6, "same expiry newer raise", 20, Some(400)),
        ],
        updates: vec![],
    };

    let view = build(Some(&needs), None).unwrap();
    let titles: Vec<_> = view.decisions.iter().map(|d| d.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "expires sooner",
            "same expiry older raise",
            "same expiry newer raise",
            "expires later",
            "no expiry older raise",
            "no expiry newer raise",
        ]
    );
}

#[test]
fn todos_order_overdue_first_and_no_date_last() {
    let todo_state = TodoState {
        today: "2026-11-08".into(),
        open: vec![
            todo(1, "later", Some("2026-11-20"), OpenStatus::Later),
            todo(2, "no date", None, OpenStatus::None),
            todo(3, "overdue older", Some("2026-11-01"), OpenStatus::Overdue),
            todo(4, "today", Some("2026-11-08"), OpenStatus::Today),
            todo(5, "overdue newer", Some("2026-11-05"), OpenStatus::Overdue),
            todo(6, "soon", Some("2026-11-10"), OpenStatus::Soon),
        ],
    };

    let view = build(None, Some(&todo_state)).unwrap();
    let titles: Vec<_> = view.todos.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "overdue older",
            "overdue newer",
            "today",
            "soon",
            "later",
            "no date",
        ]
    );
    assert_eq!(view.todos[0].status, TodoStatus::Overdue);
    assert_eq!(view.todos[5].status, TodoStatus::None);
    assert_eq!(view.today, "2026-11-08");
}

#[test]
fn updates_order_newest_first() {
    let needs = NeedsYouState {
        decisions: vec![],
        updates: vec![
            update(1, "old", 100),
            update(2, "new", 300),
            update(3, "mid", 200),
        ],
    };

    let view = build(Some(&needs), None).unwrap();
    let titles: Vec<_> = view.updates.iter().map(|u| u.title.as_str()).collect();
    assert_eq!(titles, ["new", "mid", "old"]);
}

#[test]
fn needs_count_is_open_todos_plus_unviewed_updates() {
    let needs = NeedsYouState {
        decisions: vec![decision(1, "decide", 1, None)],
        updates: vec![update(1, "a", 1), update(2, "b", 2)],
    };
    let todo_state = TodoState {
        today: "2026-11-08".into(),
        open: vec![
            todo(1, "one", None, OpenStatus::None),
            todo(2, "two", Some("2026-11-08"), OpenStatus::Today),
        ],
    };

    let view = build(Some(&needs), Some(&todo_state)).unwrap();
    assert_eq!(view.needs_count, 4);
    assert_eq!(view.decisions.len(), 1);
}

#[test]
fn any_overdue_flags_an_overdue_todo() {
    let with_overdue = TodoState {
        today: "2026-11-08".into(),
        open: vec![todo(1, "late", Some("2026-11-01"), OpenStatus::Overdue)],
    };
    assert!(build(None, Some(&with_overdue)).unwrap().any_overdue);

    let without = TodoState {
        today: "2026-11-08".into(),
        open: vec![todo(1, "today", Some("2026-11-08"), OpenStatus::Today)],
    };
    assert!(!build(None, Some(&without)).unwrap().any_overdue);
}

#[test]
fn all_caught_up_only_when_nothing_is_left() {
    let empty_needs = NeedsYouState::default();
    let empty_todo = TodoState {
        today: "2026-11-08".into(),
        open: vec![],
    };
    assert!(
        build(Some(&empty_needs), Some(&empty_todo))
            .unwrap()
            .all_caught_up
    );

    let with_decision = NeedsYouState {
        decisions: vec![decision(1, "open", 1, None)],
        updates: vec![],
    };
    assert!(
        !build(Some(&with_decision), Some(&empty_todo))
            .unwrap()
            .all_caught_up
    );

    let with_todo = TodoState {
        today: "2026-11-08".into(),
        open: vec![todo(1, "open", None, OpenStatus::None)],
    };
    assert!(
        !build(Some(&empty_needs), Some(&with_todo))
            .unwrap()
            .all_caught_up
    );

    let with_update = NeedsYouState {
        decisions: vec![],
        updates: vec![update(1, "u", 1)],
    };
    assert!(
        !build(Some(&with_update), Some(&empty_todo))
            .unwrap()
            .all_caught_up
    );
}

#[test]
fn the_all_list_is_todos_then_updates() {
    let needs = NeedsYouState {
        decisions: vec![],
        updates: vec![update(1, "update-a", 2), update(2, "update-b", 1)],
    };
    let todo_state = TodoState {
        today: "2026-11-08".into(),
        open: vec![
            todo(1, "todo-a", Some("2026-11-01"), OpenStatus::Overdue),
            todo(2, "todo-b", None, OpenStatus::None),
        ],
    };

    let view = build(Some(&needs), Some(&todo_state)).unwrap();
    let all: Vec<&str> = view
        .todos
        .iter()
        .map(|t| t.title.as_str())
        .chain(view.updates.iter().map(|u| u.title.as_str()))
        .collect();
    assert_eq!(all, ["todo-a", "todo-b", "update-a", "update-b"]);
}

#[test]
fn no_decision_appears_in_the_submenu_lists() {
    let needs = NeedsYouState {
        decisions: vec![decision(9, "Decide me", 1, None)],
        updates: vec![update(1, "Expired: X", 1)],
    };
    let todo_state = TodoState {
        today: "2026-11-08".into(),
        open: vec![todo(1, "A todo", None, OpenStatus::None)],
    };

    let view = build(Some(&needs), Some(&todo_state)).unwrap();
    assert!(
        view.todos
            .iter()
            .all(|t| t.title != "Decide me" && t.id != 9)
    );
    assert!(
        view.updates
            .iter()
            .all(|u| u.title != "Decide me" && u.id != 9)
    );
    assert_eq!(view.decisions.len(), 1);
    assert_eq!(view.decisions[0].title, "Decide me");
}

#[test]
fn answer_requests_carry_id_revision_and_choice() {
    let needs = NeedsYouState {
        decisions: vec![OpenDecision {
            id: 42,
            key: "k".into(),
            revision: 7,
            title: "T".into(),
            reason: "R".into(),
            raised_at_ms: 1,
            expires_at_ms: None,
            primary: choice("Go", None, None, None),
            secondary: choice("Stop", None, None, None),
            state: DecisionState::Open,
        }],
        updates: vec![],
    };

    let view = build(Some(&needs), None).unwrap();
    let row = &view.decisions[0];
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&row.primary.answer_request).unwrap(),
        json!({"answer":{"id":42,"revision":7,"choice":"primary"}})
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&row.secondary.answer_request).unwrap(),
        json!({"answer":{"id":42,"revision":7,"choice":"secondary"}})
    );

    let todo_state = TodoState {
        today: "2026-11-08".into(),
        open: vec![todo(99, "Tick me", None, OpenStatus::None)],
    };
    let view = build(None, Some(&todo_state)).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&view.todos[0].tick_request).unwrap(),
        json!({"tick":{"id":99}})
    );
}

#[test]
fn choice_risk_and_action_are_carried_into_the_view() {
    let needs = NeedsYouState {
        decisions: vec![OpenDecision {
            id: 1,
            key: "k".into(),
            revision: 1,
            title: "T".into(),
            reason: "R".into(),
            raised_at_ms: 1,
            expires_at_ms: None,
            primary: choice("Kill", Some("destructive"), Some("kill"), Some("ports")),
            secondary: choice("Skip", Some("read"), None, None),
            state: DecisionState::Answering {
                choice: "primary".into(),
            },
        }],
        updates: vec![],
    };

    let view = build(Some(&needs), None).unwrap();
    let row = &view.decisions[0];
    assert_eq!(row.primary.label, "Kill");
    assert_eq!(row.primary.risk.as_deref(), Some("destructive"));
    assert_eq!(row.primary.action.as_deref(), Some("kill"));
    assert_eq!(row.primary.module.as_deref(), Some("ports"));
    assert_eq!(row.secondary.label, "Skip");
    assert_eq!(row.secondary.risk.as_deref(), Some("read"));
    assert_eq!(row.secondary.action, None);
    assert_eq!(row.secondary.module, None);
    assert_eq!(
        row.state,
        DecideState::Answering {
            choice: "primary".into()
        }
    );
}
