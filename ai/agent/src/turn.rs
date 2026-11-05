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

use crate::dispatch::cap_result;
use crate::record::{
    ConversationId, ConversationLog, ConversationStore, Entry, Origin, RecordError,
    RecordedMessage, TurnId, derive_messages,
};
use crate::repair::{NOT_STARTED, OUTCOME_UNKNOWN};
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

impl From<TurnErrorKind> for TurnEnd {
    fn from(kind: TurnErrorKind) -> TurnEnd {
        TurnEnd::Error(kind)
    }
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
        cancel: CancellationToken,
    ) -> Result<TurnOutcome, TurnStartError> {
        let turn = TurnId::new();
        let mut started = Vec::new();
        let claim;
        let (conversation, turn_seq, history) = match request.conversation {
            Some(id) => {
                claim = Some(self.claim(id)?);
                let mut log = self.load(id).await?;
                let closers = stamped(repair(&log));
                if !closers.is_empty() {
                    self.append(closers).await.map_err(TurnStartError::Record)?;
                    for turn in log.turns.iter().filter(|t| t.end.is_none()) {
                        self.announce(
                            id,
                            turn.id,
                            turn.origin,
                            "interrupted",
                            u32::try_from(turn.steps()).unwrap_or(u32::MAX),
                        );
                    }
                    log = self.load(id).await?;
                }
                let next = log.turns.iter().map(|t| t.seq + 1).max().unwrap_or(1);
                (id, next, derive_messages(&log))
            }
            None => {
                claim = None;
                let id = ConversationId::new();
                started.push(Entry::ConversationStarted {
                    id,
                    ts_ms: now_ms(),
                });
                (id, 1, Vec::new())
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
            tokens: 0,
            open_step: None,
            open_calls: Vec::new(),
            in_flight: None,
            messages: history,
            unsent: 0,
            guard: RepeatGuard::new(),
            call_ids: HashSet::new(),
        };
        let (end, answer) = match self
            .drive(kernel, &mut run, &request.model, request.input, &cancel)
            .await
        {
            Ok(answer) => (TurnEnd::Completed, answer),
            Err(end) => (end, None),
        };
        let mut closing = Vec::new();
        if let Some(step) = run.open_step {
            for call_id in std::mem::take(&mut run.open_calls) {
                let text = if run.in_flight.as_ref() == Some(&call_id) {
                    OUTCOME_UNKNOWN
                } else {
                    NOT_STARTED
                };
                let body = RecordedMessage::ToolResult {
                    call_id,
                    text: text.to_owned(),
                    is_error: true,
                    synthetic: true,
                };
                closing.push(run.message_entry(Some(step), body, 0));
            }
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
        cancel: &CancellationToken,
    ) -> Result<Option<String>, TurnEnd> {
        let toolset = Toolset::from_specs(&kernel.actions());
        let defs = toolset.defs();
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
            if cancel.is_cancelled() {
                return Err(TurnEnd::Cancelled);
            }
            if run.steps >= self.limits.max_steps {
                return Err(TurnEnd::StepLimit);
            }
            let step = run.steps;
            self.record(vec![Entry::StepStarted {
                turn,
                seq: step,
                ts_ms: now_ms(),
            }])
            .await?;
            run.open_step = Some(step);
            let sending = self.gateway.send_chat(Outbound {
                request: ChatRequest {
                    model: model.to_owned(),
                    messages: run.messages.clone(),
                    tools: defs.clone(),
                },
                prior_redactions: std::mem::take(&mut run.unsent),
            });
            let reply = tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(TurnEnd::Cancelled),
                reply = sending => reply.map_err(error_kind)?,
            };
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
            let tool_calls = run.unique_ids(tool_calls);
            let mut shielded_calls = Vec::new();
            for call in &tool_calls {
                let arguments = self.shield(&call.arguments)?;
                redactions += arguments.count;
                shielded_calls.push(ToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: arguments.text,
                });
            }
            let raw_calls = tool_calls;
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
            run.tokens += usage.as_ref().map_or(0, |u| u.input + u.output);
            self.record(vec![entry, Entry::Usage { turn, step, usage }])
                .await?;
            run.open_calls = tool_calls.iter().map(|c| c.id.clone()).collect();
            if tool_calls.is_empty() {
                return Ok(text);
            }
            for (call, shown) in raw_calls.iter().zip(&tool_calls) {
                if cancel.is_cancelled() {
                    return Err(TurnEnd::Cancelled);
                }
                run.in_flight = Some(call.id.clone());
                let mut outcome = tokio::select! {
                    biased;
                    () = cancel.cancelled() => return Err(TurnEnd::Cancelled),
                    outcome = dispatcher.call(kernel, &toolset, call) => outcome,
                };
                if let Some(reminder) =
                    run.guard
                        .observe(&call.name, &call.arguments, &shown.arguments)
                {
                    outcome.text = format!("{}\n\n{reminder}", outcome.text);
                }
                let result = self.shield(&outcome.text)?;
                let result = Shielded {
                    text: cap_result(result.text),
                    ..result
                };
                run.unsent += result.count;
                run.messages.push(Message::Tool {
                    call_id: call.id.clone(),
                    text: result.text.clone(),
                });
                let entry = run.message_entry(
                    Some(step),
                    RecordedMessage::ToolResult {
                        call_id: call.id.clone(),
                        text: result.text,
                        is_error: outcome.is_error,
                        synthetic: false,
                    },
                    result.count,
                );
                self.record(vec![entry]).await?;
                run.open_calls.retain(|id| *id != call.id);
                run.in_flight = None;
            }
            self.record(vec![Entry::StepEnded {
                turn,
                seq: step,
                ts_ms: now_ms(),
            }])
            .await?;
            run.open_step = None;
            if run.tokens >= self.limits.max_tokens {
                return Err(TurnEnd::TokenLimit);
            }
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
        let severity = if end == "interrupted" || end.starts_with("error:") {
            Severity::Warn
        } else {
            Severity::Info
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
            .unwrap_or_else(|join| {
                tracing::error!(conversation = %id, panicked = join.is_panic(), "conversation load task did not finish");
                Err(RecordError::Read)
            })
            .map_err(|e| {
                if e != RecordError::UnknownConversation {
                    tracing::warn!(conversation = %id, kind = ?e, "conversation load failed");
                }
                match e {
                    RecordError::UnknownConversation => TurnStartError::UnknownConversation,
                    other => TurnStartError::Record(other),
                }
            })
    }

    async fn append(&self, entries: Vec<Entry>) -> Result<(), RecordError> {
        let store = Arc::clone(&self.store);
        let turn = entries.iter().find_map(turn_of);
        tokio::task::spawn_blocking(move || store.append(&entries))
            .await
            .unwrap_or_else(|join| {
                tracing::error!(turn = ?turn, panicked = join.is_panic(), "conversation write task did not finish");
                Err(RecordError::Write)
            })
            .inspect_err(|error| {
                tracing::warn!(turn = ?turn, kind = ?error, "conversation record failed");
            })
    }
}

