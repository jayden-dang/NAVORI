# Design: Agent loop and Conversation

Feature code: LOOP
Status: Draft
Date: 2026-11-05
Requirements: ./requirements.md

<!--
Written in an unattended run under /Users/jayden/Developer/experiments/NAVORI/.skills/_pending-agent-loop/auto/charter.md. `agent-decided` = picked by an agent session; `Weigh W-n` = agent-loop-interpret's pick in the run ledger; `L-n` = a charter Lock. Charter Lock L22 (charter.md:42): the Conversation, Turn, Step and usage tables are design work for this spec. Nothing is migrated in this run. Every persisted format below is labelled and listed under "Persisted formats".
Port source: dsh (MIT, Copyright (c) 2026 DeepSeek), packages/core/{agent-loop,session,tools} and packages/guard/{repeat-tool-reminder,timeout-policy}. Digests: auto/scan.md, auto/scan-design.md; subgraph: auto/subgraph-design.md.
-->

## Context

Today Navori AI makes one text call. `navori ask` sends a single user message through the Context Gateway, which reads the key, Shields the text, writes a `context_audit` row and then calls a text-in/text-out `Sender` (ai/context/src/gateway.rs:14-21, 122-155). That `Sender` is wired in `crates/ffi` to the `openai_compat` adapter (crates/ffi/src/ai.rs:58-97). The adapter's types know one role, carry no tools, reasoning or usage, and reject a reply with null `content` (ai/llm/src/lib.rs:13-33; openai_compat.rs:132-145). The Kernel lists every `ActionSpec` and dispatches Envelopes, but it checks neither the action name nor its Risk (crates/kernel/src/runtime.rs:323-342). There is no `ai/agent` crate.

The binding constraint is that **only the Context Gateway may call the provider, and everything it sends or stores passes the Shield first** (ARCH-6, ARCH-7; L12, L13), under a Compat obligation of External, where every table and payload shape is permanent at first ship (docs/agents/project.md:35). That rules out the shortcut of letting the loop call `LlmProvider::complete` itself and Shield on the side. It also rules out storing the Conversation as `derive(Serialize)` of the adapter's types (W16), because ROAD-11 will reshape those types.

The loop is dsh's design, cut to fit (auto/approaches.md AP1). A Turn is a sequence of Steps. Each Step is one model request plus the Actions it calls. Tool errors go back to the model as results. Every tool call in the record has exactly one result. Navori adds what dsh does not have: a trusted Turn origin (THR-11), a Read-only dispatch point, step and token limits, a per-call Context audit row and a Journal Event.

## Decisions

