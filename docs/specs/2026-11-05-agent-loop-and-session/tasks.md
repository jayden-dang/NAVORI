# Tasks: Agent loop and Conversation

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: LOOP
Status: Approved
Date: 2026-11-05
Approved by the operator ("approve", 2026-11-05 21:02 +07, on the review sheet at 4c30a7d).
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

**Goal:** Navori AI runs a Turn as an Agent loop inside the Core: it calls only Read Actions through one dispatch point, every model call goes through the Context Gateway with its own audit row, and each Turn is kept in a Conversation after the Shield, with limits, cancel, crash repair and one `ai.turn_ended` Event.

**Architecture:** `ai/llm` gains the canonical chat types (tools, reasoning, usage) and a scripted `FakeLlm`. `ai/context` re-exports those types, Shields a structured request in `Gateway::send_chat`, and sends it through a `ChatSender` that `crates/ffi` implements over `LlmProvider`. The new `ai/agent` (`navori-agent`) holds the Toolset, the Dispatcher, the repeat guard, the record types with repair, and the Turn driver; it never depends on `navori-llm`. `crates/ffi` wires it lazily into `AiRuntime`, implements `ConversationStore` on the Store (Owner `ai_session`), and exposes the plain-Rust `NavoriCore::run_agent_turn`.

**Tech Stack:** Rust 2024 (rust-version 1.90), `tokio` with `tokio-util` (`CancellationToken`), `futures`, `serde`/`serde_json`, `schemars` 1.2.3 (kernel), `rusqlite` via `navori-store`, `ulid`, `thiserror`, `tracing`; `cargo test -p <crate>`, `cargo xtask ci`, `cargo xtask perf`. No new third-party crate.

**Source classes:** every value in `requirements.md` and `design.md` is a Lock, Weigh, `agent-decided` or Unverified. A task never promotes one to the operator's choice, and no code comment says an `agent-decided` value was the operator's.

## Global Constraints

Source of truth (referenced, not copied — read before every task):

| Source | Content hash (`git hash-object`, first 12) |
|---|---|
| `docs/standards/INDEX.md` | `e61c8fc5bdc7` |
| `docs/standards/testing.md` | `b1a9b42fc1d3` |
| `docs/standards/errors-logging.md` | `68aadebff863` |
| `docs/standards/performance.md` | `f96f7a31fa5d` |
| `docs/architecture/INDEX.md` (ARCH-N) | `2e318bd23ff5` |

Compact delta every task inherits:
- **Test-first at the design's seams** (design.md "Seams for testing"). Test names are sentences in domain language. No requirement ID in test or production source. Expected values are literals. Fakes only at system boundaries (provider, Keychain, clock). Tests go through each crate's public API.
- **Planted secrets** are fake and split with `concat!`. The token is `concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123456789")`, which the Shield turns into `[REDACTED:github_token]`. The assignment is `concat!("password=", "hunter22abc")`, which becomes `password=[REDACTED:secret_assignment]`.
- **Build limits:** no agent step reads or writes a real Keychain item, runs `pass show`, or sets `NAVORI_LIVE_LLM`; only the operator does, by hand. Tests write only inside the worktree and temp directories (`tempfile::Builder::new().prefix("nv").tempdir_in("/tmp")`). `cargo xtask perf` is never run with `--record`.
- **Persisted formats** are exactly design.md "Persisted formats" (Owner `ai_session` v1, five tables plus two indexes, message body encoding v1, Event `ai.turn_ended`). No task adds, renames or reshapes one. `context_audit` is untouched.
- **ARCH-1, 3, 4, 5, 6, 7, 12, 15, 17** hold in every task. No `unsafe`. `[lints] workspace = true` on every new crate. No `unwrap`/`expect` outside tests (test files that use them start with `#![allow(clippy::unwrap_used, clippy::expect_used)]`). Logs carry ids, counts, kinds and tool names only.
- **Ported code** opens with `// Ported from dsh (MIT, Copyright (c) 2026 DeepSeek): <package path>. See ai/agent/LICENSE-dsh.`
- **Gate:** `cargo fmt --all`, then `cargo clippy --workspace --all-targets -- -D warnings`, then the task's `cargo test -p <crate>`. The whole `cargo xtask ci` runs in Task 13.
- **Commits:** Conventional Commits, one logical change per commit, every commit builds. No `git push`.
- **Unverified, stays open (no task asserts it):** whether DeepSeek sends `content` as null or `""` with tool calls (fixtures cover both); whether DeepSeek accepts a replayed `reasoning_content` that the Shield changed (LOOP Open Questions); the operator's `pass` entry name.

## File Structure

Create:
- `ai/agent/Cargo.toml`, `ai/agent/LICENSE-dsh`, `ai/agent/src/lib.rs` — the `navori-agent` crate root, re-exports and dsh's MIT text.
- `ai/agent/src/tools.rs` — `Toolset`, `Tool`: tool names and per-Action schemas from `ActionSpec`s.
- `ai/agent/src/dispatch.rs` — `Dispatcher::call`, the one dispatch point.
- `ai/agent/src/guard.rs` — `RepeatGuard`, a port of dsh repeat-tool-reminder.
- `ai/agent/src/record.rs` — `ConversationStore`, `Entry`, `RecordedMessage` (body encoding v1), `ConversationLog`, `RecordError`, ids, `Origin`.
- `ai/agent/src/repair.rs` — `repair(&ConversationLog)`, a port of dsh repair.
- `ai/agent/src/turn.rs` — `Agent`, `TurnRequest`, `TurnOutcome`, `TurnEnd`, `TurnErrorKind`, `TurnStartError`, `Limits`, and the `ai.turn_ended` Event.
- `ai/agent/tests/common/mod.rs` (`#![allow(dead_code)]`) — `ScriptedChat` (a `ChatSender` with a reply queue, recorded requests and `hold_next`), `MemoryConversations`, `MemoryAudit` (an `AuditSink`), `StubSender`, the `probe` test Module, `kernel_with_probe()` and `agent_with(script)`.
- `ai/agent/tests/tools.rs`, `ai/agent/tests/dispatch.rs`, `ai/agent/tests/guard.rs`, `ai/agent/tests/record.rs`, `ai/agent/tests/turn.rs`, `ai/agent/tests/limits.rs`, `ai/agent/tests/source.rs` — tests at the design's seams.
- `ai/llm/src/fake.rs`, `ai/llm/tests/fake.rs` — `FakeLlm` and its tests.
- `ai/context/tests/chat.rs` — `Gateway::shield` and `Gateway::send_chat`.
- `crates/ffi/tests/agent.rs` — the demos through `NavoriCore::run_agent_turn`.
- `crates/ffi/tests/actions_risk.rs` — the catalog-versus-variant Risk drift test.
- `crates/ffi/tests/live_agent.rs` — the gated live Turn.

