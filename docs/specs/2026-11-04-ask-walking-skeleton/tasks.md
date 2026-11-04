# Tasks: Ask walking skeleton

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: ASK
Status: In-progress
Date: 2026-11-04
Approved by the operator ("approve", 2026-11-04 14:14, on the review page at c59c1d5, no card selected).
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

**Goal:** Deliver text-only `navori ask --model <id> "<text>"` end to end: one `openai_compat` adapter tested against DeepSeek, one key saved from Settings → AI, and a minimal Context Gateway (Shield on text, `context_audit`, fail-closed).

**Architecture:** `ai/llm` (`navori-llm`) holds the Chat Completions adapter; `ai/context` (`navori-context`) holds the Gateway that orders key read → Shield → audit → send behind `Sender` and `AuditSink` traits; `navori-ffi` wires both, opens the Keychain lazily, implements the audit sink on `Store`, serves a new `core.ask` IPC method and exports two key calls to Swift; the CLI and a Settings tab are thin clients of those.

**Tech Stack:** Rust 2024 (rust-version 1.90), `reqwest` 0.13.5 (new), `tokio` 1.54.1, `futures`, `serde_json`, `thiserror`, `rusqlite` via `navori-store`, UniFFI 0.32, SwiftUI (Swift 6), clap; `cargo test -p <crate>`, `cargo xtask ci`, `swift test` via xtask.

**Source classes:** every value in `requirements.md` and `design.md` is a Lock, `agent-decided`, or Unverified. A task never promotes one to the operator's choice, and a code comment never says an `agent-decided` value was decided by the operator.

## Global Constraints

Source of truth (referenced, not copied — read before every task):

| Source | Content hash (`git hash-object`, first 12) |
|---|---|
| `docs/standards/INDEX.md` | `f83e09733ee4` |
| `docs/standards/testing.md` | `b1a9b42fc1d3` |
| `docs/standards/errors-logging.md` | `8696a9b77e1f` |
| `docs/standards/performance.md` | `ebf1670ff150` |
| `docs/architecture/INDEX.md` (ARCH-N) | `2e318bd23ff5` |

Compact delta every task inherits:

- **Test-first at agreed seams** (`design.md` "Seams for testing"). Test names are sentences in domain language; no requirement ID in test or production source; expected values are literals, not recomputed. Fakes only at system boundaries (Keychain, network, clock).
- **Secret-shaped fixtures** are fake and split with `concat!`. The one planted secret in tests is `concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123456789")`, which the `github_token` rule (crates/redact/src/lib.rs:56-59) turns into `[REDACTED:github_token]`.
- **Build limits** — no agent step reads or writes a real Keychain item, runs `pass show`, or sets `NAVORI_LIVE_LLM` (only the person who owns the key does, by hand). Tests write only inside the worktree and temp directories. `cargo xtask perf` and `cargo xtask run` launch the dev app as in the normal dev loop (its data lives under `~/Library`); neither is run with `--record`. The one manual check (Task 7 step 4) needs a person at the screen and does not block the build.
- **ARCH-1, 2, 4, 6, 7, 8, 11, 12, 13, 14, 15, 17** hold in every task; no `unsafe` (workspace lint); `[lints] workspace = true` on every new crate; no `unwrap`/`expect` outside tests (test files that use them start with `#![allow(clippy::unwrap_used, clippy::expect_used)]`).
- **Gate** — `cargo fmt --all` then `cargo clippy --workspace --all-targets -- -D warnings` and the task's `cargo test -p <crate>`; the whole `cargo xtask ci` runs in Task 9 and before any commit that touches Swift (Task 7).
- **Commits** — Conventional Commits, one logical change per commit, every commit builds. No `git push`.
- **Unverified, stay open (no task turns them into an assertion):** whether a Keychain call prompts and which error a locked or denied Keychain returns; the release `.app` size and idle memory with `reqwest`; DeepSeek's error body shape and whether the model ids `deepseek-flash` and `deepseek-v4-pro` are current; the name of the person's `pass` entry for the live test; the perf numbers before this change (both are measured in Task 9, in one session).

## File Structure

Create:
- `ai/llm/Cargo.toml`, `ai/llm/src/lib.rs`, `ai/llm/src/openai_compat.rs` — `navori-llm`: `LlmProvider`, `ChatRequest`, `OpenAiCompat`, `LlmError`, `DEEPSEEK_BASE_URL`.
- `ai/llm/tests/common/mod.rs`, `ai/llm/tests/openai_compat.rs`, `ai/llm/tests/live_deepseek.rs` — adapter tests against a local TCP responder; the gated live test.
- `ai/context/Cargo.toml`, `ai/context/src/lib.rs`, `ai/context/src/gateway.rs`, `ai/context/tests/gateway.rs` — `navori-context`: `Gateway`, `Sender`, `AuditSink`, `AuditRow`, `AskError`.
- `crates/protocol/tests/ask_wire.rs` — wire constants and structs.
- `crates/ffi/src/ai.rs` — `AiRuntime`, `AiDeps`, `StoreAudit`, `LlmSender`, `service_for`.
- `crates/ffi/tests/ask.rs` — `core.ask`, key calls, lazy start, audit table, error codes.
- `apps/cli/src/ask.rs` — the `ask` arm.
- `apps/macos/Navori/Features/Settings/AITab.swift`, `AIKeyModel.swift` — Settings → AI.

