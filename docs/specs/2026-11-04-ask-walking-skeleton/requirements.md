# Requirements: Ask walking skeleton

Feature code: ASK
Status: Approved
Date: 2026-11-04
Approved by the operator ("approve", 2026-11-04 14:14, on the review page at c59c1d5, no card selected).

<!--
Roadmap item: ROAD-36 ask-walking-skeleton (MILE-3). Claim: GitHub issue #18 (open, unassigned, label status:shaping). This unattended run did not assign it: the charter allows no GitHub writes beyond using that ticket.
Written in an unattended run under /Users/jayden/Developer/experiments/NAVORI/.skills/_pending-ask-skeleton/auto/charter.md (`.skills/` is gitignored: those run files are not in the repository). A value marked `agent-decided` was picked by an agent session, not by the operator; the operator confirms or reverses it on review. A value marked "Weigh" in the ledger is a session's derivation. Unmarked lines restate a Lock or a repo default (cited in auto/close-package.md and auto/ledger.md).
Persisted formats defined here, both `agent-decided` in their columns or names:
  1. Table `context_audit` under Owner `context` (ASK-5.2, 5.5). Named by the road (docs/roadmap/INDEX.md:82); columns and the `authorized` / `blocked` values are ours. Nothing is migrated in this run.
  2. The Keychain item with account `llm-provider/deepseek` under the Profile's service (ASK-1.1). First item that exists in a user's login Keychain; SECRETS wrote the account grammar, this road picks the kind and name.
"THE SYSTEM" is the running Navori app (Core + Shell) plus the `navori` CLI, unless a criterion names a part.
"Fake" means an in-memory Keychain or a local fake provider server; no default test touches the real Keychain or the network.
-->

## 1. Save a provider key from Settings

**Story:** As the user, I want to paste a provider key into Settings → AI and save it, so that `navori ask` can use it while the key never sits in a file.

- **ASK-1.1** WHEN the user saves a pasted key in Settings → AI THE SYSTEM SHALL store it through a `navori-ffi` call that opens the `KeychainStore` for the running Profile and sets the secret under kind `llm-provider` and name `deepseek`, replacing any earlier key (`agent-decided`: kind `llm-provider` and name `deepseek`, the Provider the key belongs to, so a second `openai_compat` Provider later gets its own slot and a rename never orphans this key; and that the key is saved from a Settings tab named "AI").
- **ASK-1.2** WHEN the Settings → AI tab opens THE SYSTEM SHALL NOT read the Keychain: no `get`, no `exists` (the no-read rule is a Lock; the tab itself is `agent-decided`).
- **ASK-1.3** WHEN a save succeeds THE SYSTEM SHALL show "Saved" and clear the pasted text from the field, and SHALL NOT show the key again (`agent-decided`: wording and clearing the field).
- **ASK-1.4** IF the Keychain cannot complete the save (`SecretError`) THEN THE SYSTEM SHALL show the error's own message and SHALL NOT show "no key set".
- **ASK-1.5** IF the pasted text is empty after trimming whitespace THEN THE SYSTEM SHALL make no Keychain call and say that nothing was entered (`agent-decided`).
- **ASK-1.6** THE SYSTEM SHALL carry the pasted key only through the Settings field and the `navori-ffi` save call: not through IPC, the CLI, an Action, config, the database, an Event or a log line.
- **ASK-1.7** WHILE a save or remove call is pending (a first Keychain prompt may be open) THE SYSTEM SHALL disable Save and Remove and show "Waiting for the Keychain…" in the status line, without asserting that a prompt appears (`agent-decided`: wording).

## 2. Remove the key

**Story:** As the user, I want to remove the saved key from Settings → AI, so that I can take it out of the Keychain without opening Keychain Access.

- **ASK-2.1** WHEN the user presses Remove THE SYSTEM SHALL delete the item under the running Profile's service only (a key saved by the other Profile's app stays) and show "Removed", or "No key was stored" when there was none, as success in both cases (`agent-decided`: wording).
- **ASK-2.2** IF the Keychain cannot complete the delete THEN THE SYSTEM SHALL show the error's own message.

## 3. Keep the release and dev keys apart

**Story:** As the author, I want each Profile to open its own Keychain service, so that a key saved by `Navori Dev.app` is never the key `Navori.app` reads. The demo is the test that starts a dev core and a release core and shows the service each one opens.

