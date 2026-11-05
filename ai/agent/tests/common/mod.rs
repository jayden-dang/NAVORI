#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use navori_kernel::{
    ActionDoc, ActionError, Kernel, Module, ModuleAction, ModuleCtx, NoConfig, Risk,
};
use navori_store::Store;
use serde::Deserialize;
use serde_json::json;

pub fn kernel(store: Store) -> Kernel {
    Kernel::builder(store).start()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TestAction {
    /// Look around.
    Look,
    /// Look at item n.
    LookAt { n: u32 },
    /// Change something.
    Change,
    /// Answer after a long wait.
    Slow,
}

impl ModuleAction for TestAction {
    fn risk(&self) -> Risk {
        match self {
            Self::Change => Risk::Write,
            _ => Risk::Read,
        }
    }

    fn catalog() -> Vec<ActionDoc> {
        let doc = |name, description, risk| ActionDoc {
            name,
            description,
            risk,
        };
        vec![
            doc("look", "Look around.", Risk::Read),
            doc("look_at", "Look at item n.", Risk::Read),
            doc("change", "Change something.", Risk::Write),
            doc("slow", "Answer after a long wait.", Risk::Read),
        ]
    }
}

pub struct TestModule {
    pub received: Arc<AtomicUsize>,
}

impl Module for TestModule {
    const ID: &'static str = "probe";
    type Action = TestAction;
    type Config = NoConfig;
    type State = ();

    fn initial_state(&self) {}

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        while let Some(req) = ctx.recv().await {
            self.received.fetch_add(1, Ordering::SeqCst);
            match req.action {
                TestAction::Look => req.respond(Ok(json!({"seen": "room"}))),
                TestAction::LookAt { n } => req.respond(Ok(json!({"seen": n}))),
                TestAction::Change => req.respond::<()>(Err(ActionError::Failed("changed".into()))),
                TestAction::Slow => {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    req.respond(Ok(json!({"seen": "late"})));
                }
            }
        }
        Ok(())
    }
}

pub fn kernel_with_probe() -> (Kernel, Arc<AtomicUsize>) {
    let received = Arc::new(AtomicUsize::new(0));
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .register(TestModule {
            received: Arc::clone(&received),
        })
        .start();
    (kernel, received)
}

#[derive(Default)]
pub struct MemoryConversations {
    state: std::sync::Mutex<MemoryState>,
    fail_next: std::sync::atomic::AtomicBool,
}

#[derive(Default, Clone)]
struct MemoryState {
    entries: Vec<(navori_agent::ConversationId, navori_agent::Entry)>,
    turns: std::collections::HashMap<navori_agent::TurnId, navori_agent::ConversationId>,
}

impl MemoryConversations {
    pub fn fail_next_append(&self) {
        self.fail_next.store(true, Ordering::SeqCst);
    }
}

impl navori_agent::ConversationStore for MemoryConversations {
    fn append(&self, entries: &[navori_agent::Entry]) -> Result<(), navori_agent::RecordError> {
        use navori_agent::{ConversationLog, Entry, RecordError};

        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(RecordError::Write);
        }
        let mut guard = self.state.lock().unwrap();
        let mut next = guard.clone();
        for entry in entries {
            let conversation = match entry {
                Entry::ConversationStarted { id, .. } => *id,
                Entry::TurnStarted {
                    id, conversation, ..
                } => {
                    next.turns.insert(*id, *conversation);
                    *conversation
                }
                Entry::StepStarted { turn, .. }
                | Entry::Message { turn, .. }
                | Entry::Usage { turn, .. }
                | Entry::StepEnded { turn, .. }
                | Entry::TurnEnded { turn, .. } => {
                    *next.turns.get(turn).ok_or(RecordError::Write)?
                }
            };
            next.entries.push((conversation, entry.clone()));
            let scoped: Vec<Entry> = next
                .entries
                .iter()
                .filter(|(c, _)| *c == conversation)
                .map(|(_, e)| e.clone())
                .collect();
            ConversationLog::from_entries(scoped)?;
        }
        *guard = next;
        Ok(())
    }

    fn load(
        &self,
        conversation: &navori_agent::ConversationId,
    ) -> Result<navori_agent::ConversationLog, navori_agent::RecordError> {
        let guard = self.state.lock().unwrap();
        let scoped: Vec<navori_agent::Entry> = guard
            .entries
            .iter()
            .filter(|(c, _)| c == conversation)
            .map(|(_, e)| e.clone())
            .collect();
        if scoped.is_empty() {
            return Err(navori_agent::RecordError::UnknownConversation);
        }
        navori_agent::ConversationLog::from_entries(scoped)
    }
}
