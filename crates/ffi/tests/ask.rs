//! The provider key calls of `NavoriCore`, driven with a fake Keychain.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use navori_config::Profile;
use navori_context::{SendError, Sender};
use navori_ffi::{AiDeps, CoreConfig, CoreError, CoreSnapshot, NavoriCore, SnapshotListener};
use navori_protocol::rpc;
use navori_secrets::fake::{FakeKeychain, MemoryStore};
use navori_secrets::{KeychainService, Operation, Secret, SecretKey, SecretStore};

struct Quiet;

impl SnapshotListener for Quiet {
    fn on_snapshot(&self, _: CoreSnapshot) {}
}

struct StubSender;

impl Sender for StubSender {
    fn send<'a>(
        &'a self,
        _: &'a Secret,
        _: &'a str,
        _: &'a str,
    ) -> BoxFuture<'a, Result<String, SendError>> {
        Box::pin(async { Ok("stub".to_owned()) })
    }
}

/// A short home path so the socket stays under the 104-byte `sun_path` limit.
fn home() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap()
}

fn config(home: &tempfile::TempDir, profile: &str) -> CoreConfig {
    CoreConfig {
        profile: profile.into(),
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

/// A fake Keychain whose factory counts the opens and remembers the service each one asked for.
struct Fixture {
    vault: FakeKeychain,
    opens: Arc<AtomicUsize>,
    services: Arc<Mutex<Vec<&'static str>>>,
}

impl Fixture {
    fn new() -> Self {
        captured_tracing();
        Self {
            vault: FakeKeychain::new(),
            opens: Arc::new(AtomicUsize::new(0)),
            services: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn deps(&self) -> AiDeps {
        let vault = self.vault.clone();
        let opens = self.opens.clone();
        let services = self.services.clone();
        AiDeps {
            open_secrets: Arc::new(move |service| {
                opens.fetch_add(1, Ordering::SeqCst);
                services.lock().unwrap().push(service.as_str());
                let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(service, &vault));
                Ok(store)
            }),
            make_sender: Arc::new(|| Arc::new(StubSender)),
        }
    }

    fn core(&self, home: &tempfile::TempDir, profile: &str) -> Arc<NavoriCore> {
        NavoriCore::start_with(config(home, profile), Arc::new(Quiet), self.deps()).unwrap()
    }

    fn opened(&self) -> usize {
        self.opens.load(Ordering::SeqCst)
    }

    fn stored_in(&self, service: KeychainService) -> Option<String> {
        MemoryStore::new(service, &self.vault)
            .get(&slot())
            .unwrap()
            .map(|secret| secret.expose().to_owned())
    }
}

fn slot() -> SecretKey {
    SecretKey::new("llm-provider", "deepseek").unwrap()
}

#[test]
fn the_profile_maps_to_its_keychain_service() {
    assert_eq!(
        navori_ffi::service_for(Profile::Release).as_str(),
        "dev.navori"
    );
    assert_eq!(
        navori_ffi::service_for(Profile::Dev).as_str(),
        "dev.navori.dev"
    );
}

#[test]
fn a_saved_key_is_stored_trimmed_under_the_profile_service() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");

    futures::executor::block_on(core.save_provider_key("  k1\n".into())).unwrap();

    assert_eq!(
        fixture.stored_in(KeychainService::DEV).as_deref(),
        Some("k1")
    );
    assert_eq!(fixture.stored_in(KeychainService::RELEASE), None);
    assert_eq!(fixture.vault.item_count(), 1);
}

#[test]
fn each_profile_opens_its_own_service() {
    let dev_fixture = Fixture::new();
    let release_fixture = Fixture::new();
    let (dev_home, release_home) = (home(), home());
    let dev = dev_fixture.core(&dev_home, "dev");
    let release = release_fixture.core(&release_home, "release");

    futures::executor::block_on(dev.save_provider_key("k1".into())).unwrap();
    futures::executor::block_on(release.save_provider_key("k1".into())).unwrap();

    assert_eq!(*dev_fixture.services.lock().unwrap(), ["dev.navori.dev"]);
    assert_eq!(*release_fixture.services.lock().unwrap(), ["dev.navori"]);
}

#[test]
fn saving_replaces_the_earlier_key() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");

    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();
    futures::executor::block_on(core.save_provider_key("k2".into())).unwrap();

    assert_eq!(
        fixture.stored_in(KeychainService::DEV).as_deref(),
        Some("k2")
    );
    assert_eq!(fixture.vault.item_count(), 1);
    assert_eq!(fixture.opened(), 1);
}

#[test]
fn an_empty_key_makes_no_keychain_call() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");

    let err = futures::executor::block_on(core.save_provider_key("   ".into())).unwrap_err();

    let CoreError::Secrets { message } = err else {
        panic!("expected a Keychain error, got {err:?}");
    };
    assert!(message.contains("Nothing was entered"), "{message}");
    assert_eq!(fixture.opened(), 0);
    assert_eq!(fixture.vault.item_count(), 0);
}

