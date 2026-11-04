# Requirements: Ask walking skeleton

Feature code: ASK
Status: Draft
Date: 2026-11-04

<!--
Roadmap item: ROAD-36 ask-walking-skeleton (MILE-3). Claim: GitHub issue #18 (open, unassigned, label status:shaping). This unattended run did not assign it: the charter allows no GitHub writes beyond using that ticket.
Written in an unattended run under .skills/_pending-ask-skeleton/auto/charter.md. A value marked `agent-decided` was picked by an agent session, not by the operator; the operator confirms or reverses it on review. A value marked "Weigh" in the ledger is a session's derivation. Unmarked lines restate a Lock or a repo default (cited in auto/close-package.md and auto/ledger.md).
Persisted formats defined here, both `agent-decided` in their columns or names:
  1. Table `context_audit` under Owner `context` (ASK-5.2, 5.5). Named by the road (docs/roadmap/INDEX.md:82); columns and the `authorized` / `blocked` values are ours. Nothing is migrated in this run.
  2. The Keychain item with account `llm-provider/openai-compat` under the Profile's service (ASK-1.1). First item that exists in a user's login Keychain; SECRETS wrote the account grammar, this road picks the kind and name.
"THE SYSTEM" is the running Navori app (Core + Shell) plus the `navori` CLI, unless a criterion names a part.
"Fake" means an in-memory Keychain or a local fake provider server; no default test touches the real Keychain or the network.
-->

## 1. Save a provider key from Settings

**Story:** As the user, I want to paste a provider key into Settings → AI and save it, so that `navori ask` can use it while the key never sits in a file.

- **ASK-1.1** WHEN the user saves a pasted key in Settings → AI THE SYSTEM SHALL store it through a `navori-ffi` call that opens the `KeychainStore` for the running Profile and sets the secret under kind `llm-provider` and name `openai-compat`, replacing any earlier key (`agent-decided`: kind and name).
- **ASK-1.2** WHEN the Settings → AI tab opens THE SYSTEM SHALL NOT read the Keychain: no `get`, no `exists`.
- **ASK-1.3** WHEN a save succeeds THE SYSTEM SHALL show "Saved" and clear the pasted text from the field, and SHALL NOT show the key again (`agent-decided`: wording and clearing the field).
- **ASK-1.4** IF the Keychain cannot complete the save (`SecretError`) THEN THE SYSTEM SHALL show the error's own message and SHALL NOT show "no key set".
- **ASK-1.5** IF the pasted text is empty after trimming whitespace THEN THE SYSTEM SHALL make no Keychain call and say that nothing was entered (`agent-decided`).
- **ASK-1.6** THE SYSTEM SHALL carry the pasted key only through the Settings field and the `navori-ffi` save call: not through IPC, the CLI, an Action, config, the database, an Event or a log line.

## 2. Remove the key

**Story:** As the user, I want to remove the saved key from Settings → AI, so that I can take it out of the Keychain without opening Keychain Access.

- **ASK-2.1** WHEN the user presses Remove THE SYSTEM SHALL delete the item for the running Profile and show "Removed", or "No key was stored" when there was none, as success in both cases (`agent-decided`: wording).
- **ASK-2.2** IF the Keychain cannot complete the delete THEN THE SYSTEM SHALL show the error's own message.

## 3. Keep the release and dev keys apart

**Story:** As the author, I want to save a key in `Navori Dev.app`, run `navori ask` against `Navori.app`, and get the no-key message, so that the two Profiles never share a key.

- **ASK-3.1** THE SYSTEM SHALL open the store with service `dev.navori` for Profile `release` and `dev.navori.dev` for Profile `dev`, in `navori-ffi` — verified by a test that checks both mappings.
- **ASK-3.2** WHEN a key was saved under the `dev` Profile THE SYSTEM SHALL find no key under the `release` Profile, and the reverse — verified with the fake store keyed by service.

## 4. Ask from the terminal

**Story:** As the user, I want to run `navori ask --model <id> "<question>"` in a terminal, so that the model's reply prints without opening any window.