1. **Crates.** A new crate, `navori-agent` at `ai/agent` (L17), depends on `navori-context`, `navori-kernel`, `navori-protocol`, `tokio`, `tokio-util`, `futures`, `serde`, `serde_json`, `thiserror`, `tracing` and `ulid` (all already workspace dependencies). It does not depend on `navori-llm`, and a new check-deps rule forbids that edge by its exact name (W15), so no code of the Agent loop can name the provider (LOOP-5.1). It does not depend on `navori-store` or `navori-redact`: the record goes through a trait implemented in `crates/ffi` (the `AuditSink` precedent, ai.rs:116-168), and the Shield through `Gateway::shield`.
2. **Canonical types stay in `ai/llm`** (W15; docs/ARCHITECTURE.md:172-186). `ai/context` takes a types-only edge on `ai/llm` and re-exports the types, never `LlmProvider`. `crates/ffi` implements the Gateway's new `ChatSender` over `LlmProvider`. The dependency docs change accordingly (docs/codebase/dependencies.md:29, :42).
3. **One Shield, applied once per item, counted once.** `Gateway::shield(&self, text) -> Option<Shielded>` runs the Gateway's own injectable fail-closed check (`catch_unwind` around the Shield fn, plus `well_formed`, gateway.rs:173-205). It returns `Shielded { text, count }`, where `count` excludes findings whose span is already a `[REDACTED:…]` marker. The design review found that `secret_assignment` and `url_password` re-match their own marker, so the text is idempotent but the raw finding count is not (auto/design-review.md item 8). The loop Shields each item once, when it is recorded (LOOP-3.6), and keeps its `count`. A request carries `prior_redactions`, the sum of the counts of the messages it sends for the first time. The Gateway Shields every text of the request again, since it is the enforcement point, and writes `redacted_count = prior_redactions + its own fresh count` (W9: per request, never a Turn total; resent history adds 0). A unit test pins a second pass over shielded text giving count 0 for every rule.
4. **Non-streaming, serial Steps, serial tool calls.** Each model call is one `stream:false` request (LOOP-1.5). The tool calls of a Step run one after another, in the model's order (`agent-decided`, S-parallel). A Module actor serialises its own Actions anyway (scan.md §5.9), and the results keep the model's order (LOOP-1.2).
5. **Tool names.** `{module}_{action}`, for example `ports_list` and `awake_status` (`agent-decided`, S-tools). DeepSeek allows `[a-zA-Z0-9_-]` up to 128 characters (research). Names map back through a table built from `Kernel::actions()` at the start of each Turn, never by splitting the string. Two Actions that would get the same name are both left out, with a warning.
6. **Per-Action parameter schema.** It is cut from the module's shared `schema_for!` enum schema (runtime.rs:154-163). A unit variant gets `{"type":"object","properties":{},"additionalProperties":false}`. A struct or tuple variant gets the schema under its variant key, with the enum's `$defs` re-attached. The plan review printed the schemars 1.2.3 output (auto/plan-review.md item 5). A documented unit variant is a `{"const": name}` entry. Undocumented unit variants are merged into one `{"type":"string","enum":[…]}` entry. A struct variant is an object entry keyed by its name. The cut rule matches all three forms, and the first task pins them with a test. An Action whose variant cannot be found is not offered, and a warning is logged (`agent-decided`).
7. **The one dispatch point** is `navori_agent`'s `Dispatcher::call`. It checks, in this order: the name is known; the catalog Risk is `read` (W2, LOOP-2.2); the arguments parse as JSON. Then it builds the Envelope: a unit variant becomes the bare name, any other variant `{name: args}`. It calls `Kernel::dispatch` under a 10 s limit (`agent-decided`, ≤ 30 s; LOOP-1.9). For a unit Action, the arguments `""`, `"null"` and `"{}"` all mean no arguments. Any other value is refused as not fitting, and nothing is dispatched (`agent-decided`). The dispatcher receives the arguments as the model wrote them. The Module is local, so nothing leaves the machine (ARCH-6/7 cover what is stored or sent). The record keeps the Shielded copy (LOOP-3.2, 3.6; `agent-decided`). A timed-out or cancelled call leaves its request queued or running in the Module (the dropped receiver is ignored, runtime.rs:25); for Read Actions that is harmless. For LOOP-1.8, no request reaches the Module: an argument that does not fit the schema goes through `Kernel::dispatch`, but the typed parse refuses it (`DispatchError::InvalidAction`, runtime.rs:167-176) before anything reaches the Module's inbox.
8. **Limits** (`agent-decided`, S-limits): at most 8 Steps per Turn, and a token limit of 200 000. The limit counts the sum of input and output tokens over the Turn's model calls; a call that reported no usage counts 0. Both are fields of `Limits`, which tests inject.
9. **Turn origin** (`agent-decided`, S-origin): `Origin::User`, stored as `user`. It is the only value until ROAD-15/ROAD-37 add others. It has no CHECK constraint, so new values need no migration.
10. **Shield refusal mid-Turn** (`agent-decided`, S-shield-refusal): the call is not sent, a `blocked` row is written (LOOP-5.4), and the Turn ends with the error `shield`. The whole request is withheld, as ASK refuses the whole ask (ASK design D6). A model cannot be told about a refusal without sending it something.
11. **Replies cut at the output limit, Turn numbering, concurrent Turns** (`agent-decided`). A reply with `finish = Length` is handled like any other reply: its tool calls run, and with none the Turn completes with the text it has. A Turn's `seq` is one more than the highest in its loaded Conversation, starting at 1. A second Turn started on the same Conversation at the same time is refused by `UNIQUE (conversation_id, seq)`, giving `TurnStartError::Record(Duplicate)`. Message `seq` within a Turn is a counter the driver keeps.
12. **Turn end reasons** (LOOP-3.7, `agent-decided` after dsh `TurnEndReason`): `completed`, `cancelled`, `step_limit`, `token_limit`, `error`, `interrupted`. The `error` reason carries one of the kinds `provider`, `shield`, `audit`, `record`, `no_key` or `keychain`.
13. **Journal** (W13/W14): one Event when a Turn ends, including a Turn repaired as `interrupted`. Module `ai`, kind `ai.turn_ended`. Severity `info`, or `warn` for `error` and `interrupted`. The summary holds no content, and the payload holds ids, fixed tokens and counts only. The Event is emitted through the Kernel's `EventBus`, which redacts and journals it (kernel/src/bus.rs:31-57). It is emitted only after the Turn's end is recorded. If that write fails, no Event goes out, and the repair on the next open emits the `interrupted` Event. So each Turn has at most one, and exactly one once it has been repaired.
14. **Storage** (L22, `agent-decided` throughout): Owner `ai_session` with five tables, all in data.md:37's `ai_session_*` family. Message bodies are a versioned JSON encoding owned by `navori-agent` (W16). It is migrated lazily on the first Turn (ARCH-17). Writes run through `spawn_blocking` (LOOP-11.3). See "Persisted formats".
15. **Starter seam** (`agent-decided`). `NavoriCore::run_agent_turn(TurnRequest, CancellationToken) -> Result<TurnOutcome, TurnStartError>` is a plain Rust `pub` method, not `#[uniffi::export]`, beside `start_with` (crates/ffi/src/lib.rs:262). It is not an FFI, IPC, protocol or CLI entry, so LOOP-10.4 holds. ROAD-15/ROAD-37 will wrap it.
16. **`FakeLlm`** lives in `ai/llm` behind its `fake` feature. It holds a scripted queue of replies and errors, records each request, and can be held at a barrier. It enters ffi tests through a new `AiDeps::make_provider` field. `AiDeps` also gains `make_conversations: Arc<dyn Fn(Store) -> Arc<dyn ConversationStore>>` (production: `StoreConversations::new`), so a test can wrap the store with a gate (LOOP-11.3). `navori-llm` joins check-deps' `FAKE_GUARDED` list (xtask/src/main.rs:793), so its `fake` feature is only ever a dev-dependency. In production, `AiDeps` builds one `Arc<OpenAiCompat>` that both `LlmSender` (ask) and `LlmChatSender` (loop) share, so there is still one HTTP client. `LlmSender::new` therefore takes `Arc<OpenAiCompat>`.
17. **No ADR.** The three-part gate fails. The dependency-edge change is cheap to reverse and is recorded in docs/codebase/dependencies.md. The storage shape is hard to reverse but not surprising: it follows data.md:37 and dsh's log.

Spine this design relies on: ARCH-1, ARCH-3, ARCH-4, ARCH-5, ARCH-6, ARCH-7, ARCH-12, ARCH-15, ARCH-17.

## Architecture

Vocabulary for every section below: **module**, **interface**, **implementation**,
**seam** (public surface where behavior is observable and substitutable). Prefer
deep modules — an interface much simpler than what it hides.

```
 starter (test / live demo; ROAD-15 later)
   │  NavoriCore::run_agent_turn(TurnRequest, cancel)            crates/ffi
   ▼
 AiRuntime ── lazily builds ──► Agent (ai/agent)
                                 │  run_turn(&Kernel, request, cancel)
          ┌──────────────────────┼──────────────────────────────┐
          ▼                      ▼                              ▼
  ConversationStore        Gateway::send_chat (ai/context)   Dispatcher::call (ai/agent)
  (trait; impl in ffi:     key → shield → audit row →        name → Risk=read → JSON →
   StoreConversations on    ChatSender (impl in ffi:          Kernel::dispatch (≤10 s)
   Store, Owner ai_session) LlmChatSender → OpenAiCompat)
          │
          └─► EventBus::emit("ai", "ai.turn_ended", …)  → Journal
```

### Canonical chat types and the `openai_compat` adapter (`ai/llm`)

