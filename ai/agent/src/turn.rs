// Ported from dsh (MIT, Copyright (c) 2026 DeepSeek): packages/core/agent-loop/src/agent.ts. See ai/agent/LICENSE-dsh.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_context::{
    AskError, ChatReply, ChatRequest, Gateway, Message, Outbound, Shielded, ToolCall,
};
use navori_kernel::{EventBus, Kernel, Severity};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::record::{
    ConversationId, ConversationLog, ConversationStore, Entry, Origin, RecordError,
    RecordedMessage, TurnId, derive_messages,
};
use crate::{Dispatcher, RepeatGuard, Toolset, repair};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_steps: u32,
    pub max_tokens: u64,
    pub action_timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_steps: 8,
            max_tokens: 200_000,
            action_timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnErrorKind {
    Provider,
    Shield,
    Audit,
    Record,
    NoKey,
    Keychain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnEnd {
    Completed,
    Cancelled,
    StepLimit,
    TokenLimit,
    Error(TurnErrorKind),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TurnStartError {
    #[error("Navori is shutting down.")]
    ShutDown,
    #[error("{0}")]
    Keychain(String),
    #[error("unknown conversation")]
    UnknownConversation,
    #[error("the conversation could not be recorded: {0}")]
    Record(RecordError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRequest {
    pub conversation: Option<ConversationId>,
    pub origin: Origin,
    pub model: String,
    pub input: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnOutcome {
    pub conversation: ConversationId,
    pub turn: TurnId,
    pub end: TurnEnd,
    pub answer: Option<String>,
    pub steps: u32,
}

pub struct Agent {
    gateway: Arc<Gateway>,
    store: Arc<dyn ConversationStore>,
    bus: EventBus,
    limits: Limits,
    active: Arc<Mutex<HashSet<ConversationId>>>,
}

impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl Agent {
    pub fn new(
        gateway: Arc<Gateway>,
        store: Arc<dyn ConversationStore>,
        bus: EventBus,
        limits: Limits,
    ) -> Agent {
        Agent {
            gateway,
            store,
            bus,
            limits,
            active: Arc::default(),
        }
    }

    pub async fn run_turn(
        &self,
        kernel: &Kernel,
        request: TurnRequest,
        _cancel: CancellationToken,
    ) -> Result<TurnOutcome, TurnStartError> {
        let turn = TurnId::new();
        let mut started = Vec::new();
        let claim;
        let (conversation, turn_seq, history) = match request.conversation {
            Some(id) => {
                claim = Some(self.claim(id)?);
                let mut log = self.load(id).await?;
                let closers = repair(&log);
                if !closers.is_empty() {
                    self.append(closers).await.map_err(TurnStartError::Record)?;
                    for turn in log.turns.iter().filter(|t| t.end.is_none()) {
                        let steps = turn
                            .messages
                            .iter()
                            .filter(|(_, _, m)| matches!(m, RecordedMessage::Assistant { .. }))
                            .count();
                        self.announce(
                            id,
                            turn.id,
                            turn.origin,
                            "interrupted",
                            u32::try_from(steps).unwrap_or(u32::MAX),
                        );
                    }
                    log = self.load(id).await?;
                }
                let next = log.turns.iter().map(|t| t.seq + 1).max().unwrap_or(0);
                (id, next, derive_messages(&log))
            }
            None => {
                claim = None;
                let id = ConversationId::new();
                started.push(Entry::ConversationStarted {
                    id,
                    ts_ms: now_ms(),
                });
                (id, 0, Vec::new())
            }
        };
        started.push(Entry::TurnStarted {
            id: turn,
            conversation,
            seq: turn_seq,
            origin: request.origin,
            model: request.model.clone(),
            ts_ms: now_ms(),
        });
        self.append(started).await.map_err(TurnStartError::Record)?;

        let mut run = Run {
            turn,
            seq: 0,
            steps: 0,
            open_step: None,
            messages: history,
            unsent: 0,
            guard: RepeatGuard::new(),
        };
        let (end, answer) = match self
            .drive(kernel, &mut run, &request.model, request.input)
            .await
        {
            Ok(answer) => (TurnEnd::Completed, answer),
            Err(kind) => (TurnEnd::Error(kind), None),
        };
        let mut closing = Vec::new();
        if let Some(step) = run.open_step {
            closing.push(Entry::StepEnded {
                turn,
                seq: step,
                ts_ms: now_ms(),
            });
        }
        closing.push(Entry::TurnEnded {
            turn,
            end: end_reason(end).to_owned(),
            ts_ms: now_ms(),
        });
        let end = match self.append(closing).await {
            Ok(()) => {
                self.announce(
                    conversation,
                    turn,
                    request.origin,
                    end_reason(end),
                    run.steps,
                );
                end
            }
            Err(_) => TurnEnd::Error(TurnErrorKind::Record),
        };
        drop(claim);
        Ok(TurnOutcome {
            conversation,
            turn,
            end,
            answer,
            steps: run.steps,
        })
    }

    async fn drive(
        &self,
        kernel: &Kernel,
        run: &mut Run,
        model: &str,
        input: String,
    ) -> Result<Option<String>, TurnErrorKind> {
        let toolset = Toolset::from_specs(&kernel.actions());
        let dispatcher = Dispatcher::new(self.limits.action_timeout);
        let turn = run.turn;
        let input = self.shield(&input)?;
        run.messages.push(Message::User {
            text: input.text.clone(),
        });
        run.unsent = input.count;
        let entry = run.message_entry(
            None,
            RecordedMessage::Input { text: input.text },
            input.count,
        );
        self.record(vec![entry]).await?;

        loop {
            let step = run.steps;
            self.record(vec![Entry::StepStarted {
                turn,
                seq: step,
                ts_ms: now_ms(),
            }])
            .await?;
            run.open_step = Some(step);
            let reply = self
                .gateway
                .send_chat(Outbound {
                    request: ChatRequest {
                        model: model.to_owned(),
                        messages: run.messages.clone(),
                        tools: toolset.defs(),
                    },
                    prior_redactions: std::mem::take(&mut run.unsent),
                })
                .await
                .map_err(error_kind)?;
            run.steps += 1;
            let ChatReply {
                text,
                reasoning,
                tool_calls,
                usage,
                ..
            } = reply;
            let mut redactions = 0;
            let mut shield_part = |part: Option<String>| -> Result<Option<String>, TurnErrorKind> {
                part.map(|p| {
                    let shielded = self.shield(&p)?;
                    redactions += shielded.count;
                    Ok(shielded.text)
                })
                .transpose()
            };
            let text = shield_part(text)?;
            let reasoning = shield_part(reasoning)?;
            let mut shielded_calls = Vec::new();
            for call in tool_calls {
                let arguments = self.shield(&call.arguments)?;
                redactions += arguments.count;
                shielded_calls.push(ToolCall {
                    arguments: arguments.text,
                    ..call
                });
            }
            let tool_calls = shielded_calls;
            run.unsent += redactions;
            run.messages.push(Message::Assistant {
                text: text.clone(),
                reasoning: reasoning.clone(),
                tool_calls: tool_calls.clone(),
            });
            let entry = run.message_entry(
                Some(step),
                RecordedMessage::Assistant {
                    text: text.clone(),
                    reasoning,
                    tool_calls: tool_calls.clone(),
                },
                redactions,
            );
            self.record(vec![entry, Entry::Usage { turn, step, usage }])
                .await?;
            if tool_calls.is_empty() {
                return Ok(text);
            }
            for call in &tool_calls {
                let mut outcome = dispatcher.call(kernel, &toolset, call).await;
                if let Some(reminder) = run.guard.observe(&call.name, &call.arguments) {
                    outcome.text = format!("{}\n\n{reminder}", outcome.text);
                }
                let result = self.shield(&outcome.text)?;
                run.unsent += result.count;
                run.messages.push(Message::Tool {
                    call_id: call.id.clone(),
                    text: result.text.clone(),
                });
                let entry = run.message_entry(
                    Some(step),
                    result_body(call, result.text, outcome.is_error),
                    result.count,
                );
                self.record(vec![entry]).await?;
            }
            self.record(vec![Entry::StepEnded {
                turn,
                seq: step,
                ts_ms: now_ms(),
            }])
            .await?;
            run.open_step = None;
        }
    }

    fn claim(&self, id: ConversationId) -> Result<Claim, TurnStartError> {
        let mut active = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        if !active.insert(id) {
            return Err(TurnStartError::Record(RecordError::Duplicate));
        }
        Ok(Claim {
            active: Arc::clone(&self.active),
            id,
        })
    }

    fn announce(
        &self,
        conversation: ConversationId,
        turn: TurnId,
        origin: Origin,
        end: &str,
        steps: u32,
    ) {
        tracing::info!(%conversation, %turn, end, steps, "turn ended");
        let severity = if matches!(end, "completed" | "cancelled") {
            Severity::Info
        } else {
            Severity::Warn
        };
        self.bus.emit(
            "ai",
            "ai.turn_ended",
            severity,
            format!("Navori AI finished a Turn: {end} after {steps} Steps"),
            json!({
                "conversation": conversation.to_string(),
                "turn": turn.to_string(),
                "origin": origin.as_str(),
                "end": end,
                "steps": steps,
            }),
        );
    }

    fn shield(&self, text: &str) -> Result<Shielded, TurnErrorKind> {
        self.gateway.shield(text).ok_or(TurnErrorKind::Shield)
    }

    async fn record(&self, entries: Vec<Entry>) -> Result<(), TurnErrorKind> {
        self.append(entries)
            .await
            .map_err(|_| TurnErrorKind::Record)
    }

    async fn load(&self, id: ConversationId) -> Result<ConversationLog, TurnStartError> {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || store.load(&id))
            .await
            .unwrap_or(Err(RecordError::Read))
            .map_err(|e| match e {
                RecordError::UnknownConversation => TurnStartError::UnknownConversation,
                other => TurnStartError::Record(other),
            })
    }

    async fn append(&self, entries: Vec<Entry>) -> Result<(), RecordError> {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || store.append(&entries))
            .await
            .unwrap_or(Err(RecordError::Write))
    }
}

fn result_body(call: &ToolCall, text: String, is_error: bool) -> RecordedMessage {
    RecordedMessage::ToolResult {
        call_id: call.id.clone(),
        text,
        is_error,
        synthetic: false,
    }
}

struct Claim {
    active: Arc<Mutex<HashSet<ConversationId>>>,
    id: ConversationId,
}

impl Drop for Claim {
    fn drop(&mut self) {
        self.active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

struct Run {
    turn: TurnId,
    seq: u32,
    steps: u32,
    open_step: Option<u32>,
    messages: Vec<Message>,
    unsent: u32,
    guard: RepeatGuard,
}

impl Run {
    fn message_entry(
        &mut self,
        step: Option<u32>,
        body: RecordedMessage,
        redactions: u32,
    ) -> Entry {
        let entry = Entry::Message {
            turn: self.turn,
            seq: self.seq,
            step,
            body,
            redactions,
            ts_ms: now_ms(),
        };
        self.seq += 1;
        entry
    }
}

fn end_reason(end: TurnEnd) -> &'static str {
    match end {
        TurnEnd::Completed => "completed",
        TurnEnd::Cancelled => "cancelled",
        TurnEnd::StepLimit => "step_limit",
        TurnEnd::TokenLimit => "token_limit",
        TurnEnd::Error(TurnErrorKind::Provider) => "error:provider",
        TurnEnd::Error(TurnErrorKind::Shield) => "error:shield",
        TurnEnd::Error(TurnErrorKind::Audit) => "error:audit",
        TurnEnd::Error(TurnErrorKind::Record) => "error:record",
        TurnEnd::Error(TurnErrorKind::NoKey) => "error:no_key",
        TurnEnd::Error(TurnErrorKind::Keychain) => "error:keychain",
    }
}

fn error_kind(error: AskError) -> TurnErrorKind {
    match error {
        AskError::NoKey => TurnErrorKind::NoKey,
        AskError::Keychain(_) => TurnErrorKind::Keychain,
        AskError::Shield => TurnErrorKind::Shield,
        AskError::Audit => TurnErrorKind::Audit,
        AskError::Send(_) => TurnErrorKind::Provider,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
