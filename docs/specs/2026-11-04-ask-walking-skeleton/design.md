# Design: Ask walking skeleton

Feature code: ASK
Status: Draft
Date: 2026-11-04
Requirements: ./requirements.md

<!--
Written in an unattended run (auto/charter.md). `agent-decided` marks a value a session picked; Weigh lines come from auto/ledger.md. Evidence: auto/scan.md and auto/scan-design.md (subagent readings; each cited file:line is the subagent's unless this file says it was opened), auto/research-deepseek.md, auto/subgraph-design.md.
Surfaces beyond INDEX.md:82 (for the report; the roadmap is not edited in this run): `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/NavoriBanner/CoreStartupBanner.swift` and `Tests/NavoriBannerTests/CoreStartupBannerTests.swift`, the workspace `Cargo.toml` and `Cargo.lock`, `ai/llm/` and `ai/context/` (listed), `crates/ffi/tests/ask.rs`, `docs/codebase/*`, `docs/architecture/data.md`, `docs/standards/errors-logging.md`.
Persisted formats defined here: `context_audit` (Decision 7) and the Keychain item `llm-provider/openai-compat` (Decision 8). Nothing is migrated in this run.
-->

## Context

`navori ask` does not exist. Today the `navori` CLI is a thin client over a Unix-socket JSON-RPC link: one request line, one response line, handled in order per connection (crates/ipc/src/lib.rs:148-181, :206-263), with a 1 MiB request cap (`MAX_LINE_BYTES`, :21). The server's handler is `IpcBridge::handle` in `navori-ffi`, a `match` over five `core.*` methods that falls through to `METHOD_NOT_FOUND` (crates/ffi/src/app.rs:402-427). Nothing in the Core calls an LLM, reads a Keychain item or writes an audit row: there is no `ai/` directory, `navori-ffi` does not depend on `navori-secrets`, and no HTTP client is in the workspace.

The binding constraint is where the work may happen. The CLI may link only `config`, `ipc` and `protocol` (docs/codebase/dependencies.md:23), so the Shield, the key read, the audit write and the HTTP call must all run inside the running app, reached over IPC; `navori ask` with the app down exits 3. That also rules out the alternative of routing the call through `core.dispatch` as a kernel Action: it would inherit the kernel's 30 s dispatch timeout (crates/kernel/src/runtime.rs:308), invent a Module for one call, and put `ask` into the Action list the AI will later call (ARCH-4). The second constraint is that the key never travels: Swift saves it through `navori-ffi` straight into the Keychain, and the Gateway reads it inside the Core (keychain-secrets requirements.md:119).

Three crates already carry what this feature needs and are reused as they are: `navori-redact` (`redact_text`, crates/redact/src/lib.rs:120), `navori-secrets` (`SecretStore`, `KeychainStore`, the `fake` feature) and `navori-store` (`Store::migrate`, the Owner pattern in modules/awake/src/infra/stored.rs). New code is two small crates (`ai/llm`, `ai/context`), one composition module in `navori-ffi`, a CLI subcommand and a Settings tab. The retrieval (auto/subgraph-design.md) found no feature that owns `ai/*`, `core.ask` or `context_audit` (owns_coverage 8/13); HOOKS, AWAKE, ICON and KCONTRACT share the four files `app.rs`, `lib.rs`, `main.rs` and `SettingsView.swift`, and HOOKS is still In-progress, so those edits stay small and additive.

ARCH invariants relied on: ARCH-1/2 (logic in Rust, Swift only through `CoreStore`), ARCH-4 (ask is not an Action), ARCH-6 and ARCH-7 (Shield, single egress), ARCH-8 (Keychain only), ARCH-11 (Unix socket only), ARCH-12 (migrations), ARCH-13 (a bad audit migration never stops the app), ARCH-14 (Profiles apart), ARCH-15 (no WebView/JS), ARCH-17 (lazy AI runtime).

## Decisions

1. **Entry point (Weigh W7).** `core.ask` is a new arm in `IpcBridge::handle`, not an Action. The handler runs on the Core runtime, so it avoids the kernel timeout. Params and result are typed structs in `navori-protocol` (`AskRequest { model, text }`, `AskReply { reply }`).
2. **One response, no streaming (W6).** The adapter sends `stream: false`; the whole reply returns in one JSON-RPC response. `eventsource-stream` is not added (`agent-decided` deferral of part of Lock L19). The CLI waits up to 150 s and the HTTP call up to 120 s (`agent-decided`).
3. **Adapter and endpoint (Lock L24, W5).** One `openai_compat` adapter. The base URL is a constant `DEEPSEEK_BASE_URL = "https://api.deepseek.com"` in `ai/llm` (`agent-decided`; endpoint data, not a model id). The model id is the required `--model` flag; no config key (that would be the charter's persisted-format hard stop, and ROAD-11 owns configuration).
4. **Gateway shape (W14).** `ai/context` defines the `Sender` trait; `navori-ffi` wires an `ai/llm` provider into it. No `ai/context → ai/llm` edge, no doc edit. `ai/context` depends on `navori-redact` (required, L13) and `navori-secrets` (allowed, L11); `ai/llm` depends on `navori-secrets` only for the `Secret` type.
5. **Order of an ask (W1, W2).** key read → Shield → audit row → send. A missing or unreadable key ends the ask before anything is inspected (ASK-7.1, 7.2); a Shield or audit failure ends it before anything is sent (ASK-6).
6. **Fail-closed meaning (W3, W12).** A Shield failure is a panic caught around `redact_text`, or a result whose findings are out of order or out of bounds. The whole ask is refused (the only portion is the prompt). A poisoned `LazyLock` rule table (a bad regex, crates/redact/src/lib.rs:43,:48) makes every later call panic, so every ask is refused until restart: that fails closed and is accepted for this road. The reply is returned to the CLI only, not Shield-ed, persisted or logged (ASK-5.6).
7. **Audit log format (`agent-decided` columns).** Owner `context`, migration v1 `context_audit`:
   `CREATE TABLE context_audit (id INTEGER PRIMARY KEY AUTOINCREMENT, ts_ms INTEGER NOT NULL, provider TEXT NOT NULL, kinds TEXT NOT NULL, redacted_count INTEGER NOT NULL, outcome TEXT NOT NULL CHECK (outcome IN ('authorized','blocked')));`
   `kinds` is a comma-joined list (`text` today). `authorized` means the Shield passed and the send was cleared, not that the provider answered: a send that then fails still leaves an `authorized` row, and the log cannot tell "sent" from "send failed" (usage and run results are ROAD-14). `blocked` rows are written best-effort on a Shield failure. The migration runs lazily, on the first audit write of the first ask that gets past the key read (so a person who never asks never gets the table, and startup does no AI work, ARCH-17, ASK-9.3); a failed migration is not cached, the ask fails closed at the audit step with the `Audit` error, and the app keeps running (ARCH-13). No pruning in this road (`agent-decided`; retention of the audit log belongs to ROAD-12).
8. **Key slot (W13).** `SecretKey::new("llm-provider", "openai-compat")` under the Profile's service; the key is stored trimmed (`agent-decided`: pasted keys often carry a trailing newline). Profile→service lives in one pure function in `navori-ffi`.
9. **Lazy AI runtime (ARCH-17).** `navori-ffi` holds an `AiRuntime` whose Keychain handle and Gateway are built on first use behind factories; `start` builds neither.
10. **First use is serialized.** `AiRuntime` holds `tokio::sync::OnceCell`s for the secrets handle and the Gateway and builds them with `get_or_try_init`: concurrent first asks build once, and a failed build (Keychain open, factory) is not cached, so the next ask retries. `Store` access is serialized by the mutex in `with_conn` (crates/store/src/lib.rs:119), so concurrent asks cannot interleave audit rows.
11. **Test seam.** `NavoriCore::start_with(config, listener, AiDeps)` is a plain Rust function beside the exported `start` (which calls it with production `AiDeps`). `AiDeps` holds the secrets factory and the sender factory; tests inject a fake Keychain and a fake `Sender`. `CoreConfig` is not touched, because adding a field breaks every test that builds it.
12. **No `xtask` change (W11).** The `starts_with("navori-ai")` rule (xtask/src/main.rs:719) misses `navori-llm`/`navori-context`, and nothing enforces `ai/context → navori-redact` or "only the Gateway calls the provider"; both are follow-ups listed in the requirements' Out of Scope.
13. **New dependency: `reqwest`.** Adoption is already decided by the project (docs/codebase/dependencies.md:73-75; docs/ARCHITECTURE.md:209; Lock L19), so it is not put to a user in this run. Facts from context7 (/seanmonstar/reqwest v0.13.5): TLS is rustls by default, the `json` feature is optional, latest stable `0.13.5` (confirmed with `cargo search`). Declared once in `[workspace.dependencies]` as `reqwest = { version = "0.13.5", features = ["json"] }`; trimming default features is decided after the size measurement (U2) because the exact feature names are unverified. No `async-trait`: the trait methods return `futures::BoxFuture`, as `Handler` does (crates/ipc/src/lib.rs:26-32).
14. **No ADR.** Nothing here is both hard to reverse and surprising: the one hard-to-reverse item (`context_audit`) is named by the road and recorded in `docs/architecture/data.md` by the doc task.

## Architecture

Vocabulary: **module**, **interface**, **implementation**, **seam**.

```
navori ask --model M "text"                         Settings → AI (Swift)
   │ (apps/cli, config+ipc+protocol only)              │ CoreStore (ARCH-2)
   │  core.ask {model,text}  ── Unix socket ──┐        │ saveProviderKey / removeProviderKey
   ▼                                           ▼        ▼
                              navori-ffi: IpcBridge arm        NavoriCore exports
                                   │                              │
                                   └────────► AiRuntime (lazy) ◄──┘   secrets: Arc<dyn SecretStore> (KeychainStore, service by Profile)
                                                │
                                      Gateway (ai/context)
                       1 key ← SecretStore.get   2 redact_text   3 AuditSink.record   4 Sender.send
                                                                        │                      │
                                                          StoreAudit (ffi, Store,       LlmSender (ffi) → OpenAiCompat (ai/llm) → reqwest
                                                          Owner `context`)
```

### Wire contract (`navori-protocol`)

Satisfies: ASK-4.2, ASK-7.5, ASK-9.2
Reuse: rung 2 — extends `rpc::method` and `rpc::code` (crates/protocol/src/lib.rs:140-158) and the typed wire structs at the crate root
Respects: ARCH-4, ARCH-11
Interface: `rpc::method::ASK = "core.ask"`; codes `ASK_NO_KEY = -32004`, `ASK_KEYCHAIN = -32005`, `ASK_BLOCKED = -32006`, `ASK_PROVIDER = -32007` (`agent-decided` values inside the free range -32004..-32099); `AskRequest { model: String, text: String }`; `AskReply { reply: String }`.
Depth: n/a — extends `navori-protocol`; plain data, no behavior.
Locality: `crates/protocol/src/lib.rs` only. `crates/ipc` is unchanged (framing, cap and timeouts stay), which is what ASK-9.2 guards; the 1 MiB check in the CLI uses its public `MAX_LINE_BYTES`. leave `ipc`, extend `protocol`.

### `ai/llm` (`navori-llm`)

Satisfies: ASK-4.1, ASK-4.6, ASK-7.3, ASK-10.7
Reuse: rung 5 for `futures`, `serde`, `serde_json`, `tokio`, `thiserror`, `navori-secrets` (`Secret`); rung 7 for the adapter, plus the one new dependency `reqwest` (Decision 13)
Respects: ARCH-7, ARCH-15, ARCH-17
Security: THR-1, THR-7, THR-10
Interface: `LlmProvider::complete(&self, req: ChatRequest, key: &Secret) -> BoxFuture<'_, Result<ChatReply, LlmError>>` with `ChatRequest { model: String, messages: Vec<Message> }`, `Message { role: Role, text: String }`, `ChatReply { text: String }`; `OpenAiCompat::new(base_url: impl Into<String>)`; `LlmError` (Display carries the failure kind and the HTTP status, never the request text or the key); `DEEPSEEK_BASE_URL`. The key is a per-call argument, not adapter state, so Remove and replace in Settings take effect on the next ask.
Depth: if `ai/llm` vanished, callers would still need to know how to build a Chat Completions body, send it with a bearer header, bound the wait, and read `choices[0].message.content`; they keep knowing only `complete`.
Locality: new crate `ai/llm/`; nothing else in the repo changes for the wire format. extract-ready for ROAD-11's other adapters (the trait is the canonical-format subset: no tools, no streaming, no reasoning).

The adapter POSTs `{base_url}/chat/completions` with `Authorization: Bearer <key>` (the header form is the OpenAI-compatible default and is unverified for DeepSeek, U4) and body `{ "model", "messages": [{ "role": "user", "content" }], "stream": false }` (shape from auto/research-deepseek.md). The `reqwest::Client` is created on the first `complete` call inside a `tokio::sync::OnceCell`, with `timeout(120 s)`. A non-2xx status gives `LlmError::Status(u16)`; an unparsable body or a missing/empty first choice gives `LlmError::Malformed`; no connection or timeout gives `LlmError::Network`/`Timeout`. The response body is never copied into an error.

### `ai/context` (`navori-context`) — the Gateway

Satisfies: ASK-5.1, ASK-5.2, ASK-5.3, ASK-5.6, ASK-6.1, ASK-6.2, ASK-6.3, ASK-6.4, ASK-7.1, ASK-7.2, ASK-10.6
Reuse: rung 2 — `navori_redact::redact_text` (crates/redact/src/lib.rs:120), `navori_secrets::SecretStore`; rung 5 — `tokio::task::spawn_blocking` for the blocking Keychain and audit calls; rung 7 for the Gateway
Respects: ARCH-6, ARCH-7, ARCH-8
Security: THR-1, THR-7, THR-10
Interface: `Gateway::new(secrets: Arc<dyn SecretStore>, key: SecretKey, sender: Arc<dyn Sender>, audit: Arc<dyn AuditSink>, provider: &'static str)` (uses `navori_redact::redact_text`) and `Gateway::with_shield(.., shield: fn(&str) -> Redacted)` as the test seam that lets a test return a malformed `Redacted` or panic; `Gateway::ask(&self, model: &str, text: &str) -> Result<String, AskError>`; traits `Sender { fn send(&self, key: &Secret, model: &str, text: &str) -> BoxFuture<'_, Result<String, SendError>> }` and `AuditSink { fn record(&self, row: AuditRow) -> Result<(), AuditError> }` (sync); `AuditRow { ts_ms, provider, kinds, redacted_count, outcome }`; `AskError { NoKey, Keychain(String), Shield, Audit, Send(String) }`, each with a Display that is the user-facing sentence, pinned so tests can assert it: `NoKey` "No key set. Add one in Settings → AI." (ASK-7.1); `Shield` "The Shield failed, so nothing was sent." (ASK-6.1); `Audit` "The audit log could not be recorded, so nothing was sent." (ASK-6.2); `Keychain(m)` and `Send(m)` show `m` (`agent-decided`: wording).
Depth: if the Gateway vanished, callers would still need the fixed order (key, Shield, audit, send), what counts as a Shield failure, and which failures write which audit row; they keep knowing only `ask`.
Locality: new crate `ai/context/`; the only place the order lives. ROAD-12 extends the same crate with the other Shield layers. extend-ready, not extended now.

`ask` does: (1) `secrets.get(&key)` in `spawn_blocking`: `Ok(None)` → `NoKey`; `Err(e)` → `Keychain(e.to_string())` (never the no-key sentence, ASK-7.2); a `JoinError` (the closure panicked) → `Keychain("the Keychain call did not finish")`. (2) `catch_unwind(redact_text(text))`; a panic, or findings not sorted/non-overlapping/inside the text, → write a `blocked` row best-effort and return `Shield` (ASK-6.1). (3) `audit.record(AuditRow { outcome: Authorized, .. })` in `spawn_blocking`; an `AuditError` **or a `JoinError`** → one content-free `tracing::warn!` and `Audit` (ASK-6.2, 6.3). `StoreAudit::record` runs its own `redact_text` calls under `catch_unwind`, so a panic there is an `AuditError`, not an unwind. (4) `sender.send(&secret, model, &redacted.text)`; failure → `Send(msg)`. The row, logs and errors never receive `text`, the redacted text, the reply or the key (ASK-5.3). "Only the Gateway holds the send handle" (ASK-6.4) is true by wiring: `AiRuntime` hands the `Sender` to nothing but `Gateway::new`.

### Composition in `navori-ffi` (AI runtime, audit store, IPC arm, key calls)

Satisfies: ASK-1.1, ASK-1.2, ASK-1.5, ASK-1.6, ASK-2.1, ASK-2.2, ASK-3.1, ASK-3.2, ASK-4.7, ASK-5.4, ASK-5.5, ASK-9.1, ASK-9.3, ASK-10.2, ASK-10.3, ASK-10.4, ASK-10.5
Reuse: rung 2 — `Store::migrate` and `with_conn` (crates/store/src/lib.rs:128,:119) in the Owner pattern of modules/awake/src/infra/stored.rs; the `dispatch` export pattern (crates/ffi/src/lib.rs:376: oneshot + `self.handle.spawn`); `IpcBridge` (app.rs:392); `KeychainStore::open` and `KeychainService` (crates/secrets/src/keychain.rs:21, key.rs:89); rung 7 for `AiRuntime`, `StoreAudit`, `LlmSender`
Respects: ARCH-1, ARCH-8, ARCH-11, ARCH-12, ARCH-13, ARCH-14, ARCH-17
Security: THR-7, THR-10
Surface:
- `CoreError` gains `Secrets { message }` (Display "{message}"). Readers: `CoreStartupBanner.message(for:)` exhaustive switch (apps/macos/NavoriBanner/CoreStartupBanner.swift:9-20) — **replace**, a `.Secrets(let message)` case is added in this change; `CoreStartupBannerTests` (apps/macos/Tests/NavoriBannerTests/CoreStartupBannerTests.swift:7,19) — **replace**, one test for the new case; Rust matches in crates/ffi/tests/core.rs:109,114,365,399,688 name other variants only — **replace** is not needed, they stay valid; docs/standards/errors-logging.md:12 — **replace** (one sentence names the new variant, in the doc task).
- `IpcBridge::handle` match (app.rs:402-427): new `ASK` arm; the five existing arms and the fall-through are untouched (ASK-9.1).
- `Inner` (app.rs:26-38): gains `ai: AiRuntime`; constructed only at crates/ffi/src/lib.rs:286 — **replace**.
- `NavoriCore::start` (lib.rs:232): same signature and exported name; its body moves into `start_with` — **replace** (no Swift change).
- `CoreConfig`: untouched.
Interface: exported (UniFFI) `async fn save_provider_key(&self, key: String) -> Result<(), CoreError>` and `async fn remove_provider_key(&self) -> Result<bool, CoreError>` (true = removed, false = none was stored); Rust-only `NavoriCore::start_with(config, listener, AiDeps)`; `AiDeps { open_secrets: Arc<dyn Fn(KeychainService) -> Result<Arc<dyn SecretStore>, OpenError> + Send + Sync>, make_sender: Arc<dyn Fn() -> Arc<dyn Sender> + Send + Sync> }`; `fn service_for(profile: Profile) -> KeychainService` (release → `dev.navori`, dev → `dev.navori.dev`); `fn ask_error_to_rpc(AskError) -> rpc::Error` (pure, table-tested, because the Shield and audit failures cannot be injected over the socket through `AiDeps`); `pub use` of `AiDeps`, `LlmSender`, `service_for` and `ask_error_to_rpc` from the crate root; IPC method `core.ask`.
Depth: if the module vanished, callers would still need the lazy construction rules, the Profile→service mapping, the thread hop for blocking calls and the error→code mapping; they keep knowing only the two exports and the IPC method.
Locality: `crates/ffi/src/ai.rs` (new: `AiRuntime`, `StoreAudit`, `LlmSender`, `service_for`, `ask_arm`), one arm in `app.rs`, two exports and `start_with` in `lib.rs`, new deps in `crates/ffi/Cargo.toml`. extend the composition root (the documented home of `ai/*` wiring, dependencies.md:22).

Behaviors:
- `save_provider_key`: trims; an empty result returns `Secrets { "Nothing was entered." }` with no Keychain call (ASK-1.5); otherwise `AiRuntime::secrets()` (opens `KeychainStore` once, on first use, mapping the Profile with `service_for`) then `self.handle.spawn_blocking(set(key, Secret::new(trimmed)))` awaited through the same oneshot hop as `dispatch` (lib.rs:376-390), because a UniFFI `async fn` is polled on Swift's executor, where a bare `tokio::task::spawn_blocking` has no runtime and panics; a `JoinError` maps to `Secrets { message }` ("the Keychain call did not finish"), a `SecretError` to its Display; a `SecretError` becomes `Secrets { message: err.to_string() }` (ASK-1.1, 1.4). No read, no `exists` anywhere on this path or on opening the tab (ASK-1.2). The key text lives only in this call's argument and the `Secret`; it is logged nowhere (ASK-1.6).
- `remove_provider_key`: the same hop with `delete`; `Removed` → `true`, `WasAbsent` → `false`, both success (ASK-2.1, 2.2).
- `core.ask` arm: decodes `AskRequest` (`INVALID_PARAMS` otherwise), calls `AiRuntime::gateway()` (builds secrets, `StoreAudit`, the sender through `make_sender`, and the `Gateway` on first call, ASK-10.2/10.3), awaits `Gateway::ask`, and maps `AskError` → `rpc::Error` with the error's Display as the message and this code: `NoKey` → `ASK_NO_KEY`; `Keychain` → `ASK_KEYCHAIN`; `Shield` and `Audit` → `ASK_BLOCKED`; `Send` → `ASK_PROVIDER` (`agent-decided` split; a ffi test pins each pair). The handler is an `async` block on the Core runtime; blocking work is inside `spawn_blocking` in the Gateway (ASK-10.4); other connections are served by their own tasks, so `core.status` answers while an ask waits (ASK-4.7).
- `StoreAudit`: `new(store)` does nothing; the first `record` runs `store.migrate("context", MIGRATIONS)` under a mutex-guarded flag (success is remembered, failure is retried on the next ask) and then runs one `INSERT` in `with_conn`, passing `provider` and `kinds` through `redact_text` first (ASK-5.4). Migration is transactional per migration (crates/store/src/lib.rs:156); the pre-upgrade backup applies only when the owner already has data (:143-146).
- `LlmSender`: maps `Sender::send` to `OpenAiCompat::complete` and `LlmError` to `SendError(String)`.
- `start_with` builds no HTTP client, opens no Keychain and reads no item (ASK-10.2, tested with counting factories).

### CLI `ask`

Satisfies: ASK-4.3, ASK-4.4, ASK-4.5, ASK-7.4, ASK-9.4
Reuse: rung 2 — `Command`/`Failure`/`run` and `Client::call` with `tokio::time::timeout` as in `awake_run` (apps/cli/src/main.rs:334); `Request::new` and `MAX_LINE_BYTES` for the size check; rung 4 — clap's required-argument exit code 2
Respects: ARCH-1, ARCH-11
Surface:
- `enum Command` (main.rs:34-70) and its `match cli.command` in `run` (:179+): new `Ask` variant and arm — **replace** (the match is exhaustive; all existing arms stay as they are, ASK-9.4).
- `Failure`/`From<IpcError>` (main.rs:95-101): unchanged; the `Ask` arm converts `IpcError::Rpc(e)` itself so the person sees `e.message`, not "rpc error -32004: …" — **replace** for that arm only.
- `docs`/help text: the clap doc comment on the new variant is the help.
Interface: `navori ask --model <ID> <PROMPT>`; stdout gets the reply and a trailing newline; stderr gets `navori: <message>`; exits 0/1/2/3 as today (ASK-7.5).
Depth: n/a — extends `navori-cli`.
Locality: `apps/cli/src/main.rs` plus one small `ask.rs` module if the arm grows; `apps/cli/tests/cli.rs` for tests. leave `hook.rs` and every other arm.

The arm builds an `AskRequest`, turns it into a `Value` with `serde_json::to_value`, measures `serde_json::to_string(&Request::new(1, method::ASK, params))` against `MAX_LINE_BYTES` (a fresh `Client` numbers its first call 1, so the length is exact; the server's `LinesCodec` bound excludes the newline, so the test is `len > MAX_LINE_BYTES` with no `+ 1`, with a test at exactly the limit and one byte over) and fails with a message naming the limit before connecting (ASK-4.4). It then connects (exit 3 if the app is down, ASK-7.4), calls with `timeout(wait)` where `wait` is 150 s unless the environment variable `NAVORI_ASK_WAIT_MS` holds a millisecond count (`agent-decided` test knob, undocumented in `--help`; the CLI is a separate binary, so tests drive the wait through the process), and on expiry exits 1 with "the app did not answer within <wait> s", naming the wait actually used, (ASK-4.5).

### Settings → AI tab

Satisfies: ASK-1.3, ASK-1.4, ASK-9.5, ASK-10.8
Reuse: rung 2 — the `IntegrationsModel`/`IntegrationsTab` pattern (apps/macos/Navori/Features/Settings/), `CoreStore.dispatch` as the template for a store wrapper (Core/CoreStore.swift:73), `ActionErrorLine` from `NavoriPanel`, `Form{}.formStyle(.grouped)`; rung 4 — SwiftUI `SecureField`
Respects: ARCH-1, ARCH-2
Surface:
- `enum SettingsTab` (SettingsView.swift:5-7) gains `.ai`. Readers: `TabView` in SettingsView.swift:20-33 — **replace** (one new `.tabItem`/`.tag`); `StatusBarController.showSettings`/`openSettings` (StatusBarController.swift:11,49,287) and `SettingsWindowController.show(tab:)` (:14) take a `SettingsTab` value and need no edit — **replace** not needed; they keep working.
- `CoreStore` (Core/CoreStore.swift): gains `saveProviderKey(_:)` and `removeProviderKey()`; `StoreError` gains `.keychain(String)` (LocalizedError) — **replace** (additive).
Interface: `AITab(store:)` with a `SecureField`, **Save** and **Remove**, a status line ("Saved", "Removed", "No key was stored", or the error message); the field is cleared after a successful save; the tab runs no `.task` that reads state. `CoreStore.saveProviderKey(_:) async throws` rethrows `CoreError.Secrets(message)` as `StoreError.keychain(message)` so `NavoriBanner` and views show the Rust text unchanged.
Depth: n/a — extends the Settings feature.
Locality: new `Features/Settings/AITab.swift` and `AIKeyModel.swift`; one case in `SettingsView.swift`; two methods in `CoreStore.swift`. The Swift side holds no validation or Keychain logic (empty input is refused in Rust), so no unit-testable Swift logic is added.

Accessibility (ASK-10.8): the field has `accessibilityLabel("Provider API key")`, the buttons are plain `Button`s in the tab order, and the status line is a `Text` that VoiceOver reads. This is verified by a manual keyboard and VoiceOver pass (no Swift test target exists for the `Navori` executable).

### Workspace, dependencies and docs

Satisfies: ASK-9.6, ASK-9.7, ASK-10.1
Reuse: rung 2 — the `[workspace.dependencies]` pattern and the crate template of crates/redact/Cargo.toml; cargo metadata discovery means `check-deps` needs no list edit
Respects: ARCH-15
Surface:
- `Cargo.toml` members list: add `"ai/llm"`, `"ai/context"` — **replace**.
- `[workspace.dependencies]`: add `navori-llm`, `navori-context` path deps and `reqwest` — **replace** (additive).
- `Cargo.lock`: gains reqwest's tree; versions other specs guard (SECRETS-10.2) must not move — checked by `cargo update -p` not being run and by the lockfile diff at close.
- Docs (tasks, not behavior): docs/codebase/{dependencies,map,modules}.md, docs/architecture/data.md, docs/standards/errors-logging.md:12.
Interface: new crates `navori-llm` (`ai/llm/`) and `navori-context` (`ai/context/`), each with `[lints] workspace = true`.
Depth: n/a — configuration.
Locality: root `Cargo.toml`, the two new crates' manifests, `crates/ffi/Cargo.toml` (adds `navori-llm`, `navori-context`, `navori-secrets`; dev: `navori-secrets` with `features = ["fake"]`, which `check-deps` allows as a dev-dependency).

Performance (ASK-10.1): the task runs `cargo xtask perf` on an idle machine before any code change to fix the baseline (U5) and again after, and records the release `.app` size and idle footprint added by `reqwest`. If the size or footprint breaks GOAL-11, the fallback is trimming reqwest's default features, then `native-tls`; neither is decided now.

### Tests and the gated live test

Satisfies: ASK-8.1, ASK-8.2, ASK-8.3
Reuse: rung 2 — ffi test helpers (`home()`, `config()`, `start()` in crates/ffi/tests/core.rs:27-61), the CLI fake-server helpers (apps/cli/tests/cli.rs:450-476), `navori_secrets::fake::{FakeKeychain, MemoryStore}`; rung 3 — a hand-rolled `tokio::net::TcpListener` HTTP responder for the adapter tests (no `wiremock`, which is not in the workspace)
Respects: ARCH-14
Interface: `ai/llm/tests/live_deepseek.rs` is `#[ignore]` and runs only when `NAVORI_LIVE_LLM=1`; it reads the key by running `pass show "$NAVORI_LIVE_PASS_ENTRY"` inside the test, holds it in a `Secret`, and never prints it. The entry name is the environment variable's value, not a repo value. The test passes the entry name as a `Command` argument, not through a shell, and no task step sets `NAVORI_LIVE_LLM` or runs `pass`; only the operator does. `cargo xtask ci` runs `cargo test --workspace`, which skips ignored tests (ASK-8.3).
Depth: n/a — tests.
Locality: `ai/llm/tests/`, `ai/context/tests/`, `crates/ffi/tests/ask.rs` (new), `apps/cli/tests/cli.rs` (extended).

## Seams for testing

| Seam | Kind | Covers |
|---|---|---|
| `Gateway::ask` / `Gateway::with_shield` with fake `SecretStore` (`MemoryStore`), recording `Sender`, in-memory `AuditSink`, a shield stub that panics or returns a malformed `Redacted`, an audit sink that errors or panics (ai/context/tests) | unit | ASK-5.1, 5.2, 5.3, 6.1, 6.2, 6.3, 6.4, 7.1, 7.2, 10.6 |
| `OpenAiCompat::complete` against a local TCP responder (ai/llm/tests) | integration | ASK-4.1, 4.6, 7.3 |
| `DEEPSEEK_BASE_URL` constant (ai/llm unit test) | unit | ASK-10.7 |
| `navori_protocol` constants and structs (round-trip in protocol tests) | unit | ASK-4.2, 7.5, 9.2 (ipc tests unchanged and still green) |
| `NavoriCore::start_with` + `core.ask` over the real socket with fake `AiDeps` (crates/ffi/tests/ask.rs); `context_audit` absent after start and present after the first ask | integration | ASK-4.7, 5.5, 5.4, 9.1, 9.3, 10.2, 10.3, 10.4, 10.5 |
| `NavoriCore::save_provider_key` / `remove_provider_key` with fake `AiDeps`, called from a non-tokio executor (`futures::executor::block_on`) and concurrently for the first-use race | integration | ASK-1.1, 1.2, 1.5, 1.6, 2.1, 2.2, 3.1, 3.2, 8.2 |
| `navori ask` process against a real `NavoriCore` with fake `AiDeps`, including a prompt of exactly `MAX_LINE_BYTES` minus the envelope and one byte over (apps/cli/tests/cli.rs) | e2e | ASK-4.3, 4.4, 4.5, 7.4, 8.1, 9.4 |
| Settings → AI tab, manual run of the dev app (docs/agents/verify.md) | e2e (manual) | ASK-1.3, 1.4, 9.5, 10.8 |
| Gated live test `live_deepseek.rs` | integration (opt-in) | ASK-8.2 live half; supports ASK-4.1 |
| `cargo xtask perf` before/after; `cargo xtask ci` | e2e | ASK-10.1, 9.6, 9.7, 8.3 |

New seams: `Sender` and `AuditSink` (both substitutable system boundaries: network and database-backed log) and `AiDeps` (the start-time factories). `LlmProvider` is an adapter-side seam that already exists in the architecture (docs/ARCHITECTURE.md:196-210).

ASK-8.1 and ASK-8.2 are listed under the CLI seam and the live test; ASK-8.1 is the default-test claim "ASK-4 to ASK-7 and ASK-10.2 to 10.4 are covered by fakes", and is satisfied by the table as a whole.

## Coverage check

Every ID of requirements.md appears in exactly one `Satisfies:` line (checked by script against requirements.md at the time of writing). Where an ID has two halves, the Satisfies line names the half that makes it true and the other section only renders or calls it:
- ASK-2.1, 2.2 — ffi (`remove_provider_key` outcome and error); the Settings tab shows the text.
- ASK-4.5 — CLI (the 150 s wait); the 120 s HTTP bound in `ai/llm` realises ASK-7.3's "no connection" case.
- ASK-5.4 — ffi (`StoreAudit` is where strings are written).
- ASK-5.6 — Gateway (returns the reply unchanged; Decision 6).
- ASK-8.1-8.3 — Tests and the gated live test.

Deliberately unmapped: none.