Modify:
- `Cargo.toml`, `Cargo.lock` — member `ai/agent`; `navori-agent` path entry in `[workspace.dependencies]`.
- `ai/llm/Cargo.toml`, `ai/llm/src/lib.rs`, `ai/llm/src/openai_compat.rs` — canonical types, wire mapping, the `fake` feature.
- `ai/llm/tests/openai_compat.rs`, `ai/llm/tests/live_deepseek.rs`, `ai/llm/tests/default_tests_use_fakes.rs` — the new types, new wire tests, a wider fakes scan.
- `ai/context/Cargo.toml`, `ai/context/src/lib.rs`, `ai/context/src/gateway.rs` — the `navori-llm` types edge, `Shielded`, `shield`, `ChatSender`, `Outbound`, `send_chat`.
- `crates/ffi/Cargo.toml`, `crates/ffi/src/ai.rs`, `crates/ffi/src/lib.rs` — `LlmChatSender`, `StoreConversations`, the `AiDeps` fields, `AiRuntime::agent`, `run_agent_turn` and its re-exports.
- `crates/ffi/tests/ask.rs`, `apps/cli/tests/cli.rs` — `AiDeps` literals (new fields filled by `..AiDeps::production()`) and `LlmSender::new` callers.
- `ai/context/tests/gateway.rs` — the one-call-site test allows the second `.send(` in `send_chat`.
- `apps/cli/Cargo.toml` — unchanged; `crates/ffi/Cargo.toml` gains `navori-agent`, plus `navori-llm` with `fake` as a dev-dependency.
- `xtask/src/main.rs` — the `navori-agent` → `navori-llm` rule; `navori-llm` in `FAKE_GUARDED`.
- `docs/codebase/dependencies.md` (rows :22 ffi gains `navori-agent`, :29, :42), `docs/codebase/modules.md`, `docs/codebase/map.md`, `docs/architecture/data.md`, `docs/architecture/system.md` — the built rows.

Retrieval: `blast_radius` on these paths and `cluster(LOOP)`: auto/subgraph-plan.md (not committed; owns_coverage 12/17). ASK owns every `ai/*` and `crates/ffi/src/ai.rs` file this plan edits, and its guards are LOOP-10.1–10.3. RELEASE (In-progress) shares `xtask/src/main.rs`, so rebase on its merge before Task 2.

---

### Task 1: Canonical chat types and the adapter's wire mapping

**Files:**
- Modify: `ai/llm/src/lib.rs`, `ai/llm/src/openai_compat.rs`, `ai/llm/tests/openai_compat.rs`, `ai/llm/tests/live_deepseek.rs`, `crates/ffi/src/ai.rs`

**Reuse:** rung 2 — extends `ai/llm` (`ChatRequest`, `ChatReply`, `LlmProvider`, `OpenAiCompat`)

**Interfaces:**
- Produces (every type derives `Debug, Clone, PartialEq, Eq`): `enum Message { User { text: String }, Assistant { text: Option<String>, reasoning: Option<String>, tool_calls: Vec<ToolCall> }, Tool { call_id: String, text: String } }`; `struct ToolCall { id: String, name: String, arguments: String }`; `struct ToolDef { name: String, description: String, parameters: serde_json::Value }`; `struct ChatRequest { model: String, messages: Vec<Message>, tools: Vec<ToolDef> }`; `struct ChatReply { text: Option<String>, reasoning: Option<String>, tool_calls: Vec<ToolCall>, usage: Option<Usage>, finish: Finish }`; `struct Usage { input: u64, output: u64, cache_hit: Option<u64>, cache_miss: Option<u64> }`; `enum Finish { Stop, ToolCalls, Length, Other(String) }`. `LlmProvider::complete` keeps its signature. `Role` is removed.
- Consumes: none.

**Depends-on:** none

- [x] **Step 1: Failing tests** in `ai/llm/tests/openai_compat.rs`. `a_plain_request_still_sends_only_model_messages_and_stream` is the existing key-set test, ported to `Message::User`. `tools_are_sent_as_functions_and_omitted_when_empty`: one `ToolDef { name: "ports_list", description: "List ports", parameters: {"type":"object","properties":{}} }` gives `tools == [{"type":"function","function":{"name":"ports_list","description":"List ports","parameters":{…}}}]`. `an_assistant_message_is_replayed_with_reasoning_and_tool_calls` gives `{"role":"assistant","content":null,"reasoning_content":"think","tool_calls":[{"id":"c1","type":"function","function":{"name":"ports_list","arguments":"{}"}}]}` followed by `{"role":"tool","tool_call_id":"c1","content":"[]"}`. `a_tool_call_reply_with_null_content_is_accepted`, and the same with `"content":""`: the reply `{"choices":[{"message":{"content":null,"reasoning_content":"r","tool_calls":[{"id":"c1","type":"function","function":{"name":"ports_list","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":120,"completion_tokens":30,"total_tokens":150,"prompt_cache_hit_tokens":100,"prompt_cache_miss_tokens":20}}` gives `tool_calls[0].arguments == "{}"`, `reasoning == Some("r")`, `finish == ToolCalls` and `usage == Usage { input: 120, output: 30, cache_hit: Some(100), cache_miss: Some(20) }`. `a_reply_without_usage_has_none`. `a_reply_with_neither_text_nor_tool_calls_is_malformed`. Run `cargo test -p navori-llm` — expect: compile errors on the new names.
- [x] **Step 2: Implement** the types in `lib.rs`, and the wire structs in `openai_compat.rs` with `skip_serializing_if` (`Vec::is_empty`, `Option::is_none`). Parse `content: Option<String>`. Send no `thinking` field. Port `live_deepseek.rs` (lines 5, 39-49) to `Message::User` and the `Option` text in this step, so the crate's test targets compile. Run `cargo test -p navori-llm` — expect: pass.
- [x] **Step 3: Keep the ask path whole.** In `crates/ffi/src/ai.rs`, `LlmSender` builds `vec![Message::User { text }]` with `tools: vec![]`. An empty or missing `reply.text` maps to the same error `Malformed` gave before. Run `cargo test -p navori-ffi --test ask` — expect: pass, with no assertion changed.
- [x] **Step 4: Gate and commit:** `git commit -m "feat(llm): carry tools, reasoning and usage in the canonical chat types"`.