#[test]
fn a_keychain_failure_reaches_the_message() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");
    fixture.vault.fail_next(Operation::Set);

    let err = futures::executor::block_on(core.save_provider_key("k1".into())).unwrap_err();

    let CoreError::Secrets { message } = err else {
        panic!("expected a Keychain error, got {err:?}");
    };
    assert_eq!(
        message,
        "set failed for llm-provider/deepseek in service dev.navori.dev; the Keychain could not complete the call (it may be locked, access may be denied, or the item may be unreadable), so check Keychain Access and retry"
    );
    assert!(!message.contains("no key"));
}

#[test]
fn a_failed_remove_reaches_the_message() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");
    fixture.vault.fail_next(Operation::Delete);

    let err = futures::executor::block_on(core.remove_provider_key()).unwrap_err();

    let CoreError::Secrets { message } = err else {
        panic!("expected a Keychain error, got {err:?}");
    };
    assert!(
        message.starts_with("delete failed for llm-provider/deepseek"),
        "{message}"
    );
}

#[test]
fn remove_reports_whether_a_key_was_stored() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");
    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();

    assert_eq!(
        futures::executor::block_on(core.remove_provider_key()),
        Ok(true)
    );
    assert_eq!(
        futures::executor::block_on(core.remove_provider_key()),
        Ok(false)
    );
    assert_eq!(fixture.vault.item_count(), 0);
}

#[test]
fn remove_leaves_the_other_profiles_key() {
    let fixture = Fixture::new();
    let (dev_home, release_home) = (home(), home());
    let dev = fixture.core(&dev_home, "dev");
    let release = fixture.core(&release_home, "release");
    futures::executor::block_on(release.save_provider_key("k1".into())).unwrap();

    assert_eq!(
        futures::executor::block_on(dev.remove_provider_key()),
        Ok(false)
    );

    assert_eq!(
        fixture.stored_in(KeychainService::RELEASE).as_deref(),
        Some("k1")
    );
}

#[test]
fn starting_the_core_opens_no_keychain() {
    let fixture = Fixture::new();
    let home = home();
    let _core = fixture.core(&home, "dev");

    assert_eq!(fixture.opened(), 0);
}

#[test]
fn a_failed_open_is_retried_by_the_next_call() {
    let opens = Arc::new(AtomicUsize::new(0));
    let vault = FakeKeychain::new();
    let deps = AiDeps {
        open_secrets: {
            let (opens, vault) = (opens.clone(), vault.clone());
            Arc::new(move |service| {
                if opens.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(navori_secrets::OpenError);
                }
                let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(service, &vault));
                Ok(store)
            })
        },
        make_sender: Arc::new(|| Arc::new(StubSender)),
    };
    let home = home();
    let core = NavoriCore::start_with(config(&home, "dev"), Arc::new(Quiet), deps).unwrap();

    assert!(futures::executor::block_on(core.save_provider_key("k1".into())).is_err());
    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();

    assert_eq!(vault.item_count(), 1);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
}

#[test]
fn the_key_calls_work_from_a_non_tokio_executor() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");

    futures::executor::block_on(async {
        core.save_provider_key("k1".into()).await.unwrap();
        assert_eq!(core.remove_provider_key().await, Ok(true));
    });
}

