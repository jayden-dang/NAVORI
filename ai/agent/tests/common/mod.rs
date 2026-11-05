#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures::future::BoxFuture;
use navori_agent::{Agent, Limits};
use navori_context::{
    AuditError, AuditRow, AuditSink, ChatReply, ChatRequest, ChatSender, Finish, Gateway,
    SendError, Sender, ToolCall, Usage,
};
use navori_kernel::{
    ActionDoc, ActionError, Kernel, Module, ModuleAction, ModuleCtx, NoConfig, Risk,
};
use navori_protocol::Event;
use navori_redact::Redacted;
use navori_secrets::fake::{FakeKeychain, MemoryStore};
use navori_secrets::{KeychainService, Secret, SecretKey, SecretStore};
use navori_store::Store;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::{broadcast, oneshot};

pub const PLANTED: &str = concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123456789");

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
    /// Show what was found.
    Leak,
    /// Return a very long text.
    Big,
    /// Keep a note.
    Note { text: String },
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
            doc("leak", "Show what was found.", Risk::Read),
            doc("big", "Return a very long text.", Risk::Read),
            doc("note", "Keep a note.", Risk::Read),
        ]
    }
}

pub struct TestModule {
    pub received: Arc<AtomicUsize>,
    pub notes: Arc<std::sync::Mutex<Vec<String>>>,
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
                TestAction::Big => req.respond(Ok(json!("é".repeat(40_000)))),
                TestAction::Note { ref text } => {
                    self.notes.lock().unwrap().push(text.clone());
                    req.respond(Ok(json!({"noted": true})));
                }
                TestAction::Leak => req.respond(Ok(json!({"found": PLANTED}))),
                TestAction::Slow => {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    req.respond(Ok(json!({"seen": "late"})));
                }
            }
        }
        Ok(())
    }
}

pub type Notes = Arc<std::sync::Mutex<Vec<String>>>;

pub fn kernel_with_notes() -> (Kernel, Arc<AtomicUsize>, Notes) {
    let received = Arc::new(AtomicUsize::new(0));
    let notes = Notes::default();
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .register(TestModule {
            received: Arc::clone(&received),
            notes: Arc::clone(&notes),
        })
        .start();
    (kernel, received, notes)
}

pub fn kernel_with_probe() -> (Kernel, Arc<AtomicUsize>) {
    let (kernel, received, _) = kernel_with_notes();
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

impl MemoryConversations {
    pub fn entries(&self) -> Vec<navori_agent::Entry> {
        let guard = self.state.lock().unwrap();
        guard.entries.iter().map(|(_, e)| e.clone()).collect()
    }
}

pub struct ScriptedChat {
    replies: std::sync::Mutex<std::collections::VecDeque<ChatReply>>,
    requests: std::sync::Mutex<Vec<ChatRequest>>,
    hold: std::sync::Mutex<Option<oneshot::Receiver<()>>>,
}

impl ScriptedChat {
    pub fn new(script: Vec<ChatReply>) -> Self {
        Self {
            replies: std::sync::Mutex::new(script.into()),
            requests: std::sync::Mutex::default(),
            hold: std::sync::Mutex::default(),
        }
    }

    pub fn requests(&self) -> Vec<ChatRequest> {
        self.requests.lock().unwrap().clone()
    }

    pub fn hold_next(&self) -> oneshot::Sender<()> {
        let (release, held) = oneshot::channel();
        *self.hold.lock().unwrap() = Some(held);
        release
    }
}

impl ChatSender for ScriptedChat {
    fn send<'a>(
        &'a self,
        _key: &'a Secret,
        request: ChatRequest,
    ) -> BoxFuture<'a, Result<ChatReply, SendError>> {
        self.requests.lock().unwrap().push(request);
        let held = self.hold.lock().unwrap().take();
        Box::pin(async move {
            if let Some(held) = held {
                let _ = held.await;
            }
            self.replies
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| SendError("the script ran out".to_owned()))
        })
    }
}

#[derive(Default)]
pub struct MemoryAudit {
    rows: std::sync::Mutex<Vec<AuditRow>>,
}

impl MemoryAudit {
    pub fn rows(&self) -> Vec<AuditRow> {
        self.rows.lock().unwrap().clone()
    }
}

impl AuditSink for MemoryAudit {
    fn record(&self, row: AuditRow) -> Result<(), AuditError> {
        self.rows.lock().unwrap().push(row);
        Ok(())
    }
}

pub struct StubSender;

impl Sender for StubSender {
    fn send<'a>(
        &'a self,
        _: &'a Secret,
        _: &'a str,
        _: &'a str,
    ) -> BoxFuture<'a, Result<String, SendError>> {
        Box::pin(async { Err(SendError("plain asks are not used here".to_owned())) })
    }
}

pub fn text_reply(text: &str) -> ChatReply {
    ChatReply {
        text: Some(text.to_owned()),
        reasoning: None,
        tool_calls: vec![],
        usage: None,
        finish: Finish::Stop,
    }
}

pub fn call_reply(id: &str, name: &str, arguments: &str) -> ChatReply {
    ChatReply {
        text: None,
        reasoning: None,
        tool_calls: vec![ToolCall {
            id: id.to_owned(),
            name: name.to_owned(),
            arguments: arguments.to_owned(),
        }],
        usage: None,
        finish: Finish::ToolCalls,
    }
}

pub fn usage(input: u64, output: u64) -> Usage {
    Usage {
        input,
        output,
        cache_hit: None,
        cache_miss: None,
    }
}

pub struct Rig {
    pub agent: Agent,
    pub kernel: Kernel,
    pub chat: Arc<ScriptedChat>,
    pub store: Arc<MemoryConversations>,
    pub audit: Arc<MemoryAudit>,
    pub events: broadcast::Receiver<Event>,
    pub probe: Arc<AtomicUsize>,
    pub notes: Notes,
}

pub fn agent_with(script: Vec<ChatReply>, limits: Limits) -> Rig {
    build_rig(script, limits, true, navori_redact::redact_text)
}

pub fn build_rig(
    script: Vec<ChatReply>,
    limits: Limits,
    with_key: bool,
    shield: fn(&str) -> Redacted,
) -> Rig {
    let vault = FakeKeychain::new();
    let secrets: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(KeychainService::DEV, &vault));
    let slot = SecretKey::new("provider", "deepseek").unwrap();
    if with_key {
        secrets.set(&slot, &Secret::new("stored-key-QQ7")).unwrap();
    }
    let chat = Arc::new(ScriptedChat::new(script));
    let audit = Arc::new(MemoryAudit::default());
    let gateway = Gateway::with_shield(
        secrets,
        slot,
        Arc::new(StubSender),
        audit.clone(),
        "deepseek",
        shield,
    )
    .with_chat_sender(chat.clone());
    let (kernel, probe, notes) = kernel_with_notes();
    let events = kernel.bus().subscribe();
    let store = Arc::new(MemoryConversations::default());
    let agent = Agent::new(
        Arc::new(gateway),
        store.clone(),
        kernel.bus().clone(),
        limits,
    );
    Rig {
        agent,
        kernel,
        chat,
        store,
        audit,
        events,
        probe,
        notes,
    }
}

#[derive(Clone, Default)]
pub struct Capture(pub std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = Capture;
    fn make_writer(&'a self) -> Capture {
        self.clone()
    }
}

pub fn keep_callsites_interested() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_writer(std::io::sink)
                .finish(),
        );
    });
}
