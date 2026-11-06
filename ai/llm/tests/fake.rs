#![allow(clippy::unwrap_used, clippy::expect_used)]

use navori_llm::fake::FakeLlm;
use navori_llm::{ChatReply, ChatRequest, Finish, LlmProvider, Message};
use navori_secrets::Secret;

fn request(text: &str) -> ChatRequest {
    ChatRequest {
        model: "m".to_owned(),
        messages: vec![Message::User {
            text: text.to_owned(),
        }],
        tools: vec![],
    }
}

fn reply(text: &str) -> ChatReply {
    ChatReply {
        text: Some(text.to_owned()),
        reasoning: None,
        tool_calls: vec![],
        usage: None,
        finish: Finish::Stop,
    }
}

#[tokio::test]
async fn the_fake_answers_its_script_in_order_and_records_requests() {
    let fake = FakeLlm::new(vec![Ok(reply("one")), Ok(reply("two"))]);
    let key = Secret::new("k");

    let first = fake.complete(request("a"), &key).await.unwrap();
    let second = fake.complete(request("b"), &key).await.unwrap();

    assert_eq!(first.text.as_deref(), Some("one"));
    assert_eq!(second.text.as_deref(), Some("two"));
    assert_eq!(fake.requests(), vec![request("a"), request("b")]);
}

#[tokio::test]
async fn a_fake_with_an_empty_script_fails_as_malformed() {
    let fake = FakeLlm::new(vec![]);

    let error = fake
        .complete(request("a"), &Secret::new("k"))
        .await
        .unwrap_err();

    assert_eq!(error.kind(), "malformed");
}

#[tokio::test]
async fn a_held_call_waits_until_released() {
    let fake = FakeLlm::new(vec![Ok(reply("late"))]);
    let key = Secret::new("k");
    let release = fake.hold_next();

    let mut call = Box::pin(fake.complete(request("a"), &key));
    assert!(futures::poll!(call.as_mut()).is_pending());
    assert!(futures::poll!(call.as_mut()).is_pending());

    release.release();

    let answer = call.await.unwrap();
    assert_eq!(answer.text.as_deref(), Some("late"));
}