#[test]
fn the_key_text_reaches_no_log_config_or_database() {
    let planted = concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123456789");
    let reply_marker = concat!("ghp_", "ZYXWVUTSRQPONMLKJIHGFEDCBA9876543210");
    let fixture = Fixture::new();
    let home = home();
    let data = home.path().join("Library/Application Support/Navori Dev");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("config.toml"), "# nothing configured\n").unwrap();
    let core = fixture.core_sending(&home, Recording::replying(reply_marker));

    futures::executor::block_on(core.save_provider_key(planted.into())).unwrap();
    let answered = ask(&core, &format!("review {planted} now")).unwrap();
    assert_eq!(answered, serde_json::json!({"reply": reply_marker}));
    core.shutdown();

    let logged = tracing_output();
    assert!(!logged.is_empty());
    for secret in [planted, reply_marker] {
        assert!(!logged.contains(secret), "tracing output holds {secret}");
    }
    let mut searched = 0;
    for entry in std::fs::read_dir(&data).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(&path).unwrap();
            for secret in [planted, reply_marker] {
                assert!(
                    !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                    "{} holds {secret}",
                    path.display()
                );
            }
            searched += 1;
        }
    }
    assert!(searched >= 2, "searched {searched} files");
    let columns: String = open_store(&home).with_conn(|conn| {
        conn.query_row(
            "SELECT group_concat(ts_ms || provider || kinds || redacted_count || outcome, '|') FROM context_audit",
            [],
            |row| row.get(0),
        )
        .unwrap()
    });
    assert!(!columns.is_empty());
    for secret in [planted, reply_marker] {
        assert!(!columns.contains(secret));
    }
}

struct Recording {
    texts: Mutex<Vec<String>>,
    reply: &'static str,
}

impl Recording {
    fn replying(reply: &'static str) -> Arc<Self> {
        Arc::new(Self {
            texts: Mutex::new(Vec::new()),
            reply,
        })
    }

    fn seen(&self) -> Vec<String> {
        self.texts.lock().unwrap().clone()
    }
}

impl Sender for Recording {
    fn send<'a>(
        &'a self,
        _: &'a Secret,
        _: &'a str,
        text: &'a str,
    ) -> BoxFuture<'a, Result<String, SendError>> {
        self.texts.lock().unwrap().push(text.to_owned());
        Box::pin(async { Ok(self.reply.to_owned()) })
    }
}

impl Fixture {
    fn deps_sending(&self, sender: Arc<dyn Sender>) -> AiDeps {
        AiDeps {
            make_sender: Arc::new(move || sender.clone()),
            ..self.deps()
        }
    }

    fn core_sending(&self, home: &tempfile::TempDir, sender: Arc<dyn Sender>) -> Arc<NavoriCore> {
        NavoriCore::start_with(
            config(home, "dev"),
            Arc::new(Quiet),
            self.deps_sending(sender),
        )
        .unwrap()
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn call(
    core: &NavoriCore,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, navori_ipc::IpcError> {
    let socket = core.status().socket_path;
    runtime().block_on(async {
        let mut client = navori_ipc::Client::connect(std::path::Path::new(&socket)).await?;
        client.call(method, params).await
    })
}

fn ask(core: &NavoriCore, text: &str) -> Result<serde_json::Value, navori_ipc::IpcError> {
    call(
        core,
        rpc::method::ASK,
        serde_json::json!({"model": "m", "text": text}),
    )
}

#[test]
fn core_ask_returns_the_reply_over_the_socket() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core_sending(&home, Recording::replying("stub reply"));
    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();

    let reply = ask(&core, "hello").unwrap();

    assert_eq!(reply, serde_json::json!({"reply": "stub reply"}));
    core.shutdown();
}

const PLANTED: &str = concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123456789");

fn data_dir(home: &tempfile::TempDir) -> std::path::PathBuf {
    home.path().join("Library/Application Support/Navori Dev")
}

fn open_store(home: &tempfile::TempDir) -> navori_store::Store {
    let data = data_dir(home);
    navori_store::Store::open(&data.join("navori.db"), &data.join("backups")).unwrap()
}

fn audit_rows(store: &navori_store::Store) -> Vec<(String, String, i64, String)> {
    store.with_conn(|conn| {
        let mut statement = conn
            .prepare(
                "SELECT provider, kinds, redacted_count, outcome FROM context_audit ORDER BY id",
            )
            .unwrap();
        statement
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect()
    })
}

#[test]
fn the_provider_sees_only_redacted_text_and_the_audit_row_has_no_content() {
    let fixture = Fixture::new();
    let home = home();
    let sender = Recording::replying("stub reply");
    let core = fixture.core_sending(&home, sender.clone());
    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();

    ask(&core, &format!("please review {PLANTED} today")).unwrap();

    assert_eq!(
        sender.seen(),
        ["please review [REDACTED:github_token] today"]
    );
    let store = open_store(&home);
    assert_eq!(
        audit_rows(&store),
        [(
            "deepseek".to_owned(),
            "text".to_owned(),
            1,
            "authorized".to_owned()
        )]
    );
    let everything: String = store.with_conn(|conn| {
        conn.query_row(
            "SELECT group_concat(ts_ms || provider || kinds || redacted_count || outcome, '|') FROM context_audit",
            [],
            |row| row.get(0),
        )
        .unwrap()
    });
    for content in ["please review", "REDACTED", "stub reply", PLANTED] {
        assert!(!everything.contains(content), "{everything}");
    }
    core.shutdown();
}

#[test]
fn the_audit_row_strings_pass_through_the_shield() {
    use navori_context::{AuditRow, AuditSink, Outcome};
    let store = navori_store::Store::open_in_memory().unwrap();
    let audit = navori_ffi::StoreAudit::new(store.clone());

    audit
        .record(AuditRow {
            ts_ms: 1,
            provider: PLANTED.to_owned(),
            kinds: vec![PLANTED.to_owned()],
            redacted_count: 0,
            outcome: Outcome::Authorized,
        })
        .unwrap();

    assert_eq!(
        audit_rows(&store),
        [(
            "[REDACTED:github_token]".to_owned(),
            "[REDACTED:github_token]".to_owned(),
            0,
            "authorized".to_owned()
        )]
    );
}

fn captured_tracing() -> &'static Mutex<Vec<u8>> {
    use std::sync::OnceLock;
    static BUFFER: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    BUFFER.get_or_init(|| {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || Sink(writer.clone()))
            .finish();
        tracing::subscriber::set_global_default(subscriber).unwrap();
        buffer
    })
}

