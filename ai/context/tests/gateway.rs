#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io;
use std::panic::panic_any;
use std::sync::{Arc, Mutex, OnceLock};

use futures::future::BoxFuture;
use navori_context::{
    AskError, AuditError, AuditRow, AuditSink, Gateway, Outcome, SendError, Sender,
};
use navori_redact::{Finding, Redacted};
use navori_secrets::fake::{FakeKeychain, MemoryStore};
use navori_secrets::{
    Deleted, KeychainService, Operation, Secret, SecretError, SecretKey, SecretStore,
};

const STORED_KEY: &str = "stored-key-QQ7";
const PROMPT: &str = "please review this";

fn planted() -> String {
    concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123456789").to_owned()
}

type Log = Arc<Mutex<Vec<String>>>;

fn events(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

struct Received {
    key: String,
    model: String,
    text: String,
}

struct RecordingSender {
    log: Log,
    received: Arc<Mutex<Vec<Received>>>,
    reply: Result<String, String>,
}

impl Sender for RecordingSender {
    fn send<'a>(
        &'a self,
        key: &'a Secret,
        model: &'a str,
        text: &'a str,
    ) -> BoxFuture<'a, Result<String, SendError>> {
        self.log.lock().unwrap().push("send".to_owned());
        self.received.lock().unwrap().push(Received {
            key: key.expose().to_owned(),
            model: model.to_owned(),
            text: text.to_owned(),
        });
        let reply = self.reply.clone().map_err(SendError);
        Box::pin(async move { reply })
    }
}

#[derive(Clone, Copy)]
enum AuditMode {
    Record,
    Fail,
    Panic,
}

struct RecordingAudit {
    log: Log,
    rows: Arc<Mutex<Vec<AuditRow>>>,
    mode: AuditMode,
}

impl AuditSink for RecordingAudit {
    fn record(&self, row: AuditRow) -> Result<(), AuditError> {
        match self.mode {
            AuditMode::Panic => panic_any("audit sink exploded"),
            AuditMode::Fail => return Err(AuditError),
            AuditMode::Record => {}
        }
        let name = match row.outcome {
            Outcome::Authorized => "audit:authorized",
            Outcome::Blocked => "audit:blocked",
        };
        self.log.lock().unwrap().push(name.to_owned());
        self.rows.lock().unwrap().push(row);
        Ok(())
    }
}

struct PanickingStore;

impl SecretStore for PanickingStore {
    fn set(&self, _: &SecretKey, _: &Secret) -> Result<(), SecretError> {
        unreachable!()
    }
    fn get(&self, _: &SecretKey) -> Result<Option<Secret>, SecretError> {
        panic_any("keychain exploded")
    }
    fn exists(&self, _: &SecretKey) -> Result<bool, SecretError> {
        unreachable!()
    }
    fn delete(&self, _: &SecretKey) -> Result<Deleted, SecretError> {
        unreachable!()
    }
}

struct Harness {
    log: Log,
    received: Arc<Mutex<Vec<Received>>>,
    rows: Arc<Mutex<Vec<AuditRow>>>,
    vault: FakeKeychain,
    key: SecretKey,
    store: Arc<dyn SecretStore>,
    reply: Result<String, String>,
    audit_mode: AuditMode,
}

impl Harness {
    fn with_key() -> Self {
        let harness = Self::without_key();
        harness
            .store
            .set(&harness.key, &Secret::new(STORED_KEY))
            .unwrap();
        harness
    }

    fn without_key() -> Self {
        let vault = FakeKeychain::new();
        let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(KeychainService::DEV, &vault));
        Self {
            log: Arc::default(),
            received: Arc::default(),
            rows: Arc::default(),
            vault,
            key: SecretKey::new("provider", "deepseek").unwrap(),
            store,
            reply: Ok("a reply".to_owned()),
            audit_mode: AuditMode::Record,
        }
    }

    fn gateway(&self) -> Gateway {
        Gateway::new(
            self.store.clone(),
            self.key.clone(),
            self.sender(),
            self.audit(),
            "deepseek",
        )
    }

    fn gateway_with_shield(&self, shield: Shield) -> Gateway {
        Gateway::with_shield(
            self.store.clone(),
            self.key.clone(),
            self.sender(),
            self.audit(),
            "deepseek",
            shield,
        )
    }

    fn sender(&self) -> Arc<RecordingSender> {
        Arc::new(RecordingSender {
            log: self.log.clone(),
            received: self.received.clone(),
            reply: self.reply.clone(),
        })
    }

    fn audit(&self) -> Arc<RecordingAudit> {
        Arc::new(RecordingAudit {
            log: self.log.clone(),
            rows: self.rows.clone(),
            mode: self.audit_mode,
        })
    }
}

fn prompt_with_secret() -> String {
    format!("{PROMPT} {}", planted())
}

