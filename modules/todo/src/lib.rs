//! To-dos that need the user.

mod action;
mod actor;
mod calendar;
mod state;
pub mod stored;

pub use action::TodoAction;
pub use calendar::LocalCalendar;
pub use state::{OpenTodo, TodoState, TodoStatus};

use std::sync::Arc;

use navori_config::JournalSweep;
use navori_kernel::{Module, ModuleCtx, NoConfig, TrayContribution};

pub struct TodoEnv {
    pub calendar: Arc<dyn LocalCalendar>,
}

impl std::fmt::Debug for TodoEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TodoEnv").finish_non_exhaustive()
    }
}

pub struct Todo {
    calendar: Arc<dyn LocalCalendar>,
    #[allow(dead_code)] // Task 10 prunes against this retention.
    retention: JournalSweep,
}

impl std::fmt::Debug for Todo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Todo")
            .field("retention", &self.retention)
            .finish_non_exhaustive()
    }
}

impl Todo {
    pub fn new(env: TodoEnv, retention: JournalSweep) -> Self {
        Self {
            calendar: env.calendar,
            retention,
        }
    }
}

impl Module for Todo {
    const ID: &'static str = "todo";
    type Action = TodoAction;
    type Config = NoConfig;
    type State = TodoState;

    fn initial_state(&self) -> TodoState {
        TodoState::default()
    }

    fn tray(_state: &TodoState) -> TrayContribution {
        TrayContribution::default()
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        actor::run(self, ctx).await
    }
}
