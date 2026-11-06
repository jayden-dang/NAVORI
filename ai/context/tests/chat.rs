#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::panic::panic_any;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use navori_context::{
    AskError, AuditError, AuditRow, AuditSink, ChatReply, ChatRequest, ChatSender, Finish, Gateway,
    Message, Outbound, Outcome, SendError, Sender, ToolCall, ToolDef,
};
use navori_redact::{Redacted, redact_text};
use navori_secrets::fake::{FakeKeychain, MemoryStore};
use navori_secrets::{KeychainService, Secret, SecretKey, SecretStore};
use serde_json::{Value, json};

const STORED_KEY: &str = "stored-key-QQ7";

fn planted() -> String {
    concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123456789").to_owned()
}

struct UnusedSender;

impl Sender for UnusedSender {
    fn send<'a>(
        &'a self,
        _: &'a Secret,
        _: &'a str,
        _: &'a str,
    ) -> BoxFuture<'a, Result<String, SendError>> {
        unreachable!()
    }
}

struct RecordingChat {
    received: Mutex<Vec<(String, ChatRequest)>>,
}

impl ChatSender for RecordingChat {
    fn send<'a>(
        &'a self,
        key: &'a Secret,
        request: ChatRequest,
    ) -> BoxFuture<'a, Result<ChatReply, SendError>> {
        self.received
            .lock()
            .unwrap()
            .push((key.expose().to_owned(), request));
        Box::pin(async {
            Ok(ChatReply {
                text: Some("done".to_owned()),
                reasoning: None,
                tool_calls: vec![],
                usage: None,
                finish: Finish::Stop,
            })
        })
    }
}

struct Audit {
    rows: Mutex<Vec<AuditRow>>,
    fail: bool,
}

impl AuditSink for Audit {
    fn record(&self, row: AuditRow) -> Result<(), AuditError> {
        if self.fail {
            return Err(AuditError::Write);
        }
        self.rows.lock().unwrap().push(row);
        Ok(())
    }
}

struct Harness {
    chat: Arc<RecordingChat>,
    audit: Arc<Audit>,
    gateway: Gateway,
}

fn harness_with(key: bool, fail_audit: bool, shield: Option<fn(&str) -> Redacted>) -> Harness {
    let vault = FakeKeychain::new();
    let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(KeychainService::DEV, &vault));
    let key_name = SecretKey::new("provider", "deepseek").unwrap();
    if key {
        store.set(&key_name, &Secret::new(STORED_KEY)).unwrap();
    }
    let chat = Arc::new(RecordingChat {
        received: Mutex::default(),
    });
    let audit = Arc::new(Audit {
        rows: Mutex::default(),
        fail: fail_audit,
    });
    let gateway = Gateway::with_shield(
        store,
        key_name,
        Arc::new(UnusedSender),
        audit.clone(),
        "deepseek",
        shield.unwrap_or(redact_text),
    )
    .with_chat_sender(chat.clone());
    Harness {
        chat,
        audit,
        gateway,
    }
}

fn harness() -> Harness {
    harness_with(true, false, None)
}

fn request(messages: Vec<Message>, tools: Vec<ToolDef>) -> ChatRequest {
    ChatRequest {
        model: "m".to_owned(),
        messages,
        tools,
    }
}

fn outbound(request: ChatRequest) -> Outbound {
    Outbound {
        request,
        prior_redactions: 0,
    }
}

fn tool(name: &str, description: &str, parameters: Value) -> ToolDef {
    ToolDef {
        name: name.to_owned(),
        description: description.to_owned(),
        parameters,
    }
}

fn clean_tool() -> ToolDef {
    tool("read", "reads", json!({"type": "object"}))
}

#[test]
fn shielding_a_shielded_text_again_finds_nothing_new() {
    let gateway = harness().gateway;
    let password = "password=hunter2hunter2";
    for text in [planted(), password.to_owned()] {
        let first = gateway.shield(&text).unwrap();
        assert_eq!(first.count, 1, "{text}");
        let second = gateway.shield(&first.text).unwrap();
        assert_eq!(second.text, first.text);
        assert_eq!(second.count, 0);
    }
}