_Requirements: LOOP-1.5, LOOP-1.6, LOOP-1.7, LOOP-10.1_

---

### Task 2: `FakeLlm` and the fakes guard

**Files:**
- Create: `ai/llm/src/fake.rs`, `ai/llm/tests/fake.rs`
- Modify: `ai/llm/Cargo.toml`, `ai/llm/src/lib.rs`, `ai/llm/tests/default_tests_use_fakes.rs`, `xtask/src/main.rs`

**Reuse:** rung 2 — the `fake` feature pattern of `navori-secrets` (crates/secrets/src/fake.rs:16-55); `navori-llm` is added to check-deps' `FAKE_GUARDED` (xtask/src/main.rs:793)

**Interfaces:**
- Produces: `navori_llm::fake::FakeLlm::new(script: Vec<Result<ChatReply, LlmError>>) -> FakeLlm`; `FakeLlm::requests(&self) -> Vec<ChatRequest>`; `FakeLlm::hold_next(&self) -> Release`; `Release::release(self)`; `impl LlmProvider for FakeLlm`. It relies on the `Clone` derives from Task 1.
- Consumes: Task 1 types.

**Depends-on:** Task 1

- [x] **Step 1: Failing tests** in `ai/llm/tests/fake.rs`, run with `--features fake` (a `[[test]] required-features = ["fake"]` entry). `the_fake_answers_its_script_in_order_and_records_requests`: two scripted replies come back in order and `requests()` holds both requests. `a_fake_with_an_empty_script_fails_as_malformed`. `a_held_call_waits_until_released`: a `complete` future stays pending until `release()`. Extend `every_default_test_uses_fakes` to also scan `ai/agent/tests/` and `crates/ffi/tests/agent.rs`, each only when it exists (the existing fixed paths keep their `is_file()` asserts), with `crates/ffi/tests/live_agent.rs` the only exempt file. Run `cargo test -p navori-llm --features fake` — expect: unresolved `fake`.
- [x] **Step 2: Implement** `fake.rs` behind `#[cfg(feature = "fake")]`, with `[features] fake = []`. Add `"navori-llm"` to `FAKE_GUARDED` (making the array length 3) and add an xtask test showing that a normal `[dependencies]` entry with `features = ["fake"]` is refused. Run `cargo test -p navori-llm --features fake && cargo test -p xtask` — expect: pass.
- [x] **Step 3: Gate and commit:** `git commit -m "test(llm): add a scripted FakeLlm behind the fake feature"`.

_Requirements: LOOP-8.1_

---

### Task 3: The Gateway Shields and sends a structured request

**Files:**
- Create: `ai/context/tests/chat.rs`
- Modify: `ai/context/Cargo.toml` (`navori-llm`, `serde_json`), `ai/context/src/lib.rs`, `ai/context/src/gateway.rs`, `ai/context/tests/gateway.rs`, `docs/codebase/dependencies.md`

**Reuse:** rung 2 — extends `Gateway` (`read_key`, `inspect`, `write_row`, gateway.rs:122-190) and keeps `AuditRow` and `AuditSink` unchanged

**Interfaces:**
- Produces: `pub use navori_llm::{Message, ToolCall, ToolDef, ChatRequest, ChatReply, Usage, Finish}` (never `LlmProvider`); `struct Shielded { text: String, count: u32 }`; `Gateway::shield(&self, text: &str) -> Option<Shielded>`; `trait ChatSender: Send + Sync { fn send<'a>(&'a self, key: &'a Secret, request: ChatRequest) -> BoxFuture<'a, Result<ChatReply, SendError>>; }`; `struct Outbound { request: ChatRequest, prior_redactions: u32 }`; `Gateway::with_chat_sender(self, Arc<dyn ChatSender>) -> Self`; `Gateway::send_chat(&self, Outbound) -> Result<ChatReply, AskError>`.
- Consumes: Task 1 types.

**Depends-on:** Task 1

- [x] **Step 1: Failing tests** in `ai/context/tests/chat.rs`, using a fake `SecretStore`, a recording `ChatSender` and an in-memory `AuditSink`, as in `ai/context/tests/gateway.rs`:
  - `shielding_a_shielded_text_again_finds_nothing_new`: through `Gateway::shield`, the token and the `password=` assignment get `count` 1 and 1 on the first pass. A second pass over each output gives the same text with `count` 0. This asserts `count`, not the raw findings: `secret_assignment` re-finds its own marker.
  - `a_planted_secret_in_a_tool_result_arrives_redacted_and_counted_once`: the request carries `[REDACTED:github_token]`, the audit row says `kinds "text"`, `outcome authorized`, `redacted_count 1`.
  - `prior_redactions_add_to_the_row`: `prior_redactions: 2` on a clean request gives `redacted_count 2`.
  - `a_planted_secret_in_a_tool_definition_is_redacted` (in its description).
  - `a_planted_secret_in_a_parameters_key_blocks_the_call`: a `parameters` object with the key `concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123456789")` gives a `blocked` row and sends nothing.
  - `a_panicking_shield_on_any_text_writes_blocked_and_sends_nothing`, which uses `with_shield`.
  - `an_audit_failure_sends_nothing`.
  - `a_missing_key_is_no_key`.
  - `no_message_text_reaches_the_audit_row`.
  - `send_chat_without_a_chat_sender_is_a_send_error`.

  Run `cargo test -p navori-context` — expect: unresolved names.