- **ASK-3.1** THE SYSTEM SHALL open the store with service `dev.navori` for Profile `release` and `dev.navori.dev` for Profile `dev`, in `navori-ffi` — verified by a test of the mapping function and by a test that starts one core per Profile and records the service each receives when it opens its store.
- ~~**ASK-3.2**~~ folded into ASK-3.1: a separate cross-Profile read test only proved the fake store keeps services apart, and the handoff asks for the mapping test (keychain-secrets requirements.md:123).

## 4. Ask from the terminal

**Story:** As the user, I want to run `navori ask --model <id> "<question>"` in a terminal, so that the model's reply prints without opening any window.

- **ASK-4.1** WHEN `navori ask --model <id> "<text>"` runs against the running app with a key saved THE SYSTEM SHALL send the redacted text (ASK-5.1) as the only message of a Chat Completions request with `stream: false` and model `<id>` to the DeepSeek preset base URL, print the first choice's message content on stdout and exit 0 (`agent-decided`: the preset constant `https://api.deepseek.com`, the required `--model` flag and `stream: false`; the request and response shape follow https://api-docs.deepseek.com/api/create-chat-completion and docs/ARCHITECTURE.md:206).
- **ASK-4.2** THE SYSTEM SHALL send the request as one JSON-RPC method `core.ask` handled in `IpcBridge`, not as an Action and not through `core.dispatch` (`agent-decided`: the method name and the choice not to make ask an Action; ARCH-4 keeps module capabilities as Actions).
- **ASK-4.3** IF `--model` is missing THEN THE SYSTEM SHALL exit with the existing usage code 2, and the `--help` line for `--model` SHALL say the id is the provider's own model name and give `deepseek-flash` as an example (`agent-decided`: a required flag instead of a default, and the example, which comes from https://api-docs.deepseek.com and may go stale).
- **ASK-4.4** IF the request line would exceed `MAX_LINE_BYTES` (the public request limit of `navori-ipc`, 1 MiB; the server today answers `INVALID_REQUEST` and closes the connection) THEN THE SYSTEM SHALL refuse in the CLI before sending, with a message that names the limit, so the person never sees an IPC protocol error. The CLI pre-check is new behavior; without it the client reads the server's error answer, whose id is null, and reports "response id null does not match request id 1" (crates/ipc/src/lib.rs:249-254).
- **ASK-4.5** WHILE the reply is pending THE SYSTEM SHALL let the CLI wait longer than the 5 s of the `awake run` Lease call, and IF the wait limit passes THEN exit 1 saying the app did not answer within the wait used and that the request may still finish in the app, because the CLI stops waiting but nothing cancels the provider call (`agent-decided`: the limit's value, set in design).
- **ASK-4.6** THE SYSTEM SHALL send no system prompt, history, tools or context fragments: one user message per ask.
- ~~**ASK-4.7**~~ folded into ASK-10.4: keeping other IPC methods answering while an ask waits is what running the blocking calls off the worker threads means, and the test moved there.
- **ASK-4.8** WHEN an ask has been pending for 3 seconds THE SYSTEM SHALL print `waiting for the answer…` once on stderr, never on stdout, so a first Keychain prompt or a slow provider does not look like a hang (`agent-decided`: the 3 s and the wording).

## 5. Shield and audit every ask

**Story:** As the user, I want every ask to pass the Sensitive Data Shield and leave an audit row, so that no secret goes out in my question and each ask leaves a record of which kinds of data were cleared to leave. This road gives no way to read that record beyond opening the database itself.

- **ASK-5.1** WHEN an ask is handled THE SYSTEM SHALL pass `<text>` through `redact_text` and send only the redacted text.
- **ASK-5.2** WHEN an ask passes the Shield THE SYSTEM SHALL write one row to `context_audit` before the provider call, holding an id, the time in milliseconds, the Provider id `deepseek` (the Provider the user keys, as CONTEXT.md names it, not the `openai_compat` adapter family and not a host name; `agent-decided` value, the recorded kind follows docs/architecture/data.md:37), the kinds of data sent (`text`), the number of redacted items and the outcome `authorized`, meaning the send was cleared (`agent-decided`: columns and outcome values).
- **ASK-5.3** THE SYSTEM SHALL keep the prompt and the reply text out of the audit row, the Journal and every log line.
- **ASK-5.4** THE SYSTEM SHALL pass every string written to `context_audit` through the Shield first.
- **ASK-5.5** THE SYSTEM SHALL create `context_audit` through `Store::migrate` in a transactional migration under Owner `context` (ARCH-12; `agent-decided`: the owner name `context`).
- **ASK-5.6** THE SYSTEM SHALL return the reply to the CLI only: not persisted and not logged, and not passed through the Shield (`agent-decided`).

## 6. Fail closed

**Story:** As the user, I want a failure in the Shield or the audit log to stop the send, so that a broken safeguard never turns into a leak.

- **ASK-6.1** IF `redact_text` panics or the Gateway cannot classify the input THEN THE SYSTEM SHALL make no provider call, answer the ask with an error, and record the outcome `blocked` when the audit write still works (the rule that a detector failure sends nothing is a Lock; refusing the whole ask rather than part of it is `agent-decided`, because a text-only ask has one portion).
- **ASK-6.2** IF the audit row cannot be written THEN THE SYSTEM SHALL make no provider call, tell the person that the audit log could not be recorded, and write one content-free `tracing` line (`agent-decided`: an audit failure blocks the send; the Lock says every request records an audit row).
- **ASK-6.3** THE SYSTEM SHALL write the audit row before the provider call (`agent-decided`: the order) — verified by a fake provider that records the order of calls.
- **ASK-6.4** THE SYSTEM SHALL let only the Gateway hold the handle that calls the provider: `ai/context` defines the send trait and `navori-ffi` wires an `ai/llm` provider into it (`agent-decided`). No test or `check-deps` rule enforces that no other path calls the provider (see Out of Scope).

## 7. Say what went wrong

**Story:** As the user, I want each failure of an ask to say which thing failed, so that I know whether to fix the key, the Keychain, the network or the app.

- **ASK-7.1** IF no key is stored THEN THE SYSTEM SHALL exit 1 with "No key set. Add one in Settings → AI.", before the Shield and the audit row, because nothing was inspected or authorized (`agent-decided`).
- **ASK-7.2** IF reading the key fails with a `SecretError` (locked Keychain, denied prompt, any backend failure) THEN THE SYSTEM SHALL exit 1 with that error's message and SHALL NOT print the no-key message.
- **ASK-7.3** IF the provider call fails (no connection, a non-success HTTP status, a body without a first choice with text) THEN THE SYSTEM SHALL exit 1 with a message that names the kind of failure and the HTTP status when there is one, and SHALL NOT echo the request text or the key.
- **ASK-7.6** IF the provider answers with status 401, 402, 404, 422 or 429 THEN THE SYSTEM SHALL add what the person can check: 401 the key saved in Settings → AI, 402 the provider account balance, 404 or 422 the model id given to `--model`, 429 try again shortly (`agent-decided`: the hints; the meanings of 401, 402, 422, 429 and 503 are listed at https://api-docs.deepseek.com/quick_start/error_codes, and 404 for a wrong model id is reported only by the integration guide at https://api-docs.deepseek.com/quick_start/agent_integrations/workbuddy, so the 404 and 422 hints are best-effort).
- **ASK-7.4** IF the app is not running THEN THE SYSTEM SHALL exit 3 as the other commands do.
- **ASK-7.5** THE SYSTEM SHALL map every ask failure to exit 1 and keep the codes 0, 1, 2 and 3 as they are (`agent-decided`).

## 8. Test it without the network or the real Keychain

**Story:** As the author, I want the whole path tested against a fake provider and a fake Keychain, plus one gated live test against DeepSeek, so that CI never needs a key.

- **ASK-8.1** THE SYSTEM SHALL cover ASK-4 to ASK-7 and ASK-10.2 to ASK-10.4 in default tests that use the fake store and a local fake provider server, with no real network call and no real Keychain access (docs/standards/testing.md, system boundaries only).
- **ASK-8.2** WHERE the author sets an opt-in variable THE SYSTEM SHALL run one live test that sends a short prompt to DeepSeek with the key read from the author's `pass` store; the pass entry name is a test parameter, not a value in the repo (Open Questions). Otherwise that test is ignored.
- **ASK-8.3** THE SYSTEM SHALL NOT write the live key to the repo, a fixture, a snapshot, a log or a test output, and `cargo xtask ci` SHALL NOT run the live test.

## 9. Existing behavior stays intact

**Story:** As the author, I want the existing commands, IPC methods and Settings tabs to keep working, so that `cargo xtask ci` still passes.

Files this change touches, and what each guards:
- `crates/protocol/src/lib.rs` — existing methods and error codes: guard ASK-9.1.
- `crates/ipc/src/lib.rs` — framing and limits: guard ASK-9.2.
- `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs`, `crates/ffi/Cargo.toml` — existing IPC arms, `NavoriCore` methods and startup: guards ASK-9.1, 9.3.
- `apps/cli/src/main.rs` — existing subcommands and exit codes: guard ASK-9.4.
- `apps/macos/Navori/Features/Settings/SettingsView.swift` and `CoreStore.swift` — existing tabs: guard ASK-9.5.
- `Cargo.toml` (workspace) and `Cargo.lock` — dependency declarations: guard ASK-9.6.
- `crates/store`, `crates/redact`, `crates/secrets`, `xtask/` — read or used, not changed: no behavior to guard here beyond ASK-9.7.
- `ai/llm/`, `ai/context/` — new: no behavior to guard.

- **ASK-9.1** (guard) WHEN a client calls `core.status`, `core.actions`, `core.dispatch`, `core.events` or `core.doctor` THE SYSTEM SHALL CONTINUE TO answer as before, and `core.dispatch` SHALL CONTINUE TO time out at 30 s.
- **ASK-9.2** (guard) WHEN a request line exceeds `MAX_LINE_BYTES` (server side) or a response exceeds `MAX_RESPONSE_BYTES` (the client's private 16 MiB cap) THE SYSTEM SHALL CONTINUE TO refuse it as the IPC crate does today.
- **ASK-9.3** (guard) WHEN the app starts THE SYSTEM SHALL CONTINUE TO open the Store, the Kernel and the IPC server without waiting for any AI code, and the GOAL-11 budget SHALL CONTINUE TO hold.
- **ASK-9.4** (guard) WHEN the user runs `status`, `doctor`, `events`, `actions`, `dispatch`, `hook` or `awake` THE SYSTEM SHALL CONTINUE TO behave and exit as before.
- **ASK-9.5** (guard) WHEN the user opens General, Diagnostics, Integrations or About in Settings THE SYSTEM SHALL CONTINUE TO show them unchanged.
- **ASK-9.6** (guard) THE SYSTEM SHALL CONTINUE TO declare each third-party crate once in `[workspace.dependencies]` and keep `Cargo.lock` versions that other specs guard.
- **ASK-9.7** (guard) WHEN `cargo xtask ci` runs THE SYSTEM SHALL CONTINUE TO pass `check-deps`: the CLI depends on `config`, `ipc` and `protocol` only, `ai/*` crates on no `modules/*` (the `is_ai && dep_is_module` rule, xtask/src/main.rs:721; it has no `ai/` crate to act on until this change), and the `fake` feature of the guarded crates stays a dev-dependency.

## 10. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:**
  - **ASK-10.1** WHEN `cargo xtask perf` runs after this change THE SYSTEM SHALL show no regression against the base commit measured in the same session as the branch for idle CPU, wakeups, footprint, cold start and menu open (GOAL-11, `docs/standards/performance.md`), and the task SHALL report the release `.app` size and idle footprint added by `reqwest` — verified by `cargo xtask perf` on an idle machine. The baseline before this road is provisional (Open Questions).
  - **ASK-10.2** WHEN the Core starts THE SYSTEM SHALL build no HTTP client, read no Keychain item and open no provider connection (ARCH-17) — verified by a test that starts the Core with a counting factory and sees zero builds.
  - **ASK-10.3** WHEN the first ask arrives THE SYSTEM SHALL build the client then and reuse it for later asks.
  - **ASK-10.4** THE SYSTEM SHALL run the blocking Keychain and `Store` calls of an ask off the async worker threads (the Core runtime has two) — verified by a test in which a second connection's `core.status` answers while an ask waits.
- **Security:**
  - **ASK-10.5** THE SYSTEM SHALL let no key text, prompt text or reply text reach a log line, the Journal, the database, the config file or the audit row (THR-7, ARCH-8, ARCH-6) — verified by a test that saves a fake key, asks with a fake secret in the prompt, and searches the log output, the database and the config file for both strings.
  - **ASK-10.6** THE SYSTEM SHALL send nothing to the provider that has not passed `redact_text` (THR-1, THR-10, ARCH-7) — verified by a fake provider server that asserts the received body holds the `[REDACTED:<rule>]` marker and not the planted secret.
  - **ASK-10.7** THE SYSTEM SHALL talk only to the provider's base URL over HTTPS and listen on no network socket (ARCH-11) — verified by a test that the preset base URL begins `https://` and by `core.ask` being reachable only on the Unix socket.
- **Reliability:** None — a single-user local tool with no standing target (docs/ops/reliability.md is absent); failures are reported by ASK-6 and ASK-7, and the audit-write failure by ASK-6.2.
- **Accessibility:** None — a single-user tool with no standing accessibility target (`docs/standards/accessibility.md` is absent and neither `docs/product/vision.md` nor `docs/standards/` names one); the tab uses standard SwiftUI controls with their default labels. ~~**ASK-10.8**~~ (labels and a VoiceOver check) cut: nothing requires it.

## Out of Scope

- Provider configuration beyond the one key, the model catalog, model roles and fallback — ROAD-11. This includes a default model id, a `[ai]` config key and a base-URL setting.
- The Shield's other layers (app denylist, path blocks, OCR, screenshots), the full Gateway — ROAD-12 (docs/roadmap/INDEX.md:84).
- A way for the person to read the audit log, and its retention or pruning: no roadmap item owns either yet (`agent-decided` that this road does not; docs/roadmap/INDEX.md:84 does not list them). GOAL-6 (docs/product/vision.md:23) says the user can see what the AI has seen; this road records the data kinds but gives no reader.
- Cancelling an ask. Known behavior, not a requirement: when the person interrupts `navori ask` or the CLI's wait ends, the app still finishes the provider call (the provider may bill it) and the audit row stays `authorized`, because the IPC server awaits the handler without watching the socket (crates/ipc/src/lib.rs `serve`).
- Streaming, `eventsource-stream` (its deferral is `agent-decided`), the agent loop, tool calls, history, session storage, usage and run origin — ROAD-14.
- Approval policy — ROAD-37; menubar chat, quick-ask and moving `navori ask` onto the agent loop — ROAD-15; memory — ROAD-16.
- Redacting the model's reply.
- Any `xtask` change. Two follow-ups are recorded: the `check-deps` module→AI rule matches package names starting `navori-ai` (`xtask/src/main.rs:719`), which `navori-llm` and `navori-context` do not; and the `ai/context → navori-redact` edge and "only the Gateway calls the provider" are not enforced.
- The documentation edits (`docs/codebase/dependencies.md`, `map.md`, `modules.md`, `docs/architecture/data.md`) and the catalog card status: they are tasks under the Definition of Done (docs/standards), not behavior.
- Reading or writing secrets from the CLI, over IPC, as an Action or by the AI.
- Provider adapters other than `openai_compat`.

## Open Questions

- Keychain prompt behaviour and the real error for a locked or denied Keychain on `get`/`set` — owner: operator (manual real-Keychain run) — date: before the first release build — forbid-guess: no criterion or test may assert a prompt or a specific error.
- DeepSeek's error body shape, whether the model ids `deepseek-flash` and `deepseek-v4-pro` are current when the live test first runs, and the 404 for an unknown model — owner: author of the live test — date: when ASK-8.2 is first run — forbid-guess: fixtures may use the documented shapes (https://api-docs.deepseek.com/api/create-chat-completion, https://api-docs.deepseek.com/quick_start/error_codes) but a criterion may not assert an error body or a model id.
- The name of the `pass` entry holding the DeepSeek key — owner: operator — date: before ASK-8.2 is first run — forbid-guess: no agent runs `pass show` or invents a name.
- The Keychain item `llm-provider/deepseek` is the first item any build writes to a user's login Keychain (SECRETS says none exists until ROAD-11 or ROAD-19 ships) — owner: ROAD-11 author — date: when ROAD-11 is specified — forbid-guess: ROAD-11 must keep this kind and name or migrate the item; a rename orphans the user's key.
- Perf baseline and `.app` size before this road are provisional (`menu_open` over 80 ms from machine load, unconfirmed) — owner: operator — date: Task 9 — forbid-guess: ASK-10.1 compares to the base commit and the branch measured in the same session, not to a recorded number.