Satisfies: LOOP-1.5, LOOP-1.6, LOOP-1.7, LOOP-10.1
Reuse: rung 2 — extends `ai/llm` (`ChatRequest`, `ChatReply`, `LlmProvider`, `OpenAiCompat`)
Respects: ARCH-17
Surface:
- `crates/ffi/src/ai.rs` `LlmSender` (builds `Message { role: Role::User, text }`, reads `reply.text`, ai.rs:58-97) — **replace**: it builds `Message::User { text }`, reads `reply.text` (now an `Option`), and takes `Arc<OpenAiCompat>`. Callers `LlmSender::new` at crates/ffi/tests/ask.rs:713, :834 and the `pub use` at lib.rs:35 — **replace**. An empty or missing text is still `Malformed` on the ask path, so ASK-4 behavior holds.
- `ai/llm/tests/openai_compat.rs` (`hello()` builds `Message { role: Role::User, .. }`, reads `reply.text` as a `String`, :30; asserts the body keys are exactly `messages, model, stream`, :44-52) — **replace**: the constructors and the reply reads are updated mechanically. The key-set assertion stays unchanged and must pass, because new fields are `skip_serializing_if` empty or `None` (LOOP-10.1).
- `ai/llm/tests/live_deepseek.rs` — **replace**: uses `Message::User` and the `Option` text.
- `openai_compat.rs` internals (`WireMessage`, `Role::User =>`, `ChatReply { text }`, `WireContent { content: String }`, :53-56, 86, 132-145) — **replace**.
- DeepSeek wire (external) — **frozen**: the design builds around it (research-deepseek-tools.md) and changes nothing on the provider side.
Interface:
```rust
pub enum Message {
    User { text: String },
    Assistant { text: Option<String>, reasoning: Option<String>, tool_calls: Vec<ToolCall> },
    Tool { call_id: String, text: String },
}
pub struct ToolCall { pub id: String, pub name: String, pub arguments: String } // arguments: the model's raw text
pub struct ToolDef { pub name: String, pub description: String, pub parameters: serde_json::Value }
pub struct ChatRequest { pub model: String, pub messages: Vec<Message>, pub tools: Vec<ToolDef> }
pub struct ChatReply { pub text: Option<String>, pub reasoning: Option<String>, pub tool_calls: Vec<ToolCall>,
                       pub usage: Option<Usage>, pub finish: Finish }
pub struct Usage { pub input: u64, pub output: u64, pub cache_hit: Option<u64>, pub cache_miss: Option<u64> }
pub enum Finish { Stop, ToolCalls, Length, Other(String) }
// LlmProvider::complete(ChatRequest, &Secret) -> BoxFuture<Result<ChatReply, LlmError>>  (unchanged signature)
```
Depth: n/a — extends `ai/llm`.
Locality: extend `ai/llm/src/lib.rs` and `openai_compat.rs` only. Neighbors: `crates/ffi/src/ai.rs` LlmSender is a mechanical update. These types are a subset of the seed canonical format (docs/ARCHITECTURE.md:172-186: no `system` role, no `ContentPart`, no `tool_choice`); ROAD-11 reshapes them, and the stored encoding does not depend on them (W16).

Wire mapping in `openai_compat.rs`:
- **Sending.** `tools` becomes `[{type:"function", function:{name, description, parameters}}]` and is omitted when empty. An assistant message becomes `{role:"assistant", content: text|null, reasoning_content?: reasoning, tool_calls?: [{id, type:"function", function:{name, arguments}}]}`, with `reasoning_content` sent whenever recorded (LOOP-1.7). A tool message becomes `{role:"tool", tool_call_id, content}`. No `thinking` field is sent, so the Provider's default (thinking on) stays (LOOP-1.7 `agent-decided`).
- **Parsing.** `choices[0].message` with `content: Option<String>`, `reasoning_content`, `tool_calls`, `finish_reason` and `usage`. `prompt_tokens` maps to `input`, `completion_tokens` to `output`, `prompt_cache_hit_tokens` to `cache_hit`, and `prompt_cache_miss_tokens` to `cache_miss`. A reply with no text and no tool call is `Malformed`, as is a reply with no choice. A null or empty `content` with tool calls is accepted (LOOP-1.6); fixtures cover both shapes, since which one DeepSeek sends is unconfirmed.

### `FakeLlm` (`ai/llm`, feature `fake`)

Satisfies: LOOP-8.1
Reuse: rung 2 — the `fake` feature pattern of `navori-secrets` (crates/secrets/src/fake.rs:16-55); `navori-llm` is added to check-deps' `FAKE_GUARDED` (xtask/src/main.rs:793)
Interface: `FakeLlm::new(script: Vec<Result<ChatReply, LlmError>>)`; `requests() -> Vec<ChatRequest>`; `hold_next() -> Release` (parks the next call until released); it implements `LlmProvider`.
Depth: a scripted queue that answers `complete` in order and records what it was sent. Callers know only the script and the recorded requests.
Locality: new file `ai/llm/src/fake.rs`; `ai/llm/Cargo.toml` gains `[features] fake = []`. `ai/llm/tests/default_tests_use_fakes.rs` extends its scan to `ai/agent/tests/` and `crates/ffi/tests/agent.rs`, so LOOP-8.1 is checked rather than left to review.

### Context Gateway: structured send and the public Shield (`ai/context`)

