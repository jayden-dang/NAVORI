#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::MemoryConversations;
use navori_agent::{
    ConversationId, ConversationLog, ConversationStore, Entry, Origin, RecordedMessage, TurnId,
    derive_messages, repair,
};
use navori_context::{Message, ToolCall};

const INPUT: &str = r#"{"v":1,"text":"hi"}"#;
const ASSISTANT: &str = r#"{"v":1,"text":null,"reasoning":"r","tool_calls":[{"id":"c1","name":"probe_look","arguments":"{}"}]}"#;
const RESULT: &str = r#"{"v":1,"call_id":"c1","text":"[]","is_error":false,"synthetic":false}"#;

#[test]
fn bodies_round_trip_in_encoding_v1() {
    let input = RecordedMessage::Input { text: "hi".into() };
    let assistant = RecordedMessage::Assistant {
        text: None,
        reasoning: Some("r".into()),
        tool_calls: vec![ToolCall {
            id: "c1".into(),
            name: "probe_look".into(),
            arguments: "{}".into(),
        }],
    };
    let result = RecordedMessage::ToolResult {
        call_id: "c1".into(),
        text: "[]".into(),
        is_error: false,
        synthetic: false,
    };
    for (message, literal) in [(input, INPUT), (assistant, ASSISTANT), (result, RESULT)] {
        assert_eq!(message.to_body(), literal);
        assert_eq!(RecordedMessage::from_body(literal).unwrap(), message);
    }
}

#[test]
fn a_body_with_another_version_is_unreadable() {
    let other = r#"{"v":2,"text":"hi"}"#;
    assert_eq!(
        RecordedMessage::from_body(other),
        Err(navori_agent::RecordError::Read)
    );
}

fn user_turn(conversation: ConversationId, turn: TurnId, seq: u32) -> Vec<Entry> {
    vec![
        Entry::TurnStarted {
            id: turn,
            conversation,
            seq,
            origin: Origin::User,
            model: "m".into(),
            ts_ms: 1,
        },
        Entry::Message {
            turn,
            seq: 0,
            step: None,
            body: RecordedMessage::Input { text: "hi".into() },
            redactions: 0,
            ts_ms: 1,
        },
    ]
}

fn call(id: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: "probe_look".into(),
        arguments: "{}".into(),
    }
}

fn open_step_with_calls(turn: TurnId, calls: &[&str]) -> Vec<Entry> {
    vec![
        Entry::StepStarted {
            turn,
            seq: 1,
            ts_ms: 2,
        },
        Entry::Message {
            turn,
            seq: 1,
            step: Some(1),
            body: RecordedMessage::Assistant {
                text: None,
                reasoning: None,
                tool_calls: calls.iter().map(|c| call(c)).collect(),
            },
            redactions: 0,
            ts_ms: 2,
        },
    ]
}

#[test]
fn messages_are_derived_in_order_and_steps_add_nothing() {
    let conversation = ConversationId::new();
    let turn = TurnId::new();
    let store = MemoryConversations::default();
    let mut entries = vec![Entry::ConversationStarted {
        id: conversation,
        ts_ms: 0,
    }];
    entries.extend(user_turn(conversation, turn, 1));
    entries.extend(open_step_with_calls(turn, &["c1"]));
    entries.extend([
        Entry::Message {
            turn,
            seq: 2,
            step: Some(1),
            body: RecordedMessage::ToolResult {
                call_id: "c1".into(),
                text: "[]".into(),
                is_error: false,
                synthetic: false,
            },
            redactions: 0,
            ts_ms: 3,
        },
        Entry::Usage {
            turn,
            step: 1,
            usage: None,
        },
        Entry::StepEnded {
            turn,
            seq: 1,
            ts_ms: 4,
        },
        Entry::TurnEnded {
            turn,
            end: "completed".into(),
            ts_ms: 5,
        },
    ]);
    store.append(&entries).unwrap();

    let log = store.load(&conversation).unwrap();

    assert_eq!(
        derive_messages(&log),
        vec![
            Message::User { text: "hi".into() },
            Message::Assistant {
                text: None,
                reasoning: None,
                tool_calls: vec![call("c1")]
            },
            Message::Tool {
                call_id: "c1".into(),
                text: "[]".into()
            },
        ]
    );
    assert_eq!(log.turns[0].end.as_deref(), Some("completed"));
}

fn closed_turn(conversation: ConversationId, turn: TurnId) -> Vec<Entry> {
    let mut entries = vec![Entry::ConversationStarted {
        id: conversation,
        ts_ms: 0,
    }];
    entries.extend(user_turn(conversation, turn, 1));
    entries.push(Entry::TurnEnded {
        turn,
        end: "completed".into(),
        ts_ms: 3,
    });
    entries
}

#[test]
fn a_closed_turn_needs_no_repair() {
    let log =
        ConversationLog::from_entries(closed_turn(ConversationId::new(), TurnId::new())).unwrap();

    assert_eq!(repair(&log), Vec::<Entry>::new());
}

const UNKNOWN_OUTCOME: &str = "The tool call was interrupted after it was recorded, but no result was durably recorded. Its outcome is unknown. Decide whether to retry from the tool semantics: retry only if the operation is read-only or idempotent; if it may have side effects, first verify external state or ask the user. Do not retry blindly.";

fn interrupted_entries(conversation: ConversationId, turn: TurnId) -> Vec<Entry> {
    let mut entries = vec![Entry::ConversationStarted {
        id: conversation,
        ts_ms: 0,
    }];
    entries.extend(user_turn(conversation, turn, 1));
    entries.extend(open_step_with_calls(turn, &["c1", "c2"]));
    entries.push(Entry::Message {
        turn,
        seq: 2,
        step: Some(1),
        body: RecordedMessage::ToolResult {
            call_id: "c1".into(),
            text: "[]".into(),
            is_error: false,
            synthetic: false,
        },
        redactions: 0,
        ts_ms: 3,
    });
    entries
}

#[test]
fn an_open_turn_is_closed_as_interrupted_with_each_open_call_answered() {
    let turn = TurnId::new();
    let log =
        ConversationLog::from_entries(interrupted_entries(ConversationId::new(), turn)).unwrap();

    assert_eq!(
        repair(&log),
        vec![
            Entry::Message {
                turn,
                seq: 3,
                step: Some(1),
                body: RecordedMessage::ToolResult {
                    call_id: "c2".into(),
                    text: UNKNOWN_OUTCOME.into(),
                    is_error: true,
                    synthetic: true,
                },
                redactions: 0,
                ts_ms: 0,
            },
            Entry::StepEnded {
                turn,
                seq: 1,
                ts_ms: 0
            },
            Entry::TurnEnded {
                turn,
                end: "interrupted".into(),
                ts_ms: 0
            },
        ]
    );
}

#[test]
fn repair_run_twice_adds_nothing_the_second_time() {
    let conversation = ConversationId::new();
    let store = MemoryConversations::default();
    store
        .append(&interrupted_entries(conversation, TurnId::new()))
        .unwrap();

    store
        .append(&repair(&store.load(&conversation).unwrap()))
        .unwrap();

    let second = repair(&store.load(&conversation).unwrap());
    assert_eq!(second, Vec::<Entry>::new());
}