fn tracing_output() -> String {
    String::from_utf8_lossy(&captured_tracing().lock().unwrap()).into_owned()
}

fn rpc_error(result: Result<serde_json::Value, navori_ipc::IpcError>) -> rpc::Error {
    match result {
        Err(navori_ipc::IpcError::Rpc(error)) => error,
        other => panic!("expected an rpc error, got {other:?}"),
    }
}

#[test]
fn the_audit_table_exists_only_after_the_first_ask() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core_sending(&home, Recording::replying("r"));
    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();
    let store = open_store(&home);
    let has_table = || -> bool {
        store.with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'context_audit'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
                == 1
        })
    };
    assert!(!has_table());
    assert_eq!(store.schema_version("context").unwrap(), 0);

    ask(&core, "hello").unwrap();

    assert!(has_table());
    assert_eq!(store.schema_version("context").unwrap(), 1);
    core.shutdown();
}

#[test]
fn ask_error_to_rpc_maps_every_variant_to_its_code() {
    use navori_context::AskError;
    let table = [
        (
            AskError::NoKey,
            -32004,
            "No key set. Add one in Settings → AI.",
        ),
        (AskError::Keychain("locked".into()), -32005, "locked"),
        (
            AskError::Shield,
            -32006,
            "The Shield failed, so nothing was sent.",
        ),
        (
            AskError::Audit,
            -32006,
            "The audit log could not be recorded, so nothing was sent.",
        ),
        (AskError::Send("status 500".into()), -32007, "status 500"),
    ];
    for (error, code, message) in table {
        let mapped = navori_ffi::ask_error_to_rpc(error);
        assert_eq!((mapped.code, mapped.message.as_str()), (code, message));
    }
}

#[test]
fn no_key_keychain_failure_and_send_failure_return_their_codes() {
    struct Failing;
    impl Sender for Failing {
        fn send<'a>(
            &'a self,
            _: &'a Secret,
            _: &'a str,
            _: &'a str,
        ) -> BoxFuture<'a, Result<String, SendError>> {
            Box::pin(async { Err(SendError("provider said no".to_owned())) })
        }
    }
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core_sending(&home, Arc::new(Failing));

    let no_key = rpc_error(ask(&core, "hello"));
    assert_eq!(
        (no_key.code, no_key.message.as_str()),
        (-32004, "No key set. Add one in Settings → AI.")
    );

    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();
    fixture.vault.fail_next(Operation::Get);
    let keychain = rpc_error(ask(&core, "hello"));
    assert_eq!(keychain.code, -32005);
    assert!(
        keychain.message.starts_with("get failed"),
        "{}",
        keychain.message
    );

    let send = rpc_error(ask(&core, "hello"));
    assert_eq!(
        (send.code, send.message.as_str()),
        (-32007, "provider said no")
    );
    core.shutdown();
}