- [x] **Step 2: Implement.** `shield` calls the existing private `inspect` (gateway.rs:173-176) and counts only findings whose original span does not start with `[REDACTED:`. In `ai/context/tests/gateway.rs`, `the_context_crate_calls_the_sender_in_one_place` (:725-737) is changed to assert every `.send(` site is in `gateway.rs` (two now: `ask` and `send_chat`). `send_chat` reads the key, then Shields every message text, reasoning, tool-call argument, tool text, `ToolDef` name and description, and every string inside `parameters`, keys and values (design.md:140). If the Shield would change a key, the call is `blocked` and nothing is sent: a redacted key would no longer be the schema. Then it writes the row (`prior_redactions` + its fresh count) and sends. `ai/context/Cargo.toml` gains `navori-llm.workspace = true`. Update dependencies.md:29 so `navori-context` gains `navori-llm`, types only. Run `cargo test -p navori-context` — expect: pass, with every other `ai/context/tests/gateway.rs` assertion unchanged.
- [x] **Step 3: Gate and commit:** `git commit -m "feat(context): shield and audit a structured chat request in the Gateway"`.

_Requirements: LOOP-5.1, LOOP-5.2, LOOP-5.3, LOOP-5.4, LOOP-5.5, LOOP-5.6, LOOP-10.2, LOOP-11.4_

---

### Task 4: The `navori-agent` crate, its licence, its dependency rule and the Toolset

**Files:**
- Create: `ai/agent/Cargo.toml`, `ai/agent/LICENSE-dsh`, `ai/agent/src/lib.rs`, `ai/agent/src/tools.rs`, `ai/agent/tests/common/mod.rs`, `ai/agent/tests/tools.rs`, `ai/agent/tests/source.rs`
- Modify: `Cargo.toml`, `Cargo.lock`, `xtask/src/main.rs`, `docs/codebase/dependencies.md`

**Reuse:** rung 2 — `Kernel::actions()` (runtime.rs:323-328) and the `ActionSpec` schema built by `schema_for!` (runtime.rs:154-163); rung 2 — `check-deps` (xtask/src/main.rs:696-762); workspace dependencies (Cargo.toml)

**Interfaces:**
- Produces: `Toolset::from_specs(specs: &[ActionSpec]) -> Toolset`; `Toolset::defs(&self) -> Vec<ToolDef>`; `Toolset::lookup(&self, name: &str) -> Option<&Tool>`; `struct Tool { module: String, action: String, risk: Risk, unit: bool }`.
- Consumes: `navori_context::ToolDef` (Task 3); `navori_protocol::{ActionSpec, Risk}`.

**Depends-on:** Task 2, Task 3

- [x] **Step 1: Pin the schema shape first.** `the_system_actions_schema_has_one_entry_per_variant` in `ai/agent/tests/tools.rs` builds `Kernel::builder(Store::open_in_memory()?).start()` inside a `#[tokio::test]` (it needs a runtime), and asserts the `system` module's `input_schema` holds a `oneOf` with `{"const":"ping"}` and an object entry keyed `prune_journal` with `older_than_days`. `undocumented_unit_variants_share_one_enum_entry` uses a test enum with two undocumented unit variants and asserts one `{"type":"string","enum":[…]}` entry (design Decision 6). Run `cargo test -p navori-agent` — expect: the crate does not exist yet.
- [x] **Step 2: Create the crate.** `navori-agent` depends on `navori-context`, `navori-kernel`, `navori-protocol`, `tokio`, `tokio-util`, `futures`, `serde`, `serde_json`, `thiserror`, `tracing` and `ulid` (workspace), and its dev-dependencies are `navori-store`, `navori-secrets` (`features = ["fake"]`), `schemars`, `anyhow`, `tracing-subscriber` and `tempfile`. `tests/common/mod.rs` starts with `#![allow(dead_code)]` and holds `kernel(store)` for now (Task 5 adds the probe). Add `[lints] workspace = true`, the workspace member, and `navori-agent = { path = "ai/agent" }`. `LICENSE-dsh` holds the text of /Users/jayden/Developer/dsh/LICENSE. Extract the inline rule chain of `check_deps` (xtask/src/main.rs:718-745) into a pure `fn internal_dep_violation(package: &str, path: &str, dep: &str) -> Option<String>` with no behavior change. The existing xtask tests and `cargo xtask check-deps` stay green. Then add the rule `navori-agent must not depend on navori-llm`, with a test on the helper. Update dependencies.md:42 so `ai/agent` depends on `ai/context`, kernel and protocol, not `ai/llm`. `ai/agent/tests/source.rs` asserts that `LICENSE-dsh` holds `Copyright (c) 2026 DeepSeek` and that every ported file opens with the port header. Run `cargo test -p xtask && cargo xtask check-deps` — expect: pass.
- [x] **Step 3: Toolset tests, then implementation.**
  - `only_read_actions_are_offered_as_tools`: from `system` (`ping` read, `prune_journal` write), `defs()` names only `system_ping`, and `lookup("system_prune_journal")` still finds a write `Tool`.
  - `a_unit_action_takes_an_empty_object`, for both a `const` unit and a merged-`enum` unit: `parameters == {"type":"object","properties":{},"additionalProperties":false}`.
  - `a_struct_action_takes_its_fields_with_defs_attached`.
  - `an_action_whose_variant_is_missing_from_the_schema_is_not_offered`.
  - `two_actions_with_the_same_tool_name_are_both_left_out`.

  Implement `tools.rs`. Run `cargo test -p navori-agent` — expect: pass.
- [x] **Step 4: Gate and commit:** `git commit -m "feat(agent): add navori-agent with the Read-only Toolset and the dsh licence"`.

_Requirements: LOOP-1.1, LOOP-10.7, LOOP-10.8_

---

### Task 5: The one dispatch point

**Files:**
- Create: `ai/agent/src/dispatch.rs`, `ai/agent/tests/dispatch.rs`
- Modify: `ai/agent/src/lib.rs`, `ai/agent/tests/common/mod.rs`, `ai/agent/tests/source.rs`

**Reuse:** rung 2 — `Kernel::dispatch` and `Envelope` (runtime.rs:330-342; protocol lib.rs:35-39); `tokio::time::timeout` (rung 3)