Modify:
- `Cargo.toml`, `Cargo.lock` — members, `navori-llm`/`navori-context` path deps, `reqwest`.
- `crates/protocol/src/lib.rs` — `rpc::method::ASK`, four codes, `AskRequest`, `AskReply`.
- `crates/ffi/Cargo.toml`, `crates/ffi/src/lib.rs`, `crates/ffi/src/app.rs` — deps, `CoreError::Secrets`, `start_with`, exports, `Inner.ai`, the `ASK` arm.
- `apps/cli/Cargo.toml`, `apps/cli/src/main.rs`, `apps/cli/tests/cli.rs` — dev-deps, `Ask` variant, tests.
- `apps/macos/Navori/Features/Settings/SettingsView.swift`, `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/NavoriBanner/CoreStartupBanner.swift`, `apps/macos/Tests/NavoriBannerTests/CoreStartupBannerTests.swift`.
- `docs/codebase/dependencies.md`, `docs/codebase/map.md`, `docs/codebase/modules.md`, `docs/architecture/data.md`, `docs/standards/errors-logging.md`.

Retrieval: `blast_radius` on the mapped paths and `cluster(ASK)` ran as in /Users/jayden/Developer/experiments/NAVORI/.skills/_pending-ask-skeleton/auto/subgraph-design.md (not committed) (owns_coverage 8/13): HOOKS (In-progress) shares `apps/cli/src/main.rs`, `crates/ffi/src/{app,lib}.rs` and `SettingsView.swift`; rebase on its merge before Tasks 5a, 5b, 6, 7.

---

### Task 1: Workspace and crate shells

**Files:**
- Modify: `Cargo.toml`
- Create: `ai/llm/Cargo.toml`, `ai/llm/src/lib.rs`, `ai/context/Cargo.toml`, `ai/context/src/lib.rs`

**Reuse:** existing — the `[workspace.dependencies]` pattern and the crate manifest of `crates/redact/Cargo.toml` (rung 2); `check-deps` discovers crates by `cargo metadata` (rung 2).

**Interfaces:**
- Produces: workspace members `ai/llm` (`navori-llm`) and `ai/context` (`navori-context`); `reqwest = { version = "0.13.5", features = ["json"] }` in `[workspace.dependencies]`; `navori-llm`, `navori-context` path entries.

**Depends-on:** none

- [x] **Step 1: Failing check** — add the two members to `Cargo.toml` with empty crates absent; run `cargo check --workspace` — expect: manifest not found for `ai/llm`.
- [x] **Step 2: Implement** — create both crates (`[package]` from the workspace, `[lints] workspace = true`, empty `lib.rs` with a one-line doc comment), `navori-context` depending on `navori-redact`, `navori-secrets`, `tokio`, `futures`, `thiserror`, `tracing`; `navori-llm` depending on `navori-secrets`, `reqwest`, `serde`, `serde_json`, `futures`, `tokio`, `thiserror`. Run `cargo check --workspace && cargo xtask check-deps` — expect: pass.
- [x] **Step 3: Verify the lockfile** — `git diff --stat Cargo.lock` shows added packages only; no existing version moves (SECRETS-10.2 guard). Run `cargo update` is forbidden.
- [x] **Step 4: Commit** — `git commit -m "build(ai): add navori-llm and navori-context crate shells and reqwest"`.

_Requirements: ASK-9.6, ASK-9.7_

---

### Task 2: The `core.ask` wire contract

**Files:**
- Modify: `crates/protocol/src/lib.rs`
- Test: `crates/protocol/tests/ask_wire.rs`

**Reuse:** existing — `rpc::method`/`rpc::code` consts (crates/protocol/src/lib.rs:140-158) and the typed root structs (rung 2).

**Interfaces:**
- Produces: `rpc::method::ASK: &str = "core.ask"`; `rpc::code::{ASK_NO_KEY = -32004, ASK_KEYCHAIN = -32005, ASK_BLOCKED = -32006, ASK_PROVIDER = -32007}`; `AskRequest { model: String, text: String }`, `AskReply { reply: String }` (both `Debug, Clone, PartialEq, Serialize, Deserialize`).

**Depends-on:** Task 1

- [x] **Step 1: Failing tests** — `core_ask_has_its_method_name_and_four_distinct_codes_in_the_free_range`: assert `ASK == "core.ask"`, the four codes equal the literals above, all are `< -32003` and `>= -32099`, and all differ from every existing code. `ask_params_and_reply_round_trip_through_json`: serialize `AskRequest{model:"m", text:"hi"}` and expect `{"model":"m","text":"hi"}`; parse `{"reply":"ok"}` into `AskReply`. Run `cargo test -p navori-protocol --test ask_wire` — expect: unresolved names.
- [x] **Step 2: Implement** the constants and structs; `navori-protocol` gains no dependency. Run the same command — expect: pass.
- [x] **Step 3: Guard** — run `cargo test -p navori-protocol -p navori-ipc` — expect: all existing tests pass unchanged.
- [x] **Step 4: Commit** — `git commit -m "feat(protocol): add the core.ask method, its codes and wire structs"`.

_Requirements: ASK-4.2, ASK-7.5, ASK-9.2_

---

### Task 3: The `openai_compat` adapter

**Files:**
- Create: `ai/llm/src/openai_compat.rs`, `ai/llm/tests/common/mod.rs`, `ai/llm/tests/openai_compat.rs`
- Modify: `ai/llm/src/lib.rs`

**Reuse:** existing — `futures::future::BoxFuture` as in `Handler` (crates/ipc/src/lib.rs:26-32), `navori_secrets::Secret` (rung 2/5); new dependency `reqwest` (rung 7, decided by dependencies.md:72 and Lock L19); local TCP responder on `tokio::net::TcpListener` (rung 3; `wiremock` is not in the workspace).

**Interfaces:**
- Produces: `trait LlmProvider: Send + Sync { fn complete<'a>(&'a self, req: ChatRequest, key: &'a Secret) -> BoxFuture<'a, Result<ChatReply, LlmError>>; }`; `ChatRequest { model: String, messages: Vec<Message> }`; `Message { role: Role, text: String }`; `Role::User`; `ChatReply { text: String }`; `OpenAiCompat::new(base_url: impl Into<String>) -> Self`; `LlmError::{Network, Timeout, Status(u16), Malformed}` with Display naming the kind and status only; `pub const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com"` and `pub const DEEPSEEK_PROVIDER: &str = "deepseek"` (both `agent-decided`; the second names the Provider for the Keychain slot and the audit row, not the adapter).
- Consumes: `navori_secrets::Secret`.