Satisfies: LOOP-5.1, LOOP-5.2, LOOP-5.3, LOOP-5.4, LOOP-5.5, LOOP-5.6, LOOP-10.2, LOOP-11.4
Reuse: rung 2 — extends `Gateway` (`read_key`, `inspect`, `write_row`, gateway.rs:122-190) and keeps `AuditRow` and `AuditSink` unchanged
Respects: ARCH-6, ARCH-7
Security: THR-1, THR-10
Surface:
- `crates/ffi/src/ai.rs` `AiRuntime::gateway()` (builds `Gateway::new(...)`) — **replace**: it adds `.with_chat_sender(Arc<dyn ChatSender>)`. `Gateway::new`'s signature is unchanged.
- `ai/context` tests building `Gateway::new`/`with_shield` (ai/context/tests/gateway.rs:146, :156, :421) — **frozen**: kept unchanged, since the builder is additive.
- `context_audit` rows (persisted, read by ROAD-12 later) — **frozen**: columns and values are untouched (W6, W7). Only data.md:27's wording changes, from "per ask" to "per model call" (W8). ROAD-12 inherits the meaning.
- `navori-context` dependency row (docs/codebase/dependencies.md:29) — **replace**: it gains `navori-llm`, types only (W15).
Interface:
```rust
pub use navori_llm::{Message, ToolCall, ToolDef, ChatRequest, ChatReply, Usage, Finish}; // never LlmProvider
pub struct Shielded { pub text: String, pub count: u32 }      // count excludes spans already `[REDACTED:…]`
pub trait ChatSender: Send + Sync {
    fn send<'a>(&'a self, key: &'a Secret, request: ChatRequest) -> BoxFuture<'a, Result<ChatReply, SendError>>;
}
pub struct Outbound { pub request: ChatRequest, pub prior_redactions: u32 }
impl Gateway {
    pub fn with_chat_sender(self, sender: Arc<dyn ChatSender>) -> Self;
    pub fn shield(&self, text: &str) -> Option<Shielded>;    // fail-closed: None on panic or malformed findings
    pub async fn send_chat(&self, outbound: Outbound) -> Result<ChatReply, AskError>;
}
```
Depth: n/a — extends `Gateway`.
Locality: extend `ai/context/src/gateway.rs` (and lib.rs re-exports); `ai/context/Cargo.toml` gains `navori-llm`. Leave `ask` and its order untouched (LOOP-10.2).

`send_chat` runs in the order `ask` uses: read the key (missing → `NoKey`), Shield every text of every message (user text, assistant text and reasoning, tool-call arguments, tool text) with `self.shield`, then write the audit row, then send. The audit row is `{provider: "deepseek", kinds: ["text"], redacted_count: prior_redactions + findings, outcome}`.
- A Shield failure on any text writes a best-effort `blocked` row and returns `AskError::Shield`; nothing is sent (LOOP-5.4).
- An audit failure returns `AskError::Audit`; nothing is sent (LOOP-5.5).
- A send without a chat sender returns `AskError::Send("no chat sender")`.

No message text reaches the row (LOOP-5.6). Tool definitions are Shielded too (ARCH-6 has no own-text exception): each `ToolDef`'s name, its description, and every string inside its `parameters` JSON, keys and values, go through `self.shield`. A key the Shield would change blocks the call, and nothing is sent. This is fail-closed like the messages, and their fresh findings, expected 0, are added to the row's count (review-design-r1 finding 1).

### Tool registry (`ai/agent/src/tools.rs`)

Satisfies: LOOP-1.1
Reuse: rung 2 — `Kernel::actions()` (runtime.rs:323-328) and the `ActionSpec` schema built by `schema_for!` (runtime.rs:154-163)
Respects: ARCH-4
Interface: `Toolset::from_specs(&[ActionSpec]) -> Toolset`; `toolset.defs() -> Vec<ToolDef>`; `toolset.lookup(name) -> Option<&Tool>`, where `Tool { module, action, risk, unit: bool }`.
Depth: if it vanished, callers would still need only "which tool names exist, and which Action and Risk each maps to". Name derivation and schema cutting (Decisions 5, 6) stay hidden.
Locality: new file. Leave the Kernel as it is (LOOP-10.5).

`from_specs` keeps every spec in the lookup table, so a Write name is recognised and refused rather than reported unknown (LOOP-2.3 versus 2.4). Only `risk == read` specs go into `defs()` (LOOP-1.1, `agent-decided`).

### The one dispatch point (`ai/agent/src/dispatch.rs`)

Satisfies: LOOP-1.2, LOOP-1.8, LOOP-1.9, LOOP-2.1, LOOP-2.2, LOOP-2.3, LOOP-2.4, LOOP-2.5, LOOP-2.6, LOOP-11.6
Reuse: rung 2 — `Kernel::dispatch` and `Envelope` (runtime.rs:330-342; protocol lib.rs:35-39); `tokio::time::timeout` (rung 3)
Respects: ARCH-3, ARCH-5
Security: THR-2, THR-11
Interface: `Dispatcher::call(&self, kernel: &Kernel, toolset: &Toolset, call: &ToolCall) -> ToolOutcome`, where `ToolOutcome { text: String, is_error: bool }`. `call.arguments` is the raw text the model wrote (Decision 7).
Depth: if it vanished, callers would need to rebuild "map a tool call to an Envelope only when its catalog Risk is read". The check order, the Envelope shape and the timeout are hidden.
Locality: new file. It is the only place in `ai/agent` that names `Kernel::dispatch`, and a source test asserts that (LOOP-2.1).

Outcomes, each a result returned to the model, after which the Turn goes on (LOOP-2.5, LOOP-1.9):

| Case | Text sent to the model | Dispatched? |
|---|---|---|
| Unknown name | `Error: no Action named <name>.` | no |
| Catalog Risk `write`/`destructive` | `Error: Navori AI cannot run <name> yet: it changes things and needs your approval, which is not available.` (`agent-decided`) | no |
| Arguments not JSON | `Error: the arguments are not valid JSON: <parser message>.` | no |
| Arguments do not fit (`InvalidAction`) | `Error: the arguments do not fit <name>: <message>.` | Module receives nothing |
| Module error / unavailable | `Error: <DispatchError text>.` | yes |
| Over 10 s | `Error: <name> did not answer within 10 s.` | yes (the Module may still run it) |
| Reply | the reply `Value` as compact JSON | yes |

LOOP-2.6 is a new `crates/ffi/tests/actions_risk.rs`. For every `ActionSpec` of a started core, it builds a sample value of the Action from JSON, deserializes it into that module's enum (`SystemAction`, `AwakeAction`, `PortsAction`, `IntegrationsAction`; all `pub`, scan-design §7) and asserts `action.risk() == spec.risk`. An Action with no sample fails the test.

### Repeat reminder guard (`ai/agent/src/guard.rs`)

