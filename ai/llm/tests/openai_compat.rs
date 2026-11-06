#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Responder, refused_url};
use navori_llm::{
    ChatRequest, DEEPSEEK_BASE_URL, Finish, LlmError, LlmProvider, Message, OpenAiCompat, ToolCall,
    ToolDef, Usage,
};
use navori_secrets::Secret;

const KEY: &str = "test-key-ZZ9";

fn hello() -> ChatRequest {
    ChatRequest {
        model: "m".to_owned(),
        messages: vec![Message::User {
            text: "hello".to_owned(),
        }],
        tools: vec![],
    }
}

fn sent_body(server: &Responder) -> serde_json::Value {
    let requests = server.requests();
    serde_json::from_str(requests[0].split("\r\n\r\n").nth(1).unwrap()).unwrap()
}

const TOOL_CALL_NULL_CONTENT: &str = r#"{"choices":[{"message":{"content":null,"reasoning_content":"r","tool_calls":[{"id":"c1","type":"function","function":{"name":"ports_list","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":120,"completion_tokens":30,"total_tokens":150,"prompt_cache_hit_tokens":100,"prompt_cache_miss_tokens":20}}"#;
const TOOL_CALL_EMPTY_CONTENT: &str = r#"{"choices":[{"message":{"content":"","reasoning_content":"r","tool_calls":[{"id":"c1","type":"function","function":{"name":"ports_list","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":120,"completion_tokens":30,"total_tokens":150,"prompt_cache_hit_tokens":100,"prompt_cache_miss_tokens":20}}"#;

#[tokio::test]
async fn tools_are_sent_as_functions_and_omitted_when_empty() {
    let server = Responder::start(200, r#"{"choices":[{"message":{"content":"ok"}}]}"#).await;
    let adapter = OpenAiCompat::new(server.base_url.clone());
    let mut request = hello();
    request.tools = vec![ToolDef {
        name: "ports_list".to_owned(),
        description: "List ports".to_owned(),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
    }];

    adapter.complete(request, &Secret::new(KEY)).await.unwrap();

    assert_eq!(
        sent_body(&server)["tools"],
        serde_json::json!([{"type": "function", "function": {
            "name": "ports_list",
            "description": "List ports",
            "parameters": {"type": "object", "properties": {}}
        }}])
    );
}

#[tokio::test]
async fn an_assistant_message_is_replayed_with_reasoning_and_tool_calls() {
    let server = Responder::start(200, r#"{"choices":[{"message":{"content":"ok"}}]}"#).await;
    let adapter = OpenAiCompat::new(server.base_url.clone());
    let mut request = hello();
    request.messages.push(Message::Assistant {
        text: None,
        reasoning: Some("think".to_owned()),
        tool_calls: vec![ToolCall {
            id: "c1".to_owned(),
            name: "ports_list".to_owned(),
            arguments: "{}".to_owned(),
        }],
    });
    request.messages.push(Message::Tool {
        call_id: "c1".to_owned(),
        text: "[]".to_owned(),
    });

    adapter.complete(request, &Secret::new(KEY)).await.unwrap();

    assert_eq!(
        sent_body(&server)["messages"],
        serde_json::json!([
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": null, "reasoning_content": "think",
             "tool_calls": [{"id": "c1", "type": "function",
                             "function": {"name": "ports_list", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "c1", "content": "[]"}
        ])
    );
}

#[tokio::test]
async fn a_tool_call_reply_with_null_content_is_accepted() {
    let server = Responder::start(200, TOOL_CALL_NULL_CONTENT).await;
    let adapter = OpenAiCompat::new(server.base_url.clone());

    let reply = adapter.complete(hello(), &Secret::new(KEY)).await.unwrap();

    assert_eq!(reply.text, None);
    assert_eq!(
        reply.tool_calls,
        vec![ToolCall {
            id: "c1".to_owned(),
            name: "ports_list".to_owned(),
            arguments: "{}".to_owned()
        }]
    );
    assert_eq!(reply.reasoning.as_deref(), Some("r"));
    assert_eq!(reply.finish, Finish::ToolCalls);
    assert_eq!(
        reply.usage,
        Some(Usage {
            input: 120,
            output: 30,
            cache_hit: Some(100),
            cache_miss: Some(20)
        })
    );
}

#[tokio::test]
async fn a_tool_call_reply_with_empty_content_is_accepted() {
    let server = Responder::start(200, TOOL_CALL_EMPTY_CONTENT).await;
    let adapter = OpenAiCompat::new(server.base_url.clone());

    let reply = adapter.complete(hello(), &Secret::new(KEY)).await.unwrap();

    assert_eq!(reply.text, None);
    assert_eq!(reply.tool_calls.len(), 1);
    assert_eq!(reply.finish, Finish::ToolCalls);
}

#[tokio::test]
async fn a_reply_without_usage_has_none() {
    let server = Responder::start(200, r#"{"choices":[{"message":{"content":"hi"}}]}"#).await;
    let adapter = OpenAiCompat::new(server.base_url.clone());

    let reply = adapter.complete(hello(), &Secret::new(KEY)).await.unwrap();

    assert_eq!(reply.usage, None);
    assert_eq!(reply.text.as_deref(), Some("hi"));
}

#[tokio::test]
async fn a_reply_with_neither_text_nor_tool_calls_is_malformed() {
    let server = Responder::start(
        200,
        r#"{"choices":[{"message":{"content":null,"reasoning_content":"r"}}]}"#,
    )
    .await;
    let adapter = OpenAiCompat::new(server.base_url.clone());

    let error = adapter
        .complete(hello(), &Secret::new(KEY))
        .await
        .unwrap_err();

    assert!(matches!(error, LlmError::Malformed), "{error}");
}

#[tokio::test]
async fn a_plain_request_still_sends_only_model_messages_and_stream() {
    let server = Responder::start(200, r#"{"choices":[{"message":{"content":"hi there"}}]}"#).await;
    let adapter = OpenAiCompat::new(server.base_url.clone());

    let reply = adapter.complete(hello(), &Secret::new(KEY)).await.unwrap();

    assert_eq!(reply.text.as_deref(), Some("hi there"));
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
fn each_failure_has_a_coarse_content_free_kind() {
    let kinds = [
        (LlmError::Network, "network"),
        (LlmError::Timeout, "timeout"),
        (LlmError::Status(401), "status"),
        (LlmError::Malformed, "malformed"),
    ];
    for (error, kind) in kinds {
        assert_eq!(error.kind(), kind);
    }
}

#[test]
fn the_deepseek_preset_is_an_https_url() {
    assert!(DEEPSEEK_BASE_URL.starts_with("https://"));
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