**Depends-on:** Task 1

- [x] **Step 1: Failing tests** with a responder in `tests/common` that serves any number of connections and requests in a loop, records each raw request, counts accepted connections, and replies with a canned status and body. `the_adapter_posts_one_user_message_without_streaming`: after `complete(model "m", one user message "hello", key "test-key-ZZ9")` against a 200 reply `{"choices":[{"message":{"content":"hi there"}}]}` expect the reply text `"hi there"`, request line `POST /chat/completions`, header `authorization: Bearer test-key-ZZ9`, and a JSON body whose keys are exactly `model`, `messages`, `stream` with `stream == false` and `messages == [{"role":"user","content":"hello"}]` (no system prompt, history or tools). `a_non_success_status_is_reported_by_status_only`: with the key `test-key-ZZ9` and prompt `hello`, a 401 reply with body `echo hello test-key-ZZ9` yields `LlmError::Status(401)` whose Display contains `401` and neither `hello` nor `test-key-ZZ9`. `a_status_adds_what_the_person_can_check`: table-driven over 401 (the key in Settings → AI), 402 (the provider account balance), 404 and 422 (the model id given to `--model`), 429 (try again shortly), each Display holding the status and its hint and still neither the prompt nor the key; `a_body_without_a_first_choice_with_text_is_malformed` (empty `choices`, and empty `content`). `a_refused_connection_is_a_network_error`. `the_deepseek_preset_is_an_https_url`: `DEEPSEEK_BASE_URL.starts_with("https://")` and `DEEPSEEK_PROVIDER == "deepseek"`. Run `cargo test -p navori-llm` — expect: unresolved names.
- [x] **Step 2: Implement** `OpenAiCompat` (a hand-written `Debug`, since it holds a `OnceCell<reqwest::Client>` and a base URL) with a `tokio::sync::OnceCell<reqwest::Client>` built on the first call (`timeout(120 s)`, `agent-decided`), `bearer_auth`, a private `#[derive(Deserialize)]` for `choices[0].message.content`; map reqwest errors to `Network`/`Timeout` without including the URL query or body. Run `cargo test -p navori-llm` — expect: pass.
- [x] **Step 3: Client is built on first use** — add `the_client_is_not_built_until_the_first_call`: `OpenAiCompat::new(..)` followed by no call makes no connection (the responder's accept count stays 0), and two calls reuse one client (a public `OpenAiCompat::client_builds(&self) -> usize` counter reads 0 before the first call and 1 after two calls). Run — expect: pass.
- [x] **Step 4: Gate** — `cargo fmt --all && cargo clippy -p navori-llm --all-targets -- -D warnings`.
- [x] **Step 5: Commit** — `git commit -m "feat(llm): add the openai_compat Chat Completions adapter"`.

_Requirements: ASK-4.1, ASK-4.6, ASK-7.3, ASK-7.6, ASK-10.7_

---

### Task 4: The Gateway

**Files:**
- Create: `ai/context/src/gateway.rs`, `ai/context/tests/gateway.rs`
- Modify: `ai/context/src/lib.rs`

**Reuse:** existing — `navori_redact::redact_text` and `Redacted` (crates/redact/src/lib.rs:19-30,:120), `navori_secrets::{SecretStore, SecretKey, Secret}`, `tokio::task::spawn_blocking`, `futures::future::BoxFuture` (rungs 2/5); `Gateway` itself is rung 7.

**Interfaces:**
- Produces: `trait Sender: Send + Sync { fn send<'a>(&'a self, key: &'a Secret, model: &'a str, text: &'a str) -> BoxFuture<'a, Result<String, SendError>>; }`; `struct SendError(pub String)`; `trait AuditSink: Send + Sync { fn record(&self, row: AuditRow) -> Result<(), AuditError>; }`; `struct AuditError`; `struct AuditRow { ts_ms: i64, provider: String, kinds: Vec<String>, redacted_count: u32, outcome: Outcome }`; `enum Outcome { Authorized, Blocked }`; `enum AskError { NoKey, Keychain(String), Shield, Audit, Send(String) }` with Display `No key set. Add one in Settings → AI.` / `{m}` / `The Shield failed, so nothing was sent.` / `The audit log could not be recorded, so nothing was sent.` / `{m}`; `Gateway::new(secrets: Arc<dyn SecretStore>, key: SecretKey, sender: Arc<dyn Sender>, audit: Arc<dyn AuditSink>, provider: &'static str)`; `Gateway::with_shield(.., shield: fn(&str) -> Redacted)`; `async fn Gateway::ask(&self, model: &str, text: &str) -> Result<String, AskError>`.
- Consumes: `navori_secrets::fake::{FakeKeychain, MemoryStore}` (dev-dependency, feature `fake`) in tests.

**Depends-on:** Task 1

- [x] **Step 1: Failing tests** (recording `Sender` and `AuditSink` fakes that append to one shared `Vec<String>` of event names so order is observable). `an_ask_reads_the_key_then_shields_then_audits_then_sends`: with a stored key and text containing the planted secret, the shared log is exactly `["audit:authorized", "send"]` and the sender received a text containing `[REDACTED:github_token]` and not the planted secret, a key equal to the stored one, and the audit row has `provider == "deepseek"`, `kinds == ["text"]`, `redacted_count == 1`, `outcome == Authorized`, and no field containing the prompt text. `the_reply_comes_back_unchanged` (sender returns text with the planted secret; `ask` returns it as is). `with_no_key_stored_nothing_is_inspected_or_audited`: `Err(AskError::NoKey)` and an empty log. `a_keychain_failure_is_not_reported_as_no_key`: `FakeKeychain::fail_next(Operation::Get)` yields `Keychain(m)` with `m` equal to the `SecretError`'s Display, and the Display of the result does not contain `No key set`. `a_shield_panic_refuses_the_ask_and_records_a_blocked_row` (shield stub that panics): `Err(Shield)`, log `["audit:blocked"]`, no `send`. `a_malformed_shield_result_is_refused` (stub returning findings out of order). `a_failing_audit_write_stops_the_send` and `a_panicking_audit_sink_stops_the_send`: `Err(Audit)`, no `send`. `a_failing_send_is_reported_with_its_message`: `Err(Send(m))`, the audit row stays `Authorized`. Run `cargo test -p navori-context` — expect: unresolved names.
- [x] **Step 2: Implement** `Gateway::ask` in the order of design.md: `secrets.get` in `spawn_blocking` (JoinError → `Keychain("the Keychain call did not finish")`), `catch_unwind(AssertUnwindSafe(|| shield(text)))` plus the sorted/in-bounds check, `audit.record` in `spawn_blocking` (JoinError or `AuditError` → one `tracing::warn!("context audit write failed")` with no field, then `Audit`), then `sender.send`. The `Blocked` row is best-effort after a Shield failure. Hand-write `Debug` for `Gateway` (it holds `Arc<dyn …>`). No log line or error receives `text`, the redacted text, the reply or the key. Run `cargo test -p navori-context` — expect: pass.
- [x] **Step 3: One call site in this crate** — add `the_context_crate_calls_the_sender_in_one_place`: a source-grep test over `ai/context/src` finding exactly one `.send(` call site, in `gateway.rs`. It is crate-local and does not satisfy the follow-up that enforces "only the Gateway calls the provider" across the app (requirements Out of Scope; ASK-6.4 stays as written). Run `cargo test -p navori-context` — expect: pass.
- [x] **Step 4: Gate** — `cargo fmt --all && cargo clippy -p navori-context --all-targets -- -D warnings`.
- [x] **Step 5: Commit** — `git commit -m "feat(context): add the Gateway with Shield, audit and fail-closed ordering"`.

_Requirements: ASK-5.1, ASK-5.2, ASK-5.3, ASK-5.6, ASK-6.1, ASK-6.2, ASK-6.3, ASK-6.4, ASK-7.1, ASK-7.2, ASK-10.6_

---

### Task 5a: Save and remove the key through `navori-ffi`

**Files:**
- Modify: `crates/ffi/Cargo.toml`, `crates/ffi/src/lib.rs`, `crates/ffi/src/app.rs`
- Create: `crates/ffi/src/ai.rs`
- Test: `crates/ffi/tests/ask.rs`

**Reuse:** existing — the `dispatch` export pattern (crates/ffi/src/lib.rs:377-391: oneshot + `self.handle.spawn`), `KeychainStore::open`/`KeychainService` (crates/secrets/src/keychain.rs:21, key.rs:89), ffi test helpers `home()`, `config()`, `start()` (crates/ffi/tests/core.rs:27-61, copied into `ask.rs`), `navori_secrets::fake::{FakeKeychain, MemoryStore}` (rungs 2/5); `AiRuntime`, `AiDeps` and `service_for` are rung 7.

**Interfaces:**
- Produces: `CoreError::Secrets { message: String }` (Display `{message}`); `pub struct AiDeps { pub open_secrets: Arc<dyn Fn(KeychainService) -> Result<Arc<dyn SecretStore>, OpenError> + Send + Sync>, pub make_sender: Arc<dyn Fn() -> Arc<dyn Sender> + Send + Sync> }` with `AiDeps::production()`; `NavoriCore::start_with(config: CoreConfig, listener: Arc<dyn SnapshotListener>, deps: AiDeps) -> Result<Arc<Self>, CoreError>` in a plain `impl` block, `start` calling it with `AiDeps::production()`; `pub fn service_for(profile: Profile) -> KeychainService`; exported `async fn save_provider_key(&self, key: String) -> Result<(), CoreError>` and `async fn remove_provider_key(&self) -> Result<bool, CoreError>`; `Inner.ai: AiRuntime` with `secrets()` built by `tokio::sync::OnceCell::get_or_try_init`.
- Consumes: `navori_context::Sender` (Task 4), `navori_config::Profile`.

**Depends-on:** Task 4

- [x] **Step 1: Failing tests** in `crates/ffi/tests/ask.rs`, with `AiDeps` whose `open_secrets` builds `MemoryStore::new(service, &vault)` on a shared `FakeKeychain` and counts calls, and whose `make_sender` returns a stub. `the_profile_maps_to_its_keychain_service`: `service_for(Profile::Release).as_str() == "dev.navori"` and `service_for(Profile::Dev).as_str() == "dev.navori.dev"`. `a_saved_key_is_stored_trimmed_under_the_profile_service`: `save_provider_key("  k1\n")` then `MemoryStore::new(KeychainService::DEV, &vault).get(SecretKey::new("llm-provider","deepseek"))` exposes `"k1"`; the release service holds nothing; `vault.item_count() == 1`. `each_profile_opens_its_own_service` (a dev core and a release core on separate homes, each with a factory that records the `KeychainService` it receives: `dev.navori.dev` and `dev.navori`). `saving_replaces_the_earlier_key` (`item_count()` stays 1). `an_empty_key_makes_no_keychain_call`: `"   "` yields `CoreError::Secrets` whose message contains `Nothing was entered` and the open counter stays 0. `a_keychain_failure_reaches_the_message` (`fail_next(Operation::Set)`; the message equals the `SecretError` Display; it does not contain `no key`). `remove_reports_whether_a_key_was_stored` (`true` after a save, `false` the next time, both `Ok`). `starting_the_core_opens_no_keychain`: after `start_with`, the open counter is 0. `the_key_calls_work_from_a_non_tokio_executor`: drive both with `futures::executor::block_on`. `the_key_text_reaches_no_log_config_or_database`: save `concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123456789")`, then search the `config.toml` and the SQLite file bytes for it — absent (Task 5b Step 3 adds the captured tracing output to this test). Run `cargo test -p navori-ffi --test ask` — expect: unresolved names.
- [x] **Step 2: Implement** `CoreError::Secrets`, `AiDeps`, `AiRuntime::secrets()` (not caching a failed open), `start_with` (the body of `start` moved; `Inner` gains `ai`), the two exports using the `self.handle` oneshot hop with `spawn_blocking` inside it (JoinError → `Secrets { "the Keychain call did not finish" }`), `service_for`, and `mod ai;` with `pub use ai::{AiDeps, service_for}` in `lib.rs`, and `crates/ffi/Cargo.toml` entries (`navori-context`, `navori-secrets`; dev: `navori-secrets` with `features = ["fake"]`, `futures`, `tracing-subscriber`). Every new public type that holds an `Arc<dyn …>` gets a hand-written `Debug` (the workspace lint `missing_debug_implementations` is an error under `-D warnings`; `IpcBridge` is the pattern). Run the test file — expect: pass.
- [x] **Step 3: Guard** — `cargo test -p navori-ffi` — expect: every existing ffi test passes unchanged (core start, dispatch, shutdown, single instance).
- [x] **Step 4: Gate** — `cargo fmt --all && cargo clippy -p navori-ffi --all-targets -- -D warnings && cargo xtask check-deps`.
- [x] **Step 5: Commit** — `git commit -m "feat(ffi): save and remove the provider key through the Keychain"`.

_Requirements: ASK-1.1, ASK-1.2, ASK-1.5, ASK-1.6, ASK-2.1, ASK-2.2, ASK-3.1_

---

### Task 5b: Serve `core.ask` and write the audit log

**Files:**
- Modify: `crates/ffi/src/ai.rs`, `crates/ffi/src/app.rs`, `crates/ffi/tests/ask.rs`

**Reuse:** existing — `Store::migrate`/`with_conn` in the Owner pattern (crates/store/src/lib.rs:119,:128; modules/awake/src/infra/stored.rs:4-53), `IpcBridge::handle` and `rpc_err` (crates/ffi/src/app.rs:394-442), `navori_ipc::Client` and `navori_protocol::rpc` in tests (crates/ffi/tests/core.rs:151), `navori_redact::redact_text` (rungs 2/5); `StoreAudit` and `LlmSender` are rung 7.

**Interfaces:**
- Produces: `pub fn ask_error_to_rpc(err: AskError) -> rpc::Error`; `StoreAudit::new(store: Store) -> Self` implementing `navori_context::AuditSink` (migrates owner `context` on the first `record`, success remembered, failure retried; passes `provider` and `kinds` through `redact_text` under `catch_unwind` before the `INSERT`); `LlmSender::new(provider: OpenAiCompat)` implementing `Sender` (`LlmError` → `SendError(String)`); `AiRuntime::gateway()` via `get_or_try_init`; the `rpc::method::ASK` arm in `IpcBridge::handle` mapping `AskError` → `rpc::Error` (`NoKey`→`ASK_NO_KEY`, `Keychain`→`ASK_KEYCHAIN`, `Shield`/`Audit`→`ASK_BLOCKED`, `Send`→`ASK_PROVIDER`; message = the error's Display; bad params → `INVALID_PARAMS`). DDL (`agent-decided` columns): `CREATE TABLE context_audit (id INTEGER PRIMARY KEY AUTOINCREMENT, ts_ms INTEGER NOT NULL, provider TEXT NOT NULL, kinds TEXT NOT NULL, redacted_count INTEGER NOT NULL, outcome TEXT NOT NULL CHECK (outcome IN ('authorized','blocked')));`
- Consumes: `Gateway`, `AuditSink`, `Sender` (Task 4), `AskRequest`/`AskReply`/codes (Task 2), `AiDeps`/`AiRuntime` (Task 5a).

**Depends-on:** Task 2, Task 3, Task 5a

- [x] **Step 1: Failing tests** (real socket via `navori_ipc::Client`, stub `Sender` that records and replies). `core_ask_returns_the_reply_over_the_socket`: with a key saved through `save_provider_key` (never by writing the vault directly, so the slot Task 5a saves to and the slot the Gateway reads, `navori_llm::DEEPSEEK_PROVIDER`, must agree or this test fails), `call(ASK, {"model":"m","text":"hello"})` returns `{"reply": <stub reply>}`. `the_provider_sees_only_redacted_text_and_the_audit_row_has_no_content`: text with the planted secret; the stub's received text holds `[REDACTED:github_token]` and not the secret; `context_audit` (read with `Store::open` on the dev DB path via `with_conn`) has one row: provider `deepseek`, kinds `text`, `redacted_count` 1, outcome `authorized`, and no column containing the prompt, the redacted text or the reply. `the_audit_table_exists_only_after_the_first_ask` (no `context_audit` after `start_with`; present after one ask; `schema_version("context") == 1`). `ask_error_to_rpc_maps_every_variant_to_its_code` (a pure table test on `ask_error_to_rpc`: `NoKey`, `Keychain(m)`, `Shield`, `Audit`, `Send(m)` → `-32004`, `-32005`, `-32006`, `-32006`, `-32007`, message equal to the pinned Display string), and over the socket `no_key_keychain_failure_and_send_failure_return_their_codes` (no saved key; `fail_next(Operation::Get)`; a `Sender` that errors). `the_wire_body_holds_the_marker_not_the_secret`: `make_sender` builds a real `LlmSender` over `OpenAiCompat::new(<local TCP responder url>)` (a small responder copied into `tests/ask.rs`); the recorded HTTP body contains `[REDACTED:github_token]` and not the planted secret. `status_answers_while_an_ask_is_pending` (the ASK-10.4 check: a stub `Sender` that waits on a channel; a second connection's `core.status` returns before the ask completes). `concurrent_first_asks_build_the_runtime_once` (two threads; the `make_sender` and `open_secrets` counters are 1 each). `a_failed_build_is_retried_on_the_next_ask` (`open_secrets` errors once, then succeeds). `starting_the_core_builds_no_sender_and_no_client` (counters 0 after `start_with`). `existing_methods_still_answer` (`core.status`, `core.actions`, `core.events`, `core.doctor`, `core.dispatch` of an unknown module → `UNKNOWN_MODULE`, unchanged). `an_unknown_method_is_still_rejected`. Run `cargo test -p navori-ffi --test ask` — expect: failures on the new tests.
- [x] **Step 2: Implement** `StoreAudit`, `LlmSender`, `AiRuntime::gateway()` (built with the key `SecretKey::new("llm-provider", navori_llm::DEEPSEEK_PROVIDER)` and the same constant as the Gateway's provider id, so the Keychain slot and the audit row name the Provider), `ask_error_to_rpc`, and the `ASK` arm; add `navori-llm` to `crates/ffi/Cargo.toml` and `pub use ai::{LlmSender, ask_error_to_rpc}` in `lib.rs`; hand-write `Debug` for the new public types that hold `Arc<dyn …>` (the arm is an `async` block on the Core runtime; blocking work stays inside the Gateway's `spawn_blocking`). Run the test file — expect: pass.
- [x] **Step 3: Log hygiene** — `tests/ask.rs` runs in its own process and installs one global `tracing` subscriber that writes to a shared in-memory buffer (`tracing-subscriber`, a workspace dependency; the Core's file logging stays off, `log_to_file: false`). Extend `the_key_text_reaches_no_log_config_or_database` (Task 5a) to read that buffer instead of a log directory, and to run an ask with the planted secret in the prompt and a stub reply containing a second planted marker `concat!("ghp_", "ZYXWVUTSRQPONMLKJIHGFEDCBA9876543210")`, and assert neither string appears in the captured tracing output, the config file, the database or any `context_audit` column. Run — expect: pass.
- [x] **Step 4: Gate** — `cargo fmt --all && cargo clippy -p navori-ffi --all-targets -- -D warnings && cargo test -p navori-ffi && cargo xtask check-deps`.
- [x] **Step 5: Commit** — `git commit -m "feat(ffi): serve core.ask through the Gateway and write the audit log"`.

_Requirements: ASK-5.4, ASK-5.5, ASK-9.1, ASK-9.3, ASK-10.2, ASK-10.3, ASK-10.4, ASK-10.5_

---

### Task 6: `navori ask` in the CLI

**Files:**
- Create: `apps/cli/src/ask.rs`
- Modify: `apps/cli/src/main.rs`, `apps/cli/Cargo.toml`, `apps/cli/tests/cli.rs`

**Reuse:** existing — `Command`, `Failure`, `run`, `socket_path` and the `tokio::time::timeout` call in `awake_run` (apps/cli/src/main.rs:34-70,:95-101,:167,:179,:334), `Request::new` and `MAX_LINE_BYTES` (crates/protocol/src/lib.rs:170; crates/ipc/src/lib.rs:21), the test helpers `start_core`, `navori(&home)`, `Running` (apps/cli/tests/cli.rs:18-78), plus a new helper `start_core_with(AiDeps)` beside `start_core` that calls `NavoriCore::start_with` (rungs 2/4).

**Interfaces:**
- Produces: `Command::Ask { model: String /* --model, required */, prompt: String }`; `ask::run(client: &mut Client, model: &str, prompt: &str) -> Result<String, Failure>`; `ASK_WAIT: Duration = 150 s` (`agent-decided`), overridden by the environment variable `NAVORI_ASK_WAIT_MS` (`agent-decided` test knob).
- Consumes: `rpc::method::ASK`, `AskRequest`, `AskReply`, `rpc::code::*` (Task 2); `NavoriCore::start_with`, `AiDeps` (Task 5a/5b) via the existing dev-dependency on `navori-ffi`; new dev-dependencies `navori-context` and `navori-secrets` (`features = ["fake"]`) for the stub `Sender` and the fake Keychain (dev-dependencies are exempt from `check-deps`, xtask/src/main.rs:664-667).

**Depends-on:** Task 5b

- [x] **Step 1: Failing tests** in `apps/cli/tests/cli.rs`. `ask_prints_the_reply_and_exits_zero`: a real core from `start_with` (stub `Sender` returning `hello back`, a saved fake key), `navori ask --model m "hi"` exits 0 with stdout `hello back\n`. `ask_without_model_is_a_usage_error` (exit 2) and `ask_help_describes_the_model_id` (`navori ask --help` mentions the provider's model name and `deepseek-flash`). `ask_prints_a_waiting_notice_on_stderr_when_slow` (a stub `Sender` that waits 3.5 s: stderr holds `waiting for the answer…` once, stdout holds only the reply). `ask_with_no_key_prints_the_sentence_and_exits_one` (stderr contains `navori: No key set. Add one in Settings → AI.` and not `rpc error`). `ask_reports_a_keychain_failure_without_the_no_key_sentence`. `ask_reports_a_shield_or_audit_refusal_as_a_message_with_exit_one`. `ask_when_the_app_is_not_running_exits_three`. `ask_refuses_a_request_line_over_the_limit_before_sending`: build the prompt so `serde_json::to_string(&Request::new(1, ASK, params))` is exactly `MAX_LINE_BYTES` (accepted: reaches the recording fake server) and one byte longer (refused: exit 1, stderr names `1048576`, the fake server saw no request). `ask_gives_up_when_the_app_does_not_answer` (fake server `Reply::Never`; the process is run with `NAVORI_ASK_WAIT_MS=200`, so it gives up long before the fake's 30 s sleep; exit 1, stderr contains `did not answer within 0.2 s` and `may still finish`). Existing tests unchanged. The two request-line tests only need the recording fake server to see (or not see) a request; its `{}` reply is not parsed as an `AskReply` there. Run `cargo test -p navori-cli --test cli ask` — expect: failures.
- [x] **Step 2: Implement** the `Ask` variant and arm, `ask.rs` (`serde_json::to_value(AskRequest{..})`, the length check `len > MAX_LINE_BYTES` with no `+ 1`, `timeout(wait, client.call(..))` (on expiry the message is `the app did not answer within {wait} s (the request may still finish in the app)`, printed from the wait actually used, e.g. `0.2`; and `waiting for the answer…` is printed once on stderr after 3 s), `IpcError::Rpc(e)` → `Failure::Failed(e.message)`, print reply plus newline). Run the same command — expect: pass.
- [x] **Step 3: Guard** — `cargo test -p navori-cli` — expect: every existing CLI test (status, doctor, events, actions, dispatch, hook, awake) passes unchanged, exit codes 0/1/2/3 as before.
- [x] **Step 4: Gate** — `cargo fmt --all && cargo clippy -p navori-cli --all-targets -- -D warnings && cargo xtask check-deps`.
- [x] **Step 5: Commit** — `git commit -m "feat(cli): add navori ask over core.ask"`.

_Requirements: ASK-4.3, ASK-4.4, ASK-4.5, ASK-4.8, ASK-7.4, ASK-9.4_

---

### Task 7: Settings → AI

**Files:**
- Create: `apps/macos/Navori/Features/Settings/AITab.swift`, `apps/macos/Navori/Features/Settings/AIKeyModel.swift`
- Modify: `apps/macos/Navori/Features/Settings/SettingsView.swift`, `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/NavoriBanner/CoreStartupBanner.swift`, `apps/macos/Tests/NavoriBannerTests/CoreStartupBannerTests.swift`

**Reuse:** existing — `IntegrationsModel`/`IntegrationsTab` as the model and tab pattern (apps/macos/Navori/Features/Settings/), `CoreStore.dispatch` (apps/macos/Navori/Core/CoreStore.swift:73) as the wrapper template, `ActionErrorLine` from `NavoriPanel`, the exhaustive switch in `CoreStartupBanner.message(for:)` (apps/macos/NavoriBanner/CoreStartupBanner.swift:9-20) (rung 2); SwiftUI `SecureField` (rung 4).

**Interfaces:**
- Produces: `SettingsTab.ai`; `CoreStore.saveProviderKey(_ key: String) async throws` and `removeProviderKey() async throws -> Bool` (rethrowing `CoreError.Secrets(message)` as `StoreError.keychain(message)`, a `LocalizedError` whose `errorDescription` is the message); `AIKeyModel` (`@MainActor @Observable`: `status: String?`, `isBusy`, `save(_:)`, `remove()`); `AITab(store:)`.
- Consumes: the UniFFI Swift methods `saveProviderKey(key:)` and `removeProviderKey()` generated from Task 5a; `CoreError.Secrets`.

**Depends-on:** Task 5a

- [ ] **Step 1: Failing test** — in `CoreStartupBannerTests.swift` add `a_keychain_failure_shows_the_core_message_unchanged`: `CoreStartupBanner.message(for: CoreError.Secrets(message: "keychain is locked"))` equals `"keychain is locked"`. Run `cargo xtask build-core` then `swift test --arch arm64 --package-path apps/macos` (the commands xtask uses, xtask/src/main.rs:1184) — expect: the build fails on the non-exhaustive switch or the test fails.
- [ ] **Step 2: Implement** the `.Secrets(let message)` case, `CoreStore` methods and `StoreError.keychain`, `AIKeyModel`, `AITab` (a `Form` with a `SecureField("Provider API key")` bound to `@State`, **Save** and **Remove** buttons, a status `Text`; no `.task` and no call at open; while a save or remove call is pending both buttons are disabled and the status reads `Waiting for the Keychain…`; after a successful save the field is cleared and the status reads `Saved`; `Removed` or `No key was stored` after remove; a thrown error shows its message in red caption text as `IntegrationsTab` does), the `.ai` case with `Label("AI", systemImage: "sparkles")` in `SettingsView`. Run `cargo xtask build-core && swift test --arch arm64 --package-path apps/macos` — expect: pass.
- [ ] **Step 3: Guard** — the Swift build and tests still pass with General, Diagnostics, Integrations and About unchanged; `grep -rn "import NavoriCore" apps/macos/Navori` lists only `CoreStore.swift`; `AITab.swift` and `AIKeyModel.swift` must not import it (ARCH-2).
- [ ] **Step 4: Manual check (needs a person at the screen; does not block the build)** — `cargo xtask run`; open Settings → AI: no Keychain prompt on open; paste a key and Save (`Waiting for the Keychain…` while the call runs, then `Saved`, field cleared); Save with empty text (message `Nothing was entered.`); Remove twice (`Removed`, then `No key was stored`). Record what was seen; whether a prompt appears and the locked-Keychain error stay open questions.
- [ ] **Step 5: Commit** — `git commit -m "feat(settings): add the AI tab to save and remove the provider key"`.

_Requirements: ASK-1.3, ASK-1.4, ASK-1.7, ASK-9.5_

---

### Task 8: Fake-covered paths and the gated live test

**Files:**
- Create: `ai/llm/tests/live_deepseek.rs`, `ai/llm/tests/default_tests_use_fakes.rs`
- Modify: `ai/llm/Cargo.toml` (dev-dependency `navori-secrets`)

**Reuse:** existing — the manual real-Keychain test pattern `crates/secrets/tests/keychain_real.rs` (an `#[ignore]` test with an opt-in variable) (rung 2); `OpenAiCompat` and `DEEPSEEK_BASE_URL` (Task 3).

**Interfaces:**
- Produces: test `a_live_deepseek_call_returns_text` marked `#[ignore]`, run only with `NAVORI_LIVE_LLM=1`, reading the entry name from `NAVORI_LIVE_PASS_ENTRY` and the model id from `NAVORI_LIVE_MODEL`.

**Depends-on:** Task 3

- [ ] **Step 1: Failing test** — write `every_default_test_uses_fakes` in `ai/llm/tests/default_tests_use_fakes.rs`: it reads whichever of the test sources exist (`ai/llm/tests/*.rs` other than itself and `live_deepseek.rs`, `ai/context/tests/*.rs`, `crates/ffi/tests/ask.rs`) and fails if one calls `KeychainStore::open` or `Command::new("pass")` (the patterns are assembled with `concat!` so this file does not contain them whole), or if `live_deepseek.rs` is missing or lacks `#[ignore`. Run `cargo test -p navori-llm --test default_tests_use_fakes` — expect: fails because `live_deepseek.rs` does not exist yet.
- [ ] **Step 2: Implement** the live test: skip with a printed note unless `NAVORI_LIVE_LLM == "1"`; run `Command::new("pass").arg("show").arg(&entry)` with stdout captured (the entry is an argument, not a shell string); hold the output in a `Secret`; call `OpenAiCompat::new(DEEPSEEK_BASE_URL).complete(..)` with a short prompt; assert the reply is non-empty; never print the key or the prompt reply on failure, only the `LlmError` Display. No task step sets the variables or runs `pass`; only the operator does.
- [ ] **Step 3: Verify the default run skips it** — `cargo test -p navori-llm` — expect: the live test is reported `ignored`, everything else passes; `cargo test --workspace` likewise (ASK-8.3).
- [ ] **Step 4: Commit** — `git commit -m "test(llm): add the gated live DeepSeek test"`.

_Requirements: ASK-8.1, ASK-8.2, ASK-8.3_

---

### Task 9: Docs, perf comparison and the whole gate

**Files:**
- Modify: `docs/codebase/dependencies.md`, `docs/codebase/map.md`, `docs/codebase/modules.md`, `docs/architecture/data.md`, `docs/standards/errors-logging.md`

**Reuse:** existing — the planned rows in `docs/codebase/dependencies.md` (~35-41, third-party table `HTTP, SSE`), `map.md:41`, `modules.md:45`, `data.md:37`, `errors-logging.md:12` (rung 2).

**Interfaces:**
- Consumes: every earlier task's crate names and the `context_audit` DDL.

**Depends-on:** Task 5b, Task 6, Task 7, Task 8

- [ ] **Step 1: Edit the docs** — `dependencies.md`: `navori-llm`/`navori-context` in the crate table with allowed edges (`ai/context` → `redact`, `secrets`; `ai/llm` → `secrets`; `ffi` → both), `reqwest` row marked in use with its version; `map.md` and `modules.md`: `ai/llm/` and `ai/context/` landed, `navori-llm`/`navori-context` named; `data.md`: Owner `context`, table `context_audit` (columns as in design Decision 7) and the Keychain account `llm-provider/deepseek`; `errors-logging.md:12`: name `CoreError::Secrets`; catalog card `ASK` status stays `Draft` (the execute skill moves it).
- [ ] **Step 2: Whole gate** — `cargo xtask ci` — expect: fmt, clippy, tests, `check-deps`, `build-core` and the Swift build all pass; report the scope run.
- [ ] **Step 3: Perf comparison, one session on an idle machine** — create a temporary worktree at `git merge-base HEAD main`, run `cargo xtask perf` there and then on this branch, without `--record`; compare idle CPU, wakeups, footprint, cold start and menu open (no regression against the base measured in the same session), and record the release `.app` size and idle footprint added by `reqwest`. If a budget breaks, the fallbacks are trimming reqwest's default features, then `native-tls`; neither is decided here. Remove the temporary worktree afterwards.
- [ ] **Step 4: Trace check** — `audit-trace` over `docs/specs/2026-11-04-ask-walking-skeleton/`: every ASK ID is defined once and cited by a task footer.
- [ ] **Step 5: Commit** — `git commit -m "docs(ask): record the AI crates, the audit table and the Keychain slot"`.

_Requirements: ASK-10.1_

---

## Coverage

Every live ID of `requirements.md` (50; ASK-3.2, ASK-4.7 and ASK-10.8 are struck through there) is cited by exactly one task footer (checked by script at the time of writing): Task 1 — 9.6, 9.7; Task 2 — 4.2, 7.5, 9.2; Task 3 — 4.1, 4.6, 7.3, 7.6, 10.7; Task 4 — 5.1, 5.2, 5.3, 5.6, 6.1, 6.2, 6.3, 6.4, 7.1, 7.2, 10.6; Task 5a — 1.1, 1.2, 1.5, 1.6, 2.1, 2.2, 3.1; Task 5b — 5.4, 5.5, 9.1, 9.3, 10.2, 10.3, 10.4, 10.5; Task 6 — 4.3, 4.4, 4.5, 4.8, 7.4, 9.4; Task 7 — 1.3, 1.4, 1.7, 9.5; Task 8 — 8.1, 8.2, 8.3; Task 9 — 10.1.

Dependencies for parallelism: Tasks 2, 3 and 4 depend only on Task 1 and share no files, so they may run together; Task 5a follows Task 4; Task 5b follows Tasks 2, 3 and 5a; Tasks 6 and 7 follow Task 5b and Task 5a respectively and share no files; Task 8 follows Task 3; Task 9 closes.