#[tokio::test]
async fn a_planted_secret_in_a_tool_result_arrives_redacted_and_counted_once() {
    let h = harness();
    let messages = vec![
        Message::User {
            text: "go".to_owned(),
        },
        Message::Tool {
            call_id: "c1".to_owned(),
            text: format!("found {}", planted()),
        },
    ];

    let reply = h
        .gateway
        .send_chat(outbound(request(messages, vec![clean_tool()])))
        .await
        .unwrap();

    assert_eq!(reply.text.as_deref(), Some("done"));
    let received = h.chat.received.lock().unwrap();
    assert_eq!(received[0].0, STORED_KEY);
    assert_eq!(
        received[0].1.messages[1],
        Message::Tool {
            call_id: "c1".to_owned(),
            text: "found [REDACTED:github_token]".to_owned()
        }
    );
    let rows = h.audit.rows.lock().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kinds, ["text"]);
    assert_eq!(rows[0].outcome, Outcome::Authorized);
    assert_eq!(rows[0].redacted_count, 1);
    assert_eq!(rows[0].provider, "deepseek");
}

#[tokio::test]
async fn every_message_field_is_shielded() {
    let h = harness();
    let secret = planted();
    let messages = vec![
        Message::User {
            text: secret.clone(),
        },
        Message::Assistant {
            text: Some(secret.clone()),
            reasoning: Some(secret.clone()),
            tool_calls: vec![ToolCall {
                id: "c1".to_owned(),
                name: "read".to_owned(),
                arguments: format!("{{\"p\":\"{secret}\"}}"),
            }],
        },
    ];

    h.gateway
        .send_chat(outbound(request(messages, vec![])))
        .await
        .unwrap();

    let received = h.chat.received.lock().unwrap();
    let sent = format!("{:?}", received[0].1);
    assert!(!sent.contains("ghp_"), "{sent}");
    assert_eq!(h.audit.rows.lock().unwrap()[0].redacted_count, 4);
}

#[tokio::test]
async fn prior_redactions_add_to_the_row() {
    let h = harness();
    let messages = vec![Message::User {
        text: "clean".to_owned(),
    }];

    h.gateway
        .send_chat(Outbound {
            request: request(messages, vec![]),
            prior_redactions: 2,
        })
        .await
        .unwrap();

    assert_eq!(h.audit.rows.lock().unwrap()[0].redacted_count, 2);
}

#[tokio::test]
async fn a_planted_secret_in_a_tool_definition_is_redacted() {
    let h = harness();
    let defs = vec![tool(
        "read",
        &format!("uses {}", planted()),
        json!({"type": "object", "properties": {"path": {"description": planted()}}}),
    )];

    h.gateway
        .send_chat(outbound(request(vec![], defs)))
        .await
        .unwrap();

    let received = h.chat.received.lock().unwrap();
    let sent = &received[0].1.tools[0];
    assert_eq!(sent.description, "uses [REDACTED:github_token]");
    assert_eq!(
        sent.parameters["properties"]["path"]["description"],
        "[REDACTED:github_token]"
    );
    assert_eq!(h.audit.rows.lock().unwrap()[0].redacted_count, 2);
}

#[tokio::test]
async fn a_planted_secret_in_a_parameters_key_blocks_the_call() {
    let h = harness();
    let mut parameters = serde_json::Map::new();
    parameters.insert(planted(), json!("x"));
    let defs = vec![tool("read", "reads", Value::Object(parameters))];

    let error = h
        .gateway
        .send_chat(outbound(request(vec![], defs)))
        .await
        .unwrap_err();

    assert_eq!(error, AskError::Shield);
    assert!(h.chat.received.lock().unwrap().is_empty());
    let rows = h.audit.rows.lock().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].outcome, Outcome::Blocked);
}

