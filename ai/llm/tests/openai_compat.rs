#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Responder, refused_url};
use navori_llm::{
    ChatRequest, DEEPSEEK_BASE_URL, DEEPSEEK_PROVIDER, LlmError, LlmProvider, Message,
    OpenAiCompat, Role,
};
use navori_secrets::Secret;

const KEY: &str = "test-key-ZZ9";

fn hello() -> ChatRequest {
    ChatRequest {
        model: "m".to_owned(),
        messages: vec![Message {
            role: Role::User,
            text: "hello".to_owned(),
        }],
    }
}

#[tokio::test]
async fn the_adapter_posts_one_user_message_without_streaming() {
    let server = Responder::start(200, r#"{"choices":[{"message":{"content":"hi there"}}]}"#).await;
    let adapter = OpenAiCompat::new(server.base_url.clone());

    let reply = adapter.complete(hello(), &Secret::new(KEY)).await.unwrap();

    assert_eq!(reply.text, "hi there");
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    let raw = &requests[0];
    assert!(raw.starts_with("POST /chat/completions "), "{raw}");
    assert!(
        raw.to_ascii_lowercase()
            .contains("authorization: bearer test-key-zz9"),
        "{raw}"
    );
    let body: serde_json::Value =
        serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["messages", "model", "stream"]);
    assert_eq!(body["model"], "m");
    assert_eq!(body["stream"], false);
    assert_eq!(
        body["messages"],
        serde_json::json!([{"role": "user", "content": "hello"}])
    );
}

#[tokio::test]
async fn a_non_success_status_is_reported_by_status_only() {
    let server = Responder::start(401, "echo hello test-key-ZZ9").await;
    let adapter = OpenAiCompat::new(server.base_url.clone());

    let error = adapter
        .complete(hello(), &Secret::new(KEY))
        .await
        .unwrap_err();

    assert!(matches!(error, LlmError::Status(401)));
    let shown = error.to_string();
    assert!(shown.contains("401"), "{shown}");
    assert!(!shown.contains("hello"), "{shown}");
    assert!(!shown.contains(KEY), "{shown}");
    assert!(!format!("{error:?}").contains(KEY));
}

#[tokio::test]
async fn a_status_adds_what_the_person_can_check() {
    let table = [
        (401, "key saved in Settings → AI"),
        (402, "provider account balance"),
        (404, "model id given to --model"),
        (422, "model id given to --model"),
        (429, "try again shortly"),
    ];
    for (status, hint) in table {
        let server = Responder::start(status, "echo hello test-key-ZZ9").await;
        let adapter = OpenAiCompat::new(server.base_url.clone());

        let shown = adapter
            .complete(hello(), &Secret::new(KEY))
            .await
            .unwrap_err()
            .to_string();

        assert!(shown.contains(&status.to_string()), "{shown}");
        assert!(shown.contains(hint), "{status}: {shown}");
        assert!(!shown.contains("hello"), "{shown}");
        assert!(!shown.contains(KEY), "{shown}");
    }
}

#[tokio::test]
async fn a_body_without_a_first_choice_with_text_is_malformed() {
    for body in [
        r#"{"choices":[]}"#,
        r#"{"choices":[{"message":{"content":""}}]}"#,
    ] {
        let server = Responder::start(200, body).await;
        let adapter = OpenAiCompat::new(server.base_url.clone());

        let error = adapter
            .complete(hello(), &Secret::new(KEY))
            .await
            .unwrap_err();

        assert!(matches!(error, LlmError::Malformed), "{body}: {error}");
    }
}

#[tokio::test]
async fn a_refused_connection_is_a_network_error() {
    let adapter = OpenAiCompat::new(refused_url().await);

    let error = adapter
        .complete(hello(), &Secret::new(KEY))
        .await
        .unwrap_err();

    assert!(matches!(error, LlmError::Network), "{error}");
    let shown = error.to_string();
    assert!(!shown.contains(KEY) && !shown.contains("hello") && !shown.contains("127.0.0.1"));
}

#[tokio::test]
async fn a_redirect_is_reported_as_a_status_and_never_followed() {
    let elsewhere = Responder::start(200, r#"{"choices":[{"message":{"content":"x"}}]}"#).await;
    let provider =
        Responder::redirecting_to(&format!("{}/chat/completions", elsewhere.base_url)).await;
    let adapter = OpenAiCompat::new(provider.base_url.clone());

    let error = adapter
        .complete(hello(), &Secret::new("k"))
        .await
        .unwrap_err();

    assert!(matches!(error, LlmError::Status(302)), "{error:?}");
    assert_eq!(provider.requests().len(), 1);
    assert!(elsewhere.requests().is_empty());
}

#[test]
fn the_deepseek_preset_is_an_https_url() {
    assert!(DEEPSEEK_BASE_URL.starts_with("https://"));
    assert_eq!(DEEPSEEK_PROVIDER, "deepseek");
}

#[tokio::test]
async fn the_client_is_not_built_until_the_first_call() {
    let server = Responder::start(200, r#"{"choices":[{"message":{"content":"ok"}}]}"#).await;
    let adapter = OpenAiCompat::new(server.base_url.clone());

    assert_eq!(adapter.client_builds(), 0);
    assert_eq!(server.connections(), 0);

    adapter.complete(hello(), &Secret::new(KEY)).await.unwrap();
    adapter.complete(hello(), &Secret::new(KEY)).await.unwrap();

    assert_eq!(adapter.client_builds(), 1);
    assert_eq!(server.requests().len(), 2);
}
