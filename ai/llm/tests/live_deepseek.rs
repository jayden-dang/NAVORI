#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::{Command, Stdio};

use navori_llm::{ChatRequest, DEEPSEEK_BASE_URL, LlmProvider, Message, OpenAiCompat, Role};
use navori_secrets::Secret;

fn required(name: &str) -> String {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => value,
        _ => panic!("set {name} to run the live DeepSeek test"),
    }
}

// Operator: NAVORI_LIVE_LLM=1 NAVORI_LIVE_PASS_ENTRY=<entry> NAVORI_LIVE_MODEL=<model> cargo test -p navori-llm --test live_deepseek -- --ignored
#[tokio::test]
#[ignore = "calls DeepSeek with a key from `pass`; run by hand with NAVORI_LIVE_LLM=1 and --ignored"]
async fn a_live_deepseek_call_returns_text() {
    if std::env::var("NAVORI_LIVE_LLM").as_deref() != Ok("1") {
        println!("skipped: set NAVORI_LIVE_LLM=1 to call DeepSeek");
        return;
    }
    let entry = required("NAVORI_LIVE_PASS_ENTRY");
    let model = required("NAVORI_LIVE_MODEL");

    let output = Command::new("pass")
        .arg("show")
        .arg(&entry)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .unwrap_or_else(|error| panic!("could not run pass: {:?}", error.kind()));
    assert!(output.status.success(), "pass could not show the entry");
    let Ok(text) = String::from_utf8(output.stdout) else {
        panic!("the pass entry is not valid UTF-8");
    };
    let key = Secret::new(text.trim());

    let request = ChatRequest {
        model,
        messages: vec![Message {
            role: Role::User,
            text: "Say hello in one short sentence.".to_owned(),
        }],
    };
    let result = OpenAiCompat::new(DEEPSEEK_BASE_URL)
        .complete(request, &key)
        .await;
    match result {
        Ok(reply) => assert!(!reply.text.trim().is_empty(), "the reply had no text"),
        Err(error) => panic!("the live call failed: {error}"),
    }
}