fn shield_that_panics_on_boom(text: &str) -> Redacted {
    if text.contains("BOOM") {
        panic_any("shield exploded");
    }
    redact_text(text)
}

#[tokio::test]
async fn a_panicking_shield_on_any_text_writes_blocked_and_sends_nothing() {
    let boom = || "BOOM".to_owned();
    let assistant = |text, reasoning, arguments, name| Message::Assistant {
        text,
        reasoning,
        tool_calls: vec![ToolCall {
            id: "c".to_owned(),
            name,
            arguments,
        }],
    };
    let ok = || "ok".to_owned();
    let cases = vec![
        request(vec![Message::User { text: boom() }], vec![]),
        request(vec![assistant(Some(boom()), None, ok(), ok())], vec![]),
        request(vec![assistant(None, Some(boom()), ok(), ok())], vec![]),
        request(vec![assistant(None, None, boom(), ok())], vec![]),
        request(
            vec![Message::Tool {
                call_id: "c".to_owned(),
                text: boom(),
            }],
            vec![],
        ),
        request(vec![], vec![tool(&boom(), "d", json!({}))]),
        request(vec![], vec![tool("n", &boom(), json!({}))]),
        request(vec![], vec![tool("n", "d", json!({"a": boom()}))]),
        request(vec![], vec![tool("n", "d", json!({boom(): "v"}))]),
    ];
    for case in cases {
        let h = harness_with(true, false, Some(shield_that_panics_on_boom));

        let error = h.gateway.send_chat(outbound(case.clone())).await;

        assert_eq!(error.unwrap_err(), AskError::Shield, "{case:?}");
        assert!(h.chat.received.lock().unwrap().is_empty());
        let rows = h.audit.rows.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].outcome, Outcome::Blocked);
        assert_eq!(rows[0].redacted_count, 0);
    }
}

#[tokio::test]
async fn an_audit_failure_sends_nothing() {
    let h = harness_with(true, true, None);

    let error = h
        .gateway
        .send_chat(outbound(request(
            vec![Message::User {
                text: "hi".to_owned(),
            }],
            vec![],
        )))
        .await
        .unwrap_err();

    assert_eq!(error, AskError::Audit);
    assert!(h.chat.received.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_missing_key_is_no_key() {
    let h = harness_with(false, false, None);

    let error = h
        .gateway
        .send_chat(outbound(request(vec![], vec![])))
        .await
        .unwrap_err();

    assert_eq!(error, AskError::NoKey);
    assert!(h.audit.rows.lock().unwrap().is_empty());
    assert!(h.chat.received.lock().unwrap().is_empty());
}

#[tokio::test]
async fn no_message_text_reaches_the_audit_row() {
    let h = harness();
    let messages = vec![Message::User {
        text: format!("distinctive-prompt {}", planted()),
    }];

    h.gateway
        .send_chat(outbound(request(messages, vec![clean_tool()])))
        .await
        .unwrap();

    let rows = h.audit.rows.lock().unwrap();
    let fields = format!("{:?}", rows[0]);
    assert!(!fields.contains("distinctive-prompt"), "{fields}");
    assert!(!fields.contains("ghp_"), "{fields}");
}

#[tokio::test]
async fn send_chat_without_a_chat_sender_is_a_send_error() {
    let vault = FakeKeychain::new();
    let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(KeychainService::DEV, &vault));
    let key = SecretKey::new("provider", "deepseek").unwrap();
    store.set(&key, &Secret::new(STORED_KEY)).unwrap();
    let audit = Arc::new(Audit {
        rows: Mutex::default(),
        fail: false,
    });
    let gateway = Gateway::new(
        store,
        key,
        Arc::new(UnusedSender),
        audit.clone(),
        "deepseek",
    );

    let error = gateway
        .send_chat(outbound(request(vec![], vec![])))
        .await
        .unwrap_err();

    assert!(matches!(error, AskError::Send(_)));
    assert!(audit.rows.lock().unwrap().is_empty());
}
