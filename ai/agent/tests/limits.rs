#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::atomic::Ordering;
use std::time::Duration;

use navori_agent::{Entry, Limits, Origin, RecordedMessage, TurnEnd, TurnErrorKind, TurnRequest};
use navori_context::{ChatReply, Finish, ToolCall};
use tokio_util::sync::CancellationToken;

fn request(input: &str) -> TurnRequest {
    TurnRequest {
        conversation: None,
        origin: Origin::User,
        model: "deepseek-chat".to_owned(),
        input: input.to_owned(),
    }
}

#[tokio::test]
async fn a_model_that_never_stops_is_ended_at_the_step_limit() {
    let script = (0..6)
        .map(|n| common::call_reply(&format!("c{n}"), "probe_look", "{}"))
        .collect();
    let limits = Limits {
        max_steps: 3,
        ..Limits::default()
    };
    let rig = common::agent_with(script, limits);

    let outcome = rig
        .agent
        .run_turn(&rig.kernel, request("loop"), CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(outcome.end, TurnEnd::StepLimit);
    assert_eq!(outcome.steps, 3);
    assert_eq!(rig.chat.requests().len(), 3);
}

#[tokio::test]
async fn a_turn_over_its_token_limit_ends() {
    let with_usage = |id: &str| {
        let mut reply = common::call_reply(id, "probe_look", "{}");
        reply.usage = Some(common::usage(120, 30));
        reply
    };
    let script = vec![
        with_usage("c1"),
        with_usage("c2"),
        common::text_reply("late"),
    ];
    let limits = Limits {
        max_tokens: 300,
        ..Limits::default()
    };
    let rig = common::agent_with(script, limits);

    let outcome = rig
        .agent
        .run_turn(&rig.kernel, request("loop"), CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(outcome.end, TurnEnd::TokenLimit);
    assert_eq!(outcome.steps, 2);
    assert_eq!(rig.chat.requests().len(), 2);
}

async fn until_requests(rig: &common::Rig, count: usize) {
    while rig.chat.requests().len() < count {
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

#[tokio::test]
async fn a_cancel_during_a_model_call_ends_the_turn_and_sends_nothing_more() {
    let rig = common::agent_with(
        vec![
            common::call_reply("c1", "probe_look", "{}"),
            common::text_reply("never sent"),
        ],
        Limits::default(),
    );
    let release = rig.chat.hold_next();
    let cancel = CancellationToken::new();

    let (outcome, ()) = tokio::join!(
        rig.agent
            .run_turn(&rig.kernel, request("look"), cancel.clone()),
        async {
            until_requests(&rig, 1).await;
            cancel.cancel();
            release.send(()).ok();
        }
    );

    assert_eq!(outcome.unwrap().end, TurnEnd::Cancelled);
    assert_eq!(rig.chat.requests().len(), 1);
}

#[tokio::test]
async fn a_cancel_while_the_model_answers_reaches_no_module() {
    let rig = common::agent_with(
        vec![common::call_reply("c1", "probe_look", "{}")],
        Limits::default(),
    );
    let release = rig.chat.hold_next();
    let cancel = CancellationToken::new();

    let (outcome, ()) = tokio::join!(
        rig.agent
            .run_turn(&rig.kernel, request("look"), cancel.clone()),
        async {
            until_requests(&rig, 1).await;
            cancel.cancel();
            release.send(()).ok();
        }
    );

    assert_eq!(outcome.unwrap().end, TurnEnd::Cancelled);
    assert_eq!(rig.probe.load(Ordering::SeqCst), 0);
}

fn tool_results(rig: &common::Rig) -> Vec<(String, String, bool, bool)> {
    rig.store
        .entries()
        .into_iter()
        .filter_map(|e| match e {
            Entry::Message {
                body:
                    RecordedMessage::ToolResult {
                        call_id,
                        text,
                        is_error,
                        synthetic,
                    },
                ..
            } => Some((call_id, text, is_error, synthetic)),
            _ => None,
        })
        .collect()
}

fn two_calls() -> ChatReply {
    let call = |id: &str, name: &str| ToolCall {
        id: id.to_owned(),
        name: name.to_owned(),
        arguments: "{}".to_owned(),
    };
    ChatReply {
        text: None,
        reasoning: None,
        tool_calls: vec![call("c1", "probe_slow"), call("c2", "probe_look")],
        usage: None,
        finish: Finish::ToolCalls,
    }
}

#[tokio::test]
async fn every_recorded_call_has_one_result_after_cancel() {
    let rig = common::agent_with(vec![two_calls()], Limits::default());
    let cancel = CancellationToken::new();

    let (outcome, ()) = tokio::join!(
        rig.agent
            .run_turn(&rig.kernel, request("look"), cancel.clone()),
        async {
            while rig.probe.load(Ordering::SeqCst) < 1 {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            cancel.cancel();
        }
    );

    assert_eq!(outcome.unwrap().end, TurnEnd::Cancelled);
    let results = tool_results(&rig);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, "c1");
    assert!(
        results[0].1.contains("Its outcome is unknown"),
        "{results:?}"
    );
    assert_eq!(results[1].0, "c2");
    assert!(results[1].1.contains("but not started"), "{results:?}");
    assert!(results.iter().all(|r| r.2 && r.3));
    assert_eq!(rig.probe.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn every_recorded_call_has_one_result_after_an_error_end() {
    let limits = Limits {
        action_timeout: Duration::from_millis(50),
        ..Limits::default()
    };
    let rig = common::agent_with(vec![two_calls()], limits);

    let (outcome, ()) = tokio::join!(
        rig.agent
            .run_turn(&rig.kernel, request("look"), CancellationToken::new()),
        async {
            while rig.probe.load(Ordering::SeqCst) < 1 {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            rig.store.fail_next_append();
        }
    );

    assert_eq!(outcome.unwrap().end, TurnEnd::Error(TurnErrorKind::Record));
    let results = tool_results(&rig);
    assert_eq!(results.len(), 2);
    assert!(
        results[0].1.contains("Its outcome is unknown"),
        "{results:?}"
    );
    assert!(results[1].1.contains("but not started"), "{results:?}");
    assert!(results.iter().all(|r| r.3));
}

fn two_quick_calls() -> ChatReply {
    let call = |id: &str, name: &str| ToolCall {
        id: id.to_owned(),
        name: name.to_owned(),
        arguments: "{}".to_owned(),
    };
    ChatReply {
        text: None,
        reasoning: None,
        tool_calls: vec![call("c1", "probe_look"), call("c2", "probe_look")],
        usage: Some(common::usage(200, 100)),
        finish: Finish::ToolCalls,
    }
}

#[tokio::test]
async fn the_calls_of_the_step_that_reaches_the_token_limit_still_run() {
    let limits = Limits {
        max_tokens: 300,
        ..Limits::default()
    };
    let rig = common::agent_with(vec![two_quick_calls(), common::text_reply("never")], limits);

    let outcome = rig
        .agent
        .run_turn(&rig.kernel, request("look"), CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(outcome.end, TurnEnd::TokenLimit);
    assert_eq!(rig.probe.load(Ordering::SeqCst), 2);
    let results = tool_results(&rig);
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| !r.3 && !r.2), "{results:?}");
    assert_eq!(rig.chat.requests().len(), 1);
    let step_ends = rig
        .store
        .entries()
        .iter()
        .filter(|e| matches!(e, Entry::StepEnded { .. }))
        .count();
    assert_eq!(step_ends, 1);
}

#[tokio::test]
async fn a_final_answer_over_the_token_limit_still_completes() {
    let mut reply = common::text_reply("the answer");
    reply.usage = Some(common::usage(500, 100));
    let limits = Limits {
        max_tokens: 300,
        ..Limits::default()
    };
    let rig = common::agent_with(vec![reply], limits);

    let outcome = rig
        .agent
        .run_turn(&rig.kernel, request("hi"), CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(outcome.end, TurnEnd::Completed);
    assert_eq!(outcome.answer.as_deref(), Some("the answer"));
}

#[tokio::test]
async fn a_repeated_call_carries_the_reminder_in_its_result() {
    let script = (1..=3)
        .map(|n| common::call_reply(&format!("c{n}"), "probe_look", "{}"))
        .chain([common::text_reply("stuck")])
        .collect();
    let rig = common::agent_with(script, Limits::default());

    rig.agent
        .run_turn(&rig.kernel, request("loop"), CancellationToken::new())
        .await
        .unwrap();

    let results = tool_results(&rig);
    assert_eq!(results[1].1, r#"{"seen":"room"}"#);
    let third = results[2]
        .1
        .strip_prefix("{\"seen\":\"room\"}\n\n")
        .unwrap();
    assert!(third.starts_with("You are repeating the exact same tool call"));
}
