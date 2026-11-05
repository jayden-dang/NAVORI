// Ported from dsh (MIT, Copyright (c) 2026 DeepSeek): packages/core/session/src/repair.ts. See ai/agent/LICENSE-dsh.

use crate::record::{ConversationLog, Entry, LoggedTurn, RecordedMessage};

const OUTCOME_UNKNOWN: &str = "The tool call was interrupted after it was recorded, but no result was durably recorded. Its outcome is unknown. Decide whether to retry from the tool semantics: retry only if the operation is read-only or idempotent; if it may have side effects, first verify external state or ask the user. Do not retry blindly.";

pub fn repair(log: &ConversationLog) -> Vec<Entry> {
    log.turns
        .iter()
        .filter(|turn| turn.end.is_none())
        .flat_map(closers)
        .collect()
}

fn closers(turn: &LoggedTurn) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut seq = turn
        .messages
        .iter()
        .map(|(s, _, _)| *s)
        .max()
        .map_or(0, |s| s + 1);
    if let Some(step) = turn.open_step {
        for call_id in unanswered(turn, step) {
            entries.push(Entry::Message {
                turn: turn.id,
                seq,
                step: Some(step),
                body: RecordedMessage::ToolResult {
                    call_id,
                    text: OUTCOME_UNKNOWN.to_owned(),
                    is_error: true,
                    synthetic: true,
                },
                redactions: 0,
                ts_ms: 0,
            });
            seq += 1;
        }
        entries.push(Entry::StepEnded {
            turn: turn.id,
            seq: step,
            ts_ms: 0,
        });
    }
    entries.push(Entry::TurnEnded {
        turn: turn.id,
        end: "interrupted".to_owned(),
        ts_ms: 0,
    });
    entries
}

fn unanswered(turn: &LoggedTurn, step: u32) -> Vec<String> {
    let in_step = turn
        .messages
        .iter()
        .filter(|(_, s, _)| *s == Some(step))
        .map(|(_, _, body)| body);
    let mut pending: Vec<String> = Vec::new();
    for body in in_step {
        match body {
            RecordedMessage::Assistant { tool_calls, .. } => {
                pending.extend(tool_calls.iter().map(|c| c.id.clone()));
            }
            RecordedMessage::ToolResult { call_id, .. } => pending.retain(|id| id != call_id),
            RecordedMessage::Input { .. } => {}
        }
    }
    pending
}
