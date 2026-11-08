use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct TodoState {
    pub today: String,
    pub open: Vec<OpenTodo>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OpenTodo {
    pub id: i64,
    pub title: String,
    pub due_on: Option<String>,
    pub status: TodoStatus,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Overdue,
    Today,
    Soon,
    Later,
    None,
}