#[test]
fn an_unrecordable_audit_refuses_the_ask_with_one_content_free_log_line() {
    let fixture = Fixture::new();
    let home = home();
    let sender = Recording::replying("never sent");
    let core = fixture.core_sending(&home, sender.clone());
    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();
    open_store(&home).with_conn(|conn| {
        conn.execute_batch("CREATE TABLE context_audit (blocking INTEGER)")
            .unwrap();
    });

    let refused = rpc_error(ask(&core, &format!("review {PLANTED} now")));

    assert_eq!(
        (refused.code, refused.message.as_str()),
        (
            -32006,
            "The audit log could not be recorded, so nothing was sent."
        )
    );
    assert!(sender.seen().is_empty());
    let lines: Vec<String> = tracing_output()
        .lines()
        .filter(|line| line.contains("context audit write failed"))
        .map(str::to_owned)
        .collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(!lines[0].contains("review") && !lines[0].contains(PLANTED));
    core.shutdown();
}

fn responder(body: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut received = Vec::new();
            let mut chunk = [0u8; 4096];
            while let Ok(n) = stream.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                received.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&received).into_owned();
                let Some((head, rest)) = text.split_once("\r\n\r\n") else {
                    continue;
                };
                let length = head
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if rest.len() < length {
                    continue;
                }
                seen.lock().unwrap().push(text);
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
                break;
            }
        }
    });
    (url, requests)
}

#[test]
fn the_wire_body_holds_the_marker_not_the_secret() {
    let (url, requests) = responder(r#"{"choices":[{"message":{"content":"wire reply"}}]}"#);
    let fixture = Fixture::new();
    let home = home();
    let sender: Arc<dyn Sender> = Arc::new(navori_ffi::LlmSender::new(
        navori_llm::OpenAiCompat::new(url),
    ));
    let core = fixture.core_sending(&home, sender);
    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();

    let reply = ask(&core, &format!("review {PLANTED} now")).unwrap();

    assert_eq!(reply, serde_json::json!({"reply": "wire reply"}));
    let recorded = requests.lock().unwrap().clone();
    assert_eq!(recorded.len(), 1);
    assert!(
        recorded[0].contains("[REDACTED:github_token]"),
        "{}",
        recorded[0]
    );
    assert!(!recorded[0].contains(PLANTED));
    core.shutdown();
}

struct Gated {
    entered: Mutex<std::sync::mpsc::Sender<()>>,
    release: Mutex<Option<futures::channel::oneshot::Receiver<()>>>,
}

impl Sender for Gated {
    fn send<'a>(
        &'a self,
        _: &'a Secret,
        _: &'a str,
        _: &'a str,
    ) -> BoxFuture<'a, Result<String, SendError>> {
        let release = self.release.lock().unwrap().take().unwrap();
        self.entered.lock().unwrap().send(()).unwrap();
        Box::pin(async move {
            let _ = release.await;
            Ok("late reply".to_owned())
        })
    }
}

#[test]
fn status_answers_while_an_ask_is_pending() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = futures::channel::oneshot::channel();
    let gated = Arc::new(Gated {
        entered: Mutex::new(entered_tx),
        release: Mutex::new(Some(release_rx)),
    });
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core_sending(&home, gated);
    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();
    let pending = {
        let core = core.clone();
        std::thread::spawn(move || ask(&core, "hello"))
    };
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();

    let status = call(&core, rpc::method::STATUS, serde_json::Value::Null).unwrap();

    assert_eq!(status["profile"], "dev");
    assert!(!pending.is_finished());
    release_tx.send(()).unwrap();
    assert_eq!(
        pending.join().unwrap().unwrap(),
        serde_json::json!({"reply": "late reply"})
    );
    core.shutdown();
}

struct Counted {
    fixture: Fixture,
    senders: Arc<AtomicUsize>,
}

