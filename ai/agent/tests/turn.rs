#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use navori_agent::{
    ConversationId, Entry, Limits, Origin, RecordError, RecordedMessage, TurnEnd, TurnErrorKind,
    TurnRequest, TurnStartError,
};
use navori_context::{ChatReply, Finish, Message, ToolCall, Usage};

fn request(input: &str) -> TurnRequest {
    TurnRequest {
        conversation: None,
        origin: Origin::User,
        model: "deepseek-chat".to_owned(),
        input: input.to_owned(),
    }
}

#[tokio::test]
async fn a_turn_calls_a_read_action_and_answers_from_its_result() {
    let rig = common::agent_with(
        vec![
            common::call_reply("c1", "probe_look", "{}"),
            common::text_reply("3 things"),
        ],
        Limits::default(),
    );

    let outcome = rig
        .agent
        .run_turn(&rig.kernel, request("what is around?"), Default::default())
        .await
        .unwrap();

    assert_eq!(outcome.end, TurnEnd::Completed);
    assert_eq!(outcome.answer.as_deref(), Some("3 things"));
    assert_eq!(outcome.steps, 2);
    let requests = rig.chat.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].messages.last(),
        Some(&Message::Tool {
            call_id: "c1".to_owned(),
            text: r#"{"seen":"room"}"#.to_owned()
        })
    );
}