Satisfies: LOOP-6.3
Reuse: rung 7 — a port of dsh `guard/repeat-tool-reminder` (240 TS lines); no Rust counterpart exists in the repo
Interface: `RepeatGuard::observe(&mut self, name: &str, arguments: &str) -> Option<String>`, which returns the reminder to append. A new `RepeatGuard` is made for each Turn, and each Turn is a new input.
Depth: callers know "feed each call and append what comes back". The canonical key (name plus deep key-sorted parsed arguments, or the raw text when they don't parse), the chain counter and the thresholds 3/5/8 stay inside.
Locality: new file.

Reminders use dsh's text: the first-threshold sentence at 3, and the "Repeated tool call detected" block at 5 and 8, with the arguments truncated to 500 characters. The reminder is appended to that call's tool result after a blank line, instead of dsh's separate user message. That keeps exactly one result per call (LOOP-7.2). Refused calls count too, as in dsh.

### Turn driver (`ai/agent/src/turn.rs`)

Satisfies: LOOP-1.3, LOOP-1.4, LOOP-3.1, LOOP-3.2, LOOP-3.3, LOOP-3.4, LOOP-3.5, LOOP-3.6, LOOP-3.7, LOOP-3.9, LOOP-3.10, LOOP-6.1, LOOP-6.2, LOOP-7.1, LOOP-7.2, LOOP-11.5
Reuse: rung 7 — a port of dsh `core/agent-loop` `turn()`/`step()` (agent-loop/src/agent.ts:327-578) without Cordis, streaming, retry waterfalls or the inbox
Respects: ARCH-1, ARCH-6
Security: THR-11
Interface:
```rust
pub struct Agent { /* gateway, store, bus, limits */ }
impl Agent {
    pub fn new(gateway: Arc<Gateway>, store: Arc<dyn ConversationStore>, bus: EventBus, limits: Limits) -> Self;
    pub async fn run_turn(&self, kernel: &Kernel, request: TurnRequest, cancel: CancellationToken)
        -> Result<TurnOutcome, TurnStartError>;
}
pub struct ConversationId(String); pub struct TurnId(String);           // ULIDs
pub enum Origin { User }                                                 // stored `user`
pub enum TurnErrorKind { Provider, Shield, Audit, Record, NoKey, Keychain }
pub enum TurnStartError { ShutDown, Keychain(String), UnknownConversation, Record(RecordError) } // before any Turn row exists
pub struct Toolset { /* name → Tool */ }  pub struct Tool { pub module: String, pub action: String, pub risk: Risk, pub unit: bool }
pub struct TurnRequest { pub conversation: Option<ConversationId>, pub origin: Origin, pub model: String, pub input: String }
pub struct TurnOutcome { pub conversation: ConversationId, pub turn: TurnId, pub end: TurnEnd, pub answer: Option<String>, pub steps: u32 }
pub enum TurnEnd { Completed, Cancelled, StepLimit, TokenLimit, Error(TurnErrorKind) } // `interrupted` exists only in the record, written by repair
pub struct Limits { pub max_steps: u32, pub max_tokens: u64, pub action_timeout: Duration } // default 8, 200_000, 10 s
```
Depth: if it vanished, callers would still need only "give it an origin, a model, an input and a cancel token; get back an end reason and an answer". The record sequence, message derivation, limits, cancel pairing, guards and Event stay hidden.
Locality: new file, plus `ai/agent/src/lib.rs`.

One Turn, in order:
1. If `conversation` is given, load it and repair it first (next section). Otherwise create a Conversation. Record the Turn start with `origin`, which comes from `TurnRequest` and never from text (LOOP-3.1), and with `model` (LOOP-1.4).
2. Shield and record the input (LOOP-3.6). A `None` from `shield` ends the Turn as `error(shield)`.
3. **Step loop.** If `steps == max_steps`, end `step_limit` (LOOP-6.1). Otherwise:
   - Record the Step start.
   - Build `Outbound`: the messages derived from the Conversation (LOOP-3.10) plus `toolset.defs()`, with `prior_redactions` = the counts of the messages first sent in this request.
   - If `cancel.is_cancelled()`, stop here. Otherwise `select! { biased; _ = cancel.cancelled() => …, r = gateway.send_chat(..) => … }` (LOOP-7.1). The cancel arm comes first, so a set cancel is never raced by a send's first poll.
   - Shield and record the reply with its tool calls' raw arguments (LOOP-3.2), and record its usage, or that none was reported (LOOP-3.4, 3.5).
   - If there are no tool calls, end `completed` with the reply text as the answer (LOOP-1.3).
   - Otherwise, for each call in order, check `cancel.is_cancelled()`, then run a `biased` `select!` with the cancel arm first around `dispatcher.call`. Append the `RepeatGuard` reminder, then Shield and record the result paired by `call_id` (LOOP-3.3).
   - Record the Step end. If the Turn's summed tokens are at or over `max_tokens`, end `token_limit` (LOOP-6.2).
4. **End.** Every recorded call without a result first gets a synthetic error result (LOOP-7.2), using dsh's two texts (repair.ts:14-42): "outcome unknown" for the call that was in flight, and "not started" for calls the Turn never reached. Then the end reason is recorded with those results in one `append` (LOOP-3.7). Only after that write succeeds is `ai.turn_ended` emitted (Decision 13).

- A record write that fails ends the Turn as `error(record)` and sends no further request (LOOP-3.9). The closers and the end are still attempted once. If they fail too, no Event is emitted, and the next open's repair closes the Turn and emits the one `interrupted` Event (LOOP-4.1). LOOP-7.2 then holds once that repair has run, not at the moment the Turn returns.
- A Turn with no key saved creates its Conversation and Turn rows (and runs the lazy migration) before the first `send_chat` finds no key. It then ends `error(no_key)`, which leaves a recorded Turn, as intended.
- A Gateway error maps to `error(no_key|keychain|shield|audit|provider)`.

Logging: `tracing` fields are ids, counts, end reasons, kinds and tool names only. No message text, reasoning, arguments or results are logged unless debug logging is on (LOOP-11.5).

Message derivation (dsh surface.ts:120-165): a fold over the Conversation's message rows in order. `input` becomes `Message::User`, `assistant` becomes `Message::Assistant` (with text, reasoning and tool calls), and `tool_result` becomes `Message::Tool`. Turn and Step rows give nothing.

### Crash repair (`ai/agent/src/repair.rs`)

Satisfies: LOOP-4.1
Reuse: rung 7 — a port of dsh `core/session/src/repair.ts` `openTurnClosers` / `ToolCallRecovery` (211 TS lines)
Interface: `repair(log: &ConversationLog) -> Vec<Entry>`, appended with one `append(&closers)` call, so they land in one transaction. It returns the closers to append, which is empty when the last Turn has an end.
Depth: callers append what it returns. Finding the open Turn, the open Step and the unanswered calls stays hidden.
Locality: new file. The Turn driver calls it before a continuing Turn, and the closers end with `TurnEnded(interrupted)`, which also emits `ai.turn_ended` (W13 refinement 2).

The closers are: a synthetic error result for each unanswered call, using dsh's "outcome is unknown" text (repair.ts:33-42), then `StepEnded` if a Step is open, then `TurnEnded(interrupted)`. Repair is lazy: it runs only when a Conversation is continued, never at startup (ARCH-17).

### Conversation record (`ai/agent/src/record.rs` + `crates/ffi/src/ai.rs` `StoreConversations`)

Satisfies: LOOP-3.8, LOOP-11.3
Reuse: rung 2 — the `StoreAudit` lazy-migrate pattern (crates/ffi/src/ai.rs:116-168), `Store::migrate` and `with_conn` (crates/store/src/lib.rs:119-169), and `spawn_blocking` (gateway.rs:160,187)
Respects: ARCH-12, ARCH-17
Interface:
```rust
pub trait ConversationStore: Send + Sync {          // blocking; the Agent calls it through spawn_blocking
    fn append(&self, entries: &[Entry]) -> Result<(), RecordError>;   // all entries in one transaction
    fn load(&self, conversation: &ConversationId) -> Result<ConversationLog, RecordError>;
}
pub enum Entry {
    ConversationStarted { id: ConversationId, ts_ms: i64 },
    TurnStarted { id: TurnId, conversation: ConversationId, seq: u32, origin: Origin, model: String, ts_ms: i64 },
    StepStarted { turn: TurnId, seq: u32, ts_ms: i64 },
    Message { turn: TurnId, seq: u32, step: Option<u32>, body: RecordedMessage, redactions: u32, ts_ms: i64 },
    Usage { turn: TurnId, step: u32, usage: Option<Usage> },
    StepEnded { turn: TurnId, seq: u32, ts_ms: i64 },
    TurnEnded { turn: TurnId, end: String, ts_ms: i64 },          // `completed` … `error:<kind>` | `interrupted`
}
pub enum RecordedMessage { Input { text }, Assistant { text, reasoning, tool_calls }, ToolResult { call_id, text, is_error, synthetic } }
pub struct ConversationLog { pub turns: Vec<LoggedTurn> }        // in seq order
pub struct LoggedTurn { pub id: TurnId, pub seq: u32, pub origin: Origin, pub messages: Vec<(u32, Option<u32>, RecordedMessage)>,
                       pub open_step: Option<u32>, pub end: Option<String> }
impl ConversationLog { pub fn from_entries(entries: Vec<Entry>) -> Result<ConversationLog, RecordError>; } // the one fold
pub enum RecordError { UnknownConversation, Migration, Write, Read, Duplicate }
```
Depth: two methods, append and load. Tables, the encoding, the lazy migration and the transactions stay hidden in `StoreConversations`.
Locality: new `ai/agent/src/record.rs` for the trait and the versioned body encoding, and `StoreConversations` in `crates/ffi/src/ai.rs` beside `StoreAudit`. Leave the Store crate as it is (LOOP-10.6).

`StoreConversations::append` and `load` both run `ensure_table()` first (`load` of an unknown id returns `RecordError::UnknownConversation`, not an empty log), which migrates Owner `ai_session` v1 on the first call and does not cache a failed migration (as `StoreAudit` does). `load` reads the rows back as `Entry`s in order and calls `ConversationLog::from_entries`, which is the one fold. The in-memory test store uses the same fold. Then `append` writes all its entries in one `unchecked_transaction`. A second result for the same `call_id` in a Turn is refused by the partial UNIQUE index (`RecordError::Duplicate`), and the driver treats that as "already answered". The Agent wraps each call in `spawn_blocking`, so a held write never blocks the two Core workers (LOOP-11.3). The barrier test injects, through `AiDeps::make_conversations`, a `ConversationStore` whose `append` parks on a `Gate` (the shape of crates/ffi/tests/ask.rs:905-962), and asserts that `core.status` answers meanwhile.

### Journal Event (`ai/agent/src/turn.rs`, emitted through `EventBus`)

Satisfies: LOOP-3.11
Reuse: rung 2 — `EventBus::emit` (kernel/src/bus.rs:40-57), as ffi emits `core.started` (crates/ffi/src/app.rs:521-531)
Surface:
- Journal readers (`CoreStore.swift` snapshot events, `navori events`, `core.events`) — **frozen**: kept unchanged, since they read events generically and branch on no kind (W14).
Interface: none new. `run_turn` emits `bus.emit("ai", "ai.turn_ended", severity, summary, payload)`.
Depth: n/a — extends `EventBus` use.
Locality: inside the Turn driver's end step and the repair closers, emitted only after the end is recorded (Decision 13).

The summary is `Navori AI finished a Turn: <end> after <n> Steps`, with no content. The payload is `{"conversation": "<ulid>", "turn": "<ulid>", "origin": "user", "end": "<reason>", "steps": n}`; for `error`, `end` is `error:<kind>`.

### Core wiring and the starter seam (`crates/ffi`)

Satisfies: LOOP-9.1, LOOP-10.3, LOOP-10.4, LOOP-11.2
Reuse: rung 2 — `AiRuntime` lazy `OnceCell` building (crates/ffi/src/ai.rs:192-249), `AiDeps` and `start_with` (lib.rs:262), the `on_runtime` hop (lib.rs:244-257), and the live gate pattern (ai/llm/tests/live_deepseek.rs:15-24)
Respects: ARCH-1, ARCH-17
Surface:
- `AiDeps` literal constructions — **replace**: `AiDeps::production()`, ask.rs `Fixture::deps()` and the full literals at crates/ffi/tests/ask.rs:272, :739, :764, :1140 and apps/cli/tests/cli.rs:102 gain `make_provider` and `make_conversations`. `deps_sending` uses `..self.deps()` (ask.rs:390-393) and needs no edit.
- `AiRuntime::new(profile, deps, store)` (lib.rs:319-329) — **replace**: it gains `bus: EventBus` (the Kernel is built first, lib.rs:296).
- Swift (`CoreStore.swift`) and UniFFI bindings — **frozen**: the design builds around them. `run_agent_turn` is not exported, so the generated bindings are unchanged (LOOP-10.4).
Interface:
```rust
impl NavoriCore {
    pub async fn run_agent_turn(&self, request: TurnRequest, cancel: CancellationToken)
        -> Result<TurnOutcome, TurnStartError>; // plain Rust, not uniffi; via on_runtime (ShutDown → TurnStartError::ShutDown)
}
pub use navori_agent::{TurnRequest, TurnOutcome, TurnEnd, TurnErrorKind, TurnStartError, Origin, ConversationId, ConversationStore};
```
Depth: n/a — extends `AiRuntime` and `NavoriCore`.
Locality: extend `crates/ffi/src/ai.rs` (`AiRuntime::agent()` built lazily with `get_or_try_init`, `LlmChatSender`, `StoreConversations`) and `crates/ffi/src/lib.rs` (the new method and re-exports). In the `Inner` literal (lib.rs:318-330), clone the bus out of `kernel` before `kernel` is moved. Nothing AI is built at start (LOOP-10.3, LOOP-11.2).

The live Turn (LOOP-9.1) is `crates/ffi/tests/live_agent.rs`, marked `#[ignore]` and gated on `NAVORI_LIVE_LLM=1`, `NAVORI_LIVE_PASS_ENTRY` and `NAVORI_LIVE_MODEL`. It reads the key from `pass`, the same way ai/llm's live test does. It saves the key into a `FakeKeychain`, starts a core with production providers, runs "Which TCP ports are open on this Mac? Use the tools.", and prints the answer, the end reason and the Actions called.

### Crate, licence and dependency rules

Satisfies: LOOP-10.7, LOOP-10.8
Reuse: rung 2 — `check-deps` (xtask/src/main.rs:696-762); workspace dependencies (Cargo.toml)
Surface:
- `check-deps` — **replace**: one new rule, `navori-agent` must not depend on `navori-llm` (matched by that exact name, since the `navori-ai` prefix rule matches no AI crate, xtask/src/main.rs:736).
- docs/codebase/dependencies.md :29 (`navori-context` gains `navori-llm`, types only) and :42 (`ai/agent` → `ai/context`, kernel, protocol; not `ai/llm`), docs/codebase/modules.md:41-42 and map.md:29, docs/architecture/data.md:21 (the `ai` module's `ai.turn_ended` Event) and :37 (built `ai_session_*` rows, moved from Planned like ASK's `context_audit` row at :27) — **replace**.
- `FAKE_GUARDED` (xtask/src/main.rs:793) — **replace**: gains `navori-llm`.
Interface: none.
Depth: n/a.
Locality: `xtask/src/main.rs` (one rule, the `FAKE_GUARDED` entry and their tests), the workspace `Cargo.toml` (member `ai/agent`, a `navori-agent = { path = "ai/agent" }` entry in `[workspace.dependencies]`, and `navori-llm` with `fake` as a dev-dependency of ffi), `ai/agent/Cargo.toml` (`[lints] workspace = true`), and `ai/agent/LICENSE-dsh`, which holds dsh's MIT text and copyright line. Each ported file opens with `// Ported from dsh (MIT, Copyright (c) 2026 DeepSeek): <package path>. See ai/agent/LICENSE-dsh.`

### Existing behavior and performance

Satisfies: LOOP-10.5, LOOP-10.6, LOOP-11.1
Reuse: rung 1 — no new code; these hold by leaving `crates/kernel`, `crates/store` and every Module's code untouched
Interface: none.
Depth: n/a.
Locality: leave. LOOP-11.1 is measured by `cargo xtask perf` at the base and at the head on one idle machine (performance.md:35-36).

## Persisted formats (L22; all `agent-decided`; permanent at first ship under Compat External)

1. **Owner `ai_session`**, migration v1 `ai_session_tables`, applied lazily on the first Turn:
   - `ai_session_conversation (id TEXT PRIMARY KEY, created_ms INTEGER NOT NULL)`, where `id` is a ULID.
   - `ai_session_turn (id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES ai_session_conversation(id), seq INTEGER NOT NULL, origin TEXT NOT NULL, model TEXT NOT NULL, started_ms INTEGER NOT NULL, ended_ms INTEGER, end_reason TEXT, UNIQUE (conversation_id, seq))`. `origin` takes `user`. `end_reason` takes `completed|cancelled|step_limit|token_limit|interrupted|error:<kind>`. Neither has a CHECK, so later values need no migration.
   - `ai_session_step (turn_id TEXT NOT NULL REFERENCES ai_session_turn(id), seq INTEGER NOT NULL, started_ms INTEGER NOT NULL, ended_ms INTEGER, PRIMARY KEY (turn_id, seq))`.
   - `ai_session_message (turn_id TEXT NOT NULL REFERENCES ai_session_turn(id), seq INTEGER NOT NULL, step INTEGER, kind TEXT NOT NULL, call_id TEXT, body TEXT NOT NULL, redactions INTEGER NOT NULL, created_ms INTEGER NOT NULL, PRIMARY KEY (turn_id, seq))`. `kind` takes `input|assistant|tool_result`; `step` is NULL for `input`; `call_id` is set only on `tool_result` rows.
   - The index `ai_session_one_result_per_call` is a `UNIQUE ON ai_session_message(turn_id, call_id) WHERE kind = 'tool_result'`, so storage refuses a second result for a call (LOOP-7.2).
   - `ai_session_usage (turn_id TEXT NOT NULL REFERENCES ai_session_turn(id), step INTEGER NOT NULL, reported INTEGER NOT NULL CHECK (reported IN (0,1)), input_tokens INTEGER, output_tokens INTEGER, cache_hit_tokens INTEGER, cache_miss_tokens INTEGER, PRIMARY KEY (turn_id, step))`. There is one row per model call; with `reported = 0`, the counts are NULL (LOOP-3.5).
   - The index is `ai_session_turn_by_conversation ON ai_session_turn(conversation_id, seq)`.
2. **Message body encoding v1** (JSON in `ai_session_message.body`, owned by `navori-agent` with its own serde structs and not derived from `ai/llm` types, W16). All strings are stored after the Shield:
   - `input`: `{"v":1,"text":…}`
   - `assistant`: `{"v":1,"text":…|null,"reasoning":…|null,"tool_calls":[{"id":…,"name":…,"arguments":…}]}`
   - `tool_result`: `{"v":1,"call_id":…,"text":…,"is_error":bool,"synthetic":bool}`
3. **Journal Event kind `ai.turn_ended`**, module `ai`. Payload keys: `conversation`, `turn`, `origin`, `end` and `steps` (W13, W14).
4. **Unchanged:** `context_audit`'s columns and values (W6, W7). The wording of data.md:27 changes from "per ask" to "per model call" (W8).

Not stored: estimated cost (S-cost, ROAD-11). Retention and pruning of `ai_session_*`, and reading past Conversations, are not in this road: ROAD-15 owns them (docs/roadmap/INDEX.md:90; Weigh W17, pending the operator). ROAD-12 keeps the reader and retention of `context_audit`.

## Seams for testing

New seams: two. `Agent::run_turn` lets the loop's logic be tested with a real Kernel built from `KernelBuilder` (the `system` Module plus a test Module) and a scripted `ChatSender`. `NavoriCore::run_agent_turn` is the road's demo seam with real Modules and the real Store. Every other seam already exists.

| Seam | Kind | Covers |
|---|---|---|
| `OpenAiCompat` against the local `Responder` (ai/llm/tests) | integration | LOOP-1.5, 1.6, 1.7, 10.1 |
| `FakeLlm` script and recorded requests (ai/llm/tests, feature `fake`) | unit | LOOP-8.1 |
| `Gateway::shield` (second pass over Shielded text gives `count` 0 for every rule, including `secret_assignment` and `url_password`; fail-closed on a panicking injected Shield) | unit | LOOP-3.6, 5.3 |
| `Gateway::send_chat` with fake `SecretStore`, recording `ChatSender` and in-memory `AuditSink` (ai/context/tests; includes a planted secret in a `ToolDef` description that arrives redacted, and a panicking Shield on a tool definition that sends nothing) | unit | LOOP-5.1, 5.2, 5.3, 5.4, 5.5, 5.6, 11.4 |
| `Gateway::ask` (existing tests unchanged) | unit | LOOP-10.2 |
| `Toolset::from_specs` with the real `schema_for!` of `SystemAction` and of a test enum (ai/agent/tests) | unit | LOOP-1.1 |
| `Agent::run_turn` with a `KernelBuilder` Kernel, a scripted `ChatSender` and an in-memory `ConversationStore` (ai/agent/tests) | integration | LOOP-1.2, 1.3, 1.4, 1.8, 1.9, 2.1, 2.3, 2.4, 2.5, 3.1–3.7, 3.9, 3.10, 3.11, 4.1, 6.1, 6.2, 6.3, 7.1, 7.2 |
| `RepeatGuard::observe` | unit | LOOP-6.3 |
| `repair(&ConversationLog)` | unit | LOOP-4.1 |
| `NavoriCore::run_agent_turn` via `start_with` + `AiDeps { make_provider: FakeLlm, make_conversations: gated or real }` (crates/ffi/tests/agent.rs) | integration | LOOP-1.1, 2.2, 3.8, 4.1, 5.2, 10.3, 10.4, 11.2, 11.3, 11.5, 11.6 (the demos: `ports.list` answer; `ports.kill` refused with Ports receiving nothing; three-Step audit count; records read back; `ai.turn_ended` in the Journal; reopen: a Turn held at `FakeLlm::hold_next` when the core shuts down is repaired as `interrupted` after a restart on the same home) |
| Risk drift over a started core's `actions()` (crates/ffi/tests/actions_risk.rs) | integration | LOOP-2.6 |
| Live Turn (crates/ffi/tests/live_agent.rs, `#[ignore]`, operator-run) | e2e | LOOP-9.1 |
| `check-deps` (xtask test) and `cargo xtask ci` | integration | LOOP-10.7, 10.8 (the `LICENSE-dsh` file and the per-file header, by a source test) |
| Existing kernel, store and ask suites unchanged | integration | LOOP-10.5, 10.6 |
| `cargo xtask perf` at base and head | e2e (manual) | LOOP-11.1 |

## Coverage check

Every live ID in requirements.md (after the sync-back below) appears in exactly one `Satisfies:` line:
- Chat types: LOOP-1.5, 1.6, 1.7, 10.1.
- FakeLlm: 8.1.
- Gateway: 5.1–5.6, 10.2, 11.4.
- Tools: 1.1.
- Dispatch: 1.2, 1.8, 1.9, 2.1–2.6, 11.6.
- Guard: 6.3.
- Turn driver: 1.3, 1.4, 3.1–3.7, 3.9, 3.10, 6.1, 6.2, 7.1, 7.2, 11.5.
- Repair: 4.1.
- Record: 3.8, 11.3.
- Journal: 3.11.
- Wiring: 9.1, 10.3, 10.4, 11.2.
- Rules: 10.7, 10.8.
- Existing: 10.5, 10.6, 11.1.

That makes 54 IDs, none unmapped.

**Requirements sync-back** (re-surfaced with this design):
- New LOOP-3.11, the `ai.turn_ended` Event (W13, W14).
- LOOP-4.1 amended: repair also emits it, with end `interrupted`.
- LOOP-5.4 amended: after a Shield refusal the Turn ends with an error (Decision 10).
- No new ID for the check-deps rule: it is how LOOP-5.1 is verified.
- LOOP-1.8 clarified: "dispatch nothing" means the Module receives no request (Decision 7).
- Open Questions: S-journal, S-shield-refusal, S-limits, S-origin, S-parallel and S-tools are now answered by Decisions 13, 10, 8, 9, 4 and 5–6. S-stop is answered by L22 and "Persisted formats". S-cost stays open.