fn turn_of(entry: &Entry) -> Option<TurnId> {
    match entry {
        Entry::ConversationStarted { .. } => None,
        Entry::TurnStarted { id, .. } => Some(*id),
        Entry::StepStarted { turn, .. }
        | Entry::Message { turn, .. }
        | Entry::Usage { turn, .. }
        | Entry::StepEnded { turn, .. }
        | Entry::TurnEnded { turn, .. } => Some(*turn),
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
    tokens: u64,
    open_step: Option<u32>,
    open_calls: Vec<String>,
    in_flight: Option<String>,
    messages: Vec<Message>,
    unsent: u32,
    guard: RepeatGuard,
    call_ids: HashSet<String>,
}

impl Run {
    fn unique_ids(&mut self, calls: Vec<ToolCall>) -> Vec<ToolCall> {
        calls
            .into_iter()
            .map(|mut call| {
                let mut id = call.id.clone();
                let mut n = 1;
                while !self.call_ids.insert(id.clone()) {
                    n += 1;
                    id = format!("{}-{n}", call.id);
                }
                call.id = id;
                call
            })
            .collect()
    }

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

fn stamped(mut entries: Vec<Entry>) -> Vec<Entry> {
    let now = now_ms();
    for entry in &mut entries {
        match entry {
            Entry::Message { ts_ms, .. }
            | Entry::StepEnded { ts_ms, .. }
            | Entry::TurnEnded { ts_ms, .. } => *ts_ms = now,
            _ => {}
        }
    }
    entries
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