#[tokio::test]
async fn the_record_holds_origin_input_reply_raw_arguments_result_and_usage() {
    let mut first = common::call_reply("c1", "probe_look", "{}");
    first.usage = Some(common::usage(120, 30));
    let rig = common::agent_with(vec![first, common::text_reply("done")], Limits::default());

    rig.agent
        .run_turn(&rig.kernel, request("look"), Default::default())
        .await
        .unwrap();

    let entries = rig.store.entries();
    assert!(entries.iter().any(|e| matches!(
        e,
        Entry::TurnStarted { origin: Origin::User, model, seq: 0, .. } if model == "deepseek-chat"
    )));
    let bodies: Vec<RecordedMessage> = entries
        .iter()
        .filter_map(|e| match e {
            Entry::Message { body, .. } => Some(body.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        bodies,
        vec![
            RecordedMessage::Input {
                text: "look".to_owned()
            },
            RecordedMessage::Assistant {
                text: None,
                reasoning: None,
                tool_calls: vec![ToolCall {
                    id: "c1".to_owned(),
                    name: "probe_look".to_owned(),
                    arguments: "{}".to_owned()
                }]
            },
            RecordedMessage::ToolResult {
                call_id: "c1".to_owned(),
                text: r#"{"seen":"room"}"#.to_owned(),
                is_error: false,
                synthetic: false
            },
            RecordedMessage::Assistant {
                text: Some("done".to_owned()),
                reasoning: None,
                tool_calls: vec![]
            },
        ]
    );
    let usages: Vec<Option<Usage>> = entries
        .iter()
        .filter_map(|e| match e {
            Entry::Usage { usage, .. } => Some(usage.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(usages, vec![Some(common::usage(120, 30)), None]);
}

fn reply_with(
    text: Option<&str>,
    reasoning: Option<&str>,
    id: &str,
    name: &str,
    arguments: &str,
) -> ChatReply {
    let mut reply = common::call_reply(id, name, arguments);
    reply.text = text.map(str::to_owned);
    reply.reasoning = reasoning.map(str::to_owned);
    reply
}

#[tokio::test]
async fn everything_recorded_is_shielded() {
    let token = common::PLANTED;
    let rig = common::agent_with(
        vec![
            reply_with(Some(token), None, "c1", "probe_leak", "{}"),
            reply_with(None, Some(token), "c2", "probe_look", "{}"),
            reply_with(
                None,
                None,
                "c3",
                "probe_look_at",
                &format!(r#"{{"n":1,"note":"{token}"}}"#),
            ),
            common::text_reply("ok"),
        ],
        Limits::default(),
    );

    rig.agent
        .run_turn(&rig.kernel, request(token), Default::default())
        .await
        .unwrap();

    let mut counted = Vec::new();
    for entry in rig.store.entries() {
        if let Entry::Message {
            body, redactions, ..
        } = entry
        {
            let stored = body.to_body();
            assert!(!stored.contains(token), "{stored}");
            if stored.contains("[REDACTED:github_token]") {
                counted.push(redactions);
            }
        }
    }
    assert_eq!(counted, vec![1, 1, 1, 1, 1]);
}

#[tokio::test]
async fn the_first_request_carries_the_inputs_redaction_count() {
    let rig = common::agent_with(vec![common::text_reply("ok")], Limits::default());

    rig.agent
        .run_turn(&rig.kernel, request(common::PLANTED), Default::default())
        .await
        .unwrap();

    let rows = rig.audit.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].redacted_count, 1);
}

#[tokio::test]
async fn the_model_id_comes_from_the_request() {
    let rig = common::agent_with(vec![common::text_reply("ok")], Limits::default());

    rig.agent
        .run_turn(&rig.kernel, request("hi"), Default::default())
        .await
        .unwrap();

    assert_eq!(rig.chat.requests()[0].model, "deepseek-chat");
}

#[tokio::test]
async fn a_follow_up_turn_sends_the_earlier_messages_first() {
    let rig = common::agent_with(
        vec![
            common::text_reply("first answer"),
            common::text_reply("second"),
        ],
        Limits::default(),
    );
    let first = rig
        .agent
        .run_turn(&rig.kernel, request("one"), Default::default())
        .await
        .unwrap();

    let mut follow_up = request("two");
    follow_up.conversation = Some(first.conversation);
    let second = rig
        .agent
        .run_turn(&rig.kernel, follow_up, Default::default())
        .await
        .unwrap();

    assert_eq!(second.conversation, first.conversation);
    assert_eq!(
        rig.chat.requests()[1].messages,
        vec![
            Message::User {
                text: "one".to_owned()
            },
            Message::Assistant {
                text: Some("first answer".to_owned()),
                reasoning: None,
                tool_calls: vec![]
            },
            Message::User {
                text: "two".to_owned()
            },
        ]
    );
}

async fn until_requests(rig: &common::Rig, count: usize) {
    while rig.chat.requests().len() < count {
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
}

fn refuse(_: &str) -> navori_redact::Redacted {
    std::panic::panic_any(())
}

fn end_reasons(rig: &common::Rig) -> Vec<String> {
    rig.store
        .entries()
        .into_iter()
        .filter_map(|e| match e {
            Entry::TurnEnded { end, .. } => Some(end),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_failed_record_write_ends_the_turn_and_sends_nothing_more() {
    let rig = common::agent_with(
        vec![
            common::call_reply("c1", "probe_look", "{}"),
            common::text_reply("never sent"),
        ],
        Limits::default(),
    );
    let release = rig.chat.hold_next();

    let (outcome, ()) = tokio::join!(
        rig.agent
            .run_turn(&rig.kernel, request("look"), Default::default()),
        async {
            until_requests(&rig, 1).await;
            rig.store.fail_next_append();
            release.send(()).unwrap();
        }
    );

    assert_eq!(outcome.unwrap().end, TurnEnd::Error(TurnErrorKind::Record));
    assert_eq!(rig.chat.requests().len(), 1);
    assert_eq!(end_reasons(&rig), ["error:record"]);
}

#[tokio::test]
async fn a_missing_key_ends_the_turn_as_no_key_after_recording_it() {
    let rig = common::build_rig(vec![], Limits::default(), false, navori_redact::redact_text);

    let outcome = rig
        .agent
        .run_turn(&rig.kernel, request("hi"), Default::default())
        .await
        .unwrap();

    assert_eq!(outcome.end, TurnEnd::Error(TurnErrorKind::NoKey));
    assert_eq!(end_reasons(&rig), ["error:no_key"]);
    assert!(rig.chat.requests().is_empty());
}

#[tokio::test]
async fn a_shield_refusal_ends_the_turn() {
    let rig = common::build_rig(vec![], Limits::default(), true, refuse);

    let outcome = rig
        .agent
        .run_turn(&rig.kernel, request("hi"), Default::default())
        .await
        .unwrap();

    assert_eq!(outcome.end, TurnEnd::Error(TurnErrorKind::Shield));
    assert_eq!(end_reasons(&rig), ["error:shield"]);
    assert!(rig.chat.requests().is_empty());
}

#[tokio::test]
async fn an_unknown_conversation_is_refused_before_any_row() {
    let rig = common::agent_with(vec![], Limits::default());
    let mut unknown = request("hi");
    unknown.conversation = Some(ConversationId::new());

    let refused = rig
        .agent
        .run_turn(&rig.kernel, unknown, Default::default())
        .await;

    assert_eq!(refused.unwrap_err(), TurnStartError::UnknownConversation);
    assert!(rig.store.entries().is_empty());
}

#[tokio::test]
async fn a_reply_cut_at_the_output_limit_with_no_tool_call_completes_with_its_text() {
    let mut cut = common::text_reply("half an ans");
    cut.finish = Finish::Length;
    let rig = common::agent_with(vec![cut], Limits::default());

    let outcome = rig
        .agent
        .run_turn(&rig.kernel, request("tell me"), Default::default())
        .await
        .unwrap();

    assert_eq!(outcome.end, TurnEnd::Completed);
    assert_eq!(outcome.answer.as_deref(), Some("half an ans"));
}

#[tokio::test]
async fn one_turn_ended_event_carries_ids_origin_end_and_steps() {
    let mut rig = common::agent_with(
        vec![
            common::call_reply("c1", "probe_look", "{}"),
            common::text_reply("done"),
        ],
        Limits::default(),
    );

    let outcome = rig
        .agent
        .run_turn(
            &rig.kernel,
            request("a private question"),
            Default::default(),
        )
        .await
        .unwrap();

    let event = rig.events.try_recv().unwrap();
    assert_eq!(event.module, "ai");
    assert_eq!(event.kind, "ai.turn_ended");
    assert_eq!(
        event.summary,
        "Navori AI finished a Turn: completed after 2 Steps"
    );
    let payload = event.payload.as_object().unwrap();
    let mut keys: Vec<&str> = payload.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["conversation", "end", "origin", "steps", "turn"]);
    assert_eq!(payload["conversation"], outcome.conversation.to_string());
    assert_eq!(payload["turn"], outcome.turn.to_string());
    assert_eq!(payload["origin"], "user");
    assert_eq!(payload["end"], "completed");
    assert_eq!(payload["steps"], 2);
    assert!(rig.events.try_recv().is_err());
}

async fn abandon_after_first_request(rig: &common::Rig, input: &str) -> ConversationId {
    let _held = rig.chat.hold_next();
    tokio::select! {
        _ = rig.agent.run_turn(&rig.kernel, request(input), Default::default()) => {
            panic!("the held turn cannot finish")
        }
        () = until_requests(rig, 1) => {}
    }
    rig.store
        .entries()
        .into_iter()
        .find_map(|e| match e {
            Entry::TurnStarted { conversation, .. } => Some(conversation),
            _ => None,
        })
        .unwrap()
}

#[tokio::test]
async fn continuing_an_unended_conversation_repairs_it_and_emits_interrupted() {
    let mut rig = common::agent_with(vec![common::text_reply("fresh")], Limits::default());
    let conversation = abandon_after_first_request(&rig, "left hanging").await;

    let mut next = request("again");
    next.conversation = Some(conversation);
    let outcome = rig
        .agent
        .run_turn(&rig.kernel, next, Default::default())
        .await
        .unwrap();

    assert_eq!(outcome.end, TurnEnd::Completed);
    assert_eq!(end_reasons(&rig), ["interrupted", "completed"]);
    let interrupted = rig.events.try_recv().unwrap();
    assert_eq!(interrupted.kind, "ai.turn_ended");
    assert_eq!(interrupted.payload["end"], "interrupted");
    assert_eq!(
        interrupted.payload["conversation"],
        conversation.to_string()
    );
    let completed = rig.events.try_recv().unwrap();
    assert_eq!(completed.payload["end"], "completed");
    assert!(rig.events.try_recv().is_err());
}

#[tokio::test]
async fn a_second_turn_started_at_once_on_one_conversation_is_refused() {
    let rig = common::agent_with(
        vec![common::text_reply("first"), common::text_reply("second")],
        Limits::default(),
    );
    let conversation = rig
        .agent
        .run_turn(&rig.kernel, request("one"), Default::default())
        .await
        .unwrap()
        .conversation;
    let release = rig.chat.hold_next();
    let mut held = request("two");
    held.conversation = Some(conversation);
    let mut rival = request("three");
    rival.conversation = Some(conversation);

    let (held, rival) = tokio::join!(
        rig.agent.run_turn(&rig.kernel, held, Default::default()),
        async {
            until_requests(&rig, 2).await;
            let refused = rig
                .agent
                .run_turn(&rig.kernel, rival, Default::default())
                .await;
            release.send(()).unwrap();
            refused
        }
    );

    assert_eq!(held.unwrap().end, TurnEnd::Completed);
    assert_eq!(
        rival.unwrap_err(),
        TurnStartError::Record(RecordError::Duplicate)
    );
    assert_eq!(rig.chat.requests().len(), 2);
}

#[derive(Clone, Default)]
struct Capture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

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

fn keep_callsites_interested() {
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

#[tokio::test]
async fn no_message_text_reaches_the_logs() {
    keep_callsites_interested();
    let capture = Capture::default();
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_writer(capture.clone())
            .with_ansi(false)
            .finish(),
    );
    let rig = common::agent_with(
        vec![
            reply_with(
                Some("musing aloud"),
                Some("quiet thoughts"),
                "c1",
                "probe_leak",
                "{}",
            ),
            common::text_reply("the final answer"),
        ],
        Limits::default(),
    );

    rig.agent
        .run_turn(
            &rig.kernel,
            request("my private question"),
            Default::default(),
        )
        .await
        .unwrap();

    let logs = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("turn ended"), "{logs}");
    for text in [
        "my private question",
        "musing aloud",
        "quiet thoughts",
        "the final answer",
        "probe_leak",
        common::PLANTED,
        "REDACTED",
    ] {
        assert!(!logs.contains(text), "{text} in {logs}");
    }
}

#[tokio::test]
async fn a_repeated_call_gets_the_reminder_with_its_result_and_each_turn_starts_fresh() {
    let call = |id: &str| common::call_reply(id, "probe_look", "{}");
    let script = vec![
        call("c1"),
        call("c2"),
        call("c3"),
        common::text_reply("stuck"),
        call("c4"),
        call("c5"),
        common::text_reply("fine"),
    ];
    let rig = common::agent_with(script, Limits::default());

    let first = rig
        .agent
        .run_turn(&rig.kernel, request("loop"), Default::default())
        .await
        .unwrap();
    let mut second = request("again");
    second.conversation = Some(first.conversation);
    rig.agent
        .run_turn(&rig.kernel, second, Default::default())
        .await
        .unwrap();

    let requests = rig.chat.requests();
    let tool_texts = |request: &navori_context::ChatRequest| -> Vec<String> {
        request
            .messages
            .iter()
            .filter_map(|m| match m {
                Message::Tool { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    let third = tool_texts(&requests[3]);
    assert_eq!(third[0], r#"{"seen":"room"}"#);
    assert_eq!(third[1], r#"{"seen":"room"}"#);
    assert!(third[2].starts_with(r#"{"seen":"room"}"#));
    assert!(third[2].contains("You are repeating the exact same tool call"));
    let last = tool_texts(&requests[6]);
    assert!(
        last.iter().skip(3).all(|t| t == r#"{"seen":"room"}"#),
        "{last:?}"
    );
}