#[tokio::test]
async fn an_ask_reads_the_key_then_shields_then_audits_then_sends() {
    let harness = Harness::with_key();
    let text = prompt_with_secret();

    harness.gateway().ask("model-a", &text).await.unwrap();

    assert_eq!(events(&harness.log), ["audit:authorized", "send"]);
    let received = harness.received.lock().unwrap();
    assert_eq!(received[0].key, "stored-key-QQ7");
    assert_eq!(received[0].model, "model-a");
    assert_eq!(
        received[0].text,
        "please review this [REDACTED:github_token]"
    );
    let rows = harness.rows.lock().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].provider, "deepseek");
    assert_eq!(rows[0].kinds, ["text"]);
    assert_eq!(rows[0].redacted_count, 1);
    assert_eq!(rows[0].outcome, Outcome::Authorized);
    assert!(rows[0].ts_ms > 0);
    let fields = format!("{:?}", rows[0]);
    assert!(!fields.contains(PROMPT));
    assert!(!fields.contains(&planted()));
}

#[tokio::test]
async fn the_reply_comes_back_unchanged() {
    let mut harness = Harness::with_key();
    harness.reply = Ok(format!("your token is {}", planted()));

    let reply = harness.gateway().ask("m", PROMPT).await.unwrap();

    assert_eq!(reply, format!("your token is {}", planted()));
}

#[tokio::test]
async fn with_no_key_stored_nothing_is_inspected_or_audited() {
    let harness = Harness::without_key();

    let result = harness.gateway().ask("m", PROMPT).await;

    assert!(matches!(result, Err(AskError::NoKey)));
    assert!(events(&harness.log).is_empty());
}

#[tokio::test]
async fn a_keychain_failure_is_not_reported_as_no_key() {
    let harness = Harness::with_key();
    harness.vault.fail_next(Operation::Get);
    let expected = harness.store.get(&harness.key).unwrap_err().to_string();
    harness.vault.fail_next(Operation::Get);

    let error = harness.gateway().ask("m", PROMPT).await.unwrap_err();

    assert_eq!(error.to_string(), expected);
    assert!(matches!(error, AskError::Keychain(m) if m == expected));
    assert!(!expected.contains("No key set"));
    assert!(events(&harness.log).is_empty());
}

#[tokio::test]
async fn a_keychain_call_that_panics_is_reported_as_unfinished() {
    let mut harness = Harness::with_key();
    harness.store = Arc::new(PanickingStore);

    let error = harness.gateway().ask("m", PROMPT).await.unwrap_err();

    assert!(matches!(error, AskError::Keychain(m) if m == "the Keychain call did not finish"));
    assert!(events(&harness.log).is_empty());
}

type Shield = fn(&str) -> Redacted;

fn panicking_shield(_: &str) -> Redacted {
    panic_any("shield exploded")
}

fn finding(start: usize, end: usize) -> Finding {
    Finding {
        rule: "github_token",
        start,
        end,
    }
}

fn out_of_order_shield(text: &str) -> Redacted {
    Redacted {
        text: text.to_owned(),
        findings: vec![finding(10, 14), finding(0, 4)],
    }
}

fn overlapping_shield(text: &str) -> Redacted {
    Redacted {
        text: text.to_owned(),
        findings: vec![finding(0, 6), finding(4, 8)],
    }
}

fn out_of_bounds_shield(text: &str) -> Redacted {
    Redacted {
        text: text.to_owned(),
        findings: vec![finding(0, 1000)],
    }
}

fn splitting_a_character_shield(text: &str) -> Redacted {
    Redacted {
        text: text.to_owned(),
        findings: vec![finding(1, 2)],
    }
}

#[tokio::test]
async fn a_shield_panic_refuses_the_ask_and_records_a_blocked_row() {
    let harness = Harness::with_key();

    let result = harness
        .gateway_with_shield(panicking_shield)
        .ask("m", PROMPT)
        .await;

    assert!(matches!(result, Err(AskError::Shield)));
    assert_eq!(events(&harness.log), ["audit:blocked"]);
    let rows = harness.rows.lock().unwrap();
    assert_eq!(rows[0].outcome, Outcome::Blocked);
    assert_eq!(rows[0].provider, "deepseek");
    assert_eq!(rows[0].redacted_count, 0);
}

#[tokio::test]
async fn a_malformed_shield_result_is_refused() {
    let shields: [(&str, Shield); 4] = [
        ("out of order", out_of_order_shield),
        ("overlapping", overlapping_shield),
        ("out of bounds", out_of_bounds_shield),
        ("inside a character", splitting_a_character_shield),
    ];
    for (name, shield) in shields {
        let harness = Harness::with_key();

        let result = harness
            .gateway_with_shield(shield)
            .ask("m", "héllo some text to inspect")
            .await;

        assert!(matches!(result, Err(AskError::Shield)), "{name}");
        assert_eq!(events(&harness.log), ["audit:blocked"], "{name}");
    }
}