**Interfaces:**
- Produces: `Dispatcher::new(action_timeout: Duration) -> Dispatcher`; `Dispatcher::call(&self, kernel: &Kernel, toolset: &Toolset, call: &ToolCall) -> ToolOutcome`; `struct ToolOutcome { text: String, is_error: bool }`. In tests/common: `TestModule` (id `probe`; Actions `look` read (unit), `look_at { n: u32 }` read, `change` write, and `slow` read (unit, sleeps 2 s), each variant with a doc comment, and the merged-`enum` form covered by Task 4), with a counter of requests received; `kernel_with_probe() -> (Kernel, Arc<AtomicUsize>)`.
- Consumes: `Toolset`, `Tool` (Task 4); `ToolCall` (Task 3).

**Depends-on:** Task 4

- [x] **Step 1: Failing tests** in `ai/agent/tests/dispatch.rs`, each asserting the probe's request counter:
  - `a_read_action_is_dispatched_and_its_reply_returned` (counter 1, `is_error` false).
  - `a_write_action_is_refused_without_reaching_the_module`: text `Error: Navori AI cannot run probe_change yet: it changes things and needs your approval, which is not available.`, counter 0.
  - `an_unknown_name_is_refused`: text `Error: no Action named probe_gone.`, counter 0.
  - `arguments_that_are_not_json_are_refused`.
  - `arguments_that_do_not_fit_reach_no_module`: `probe_look_at` with `{"n":"x"}`, counter 0, text starting `Error: the arguments do not fit probe_look_at`.
  - `a_unit_action_is_sent_as_its_bare_name`, for the arguments `""`, `"null"` and `"{}"`.
  - `a_unit_action_given_arguments_reaches_no_module`: `probe_look` with `{"x":1}`, counter 0.
  - `an_action_slower_than_the_limit_times_out`: `Dispatcher::new(Duration::from_millis(200))`, text `Error: probe_slow did not answer within 0.2 s.`

  In `source.rs`, `only_the_dispatcher_calls_kernel_dispatch` greps `ai/agent/src` for `.dispatch(` outside `dispatch.rs`. Run `cargo test -p navori-agent` — expect: unresolved names.
- [x] **Step 2: Implement** `dispatch.rs` in the design's order: lookup, then Risk, then JSON parse, then Envelope, then `timeout(kernel.dispatch)`. Each refusal or failure becomes a `ToolOutcome` with `is_error: true`. The timeout text uses the configured limit in seconds. Run `cargo test -p navori-agent` — expect: pass.
- [x] **Step 3: Gate and commit:** `git commit -m "feat(agent): route every tool call through one Read-only dispatch point"`.

_Requirements: LOOP-1.2, LOOP-1.8, LOOP-1.9, LOOP-2.1, LOOP-2.2, LOOP-2.3, LOOP-2.4, LOOP-2.5, LOOP-11.6_

---

### Task 6: The repeat reminder

**Files:**
- Create: `ai/agent/src/guard.rs`, `ai/agent/tests/guard.rs`
- Modify: `ai/agent/src/lib.rs`

**Reuse:** rung 7 — a port of dsh `guard/repeat-tool-reminder` (240 TS lines); no Rust counterpart exists in the repo

**Interfaces:**
- Produces: `RepeatGuard::new() -> RepeatGuard`; `RepeatGuard::observe(&mut self, name: &str, arguments: &str) -> Option<String>`.
- Consumes: none.

**Depends-on:** Task 5

- [ ] **Step 1: Failing tests:**
  - `the_third_identical_call_gets_the_first_reminder` returns dsh's first-threshold sentence, starting `You are repeating the exact same tool call with identical arguments.`
  - `the_fifth_and_eighth_calls_get_the_detailed_reminder`, whose text contains `- tool: probe_look` and `- consecutive_calls: 5`.
  - `key_order_does_not_make_a_call_different`: `{"a":1,"b":2}` then `{"b":2,"a":1}` count as 2.
  - `a_different_call_resets_the_count`.
  - `unparseable_arguments_compare_as_raw_text`.
  - `long_arguments_are_cut_at_500_characters`, ending `… (+N more chars)`.

  Run `cargo test -p navori-agent --test guard` — expect: unresolved names.
- [ ] **Step 2: Implement** `guard.rs` with dsh's texts and thresholds 3/5/8, and the port header. Run — expect: pass.
- [ ] **Step 3: Gate and commit:** `git commit -m "feat(agent): port dsh's repeat-tool reminder"`.

_Requirements: LOOP-6.3_

---

### Task 7: The record types and crash repair

**Files:**
- Create: `ai/agent/src/record.rs`, `ai/agent/src/repair.rs`, `ai/agent/tests/record.rs`
- Modify: `ai/agent/src/lib.rs`, `ai/agent/tests/common/mod.rs`

**Reuse:** rung 7 — a port of dsh `core/session/src/repair.ts` `openTurnClosers` / `ToolCallRecovery` (211 TS lines)

**Interfaces:**
- Produces: `ConversationLog::from_entries(Vec<Entry>) -> Result<ConversationLog, RecordError>` (the one fold, used by `MemoryConversations` here and `StoreConversations` in Task 10); `struct LoggedTurn { id, seq, origin, messages: Vec<(u32, Option<u32>, RecordedMessage)>, open_step: Option<u32>, end: Option<String> }`; `trait ConversationStore: Send + Sync { fn append(&self, entries: &[Entry]) -> Result<(), RecordError>; fn load(&self, conversation: &ConversationId) -> Result<ConversationLog, RecordError>; }`; `enum Entry` as in design.md "Conversation record"; `enum RecordedMessage { Input { text }, Assistant { text, reasoning, tool_calls }, ToolResult { call_id, text, is_error, synthetic } }` with `to_body(&self) -> String` and `from_body(&str) -> Result<RecordedMessage, RecordError>` (encoding v1, `"v":1`); `struct ConversationLog { turns: Vec<LoggedTurn> }`; `enum RecordError { UnknownConversation, Migration, Write, Read, Duplicate }`; `ConversationId`, `TurnId` (ULID newtypes); `enum Origin { User }` with `as_str() -> "user"`; `fn repair(log: &ConversationLog) -> Vec<Entry>`; `fn derive_messages(log: &ConversationLog) -> Vec<Message>`. In tests/common: `MemoryConversations`, which keeps `Entry`s, folds them with `from_entries`, refuses a second `tool_result` for the same `call_id` in a Turn and a second Turn `seq` in a Conversation, and can be set to fail on its next `append`.
- Consumes: `Message`, `ToolCall`, `Usage` (Task 3).

**Depends-on:** Task 6

