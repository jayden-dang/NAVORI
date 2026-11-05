//! Navori AI Turns through `NavoriCore::run_agent_turn`, with a scripted provider and a fake Keychain.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use navori_ffi::{
    AiDeps, ConversationId, CoreConfig, CoreSnapshot, NavoriCore, Origin, SnapshotListener,
    TurnEnd, TurnRequest,
};
use navori_llm::fake::FakeLlm;
use navori_llm::{ChatReply, Finish, LlmProvider, Message, ToolCall};
use navori_secrets::SecretStore;
use navori_secrets::fake::{FakeKeychain, MemoryStore};
use tokio_util::sync::CancellationToken;

struct Quiet;

impl SnapshotListener for Quiet {
    fn on_snapshot(&self, _: CoreSnapshot) {}
}

fn home() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap()
}

fn config(home: &tempfile::TempDir) -> CoreConfig {
    CoreConfig {
        profile: "dev".into(),
        home_dir: home.path().to_string_lossy().into_owned(),
        uid: std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(home.path()).unwrap()),
        app_version: "0.1.0-test".into(),
        log_to_file: false,
        helper_path: home
            .path()
            .join("Navori Dev.app/Contents/Helpers/navori")
            .to_string_lossy()
            .into_owned(),
    }
}

fn text_reply(text: &str) -> ChatReply {
    ChatReply {
        text: Some(text.to_owned()),
        reasoning: None,
        tool_calls: vec![],
        usage: None,
        finish: Finish::Stop,
    }
}

fn call_reply(id: &str, name: &str, arguments: &str) -> ChatReply {
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

fn deps(vault: &FakeKeychain, fake: &Arc<FakeLlm>) -> AiDeps {
    let vault = vault.clone();
    let fake = fake.clone();
    AiDeps {
        open_secrets: Arc::new(move |service| {
            let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(service, &vault));
            Ok(store)
        }),
        make_provider: Arc::new(move || {
            let provider: Arc<dyn LlmProvider> = fake.clone();
            provider
        }),
        ..AiDeps::production()
    }
}

fn started(home: &tempfile::TempDir, deps: AiDeps) -> Arc<NavoriCore> {
    let core = NavoriCore::start_with(config(home), Arc::new(Quiet), deps).unwrap();
    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();
    core
}

fn ask_navori_ai(core: &NavoriCore, input: &str) -> navori_ffi::TurnOutcome {
    let request = TurnRequest {
        conversation: None,
        origin: Origin::User,
        model: "deepseek-chat".into(),
        input: input.into(),
    };
    futures::executor::block_on(core.run_agent_turn(request, CancellationToken::new())).unwrap()
}

#[test]
fn navori_ai_answers_which_ports_are_open_from_ports_list() {
    let home = home();
    let fake = Arc::new(FakeLlm::new(vec![
        Ok(call_reply("c1", "ports_list", "{}")),
        Ok(text_reply("Nothing special is listening.")),
    ]));
    let core = started(&home, deps(&FakeKeychain::new(), &fake));

    let outcome = ask_navori_ai(&core, "Which ports are open?");

    assert_eq!(outcome.end, TurnEnd::Completed);
    assert_eq!(
        outcome.answer.as_deref(),
        Some("Nothing special is listening.")
    );
    let requests = fake.requests();
    assert_eq!(requests.len(), 2);
    assert!(matches!(
        requests[1].messages.last(),
        Some(Message::Tool { call_id, .. }) if call_id == "c1"
    ));
    let offered: Vec<&str> = requests[0].tools.iter().map(|t| t.name.as_str()).collect();
    assert!(offered.contains(&"ports_list"), "{offered:?}");
    for action in core.actions().iter().filter(|a| a.risk != "read") {
        let name = format!("{}_{}", action.module, action.name);
        assert!(!offered.contains(&name.as_str()), "{name} offered");
    }
    core.shutdown();
}

fn data_dir(home: &tempfile::TempDir) -> std::path::PathBuf {
    home.path().join("Library/Application Support/Navori Dev")
}

fn open_store(home: &tempfile::TempDir) -> navori_store::Store {
    let data = data_dir(home);
    navori_store::Store::open(&data.join("navori.db"), &data.join("backups")).unwrap()
}

fn texts(store: &navori_store::Store, sql: &str) -> Vec<String> {
    store.with_conn(|conn| {
        let mut statement = conn.prepare(sql).unwrap();
        statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    })
}