#[tokio::test]
async fn a_shield_failure_still_refuses_when_the_blocked_row_cannot_be_written() {
    let mut harness = Harness::with_key();
    harness.audit_mode = AuditMode::Fail;

    let result = harness
        .gateway_with_shield(panicking_shield)
        .ask("m", PROMPT)
        .await;

    assert!(matches!(result, Err(AskError::Shield)));
    assert!(events(&harness.log).is_empty());
}

#[tokio::test]
async fn a_failing_audit_write_stops_the_send() {
    let mut harness = Harness::with_key();
    harness.audit_mode = AuditMode::Fail;

    let result = harness.gateway().ask("m", PROMPT).await;

    assert!(matches!(result, Err(AskError::Audit)));
    assert!(events(&harness.log).is_empty());
}

#[tokio::test]
async fn a_panicking_audit_sink_stops_the_send() {
    let mut harness = Harness::with_key();
    harness.audit_mode = AuditMode::Panic;

    let result = harness.gateway().ask("m", PROMPT).await;

    assert!(matches!(result, Err(AskError::Audit)));
    assert!(events(&harness.log).is_empty());
}

#[tokio::test]
async fn a_failing_send_is_reported_with_its_message() {
    let mut harness = Harness::with_key();
    harness.reply = Err("the provider said no".to_owned());

    let error = harness.gateway().ask("m", PROMPT).await.unwrap_err();

    assert!(matches!(&error, AskError::Send(m) if m == "the provider said no"));
    assert_eq!(error.to_string(), "the provider said no");
    assert_eq!(events(&harness.log), ["audit:authorized", "send"]);
    assert_eq!(harness.rows.lock().unwrap()[0].outcome, Outcome::Authorized);
}

#[test]
fn each_failure_reads_as_its_user_facing_sentence() {
    assert_eq!(
        AskError::NoKey.to_string(),
        "No key set. Add one in Settings → AI."
    );
    assert_eq!(
        AskError::Shield.to_string(),
        "The Shield failed, so nothing was sent."
    );
    assert_eq!(
        AskError::Audit.to_string(),
        "The audit log could not be recorded, so nothing was sent."
    );
    assert_eq!(
        AskError::Keychain("locked".to_owned()).to_string(),
        "locked"
    );
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = Capture;
    fn make_writer(&'a self) -> Capture {
        self.clone()
    }
}

fn captured_logs() -> &'static Capture {
    static CAPTURE: OnceLock<Capture> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let capture = Capture::default();
        tracing_subscriber::fmt()
            .with_writer(capture.clone())
            .with_ansi(false)
            .init();
        capture
    })
}

#[tokio::test]
async fn no_error_log_or_audit_row_holds_the_prompt_the_redacted_text_the_reply_or_the_key() {
    let logs = captured_logs();
    let text = prompt_with_secret();
    let mut outputs: Vec<String> = Vec::new();

    let mut failing_audit = Harness::with_key();
    failing_audit.audit_mode = AuditMode::Fail;
    outputs.push(format!(
        "{:?}",
        failing_audit.gateway().ask("m", &text).await.unwrap_err()
    ));
    let mut panicking_audit = Harness::with_key();
    panicking_audit.audit_mode = AuditMode::Panic;
    outputs.push(format!(
        "{:?}",
        panicking_audit.gateway().ask("m", &text).await.unwrap_err()
    ));
    let blocked = Harness::with_key();
    outputs.push(format!(
        "{:?}",
        blocked
            .gateway_with_shield(panicking_shield)
            .ask("m", &text)
            .await
            .unwrap_err()
    ));
    outputs.push(format!("{:?}", blocked.rows.lock().unwrap()));
    let mut failing_send = Harness::with_key();
    failing_send.reply = Err("provider refused".to_owned());
    outputs.push(format!(
        "{:?}",
        failing_send.gateway().ask("m", &text).await.unwrap_err()
    ));
    outputs.push(format!("{:?}", failing_send.rows.lock().unwrap()));
    let keychain_down = Harness::with_key();
    keychain_down.vault.fail_next(Operation::Get);
    outputs.push(format!(
        "{:?}",
        keychain_down.gateway().ask("m", &text).await.unwrap_err()
    ));
    outputs.push(format!("{:?}", keychain_down.gateway()));
    outputs.push(String::from_utf8(logs.0.lock().unwrap().clone()).unwrap());

    assert!(
        outputs
            .last()
            .unwrap()
            .contains("context audit write failed")
    );
    for output in &outputs {
        for secret in [PROMPT, STORED_KEY, "REDACTED", "ghp_", "abcdefghijklmnop"] {
            assert!(!output.contains(secret), "{secret} leaked into {output}");
        }
    }
}

#[test]
fn the_context_crate_calls_the_sender_in_one_place() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sites = Vec::new();
    for entry in std::fs::read_dir(src).unwrap() {
        let path = entry.unwrap().path();
        let body = std::fs::read_to_string(&path).unwrap();
        for _ in body.matches(".send(") {
            sites.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert_eq!(sites, ["gateway.rs"]);
}