- [ ] **Step 1: Failing tests** in `ai/agent/tests/record.rs`:
  - `bodies_round_trip_in_encoding_v1`, with the literals `{"v":1,"text":"hi"}`, `{"v":1,"text":null,"reasoning":"r","tool_calls":[{"id":"c1","name":"probe_look","arguments":"{}"}]}` and `{"v":1,"call_id":"c1","text":"[]","is_error":false,"synthetic":false}`.
  - `a_body_with_another_version_is_unreadable`.
  - `messages_are_derived_in_order_and_steps_add_nothing`.
  - `a_closed_turn_needs_no_repair`.
  - `an_open_turn_is_closed_as_interrupted_with_each_open_call_answered`: closers = one synthetic error result per unanswered call, with dsh's "outcome is unknown" text, then `StepEnded`, then `TurnEnded { end: "interrupted" }`.
  - `repair_run_twice_adds_nothing_the_second_time`.

  Run `cargo test -p navori-agent --test record` — expect: unresolved names.
- [ ] **Step 2: Implement** `record.rs` (serde structs of its own; no `derive(Serialize)` on `navori_llm` types) and `repair.rs`, each with its port header. Run — expect: pass.
- [ ] **Step 3: Gate and commit:** `git commit -m "feat(agent): add the Conversation record types and port dsh's crash repair"`.

_Requirements: LOOP-4.1_

---

### Task 8: A Turn: answer, record, continue, journal

**Files:**
- Create: `ai/agent/src/turn.rs`, `ai/agent/tests/turn.rs`
- Modify: `ai/agent/src/lib.rs`, `ai/agent/tests/common/mod.rs`

**Reuse:** rung 7 — a port of dsh `core/agent-loop` `turn()`/`step()` (agent-loop/src/agent.ts:327-578) without Cordis, streaming, retry waterfalls or the inbox; `EventBus::emit` (kernel/src/bus.rs:40-57)

**Interfaces:**
- Produces: `Agent::new(gateway: Arc<Gateway>, store: Arc<dyn ConversationStore>, bus: EventBus, limits: Limits) -> Agent`; `Agent::run_turn(&self, kernel: &Kernel, request: TurnRequest, cancel: CancellationToken) -> Result<TurnOutcome, TurnStartError>`; `TurnRequest { conversation: Option<ConversationId>, origin: Origin, model: String, input: String }`; `TurnOutcome { conversation: ConversationId, turn: TurnId, end: TurnEnd, answer: Option<String>, steps: u32 }`; `enum TurnEnd { Completed, Cancelled, StepLimit, TokenLimit, Error(TurnErrorKind) }`; `enum TurnErrorKind { Provider, Shield, Audit, Record, NoKey, Keychain }`; `enum TurnStartError { ShutDown, Keychain(String), UnknownConversation, Record(RecordError) }`; `Limits { max_steps: u32, max_tokens: u64, action_timeout: Duration }` with `Default` = 8, 200 000 and 10 s. In tests/common: `ScriptedChat` (a `ChatSender` with a reply queue, `requests()` and `hold_next()`), `MemoryAudit` (an `AuditSink` with `rows()`), `StubSender`, and `agent_with(script, limits) -> Rig`, where `Rig { agent, kernel, chat: Arc<ScriptedChat>, store: Arc<MemoryConversations>, audit: Arc<MemoryAudit>, events: broadcast::Receiver<Event>, probe: Arc<AtomicUsize> }`. Its Gateway is `Gateway::new(Arc<MemorySecrets with a key>, slot, StubSender, MemoryAudit, "deepseek").with_chat_sender(ScriptedChat)`.
- Consumes: Tasks 3–7.

**Depends-on:** Task 3, Task 5, Task 6, Task 7

- [ ] **Step 1: Failing tests** in `ai/agent/tests/turn.rs`:
  - `a_turn_calls_a_read_action_and_answers_from_its_result`: the script is a `probe_look` tool call, then text `"3 things"`. Expect `end Completed`, answer `"3 things"`, `steps 2`, and the second request holding a `Message::Tool` with the probe's reply.
  - `the_record_holds_origin_input_reply_raw_arguments_result_and_usage`: origin `user`; arguments `"{}"` exactly as written; usage `input 120, output 30`; a call with no usage recorded as none reported.
  - `everything_recorded_is_shielded`: the token planted in the input, the reply, the reasoning, the arguments and the probe's result is stored as `[REDACTED:github_token]`, with each message's `redactions` 1.
  - `the_first_request_carries_the_inputs_redaction_count`: the row count from Task 3's sink is 1.
  - `the_model_id_comes_from_the_request`.
  - `a_follow_up_turn_sends_the_earlier_messages_first`.
  - `a_failed_record_write_ends_the_turn_and_sends_nothing_more`: `end Error(Record)`, and no later request.
  - `a_missing_key_ends_the_turn_as_no_key_after_recording_it`.
  - `a_shield_refusal_ends_the_turn`.
  - `one_turn_ended_event_carries_ids_origin_end_and_steps`: module `ai`, kind `ai.turn_ended`, payload keys exactly `conversation, turn, origin, end, steps`, and a summary without the input text.
  - `continuing_an_unended_conversation_repairs_it_and_emits_interrupted`.
  - `an_unknown_conversation_is_refused_before_any_row`.
  - `a_second_turn_started_at_once_on_one_conversation_is_refused`: `TurnStartError::Record(Duplicate)`.
  - `a_reply_cut_at_the_output_limit_with_no_tool_call_completes_with_its_text`.
  - `no_message_text_reaches_the_logs`, using a tracing subscriber that captures at `info` with debug off.

  Run `cargo test -p navori-agent --test turn` — expect: unresolved names.
- [ ] **Step 2: Implement** `turn.rs` per design.md "Turn driver" steps 1–4. Leave out the step limit, the token limit, cancel handling and appending the reminder, which Task 9 adds and tests: here `run_turn` takes the `CancellationToken` but does not watch it yet. Every `ConversationStore` call goes through `spawn_blocking`. The Event is emitted only after `TurnEnded` is recorded. Run — expect: pass.
- [ ] **Step 3: Gate and commit:** `git commit -m "feat(agent): run a Turn that records, continues and journals its end"`.