impl Counted {
    fn new() -> Self {
        Self {
            fixture: Fixture::new(),
            senders: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn deps(&self) -> AiDeps {
        let senders = self.senders.clone();
        AiDeps {
            make_sender: Arc::new(move || {
                senders.fetch_add(1, Ordering::SeqCst);
                Arc::new(StubSender)
            }),
            ..self.fixture.deps()
        }
    }

    fn built(&self) -> (usize, usize) {
        (self.fixture.opened(), self.senders.load(Ordering::SeqCst))
    }
}

#[test]
fn starting_the_core_builds_no_sender_and_no_client() {
    let counted = Counted::new();
    let home = home();
    let core =
        NavoriCore::start_with(config(&home, "dev"), Arc::new(Quiet), counted.deps()).unwrap();

    assert_eq!(counted.built(), (0, 0));
    core.shutdown();
}

#[test]
fn concurrent_first_asks_build_the_runtime_once() {
    let counted = Counted::new();
    let home = home();
    let core =
        NavoriCore::start_with(config(&home, "dev"), Arc::new(Quiet), counted.deps()).unwrap();
    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let askers: Vec<_> = (0..2)
        .map(|_| {
            let (core, barrier) = (core.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                ask(&core, "hello").unwrap()
            })
        })
        .collect();

    for asker in askers {
        assert_eq!(asker.join().unwrap(), serde_json::json!({"reply": "stub"}));
    }

    assert_eq!(counted.built(), (1, 1));
    core.shutdown();
}

#[test]
fn concurrent_first_saves_open_the_keychain_once() {
    let counted = Counted::new();
    let home = home();
    let core =
        NavoriCore::start_with(config(&home, "dev"), Arc::new(Quiet), counted.deps()).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let savers: Vec<_> = (0..2)
        .map(|_| {
            let (core, barrier) = (core.clone(), barrier.clone());
            std::thread::spawn(move || {
                barrier.wait();
                futures::executor::block_on(core.save_provider_key("k1".into()))
            })
        })
        .collect();

    for saver in savers {
        saver.join().unwrap().unwrap();
    }

    assert_eq!(counted.built(), (1, 0));
    core.shutdown();
}

#[test]
fn a_failed_build_is_retried_on_the_next_ask() {
    let opens = Arc::new(AtomicUsize::new(0));
    let senders = Arc::new(AtomicUsize::new(0));
    let vault = FakeKeychain::new();
    let deps = AiDeps {
        open_secrets: {
            let (opens, vault) = (opens.clone(), vault.clone());
            Arc::new(move |service| {
                if opens.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(navori_secrets::OpenError);
                }
                let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(service, &vault));
                Ok(store)
            })
        },
        make_sender: {
            let senders = senders.clone();
            Arc::new(move || {
                senders.fetch_add(1, Ordering::SeqCst);
                Arc::new(StubSender)
            })
        },
    };
    let home = home();
    let core = NavoriCore::start_with(config(&home, "dev"), Arc::new(Quiet), deps).unwrap();

    assert_eq!(rpc_error(ask(&core, "hello")).code, -32005);
    assert_eq!(rpc_error(ask(&core, "hello")).code, -32004);

    assert_eq!(opens.load(Ordering::SeqCst), 2);
    assert_eq!(senders.load(Ordering::SeqCst), 1);
    core.shutdown();
}

#[test]
fn existing_methods_still_answer() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");

    let status = call(&core, rpc::method::STATUS, serde_json::Value::Null).unwrap();
    assert_eq!(status["profile"], "dev");
    let actions = call(&core, rpc::method::ACTIONS, serde_json::Value::Null).unwrap();
    assert!(actions.as_array().is_some_and(|a| !a.is_empty()));
    let events = call(&core, rpc::method::EVENTS, serde_json::json!({"limit": 5})).unwrap();
    assert!(events.is_array());
    let doctor = call(&core, rpc::method::DOCTOR, serde_json::Value::Null).unwrap();
    assert!(doctor.is_array() || doctor.is_object());
    let unknown = rpc_error(call(
        &core,
        rpc::method::DISPATCH,
        serde_json::json!({"module": "nope", "action": "ping"}),
    ));
    assert_eq!(unknown.code, rpc::code::UNKNOWN_MODULE);
    core.shutdown();
}

#[test]
fn an_unknown_method_is_still_rejected() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");

    let rejected = rpc_error(call(&core, "core.nope", serde_json::Value::Null));

    assert_eq!(rejected.code, rpc::code::METHOD_NOT_FOUND);
    core.shutdown();
}

#[test]
fn ask_params_that_do_not_decode_are_invalid() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");

    let rejected = rpc_error(call(
        &core,
        rpc::method::ASK,
        serde_json::json!({"model": 3}),
    ));

    assert_eq!(rejected.code, rpc::code::INVALID_PARAMS);
    core.shutdown();
}
