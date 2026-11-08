//! Needs you view: join of NeedsYou and Todo Module State for the Shell.

use navori_mod_needs_you::{
    ChoiceState, DecisionState, NeedsYouState, OpenDecision, UnviewedUpdate,
};
use navori_mod_todo::{OpenTodo, TodoState, TodoStatus as OpenStatus};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum DecideState {
    Open,
    Answering { choice: String },
    Failed { choice: String, error: String },
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ChoiceView {
    pub label: String,
    pub risk: Option<String>,
    pub action: Option<String>,
    pub module: Option<String>,
    pub answer_request: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DecideRow {
    pub id: i64,
    pub title: String,
    pub reason: String,
    pub raised_at_ms: i64,
    pub state: DecideState,
    pub primary: ChoiceView,
    pub secondary: ChoiceView,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum TodoStatus {
    Overdue,
    Today,
    Soon,
    Later,
    None,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TodoRow {
    pub id: i64,
    pub title: String,
    pub due_on: Option<String>,
    pub status: TodoStatus,
    pub tick_request: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct UpdateRow {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NeedsYouView {
    pub decisions: Vec<DecideRow>,
    pub todos: Vec<TodoRow>,
    pub updates: Vec<UpdateRow>,
    pub today: String,
    pub needs_count: u32,
    pub any_overdue: bool,
    pub all_caught_up: bool,
    pub down: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HistoryEntry {
    pub kind: String,
    pub title: String,
    pub outcome: Option<String>,
    pub resolved_at_ms: i64,
    pub undo_request: Option<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HistoryView {
    pub entries: Vec<HistoryEntry>,
}

/// Pure join of Module State into the Shell's Needs you view.
pub fn build(needs: Option<&NeedsYouState>, todo: Option<&TodoState>) -> Option<NeedsYouView> {
    if needs.is_none() && todo.is_none() {
        return None;
    }

    let decisions: Vec<DecideRow> = needs
        .map(|state| {
            let mut rows: Vec<_> = state.decisions.iter().collect();
            rows.sort_by_key(|d| (expires_key(d.expires_at_ms), d.raised_at_ms, d.id));
            rows.into_iter().map(decide_row).collect()
        })
        .unwrap_or_default();

    let todos: Vec<TodoRow> = todo
        .map(|state| {
            let mut rows: Vec<_> = state.open.iter().collect();
            rows.sort_by_key(|t| (due_key(t), t.id));
            rows.into_iter().map(todo_row).collect()
        })
        .unwrap_or_default();

    let updates: Vec<UpdateRow> = needs
        .map(|state| {
            let mut rows: Vec<_> = state.updates.iter().collect();
            rows.sort_by_key(|u| (std::cmp::Reverse(u.created_at_ms), u.id));
            rows.into_iter().map(update_row).collect()
        })
        .unwrap_or_default();

    let needs_count = count(todos.len() + updates.len());
    let any_overdue = todos.iter().any(|t| t.status == TodoStatus::Overdue);
    let all_caught_up = decisions.is_empty() && todos.is_empty() && updates.is_empty();
    let today = todo.map(|state| state.today.clone()).unwrap_or_default();

    Some(NeedsYouView {
        decisions,
        todos,
        updates,
        today,
        needs_count,
        any_overdue,
        all_caught_up,
        down: false,
    })
}

/// Empty view used when a Module is down but neither State deserializes.
pub fn empty_down() -> NeedsYouView {
    NeedsYouView {
        decisions: Vec::new(),
        todos: Vec::new(),
        updates: Vec::new(),
        today: String::new(),
        needs_count: 0,
        any_overdue: false,
        all_caught_up: true,
        down: true,
    }
}

fn decide_row(decision: &OpenDecision) -> DecideRow {
    DecideRow {
        id: decision.id,
        title: decision.title.clone(),
        reason: decision.reason.clone(),
        raised_at_ms: decision.raised_at_ms,
        state: decide_state(&decision.state),
        primary: choice_view(&decision.primary, decision.id, decision.revision, "primary"),
        secondary: choice_view(
            &decision.secondary,
            decision.id,
            decision.revision,
            "secondary",
        ),
    }
}

fn choice_view(choice: &ChoiceState, id: i64, revision: u32, which: &str) -> ChoiceView {
    ChoiceView {
        label: choice.label.clone(),
        risk: choice.risk.clone(),
        action: choice.action.clone(),
        module: choice.module.clone(),
        answer_request: json!({
            "answer": {
                "id": id,
                "revision": revision,
                "choice": which,
            }
        })
        .to_string(),
    }
}

fn decide_state(state: &DecisionState) -> DecideState {
    match state {
        DecisionState::Open => DecideState::Open,
        DecisionState::Answering { choice } => DecideState::Answering {
            choice: choice.clone(),
        },
        DecisionState::Failed { choice, error } => DecideState::Failed {
            choice: choice.clone(),
            error: error.clone(),
        },
    }
}

fn todo_row(item: &OpenTodo) -> TodoRow {
    TodoRow {
        id: item.id,
        title: item.title.clone(),
        due_on: item.due_on.clone(),
        status: todo_status(item.status),
        tick_request: json!({"tick": {"id": item.id}}).to_string(),
    }
}

fn todo_status(status: OpenStatus) -> TodoStatus {
    match status {
        OpenStatus::Overdue => TodoStatus::Overdue,
        OpenStatus::Today => TodoStatus::Today,
        OpenStatus::Soon => TodoStatus::Soon,
        OpenStatus::Later => TodoStatus::Later,
        OpenStatus::None => TodoStatus::None,
    }
}

fn update_row(update: &UnviewedUpdate) -> UpdateRow {
    UpdateRow {
        id: update.id,
        kind: update.kind.clone(),
        title: update.title.clone(),
        created_at_ms: update.created_at_ms,
    }
}

fn expires_key(expires_at_ms: Option<i64>) -> (u8, i64) {
    match expires_at_ms {
        Some(ms) => (0, ms),
        None => (1, i64::MAX),
    }
}

fn due_key(item: &OpenTodo) -> (u8, String) {
    match &item.due_on {
        Some(due) => (0, due.clone()),
        None => (1, String::new()),
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[derive(Debug, Deserialize)]
struct DecisionHistory {
    kind: String,
    title: String,
    #[serde(default)]
    chosen_label: Option<String>,
    resolved_at_ms: i64,
}

#[derive(Debug, Deserialize)]
struct TodoHistory {
    id: i64,
    title: String,
    done_at_ms: i64,
}

/// Merge `needs_you.history` and `todo.history` JSON replies into one newest-first view.
pub fn merge_history(decisions: Value, todos: Value) -> Result<HistoryView, String> {
    let decision_rows: Vec<DecisionHistory> =
        serde_json::from_value(decisions).map_err(|e| e.to_string())?;
    let todo_rows: Vec<TodoHistory> = serde_json::from_value(todos).map_err(|e| e.to_string())?;

    let mut entries = Vec::with_capacity(decision_rows.len() + todo_rows.len());
    for row in decision_rows {
        entries.push(HistoryEntry {
            kind: row.kind,
            title: row.title,
            outcome: row.chosen_label,
            resolved_at_ms: row.resolved_at_ms,
            undo_request: None,
        });
    }
    for row in todo_rows {
        entries.push(HistoryEntry {
            kind: "todo".into(),
            title: row.title,
            outcome: None,
            resolved_at_ms: row.done_at_ms,
            undo_request: Some(json!({"undo": {"id": row.id}}).to_string()),
        });
    }

    entries.sort_by_key(|entry| (std::cmp::Reverse(entry.resolved_at_ms), entry.kind.clone()));
    Ok(HistoryView { entries })
}