_Requirements: LOOP-1.3, LOOP-1.4, LOOP-3.1, LOOP-3.2, LOOP-3.3, LOOP-3.4, LOOP-3.5, LOOP-3.6, LOOP-3.7, LOOP-3.9, LOOP-3.10, LOOP-3.11, LOOP-11.5_

---

### Task 9: Limits and cancel

**Files:**
- Create: `ai/agent/tests/limits.rs`
- Modify: `ai/agent/src/turn.rs`

**Reuse:** rung 7 — a port of dsh `core/agent-loop` `turn()`/`step()` (agent-loop/src/agent.ts:327-578) without Cordis, streaming, retry waterfalls or the inbox

**Interfaces:**
- Produces: no new names.
- Consumes: `Agent`, `Limits`, `ScriptedChat::hold_next` (Task 8).

**Depends-on:** Task 8

- [ ] **Step 1: Failing tests** in `ai/agent/tests/limits.rs`:
  - `a_model_that_never_stops_is_ended_at_the_step_limit`: `max_steps 3` with a script of endless `probe_look` calls gives `end StepLimit`, `steps 3` and exactly 3 requests.
  - `a_turn_over_its_token_limit_ends`: `max_tokens 300`, two replies of usage `120+30`, gives `end TokenLimit` after the second.
  - `a_cancel_during_a_model_call_ends_the_turn_and_sends_nothing_more`: hold, cancel, release gives `end Cancelled` and no second request.
  - `a_cancel_while_the_model_answers_reaches_no_module`: hold the model call, cancel, then release a reply that calls `probe_look`. `end Cancelled`, probe counter 0.
  - `every_recorded_call_has_one_result_after_cancel`: the in-flight call gets the "outcome unknown" text, and the call never reached gets the "not started" text.
  - `a_repeated_call_carries_the_reminder_in_its_result`.

  Run `cargo test -p navori-agent --test limits` — expect: failures on the missing limits and the cancel handling.
- [ ] **Step 2: Implement** the step and token checks, `biased` `select!` with the cancel arm first plus `is_cancelled()` before each send and dispatch, the two synthetic texts, and the reminder appended after a blank line. Run `cargo test -p navori-agent` — expect: pass.
- [ ] **Step 3: Gate and commit:** `git commit -m "feat(agent): stop a Turn at its limits or on cancel with every call answered"`.

_Requirements: LOOP-6.1, LOOP-6.2, LOOP-7.1, LOOP-7.2_

---

### Task 10: Wire the loop into the Core and prove it there

**Files:**
- Create: `crates/ffi/tests/agent.rs`
- Modify: `crates/ffi/Cargo.toml`, `crates/ffi/src/ai.rs`, `crates/ffi/src/lib.rs`, `crates/ffi/tests/ask.rs`, `apps/cli/tests/cli.rs`

**Reuse:** rung 2 — `AiRuntime` lazy `OnceCell` building (crates/ffi/src/ai.rs:192-249), `AiDeps` and `start_with` (lib.rs:262), the `on_runtime` hop (lib.rs:244-257), the live gate pattern (ai/llm/tests/live_deepseek.rs:15-24); the `StoreAudit` lazy-migrate pattern (crates/ffi/src/ai.rs:116-168), `Store::migrate` and `with_conn` (crates/store/src/lib.rs:119-169), `spawn_blocking` (gateway.rs:160,187)

**Interfaces:**
- Produces: `AiDeps { open_secrets, make_sender, make_provider: Arc<dyn Fn() -> Arc<dyn LlmProvider> + Send + Sync>, make_conversations: Arc<dyn Fn(Store) -> Arc<dyn ConversationStore> + Send + Sync> }`; `LlmSender::new(Arc<OpenAiCompat>)`; `LlmChatSender`; `StoreConversations::new(Store)` with the Owner `ai_session` v1 migration of design.md "Persisted formats"; `NavoriCore::run_agent_turn(&self, request: TurnRequest, cancel: CancellationToken) -> Result<TurnOutcome, TurnStartError>` (plain Rust, not `#[uniffi::export]`); re-exports `navori_ffi::{TurnRequest, TurnOutcome, TurnEnd, TurnErrorKind, TurnStartError, Origin, ConversationId, ConversationStore}`.
- Consumes: `Agent` (Tasks 8–9), `FakeLlm` (Task 2), `ChatSender` (Task 3).

**Depends-on:** Task 2, Task 9

- [ ] **Step 1: Failing tests** in `crates/ffi/tests/agent.rs`, using `start_with` with `AiDeps { make_provider: FakeLlm, .. }` and a key saved in `FakeKeychain`:
  - `navori_ai_answers_which_ports_are_open_from_ports_list`: the script calls `ports_list`, then answers. The second request holds a `Message::Tool` for that call (its content depends on the machine). `ports_list` is among the offered tools, and no Write Action is.
  - `ports_kill_is_refused_and_the_ports_module_receives_nothing`: the Journal holds no `ports.stopped`, and the refusal text is in the record.
  - `a_three_step_turn_leaves_three_authorized_audit_rows`.
  - `the_turn_is_read_back_from_the_ai_session_tables`, opening the Store on the same file.
  - `the_journal_holds_one_ai_turn_ended_event`.
  - `a_turn_cut_short_by_shutdown_is_repaired_after_restart`: hold the FakeLlm, shut the core down, then read the Conversation id from `ai_session_conversation` through a second `Store` handle (`open_store(&home)`, as ask.rs:454 does). Restart on the same home and continue that Conversation. The old Turn reads `interrupted`, with each call answered.
  - `starting_the_core_builds_no_agent_and_runs_no_ai_session_migration`: `schema_version("ai_session") == 0`.
  - `status_answers_while_a_conversation_write_is_held`: `make_conversations` wraps the store with a `Gate`.
  - `no_new_action_appears`: `core.actions()` lists exactly the current triples of module, action and Risk, read at the base commit and written into the test as literals.

  Run `cargo test -p navori-ffi --test agent` — expect: unresolved names.