#[test]
fn ports_kill_is_refused_and_the_ports_module_receives_nothing() {
    let home = home();
    let fake = Arc::new(FakeLlm::new(vec![
        Ok(call_reply("c1", "ports_kill", r#"{"selector":{"pid":1}}"#)),
        Ok(text_reply("I cannot stop that.")),
    ]));
    let core = started(&home, deps(&FakeKeychain::new(), &fake));

    let outcome = ask_navori_ai(&core, "Kill whatever listens on 3000.");

    assert_eq!(outcome.end, TurnEnd::Completed);
    let store = open_store(&home);
    assert!(
        texts(
            &store,
            "SELECT kind FROM events WHERE kind = 'ports.stopped'"
        )
        .is_empty(),
        "ports.stopped was journaled"
    );
    let results = texts(
        &store,
        "SELECT body FROM ai_session_message WHERE kind = 'tool_result'",
    );
    assert_eq!(results.len(), 1);
    assert!(
        results[0].contains(
            "Error: Navori AI cannot run ports_kill yet: it changes things and needs your approval, which is not available."
        ),
        "{results:?}"
    );
    core.shutdown();
}

#[test]
fn a_three_step_turn_leaves_three_authorized_audit_rows() {
    let home = home();
    let fake = Arc::new(FakeLlm::new(vec![
        Ok(call_reply("c1", "ports_list", "{}")),
        Ok(call_reply("c2", "system_ping", "{}")),
        Ok(text_reply("Both answered.")),
    ]));
    let core = started(&home, deps(&FakeKeychain::new(), &fake));

    let outcome = ask_navori_ai(&core, "Check the ports and ping.");

    assert_eq!(outcome.steps, 3);
    let rows = texts(
        &open_store(&home),
        "SELECT provider || ':' || outcome FROM context_audit ORDER BY id",
    );
    assert_eq!(rows, vec!["deepseek:authorized"; 3]);
    core.shutdown();
}

#[test]
fn the_turn_is_read_back_from_the_ai_session_tables() {
    let home = home();
    let fake = Arc::new(FakeLlm::new(vec![
        Ok(call_reply("c1", "system_ping", "{}")),
        Ok(text_reply("Pong received.")),
    ]));
    let core = started(&home, deps(&FakeKeychain::new(), &fake));

    let outcome = ask_navori_ai(&core, "Ping.");

    let store = open_store(&home);
    assert_eq!(
        texts(&store, "SELECT id FROM ai_session_conversation"),
        vec![outcome.conversation.to_string()]
    );
    assert_eq!(
        texts(
            &store,
            "SELECT seq || '|' || origin || '|' || model || '|' || end_reason FROM ai_session_turn"
        ),
        vec!["1|user|deepseek-chat|completed"]
    );
    assert_eq!(
        texts(
            &store,
            "SELECT seq || '|' || ifnull(ended_ms IS NOT NULL, 0) FROM ai_session_step ORDER BY seq"
        ),
        vec!["0|1", "1|1"]
    );
    assert_eq!(
        texts(
            &store,
            "SELECT seq || '|' || ifnull(step, '-') || '|' || kind || '|' || ifnull(call_id, '-') FROM ai_session_message ORDER BY seq"
        ),
        vec![
            "0|-|input|-",
            "1|0|assistant|-",
            "2|0|tool_result|c1",
            "3|1|assistant|-"
        ]
    );
    assert_eq!(
        texts(&store, "SELECT body FROM ai_session_message WHERE seq = 0"),
        vec![r#"{"v":1,"text":"Ping."}"#]
    );
    assert_eq!(
        texts(
            &store,
            "SELECT step || '|' || reported FROM ai_session_usage ORDER BY step"
        ),
        vec!["0|0", "1|0"]
    );
    core.shutdown();
}

#[test]
fn the_journal_holds_one_ai_turn_ended_event() {
    let home = home();
    let fake = Arc::new(FakeLlm::new(vec![Ok(text_reply("Hello."))]));
    let core = started(&home, deps(&FakeKeychain::new(), &fake));

    let outcome = ask_navori_ai(&core, "Hi.");

    let events = texts(
        &open_store(&home),
        "SELECT module || '|' || summary || '|' || payload FROM events WHERE kind = 'ai.turn_ended'",
    );
    assert_eq!(events.len(), 1, "{events:?}");
    let (head, payload) = events[0].split_once("|Navori AI").unwrap();
    assert_eq!(head, "ai");
    let (summary, payload) = payload.split_once('|').unwrap();
    assert_eq!(summary, " finished a Turn: completed after 1 Steps");
    let payload: serde_json::Value = serde_json::from_str(payload).unwrap();
    assert_eq!(
        payload,
        serde_json::json!({
            "conversation": outcome.conversation.to_string(),
            "turn": outcome.turn.to_string(),
            "origin": "user",
            "end": "completed",
            "steps": 1
        })
    );
    core.shutdown();
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("condition not reached in time");
}

fn turn_request(conversation: Option<ConversationId>, input: &str) -> TurnRequest {
    TurnRequest {
        conversation,
        origin: Origin::User,
        model: "deepseek-chat".into(),
        input: input.into(),
    }
}

fn shut_down_while_the_second_request_is_held(home: &tempfile::TempDir) -> ConversationId {
    let fake = Arc::new(FakeLlm::new(vec![
        Ok(call_reply("c1", "system_ping", "{}")),
        Ok(text_reply("never delivered")),
    ]));
    let first = fake.hold_next();
    let _second = fake.hold_next();
    let core = started(home, deps(&FakeKeychain::new(), &fake));
    let pending = {
        let core = core.clone();
        std::thread::spawn(move || {
            futures::executor::block_on(
                core.run_agent_turn(turn_request(None, "Ping."), CancellationToken::new()),
            )
        })
    };
    wait_until(|| fake.requests().len() == 1);
    first.release();
    wait_until(|| fake.requests().len() == 2);

    core.shutdown();

    assert_eq!(
        pending.join().unwrap().unwrap_err(),
        navori_ffi::TurnStartError::ShutDown
    );
    let ids = texts(&open_store(home), "SELECT id FROM ai_session_conversation");
    assert_eq!(ids.len(), 1);
    ids[0].parse().unwrap()
}

#[test]
fn a_turn_cut_short_by_shutdown_is_repaired_after_restart() {
    let home = home();
    let conversation = shut_down_while_the_second_request_is_held(&home);
    let fake = Arc::new(FakeLlm::new(vec![Ok(text_reply("Welcome back."))]));
    let core = started(&home, deps(&FakeKeychain::new(), &fake));

    let outcome = futures::executor::block_on(core.run_agent_turn(
        turn_request(Some(conversation), "Still there?"),
        CancellationToken::new(),
    ))
    .unwrap();

    assert_eq!(outcome.conversation, conversation);
    assert_eq!(outcome.end, TurnEnd::Completed);
    let store = open_store(&home);
    assert_eq!(
        texts(
            &store,
            "SELECT seq || '|' || end_reason FROM ai_session_turn ORDER BY seq"
        ),
        vec!["1|interrupted", "2|completed"]
    );
    assert_eq!(
        texts(
            &store,
            "SELECT call_id FROM ai_session_message WHERE kind = 'tool_result' ORDER BY seq"
        ),
        vec!["c1"]
    );
    assert_eq!(
        texts(
            &store,
            "SELECT json_extract(payload, '$.end') FROM events WHERE kind = 'ai.turn_ended' ORDER BY 1"
        ),
        vec!["completed", "interrupted"]
    );
    let replayed = fake.requests();
    assert!(matches!(
        &replayed[0].messages[..],
        [
            Message::User { .. },
            Message::Assistant { .. },
            Message::Tool { .. },
            Message::User { .. }
        ]
    ));
    core.shutdown();
}

#[test]
fn a_repaired_turn_shows_a_real_end_time() {
    let home = home();
    let conversation = shut_down_while_the_second_request_is_held(&home);
    let fake = Arc::new(FakeLlm::new(vec![Ok(text_reply("Welcome back."))]));
    let core = started(&home, deps(&FakeKeychain::new(), &fake));
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();

    futures::executor::block_on(core.run_agent_turn(
        turn_request(Some(conversation), "Still there?"),
        CancellationToken::new(),
    ))
    .unwrap();

    let store = open_store(&home);
    let ended = texts(
        &store,
        "SELECT CAST(ended_ms AS TEXT) FROM ai_session_turn WHERE end_reason = 'interrupted'",
    );
    let step_ended = texts(
        &store,
        "SELECT CAST(ended_ms AS TEXT) FROM ai_session_step WHERE seq = 1",
    );
    for stamp in ended.iter().chain(&step_ended) {
        assert!(
            stamp.parse::<u128>().unwrap() >= before,
            "{stamp} < {before}"
        );
    }
    assert_eq!((ended.len(), step_ended.len()), (1, 1));
    core.shutdown();
}

#[test]
fn starting_the_core_builds_no_agent_and_runs_no_ai_session_migration() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let home = home();
    let fake = Arc::new(FakeLlm::new(vec![Ok(text_reply("Hello."))]));
    let secrets = Arc::new(AtomicUsize::new(0));
    let providers = Arc::new(AtomicUsize::new(0));
    let conversations = Arc::new(AtomicUsize::new(0));
    let senders = Arc::new(AtomicUsize::new(0));
    let base = deps(&FakeKeychain::new(), &fake);
    let counted = AiDeps {
        open_secrets: {
            let (open, count) = (base.open_secrets.clone(), secrets.clone());
            Arc::new(move |service| {
                count.fetch_add(1, Ordering::SeqCst);
                open(service)
            })
        },
        make_sender: {
            let (make, count) = (base.make_sender.clone(), senders.clone());
            Arc::new(move || {
                count.fetch_add(1, Ordering::SeqCst);
                make()
            })
        },
        make_provider: {
            let (make, count) = (base.make_provider.clone(), providers.clone());
            Arc::new(move || {
                count.fetch_add(1, Ordering::SeqCst);
                make()
            })
        },
        make_conversations: {
            let (make, count) = (base.make_conversations.clone(), conversations.clone());
            Arc::new(move |store| {
                count.fetch_add(1, Ordering::SeqCst);
                make(store)
            })
        },
    };
    let core = NavoriCore::start_with(config(&home), Arc::new(Quiet), counted).unwrap();

    let invoked = || {
        [&secrets, &providers, &conversations, &senders].map(|count| count.load(Ordering::SeqCst))
    };
    assert_eq!(open_store(&home).schema_version("ai_session").unwrap(), 0);
    assert!(fake.requests().is_empty());
    assert_eq!(invoked(), [0, 0, 0, 0]);

    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();
    ask_navori_ai(&core, "Hi.");

    let [secrets, providers, conversations, senders] = invoked();
    assert!(secrets >= 1);
    assert_eq!((providers, conversations, senders), (1, 1, 1));
    assert_eq!(open_store(&home).schema_version("ai_session").unwrap(), 1);
    core.shutdown();
}

const PARK_LIMIT: std::time::Duration = std::time::Duration::from_secs(30);

struct Gate {
    open: std::sync::Mutex<bool>,
    changed: std::sync::Condvar,
    parked: std::sync::atomic::AtomicUsize,
}

impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            open: std::sync::Mutex::new(false),
            changed: std::sync::Condvar::new(),
            parked: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    fn park(&self) {
        self.parked
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let open = self.open.lock().unwrap();
        let _ = self
            .changed
            .wait_timeout_while(open, PARK_LIMIT, |open| !*open)
            .unwrap();
    }

    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

struct GatedConversations {
    inner: navori_ffi::StoreConversations,
    gate: Arc<Gate>,
}

impl navori_ffi::ConversationStore for GatedConversations {
    fn append(&self, entries: &[navori_agent::Entry]) -> Result<(), navori_agent::RecordError> {
        self.gate.park();
        self.inner.append(entries)
    }

    fn load(
        &self,
        conversation: &ConversationId,
    ) -> Result<navori_agent::ConversationLog, navori_agent::RecordError> {
        self.inner.load(conversation)
    }
}

#[test]
fn status_answers_while_a_conversation_write_is_held() {
    let home = home();
    let gate = Gate::new();
    let fake = Arc::new(FakeLlm::new(vec![]));
    let deps = AiDeps {
        make_conversations: {
            let gate = gate.clone();
            Arc::new(move |store| {
                Arc::new(GatedConversations {
                    inner: navori_ffi::StoreConversations::new(store),
                    gate: gate.clone(),
                })
            })
        },
        ..deps(&FakeKeychain::new(), &fake)
    };
    let core = started(&home, deps);
    let turns: Vec<_> = (0..2)
        .map(|_| {
            let core = core.clone();
            std::thread::spawn(move || {
                futures::executor::block_on(
                    core.run_agent_turn(turn_request(None, "Hi."), CancellationToken::new()),
                )
            })
        })
        .collect();
    wait_until(|| gate.parked.load(std::sync::atomic::Ordering::SeqCst) >= 2);

    let socket = core.status().socket_path;
    let (answered_tx, answered_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let status = runtime.block_on(async {
            let mut client = navori_ipc::Client::connect(std::path::Path::new(&socket)).await?;
            client
                .call(
                    navori_protocol::rpc::method::STATUS,
                    serde_json::Value::Null,
                )
                .await
        });
        let _ = answered_tx.send(status.is_ok());
    });
    let answered = answered_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap_or(false);
    let still_pending = turns.iter().all(|turn| !turn.is_finished());

    gate.release();
    for turn in turns {
        let _ = turn.join().unwrap();
    }
    assert!(still_pending, "a Turn finished while its write was held");
    assert!(answered, "status did not answer while writes were held");
    core.shutdown();
}

#[test]
fn no_new_action_appears() {
    let home = home();
    let fake = Arc::new(FakeLlm::new(vec![]));
    let core = started(&home, deps(&FakeKeychain::new(), &fake));

    let actions: Vec<String> = core
        .actions()
        .into_iter()
        .map(|a| format!("{} {} {}", a.module, a.name, a.risk))
        .collect();

    assert_eq!(
        actions,
        vec![
            "system ping read",
            "system prune_journal write",
            "awake status read",
            "awake hold_for write",
            "awake hold_until_off write",
            "awake display write",
            "awake hold_command write",
            "awake hold_session write",
            "awake stop_session write",
            "awake end_session write",
            "awake release_all write",
            "awake keep_for write",
            "awake keep_indefinitely write",
            "awake keep_off write",
            "awake tracking write",
            "awake subagent_start write",
            "awake subagent_stop write",
            "integrations status read",
            "integrations install write",
            "integrations repair write",
            "integrations remove write",
            "ports list read",
            "ports open read",
            "ports kill destructive",
        ]
    );
    core.shutdown();
}

#[test]
fn the_store_refuses_a_second_result_for_a_call_and_keeps_nothing_of_the_failed_write() {
    use navori_agent::{Entry, RecordError, RecordedMessage, TurnId};
    use navori_ffi::ConversationStore;
    let store = navori_store::Store::open_in_memory().unwrap();
    let conversations = navori_ffi::StoreConversations::new(store);
    let conversation = ConversationId::new();
    let turn = TurnId::new();
    let result = |seq| Entry::Message {
        turn,
        seq,
        step: Some(0),
        body: RecordedMessage::ToolResult {
            call_id: "c1".into(),
            text: "ok".into(),
            is_error: false,
            synthetic: false,
        },
        redactions: 0,
        ts_ms: 1,
    };
    conversations
        .append(&[
            Entry::ConversationStarted {
                id: conversation,
                ts_ms: 1,
            },
            Entry::TurnStarted {
                id: turn,
                conversation,
                seq: 0,
                origin: Origin::User,
                model: "m".into(),
                ts_ms: 1,
            },
            result(1),
        ])
        .unwrap();

    let second = conversations.append(&[result(2)]);

    assert_eq!(second, Err(RecordError::Duplicate));
    let log = conversations.load(&conversation).unwrap();
    assert_eq!(log.turns[0].messages.len(), 1);
    assert_eq!(
        conversations.load(&ConversationId::new()).unwrap_err(),
        RecordError::UnknownConversation
    );
}

#[derive(Clone, Default)]
struct Capture(Arc<std::sync::Mutex<Vec<u8>>>);

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

impl Capture {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

fn capturing_logs() -> (Capture, tracing::subscriber::DefaultGuard) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_writer(std::io::sink)
                .finish(),
        );
    });
    let capture = Capture::default();
    let guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_writer(capture.clone())
            .with_ansi(false)
            .finish(),
    );
    (capture, guard)
}

#[test]
fn a_failed_conversation_write_logs_the_sqlite_code_and_none_of_the_text() {
    use navori_agent::{Entry, RecordError, RecordedMessage, TurnId};
    use navori_ffi::ConversationStore;
    let (logs, _guard) = capturing_logs();
    let conversations =
        navori_ffi::StoreConversations::new(navori_store::Store::open_in_memory().unwrap());

    let outcome = conversations.append(&[Entry::Message {
        turn: TurnId::new(),
        seq: 0,
        step: None,
        body: RecordedMessage::Input {
            text: "a private question".into(),
        },
        redactions: 0,
        ts_ms: 1,
    }]);

    assert_eq!(outcome, Err(RecordError::Write));
    let logs = logs.text();
    assert!(logs.contains("conversation write failed"), "{logs}");
    assert!(logs.contains("code"), "{logs}");
    assert!(!logs.contains("a private question"), "{logs}");
}