- **ASK-4.1** WHEN `navori ask --model <id> "<text>"` runs against the running app with a key saved THE SYSTEM SHALL send the redacted text (ASK-5.1) as the only message of a Chat Completions request with `stream: false` and model `<id>` to the DeepSeek preset base URL, print the first choice's message content on stdout and exit 0 (`agent-decided`: the preset constant; the shape follows docs/ARCHITECTURE.md:206 and auto/research-deepseek.md).
- **ASK-4.2** THE SYSTEM SHALL send the request as one JSON-RPC method `core.ask` handled in `IpcBridge`, not as an Action and not through `core.dispatch` (`agent-decided`: method name; ARCH-4).
- **ASK-4.3** IF `--model` is missing THEN THE SYSTEM SHALL exit with the existing usage code 2.
- **ASK-4.4** IF the request line would exceed `MAX_LINE_BYTES` (the public request limit of `navori-ipc`, 1 MiB; the server today answers `INVALID_REQUEST` and closes the connection) THEN THE SYSTEM SHALL refuse in the CLI before sending, with a message that names the limit, so the person never sees an IPC protocol error. The CLI pre-check is new behavior.
- **ASK-4.5** WHILE the reply is pending THE SYSTEM SHALL let the CLI wait longer than the 5 s of the `awake run` Lease call, and IF the wait limit passes THEN exit 1 saying the app did not answer in time (`agent-decided`: the limit's value, set in design).
- **ASK-4.6** THE SYSTEM SHALL send no system prompt, history, tools or context fragments: one user message per ask.
- **ASK-4.7** WHILE an ask is pending THE SYSTEM SHALL keep answering `core.status` and the other IPC methods on other connections (`agent-decided`: follows from C1, which avoids the 30 s dispatch timeout).

## 5. Shield and audit every ask

**Story:** As the user, I want every ask to pass the Sensitive Data Shield and leave an audit row, so that no secret goes out in my question and I can later tell what kinds of data left.

- **ASK-5.1** WHEN an ask is handled THE SYSTEM SHALL pass `<text>` through `redact_text` and send only the redacted text.
- **ASK-5.2** WHEN an ask passes the Shield THE SYSTEM SHALL write one row to `context_audit` before the provider call, holding an id, the time in milliseconds, the provider id (the adapter id `openai_compat`, not a host name; `agent-decided`), the kinds of data sent (`text`), the number of redacted items and the outcome `authorized`, meaning the send was cleared (`agent-decided`: columns and outcome values).
- **ASK-5.3** THE SYSTEM SHALL keep the prompt and the reply text out of the audit row, the Journal and every log line.
- **ASK-5.4** THE SYSTEM SHALL pass every string written to `context_audit` through the Shield first.
- **ASK-5.5** THE SYSTEM SHALL create `context_audit` through `Store::migrate` in a transactional migration under Owner `context` (ARCH-12).
- **ASK-5.6** THE SYSTEM SHALL return the reply to the CLI only: not persisted and not logged, and not passed through the Shield (`agent-decided`).

## 6. Fail closed

**Story:** As the user, I want a failure in the Shield or the audit log to stop the send, so that a broken safeguard never turns into a leak.

- **ASK-6.1** IF `redact_text` panics or the Gateway cannot classify the input THEN THE SYSTEM SHALL make no provider call, answer the ask with an error, and record the outcome `blocked` when the audit write still works.
- **ASK-6.2** IF the audit row cannot be written THEN THE SYSTEM SHALL make no provider call, tell the person that the audit log could not be recorded, and write one content-free `tracing` line.
- **ASK-6.3** THE SYSTEM SHALL write the audit row before the provider call — verified by a fake provider that records the order of calls.
- **ASK-6.4** THE SYSTEM SHALL let only the Gateway hold the handle that calls the provider: `ai/context` defines the send trait and `navori-ffi` wires an `ai/llm` provider into it (`agent-decided`). No test or `check-deps` rule enforces that no other path calls the provider (see Out of Scope).

## 7. Say what went wrong

**Story:** As the user, I want each failure of an ask to say which thing failed, so that I know whether to fix the key, the Keychain, the network or the app.

- **ASK-7.1** IF no key is stored THEN THE SYSTEM SHALL exit 1 with "No key set. Add one in Settings → AI.", before the Shield and the audit row, because nothing was inspected or authorized (`agent-decided`).
- **ASK-7.2** IF reading the key fails with a `SecretError` (locked Keychain, denied prompt, any backend failure) THEN THE SYSTEM SHALL exit 1 with that error's message and SHALL NOT print the no-key message.
- **ASK-7.3** IF the provider call fails (no connection, a non-success HTTP status, a body without a first choice with text) THEN THE SYSTEM SHALL exit 1 with a message that names the kind of failure and the HTTP status when there is one, and SHALL NOT echo the request text or the key.
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
  - **ASK-10.1** WHEN `cargo xtask perf` runs after this change THE SYSTEM SHALL show no regression against the baseline measured at task start for idle CPU, wakeups, footprint, cold start and menu open (GOAL-11, `docs/standards/performance.md`), and the task SHALL report the release `.app` size and idle footprint added by `reqwest` — verified by `cargo xtask perf` on an idle machine. The baseline before this road is provisional (Open Questions).
  - **ASK-10.2** WHEN the Core starts THE SYSTEM SHALL build no HTTP client, read no Keychain item and open no provider connection (ARCH-17) — verified by a test that starts the Core with a counting factory and sees zero builds.
  - **ASK-10.3** WHEN the first ask arrives THE SYSTEM SHALL build the client then and reuse it for later asks.
  - **ASK-10.4** THE SYSTEM SHALL run the blocking Keychain and `Store` calls of an ask off the async worker threads (the Core runtime has two).
- **Security:**
  - **ASK-10.5** THE SYSTEM SHALL let no key text, prompt text or reply text reach a log line, the Journal, the database, the config file or the audit row (THR-7, ARCH-8, ARCH-6) — verified by a test that saves a fake key, asks with a fake secret in the prompt, and searches the log output, the database and the config file for both strings.
  - **ASK-10.6** THE SYSTEM SHALL send nothing to the provider that has not passed `redact_text` (THR-1, THR-10, ARCH-7) — verified by a fake provider server that asserts the received body holds the `[REDACTED:<rule>]` marker and not the planted secret.
  - **ASK-10.7** THE SYSTEM SHALL talk only to the provider's base URL over HTTPS and listen on no network socket (ARCH-11) — verified by a test that the preset base URL begins `https://` and by `core.ask` being reachable only on the Unix socket.
- **Reliability:** None — a single-user local tool with no standing target (docs/ops/reliability.md is absent); failures are reported by ASK-6 and ASK-7, and the audit-write failure by ASK-6.2.
- **Accessibility:**
  - **ASK-10.8** WHERE the Settings → AI tab is shown THE SYSTEM SHALL give the key field, Save and Remove accessibility labels and keep them reachable by Tab, without a mouse (`agent-decided`: no standing accessibility doc exists, `docs/standards/accessibility.md` is absent) — verified by a manual VoiceOver and keyboard check.

## Out of Scope

- Provider configuration beyond the one key, the model catalog, model roles and fallback — ROAD-11. This includes a default model id, a `[ai]` config key and a base-URL setting.
- The Shield's other layers (app denylist, path blocks, OCR, screenshots), the full Gateway and a way to read the audit log — ROAD-12.
- Streaming, `eventsource-stream` (its deferral is `agent-decided`), the agent loop, tool calls, history, session storage, usage and run origin — ROAD-14.
- Approval policy — ROAD-37; menubar chat, quick-ask and moving `navori ask` onto the agent loop — ROAD-15; memory — ROAD-16.
- Redacting the model's reply.
- Any `xtask` change. Two follow-ups are recorded: the `check-deps` module→AI rule matches package names starting `navori-ai` (`xtask/src/main.rs:719`), which `navori-llm` and `navori-context` do not; and the `ai/context → navori-redact` edge and "only the Gateway calls the provider" are not enforced.
- The documentation edits (`docs/codebase/dependencies.md`, `map.md`, `modules.md`, `docs/architecture/data.md`) and the catalog card status: they are tasks under the Definition of Done (docs/standards), not behavior.
- Reading or writing secrets from the CLI, over IPC, as an Action or by the AI.
- Provider adapters other than `openai_compat`.

## Open Questions

- Keychain prompt behaviour and the real error for a locked or denied Keychain on `get`/`set` — owner: operator (manual real-Keychain run) — date: before the first release build — forbid-guess: no criterion or test may assert a prompt or a specific error.
- DeepSeek auth header, error status and body shapes, and valid model ids — owner: author of the live test — date: when ASK-8.2 is first run — forbid-guess: fixtures and criteria may not assume them beyond auto/research-deepseek.md.
- The name of the `pass` entry holding the DeepSeek key — owner: operator — date: before ASK-8.2 is first run — forbid-guess: no agent runs `pass show` or invents a name.
- The Keychain item `llm-provider/openai-compat` is the first item any build writes to a user's login Keychain (SECRETS says none exists until ROAD-11 or ROAD-19 ships) — owner: ROAD-11 author — date: when ROAD-11 is specified — forbid-guess: ROAD-11 must keep this kind and name or migrate the item; a rename orphans the user's key.
- Perf baseline and `.app` size before this road are provisional (`menu_open` over 80 ms from machine load, unconfirmed) — owner: operator — date: task start — forbid-guess: ASK-10.1 compares to the baseline measured at task start, not to a recorded number.
- Subagent readings in auto/scan.md not marked verified — owner: design.md author — date: before design.md is approved — forbid-guess: cite the file, not the digest.