- [ ] **Step 2: Implement.** `AiDeps::production()` builds one `Arc<OpenAiCompat>` that both `LlmSender` and `LlmChatSender` share. `StoreConversations` runs `ensure_table()` in `append` and `load`. `AiRuntime::new` takes `bus: EventBus` (cloned before `kernel` moves into `Inner`, lib.rs:318-330). `AiRuntime::agent()` uses `get_or_try_init` and maps a Keychain failure to `TurnStartError::Keychain`. `run_agent_turn` uses `on_runtime`, with `ShutDown` mapped to `TurnStartError::ShutDown`. Update the full `AiDeps` literals at crates/ffi/tests/ask.rs:78 (`Fixture::deps()`), :272, :739, :764, :1140 and apps/cli/tests/cli.rs:102 by adding `..AiDeps::production()`. That fills the new fields; production providers build nothing until a Turn runs. Also update `LlmSender::new` at ask.rs:713 and :834. `crates/ffi/Cargo.toml` gains `navori-agent`, plus `navori-llm` with `fake` as a dev-dependency. Run `cargo test -p navori-ffi && cargo test -p navori-cli` — expect: pass, with `ask.rs` assertions unchanged.
- [ ] **Step 3: Gate and commit:** `git commit -m "feat(ffi): run Navori AI Turns in the Core with the Conversation in SQLite"`.

_Requirements: LOOP-3.8, LOOP-10.3, LOOP-10.4, LOOP-11.2, LOOP-11.3, LOOP-1.1, LOOP-2.2, LOOP-4.1, LOOP-5.2, LOOP-11.6_

---

### Task 11: Catalog Risk matches every Action's own Risk

**Files:**
- Create: `crates/ffi/tests/actions_risk.rs`

**Reuse:** rung 2 — `Kernel::dispatch` and `Envelope` (runtime.rs:330-342; protocol lib.rs:35-39); `tokio::time::timeout` (rung 3)

**Interfaces:**
- Produces: none.
- Consumes: `navori_kernel::SystemAction`, `navori_mod_awake::AwakeAction`, `navori_mod_ports::PortsAction`, `navori_mod_integrations::IntegrationsAction` and their `ModuleAction::risk`; `NavoriCore::actions()`.

**Depends-on:** Task 10

- [ ] **Step 1: Write** `every_action_s_catalog_risk_matches_its_own_risk`. For each `ActionView` of a started core, a `sample(module, name) -> serde_json::Value` match (panicking on an unknown pair) builds a JSON value, which is deserialized into the module's enum. The test then asserts `serde_json::to_value(action.risk())? == serde_json::Value::String(view.risk)` (`Risk` serializes snake_case; `risk_name` is private). The core is started with `start_with` and a minimal `AiDeps` built as ask.rs's `Fixture::deps()` is. Also assert the count of `ActionView`s equals the number of sample arms, so a new Action without a sample fails. Run `cargo test -p navori-ffi --test actions_risk` — expect: pass (it documents present agreement; a drift would fail).
- [ ] **Step 2: Gate and commit:** `git commit -m "test(ffi): check every Action's catalog Risk against its own Risk"`.

_Requirements: LOOP-2.6_

---

### Task 12: The operator's live Turn

**Files:**
- Create: `crates/ffi/tests/live_agent.rs`

**Reuse:** rung 2 — `AiRuntime` lazy `OnceCell` building (crates/ffi/src/ai.rs:192-249), `AiDeps` and `start_with` (lib.rs:262), the `on_runtime` hop (lib.rs:244-257), the live gate pattern (ai/llm/tests/live_deepseek.rs:15-24)

**Interfaces:**
- Produces: none.
- Consumes: `NavoriCore::run_agent_turn` (Task 10), `AiDeps::production()`.

**Depends-on:** Task 10

- [ ] **Step 1: Write** `a_live_turn_answers_from_a_real_read_action`, marked `#[ignore = "calls DeepSeek with a key from pass; run by hand with NAVORI_LIVE_LLM=1 and --ignored"]`. It returns early unless `NAVORI_LIVE_LLM=1`, and reads `NAVORI_LIVE_PASS_ENTRY` and `NAVORI_LIVE_MODEL`. It reads the key with `pass show <entry>` inside the test, saves it into a `FakeKeychain`, and starts a core with production providers. It runs the input `Which TCP ports are open on this Mac? Use the tools.` and prints the answer, the end and the Actions called. It asserts only `end` is `Completed` or `StepLimit`. Run `cargo test -p navori-ffi --test live_agent` — expect: 1 ignored. No agent sets the variables.
- [ ] **Step 2: Gate and commit:** `git commit -m "test(ffi): add the operator-run live Agent loop Turn"`.

_Requirements: LOOP-9.1_

---

### Task 13: Docs, unchanged behavior and the whole gate

**Files:**
- Modify: `docs/codebase/dependencies.md`, `docs/codebase/modules.md`, `docs/codebase/map.md`, `docs/architecture/data.md`, `docs/architecture/system.md`

**Reuse:** rung 1 — no new code; these hold by leaving `crates/kernel`, `crates/store` and every Module's code untouched

**Interfaces:**
- Produces: none.
- Consumes: the whole branch.

**Depends-on:** Task 10, Task 11, Task 12

- [ ] **Step 1: Docs:**
  - data.md:27 reads "one row per model call", and data.md:21 notes the `ai` module's `ai.turn_ended` Event.
  - The `ai_session_*` row moves from Planned to built, with the five tables, the two indexes and body encoding v1.
  - system.md:69/72 states what is built.
  - modules.md:41-42 and map.md:29 gain `navori-agent` and the `navori-context` → `navori-llm` types edge.
  - Estimated cost stays named as ROAD-11's (approved by the operator 2026-11-05).
- [ ] **Step 2: Unchanged behavior:** `git diff --stat $(git merge-base main HEAD) -- crates/kernel crates/store modules` shows no change. `cargo test -p navori-kernel -p navori-store -p navori-mod-awake -p navori-mod-ports -p navori-mod-integrations` passes.
- [ ] **Step 3: Whole gate:** `cargo xtask ci` — expect: pass.
- [ ] **Step 4: Perf, on an idle machine:** run `cargo xtask perf` at the merge base and at the head, without `--record`. Compare each metric's head median with the base median + max(10%, the metric's floor) (performance.md:35-36), and write both tables into the task report. A failing metric blocks the merge, not this plan.
- [ ] **Step 5: Commit:** `git commit -m "docs(loop): record the built Agent loop, Conversation tables and dependency edges"`.

_Requirements: LOOP-10.5, LOOP-10.6, LOOP-11.1_
