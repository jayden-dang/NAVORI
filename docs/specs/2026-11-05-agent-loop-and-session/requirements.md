# Requirements: Agent loop and Conversation

Feature code: LOOP
Status: Approved
Date: 2026-11-05
Approved by the operator ("approve", 2026-11-05 21:02 +07, on the review sheet at 4c30a7d).

<!--
Roadmap item: ROAD-14 agent-loop-and-session (MILE-3). Claim: GitHub issue #28 (open, label status:shaping, unassigned: the assign in this run was refused by the harness; the operator can run `gh issue edit 28 --add-assignee jayden-dang`).
Written in an unattended run under /Users/jayden/Developer/experiments/NAVORI/.skills/_pending-agent-loop/auto/charter.md (`.skills/` is gitignored, so the run files are not in the repository). A value marked `agent-decided` was picked by an agent session, not by the operator; the operator confirms or reverses it on review. A value marked `Weigh W-n` is agent-loop-interpret's pick in the run ledger. Unmarked lines restate a Lock or a repo default (auto/close-package.md, auto/ledger.md).
Reference design: dsh (MIT, /Users/jayden/Developer/dsh), ported per docs/adr/0008-agent-loop-ported-from-dsh.md.
Persisted formats: this file names no new table, column, Owner or encoding (Weigh W5). The Conversation's stored shape is in design.md "Persisted formats", under charter Lock L22 (charter.md:42). The Journal Event kind of LOOP-3.11 is named here. The one existing format it touches, `context_audit`, keeps its columns and values (LOOP-5.2).
Words: Conversation, Turn and Step are defined in CONTEXT.md. "THE SYSTEM" is the Navori Core (its AI runtime in `crates/ffi`, the `ai/*` crates and the Kernel), unless a criterion names a part. "The starter" is the code inside the Core that starts a Turn. In this road that is a test or the live demo; ROAD-15 and ROAD-37 add the person-facing entries. "Fake provider" is the scripted provider used by default tests; no default test touches the network or the real Keychain.
-->

## 1. Answer from Navori's own Read Actions

**Story:** As the user, I want Navori AI to look things up with Navori's Read Actions during a Turn and answer from what they return, so that asking "which ports are open?" gets an answer from `ports.list` instead of a guess. The demo is the `crates/ffi` test that runs this Turn against a real Kernel and the fake provider.

- **LOOP-1.1** WHEN a Turn starts THE SYSTEM SHALL offer the model, as tools, exactly the Actions whose catalog Risk is `read` in `Kernel::actions()`, each with its name, its description and a parameter schema for that one Action (`agent-decided`: offering only Read Actions; tool naming and schema cutting are design, Open Questions S-tools).
- **LOOP-1.2** WHEN a model reply asks for one or more tool calls THE SYSTEM SHALL run each call that passes the dispatch point (LOOP-2) as a `Kernel::dispatch` of that Action, then send the next request of the Turn with one tool result per call, in the order the model gave the calls.
- **LOOP-1.3** WHEN a model reply asks for no tool call THE SYSTEM SHALL end the Turn as completed and give the starter that reply's text as the answer.
- **LOOP-1.4** THE SYSTEM SHALL take the model id for a Turn from the starter, with no default model and no config key (ROAD-11 owns both; ASK requirements.md:139).
- **LOOP-1.5** THE SYSTEM SHALL make every model call of a Turn a single non-streaming request through the `openai_compat` adapter and the provider key that ROAD-36 shipped (streaming is ROAD-15's, docs/roadmap/INDEX.md:90).
- **LOOP-1.6** THE SYSTEM SHALL accept a model reply that carries tool calls with empty or null text. Today the adapter rejects any reply whose `content` is not a string, as `Malformed` (ai/llm/src/openai_compat.rs:143-145).
- **LOOP-1.7** WHEN a request follows earlier assistant messages of the Conversation THE SYSTEM SHALL resend each of them with every part the Provider requires back, including its reasoning text. With tools in the request, DeepSeek answers 400 when an earlier `reasoning_content` is missing (Weigh W5; auto/research-deepseek-tools.md). The reasoning text resent is the recorded one, after the Shield (LOOP-3.6, LOOP-5.3) (`agent-decided`: thinking is left at the Provider's default, which is on, and its reasoning is replayed, rather than sending `thinking` disabled; research-deepseek-tools.md:7,53). Whether DeepSeek accepts reasoning the Shield changed is in Open Questions.
- **LOOP-1.8** IF a tool call's arguments are not valid JSON, or do not fit the Action's parameter schema, THEN THE SYSTEM SHALL dispatch nothing for that call, so no request reaches the Module, and return to the model an error result that names the problem.
- **LOOP-1.9** IF a dispatched Action returns an error, or does not answer within the Agent loop's time limit for one Action, THEN THE SYSTEM SHALL return that failure to the model as an error result, a timeout result when the limit passed, and continue the Turn (dsh `timeout-policy`; `agent-decided`: the limit, no longer than the Kernel's 30 s dispatch timeout, crates/kernel/src/runtime.rs:308).

## 2. Run only Read Actions until approvals exist

**Story:** As the user, I want Navori AI to be unable to run anything but Read Actions before approvals exist, so that no Turn can stop my processes or change my settings. The demo is the test whose model calls `ports.kill` and then an Action that does not exist, after which the Ports Module has received no request.

- **LOOP-2.1** THE SYSTEM SHALL send every tool call of a Turn through one dispatch point, and no other code of the Agent loop SHALL call `Kernel::dispatch`.
- **LOOP-2.2** THE SYSTEM SHALL decide at the dispatch point from the Action's catalog Risk, the `risk` of its `ActionSpec` in `Kernel::actions()`, which is the same set LOOP-1.1 offers.
- **LOOP-2.3** IF a tool call names an Action whose catalog Risk is `write` or `destructive` THEN THE SYSTEM SHALL dispatch nothing and return to the model an error result saying that Navori AI cannot run that Action yet (`agent-decided`: the wording; Weigh W2).
- **LOOP-2.4** IF a tool call names no Action in `Kernel::actions()` THEN THE SYSTEM SHALL dispatch nothing and return to the model an error result naming the unknown name (Weigh W2). The dispatch point checks the name itself: `Kernel::dispatch` checks only the Module (crates/kernel/src/runtime.rs:333-336).
- **LOOP-2.5** WHEN a tool call is refused under LOOP-2.3 or LOOP-2.4 THE SYSTEM SHALL continue the Turn.
- **LOOP-2.6** THE SYSTEM SHALL keep every Action's catalog Risk equal to the Risk its parsed values report, for every Module and the built-in `system` actions — verified by one test in `crates/ffi/tests/` that covers each of them. Today only Awake has such a test (modules/awake/tests/actions.rs:359); Ports pins `kill` and `list` (modules/ports/tests/kill.rs:169-171).

## 3. Record each Turn in the Conversation and continue from it

**Story:** As the user, I want each Turn kept in its Conversation with what started it, its messages, every tool call with its result and the tokens each model call used, and a follow-up Turn to carry that record to the model, so that what Navori AI saw and did can be read back and built on. This road gives no reader. The demo is the test that runs a Turn, reads the record back, then runs a follow-up Turn whose request holds the first Turn's recorded messages.

- **LOOP-3.1** WHEN a Turn starts THE SYSTEM SHALL record what started it, as given by the starter and never taken from any message text (THR-11; the set of origin values is `agent-decided` in design, Open Questions S-origin).
- **LOOP-3.2** WHEN a model reply arrives THE SYSTEM SHALL record it, with each tool call's arguments as the exact text the model wrote, before running any of its tool calls.
- **LOOP-3.3** WHEN a tool call gets its result, whether dispatched, refused or failed, THE SYSTEM SHALL record that result paired with its tool call.
- **LOOP-3.4** WHEN a model call returns token usage THE SYSTEM SHALL record that call's input and output token counts, and the cache-hit and cache-miss counts when the Provider gives them, starting from the first model call.
- **LOOP-3.5** IF a model call returns no token usage THEN THE SYSTEM SHALL record that the call reported none.
- **LOOP-3.6** THE SYSTEM SHALL pass every text it records through the Shield before recording it: the input, the assistant text, the reasoning text, each tool call's arguments and each tool result (ARCH-6).
- **LOOP-3.7** WHEN a Turn ends THE SYSTEM SHALL record why: completed, cancelled, a step or token limit, or an error (`agent-decided`: the set of reasons, after dsh's `TurnEndReason`).
- **LOOP-3.8** THE SYSTEM SHALL keep the Conversation in the app's SQLite store, created on the first Turn and never at startup (L18, ARCH-12, ARCH-17; its stored shape is design, Open Questions S-stop).
- **LOOP-3.9** IF a record write fails THEN THE SYSTEM SHALL end the Turn with an error and send no further model request in it (`agent-decided`, after dsh's "model-visible means logged"). An end that cannot itself be recorded is repaired by LOOP-4.1 on the next open.
- **LOOP-3.10** WHEN a Turn continues an existing Conversation THE SYSTEM SHALL send the model that Conversation's earlier messages, as recorded after the Shield (LOOP-3.6), before the new input (`agent-decided`, after dsh's session history and CONTEXT.md's "a sequence of Turns").
- **LOOP-3.11** WHEN a Turn ends THE SYSTEM SHALL write one Event to the Journal, from module `ai` with kind `ai.turn_ended`. Its summary SHALL carry no content, and its payload only the Conversation and Turn ids, the origin, the end reason and the number of Steps (Weigh W13, W14; `agent-decided`: the kind, the keys and the summary; L1 "on … Journal").

## 4. Repair a Turn cut short by a crash

**Story:** As the user, I want a Turn that a crash or a quit cut short to be closed and its open tool calls answered before the next Turn runs, so that a Conversation never resumes with half a Turn in it. The demo is the test that leaves a Turn without an end, reopens the Core and reads the repaired record.

- **LOOP-4.1** WHEN the Core next opens a Conversation whose last Turn has no recorded end THE SYSTEM SHALL record that Turn as interrupted, and give each of its unanswered tool calls an error result, before any new Turn of that Conversation starts, and write that Turn's `ai.turn_ended` Event with the end `interrupted` (`agent-decided`, after dsh's crash repair; Weigh W13).

## 5. Send every model call through the Gateway, audited

**Story:** As the user, I want every model call of a Turn to pass the Shield and leave its own Context audit row, so that the audit log shows each time Navori AI's context left my Mac. The demo is the test that counts one `authorized` row per model call after a three-Step Turn.

- **LOOP-5.1** THE SYSTEM SHALL send every model call of a Turn through the Context Gateway, and no other code of the Agent loop SHALL reach the provider (ARCH-7).
- **LOOP-5.2** WHEN a model call is about to be sent THE SYSTEM SHALL first write one `context_audit` row for that call, with the columns and values ASK writes (ASK-5.2): Provider `deepseek`, kinds `text`, outcome `authorized`, and as `redacted_count` the number of items the Shield redacted in that request. This row is never a total for the Turn (Weigh W7, W9).
- **LOOP-5.3** THE SYSTEM SHALL pass every message of a request through the Shield before sending it: the input, the earlier messages, each tool call's arguments and each tool result, and the tool definitions it offers (ARCH-6).
- **LOOP-5.4** IF the Shield fails on a request THEN THE SYSTEM SHALL write a `blocked` row for that call, send nothing for that call, and end the Turn with an error (Weigh W9; `agent-decided`: ending the Turn, design Decision 10).
- **LOOP-5.5** IF the audit row for a call cannot be written THEN THE SYSTEM SHALL send nothing for that call and end the Turn with an error.
- **LOOP-5.6** THE SYSTEM SHALL keep message text, tool arguments and tool results out of every `context_audit` row.

## 6. Stop a runaway model

**Story:** As the user, I want a Turn whose model keeps calling tools to be stopped by a step limit or a token limit, and warned when it repeats a call, so that a confused model cannot loop forever. The demo is the test whose scripted model never stops calling tools.

- **LOOP-6.1** WHEN a Turn has run its step limit of Steps without an answer THE SYSTEM SHALL end it with the limit reason and send no further model request (`agent-decided`: the value, Open Questions S-limits).
- **LOOP-6.2** WHEN the tokens used by a Turn's model calls reach its token limit THE SYSTEM SHALL end it with the limit reason and send no further model request (`agent-decided`: the value and which counts it sums, S-limits).
- **LOOP-6.3** WHEN the model calls the same Action with the same arguments a 3rd, 5th or 8th time in a row since the last input THE SYSTEM SHALL add a reminder to that call's result saying the call repeats, and SHALL still run the call (dsh `repeat-tool-reminder`).

## 7. Cancel a Turn

**Story:** As the starter, I want to cancel a Turn midway and have it end with its history whole, so that a Turn nobody needs any more stops calling the model and every tool call it made still has a result. The demo is the test that cancels a Turn while its fake provider is answering a Step.

- **LOOP-7.1** WHEN the starter cancels a Turn THE SYSTEM SHALL start no further model call or dispatch, stop waiting for the call in flight, and end the Turn as cancelled.
- **LOOP-7.2** WHEN a Turn ends by cancel, limit or error with recorded tool calls that have no result THE SYSTEM SHALL record an error result for each, so every recorded tool call has exactly one result. IF the store cannot be written at that moment THEN LOOP-4.1 completes this on the next open (design review item 9).

## 8. Test the loop without a key

**Story:** As the author, I want the whole loop tested against a scripted fake provider through a real Kernel, so that CI never needs a key. The demo is `cargo xtask ci` passing with no key present.

- **LOOP-8.1** THE SYSTEM SHALL provide a scripted fake provider that returns given replies, tool calls and usage in order, and every default test of this feature SHALL use it or a local fake server, never the network or the real Keychain (`FakeLlm`, docs/standards/testing.md:16).

## 9. Run one live Turn by hand

**Story:** As the operator, I want to run one gated Turn against DeepSeek by hand, through a real Kernel with its real Read Actions, so that I can see the loop answer from a real model before ROAD-15 gives it a screen. The demo is the operator running that test (Weigh W3); this run does not claim it.

- **LOOP-9.1** WHERE `NAVORI_LIVE_LLM=1`, `NAVORI_LIVE_PASS_ENTRY=<entry>` and `NAVORI_LIVE_MODEL=<model>` are set and the test is run with `--ignored` THE SYSTEM SHALL run one Turn against DeepSeek with that model (LOOP-1.4) through a real Kernel with its real Read Actions, using the key from that `pass` entry, and SHALL report the answer and the Actions the Turn called (gate pattern of ai/llm/tests/live_deepseek.rs:15-24). No agent runs `pass show`.

## 10. Existing behavior stays intact

**Story:** As the author, I want `navori ask`, every IPC and FFI method, every Action and startup to behave as before, so that `cargo xtask ci` and `cargo xtask perf` still pass.

Files this change touches, and what each guards:
- `ai/llm/src/lib.rs`, `ai/llm/src/openai_compat.rs` — the text path `navori ask` uses: guard LOOP-10.1.
- `ai/context/src/gateway.rs` — `Gateway::ask` and its audit row: guard LOOP-10.2.
- `crates/ffi/src/ai.rs` — the lazy `AiRuntime` and `StoreAudit`: guards LOOP-10.2, 10.3.
- `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs` — IPC arms, `NavoriCore` methods and startup: guards LOOP-10.3, 10.4.
- `crates/kernel/src/runtime.rs` — used through `actions()` and `dispatch()`: guard LOOP-10.5.
- `crates/store/` — a new Owner's migration (its shape is design): guard LOOP-10.6.
- `crates/ffi/tests/` — new tests, the Risk drift test among them (LOOP-2.6); each Module's code is unchanged: guard LOOP-10.5.
- `xtask/src/main.rs` — `check-deps` gets a rule for `ai/agent` if design needs one: guard LOOP-10.7.
- `Cargo.toml` (workspace) and `Cargo.lock` — a new member: guard LOOP-10.7.
- `ai/agent/` — new: no behavior to guard; carries the licence notice (LOOP-10.8).
- `docs/architecture/data.md:27`, `docs/codebase/*`, `docs/features/`, `CONTEXT.md` — documentation: no behavior to guard.

- **LOOP-10.1** (guard) WHEN `navori ask --model <id> "<text>"` runs THE SYSTEM SHALL CONTINUE TO send one user message with `stream: false`, no tools and no history, and print the reply (ASK-4.1, ASK-4.6).
- **LOOP-10.2** (guard) WHEN an ask is handled THE SYSTEM SHALL CONTINUE TO write exactly one `context_audit` row for it before the send, and to refuse the ask when the Shield or the audit write fails (ASK-5.2, ASK-6).
- **LOOP-10.3** (guard) WHEN the app starts THE SYSTEM SHALL CONTINUE TO open the Store, the Kernel and the IPC server without building any AI runtime, provider client, Keychain read or AI migration (ASK-9.3, ASK-10.2, ASK-5.5 with docs/architecture/data.md:27 "Created lazily on the first audit write", ARCH-17).
- **LOOP-10.4** (guard) WHEN a client calls any existing IPC method or `NavoriCore` method THE SYSTEM SHALL CONTINUE TO answer as before, and this road SHALL add no IPC method, protocol type or CLI command.
- **LOOP-10.5** (guard) WHEN the UI, the CLI or IPC dispatches an Envelope THE SYSTEM SHALL CONTINUE TO route it as before, with each Action's name, description, schema, Risk and reply unchanged and `core.dispatch` still timing out at 30 s.
- **LOOP-10.6** (guard) WHEN the store opens THE SYSTEM SHALL CONTINUE TO keep every existing Owner's tables, migrations, backups and Journal retention as they are.
- **LOOP-10.7** (guard) WHEN `cargo xtask ci` runs THE SYSTEM SHALL CONTINUE TO pass `check-deps`, with `ai/*` crates depending on no `modules/*` crate, and SHALL CONTINUE TO declare each third-party crate once in `[workspace.dependencies]` (a convention, docs/codebase/dependencies.md:59, that `check-deps` does not enforce).
- **LOOP-10.8** (constraint) THE SYSTEM SHALL keep dsh's MIT licence text, with its copyright line, beside the ported code (ADR 0008:5).

## 11. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:**
  - **LOOP-11.1** WHEN `cargo xtask perf` runs after this change THE SYSTEM SHALL keep every gated metric within its budget (GOAL-11, docs/standards/performance.md:35) and, measured at the base commit and at the head on the same idle machine, keep each metric's head median within the base median + max(10% of it, the metric's floor) (performance.md:35-36; `agent-decided`: applying that rule base-to-head, since no `perf-baseline.json` exists) — verified by `cargo xtask perf` run at both commits.
  - **LOOP-11.2** WHEN the Core starts THE SYSTEM SHALL build no part of the Agent loop and run no Conversation migration (ARCH-17) — verified by a test that starts the Core and finds neither built.
  - **LOOP-11.3** THE SYSTEM SHALL run a Turn's blocking store and Keychain calls off the async worker threads — verified by a test in which another connection's `core.status` answers while a Turn's Conversation record write or audit write is held blocked by a fake (for example, waiting on a barrier).
- **Security:** (consulted docs/security/threat-model.md, Approved)
  - **LOOP-11.4** THE SYSTEM SHALL send nothing to the provider that has not passed the Shield, including a secret that a Read Action's result carries (THR-1, THR-10) — verified by a fake provider that asserts the received body holds the `[REDACTED:<rule>]` marker and not the planted secret.
  - **LOOP-11.5** WHILE debug logging is off THE SYSTEM SHALL let no input text, reply text, reasoning text, tool argument or tool result reach a log line (THR-3; docs/architecture/data.md "no prompt content or secrets unless debug is on") — verified by a test, run with debug logging off, that searches the log output for planted strings after a Turn. What a Turn writes to the Journal is Open Questions S-journal.
  - **LOOP-11.6** THE SYSTEM SHALL run no `write` or `destructive` Action from a Turn, whatever the model asks and whatever started the Turn (THR-2, THR-11, ARCH-5) — verified by LOOP-2's tests.
- **Reliability:** None — no standing reliability target (docs/ops/reliability.md is absent). Failures are covered by LOOP-1.8, LOOP-1.9, LOOP-3.9, LOOP-5.4 and LOOP-5.5, and interrupted Turns by LOOP-4.1 and LOOP-7.2.
- **Accessibility:** None — this road ships no UI (docs/standards/accessibility.md is absent).

## Out of Scope

- A CLI command, IPC method, protocol type, FFI method or any UI that starts or shows a Turn: chat, quick ask and moving `navori ask` onto the loop are ROAD-15; the first agent IPC entry comes with ROAD-37 (docs/roadmap/INDEX.md:88, 90; Weigh W1).
- Streaming replies: ROAD-15, their first consumer (moved from ROAD-14, docs/roadmap/INDEX.md:90).
- Estimated cost per Turn: ROAD-11, from its catalog prices (docs/roadmap/INDEX.md:84; Locks L19/L20 partly deferred, approved by the operator 2026-11-05). Token counts are recorded now (LOOP-3.4).
- Running `write` or `destructive` Actions, approvals, permission presets and Decisions in Needs you: ROAD-37.
- Providers other than DeepSeek through `openai_compat`, the model catalog, Model roles and fallback: ROAD-11.
- Memory (ROAD-16), the eval harness (ROAD-38), computer use (ROAD-40), other Shield layers (ROAD-39), and a reader or retention for `context_audit` (ROAD-12). Reading past Conversations and retention of the `ai_session_*` tables: ROAD-15, which ships the first person-facing writer (docs/roadmap/INDEX.md:90; Weigh W17, approved by the operator 2026-11-05).
- A new `context_audit` column or `kinds` value (Weigh W6).
- dsh's inbox and steering, fork, compaction, presentation, subagents, coding tools and Cordis (ADR 0008; auto/approaches.md).
- Writing a migration in this run.

## Open Questions

- Answered by the operator (2026-11-05), no longer open: S-cost (estimated cost goes to ROAD-11; Locks L19/L20 partly deferred).
- Answered in design.md (2026-11-05), no longer open: S-stop (charter Lock L22 and design "Persisted formats"), S-journal (Decision 13, LOOP-3.11), S-shield-refusal (Decision 10, LOOP-5.4), S-limits (Decision 8: 8 Steps, 200 000 tokens summed over input and output), S-origin (Decision 9: `user`), S-parallel (Decision 4: serial), S-tools (Decisions 5–6: `{module}_{action}`, schema cut from the enum's `oneOf`).
- Whether DeepSeek sends `content` as null or as an empty string with tool calls (research: unconfirmed) — owner: author of the live test — date: when LOOP-9.1 first runs — forbid-guess: fixtures cover both (LOOP-1.6).
- Does DeepSeek accept a replayed `reasoning_content` that the Shield changed (research-deepseek-tools.md:38-40, unconfirmed)? — owner: author of the live test — date: when LOOP-9.1 first runs — forbid-guess: a fixture covers a redacted replay; no test asserts the Provider accepts it.
- The `pass` entry that holds the DeepSeek key — owner: operator — date: before LOOP-9.1 first runs — forbid-guess: no agent runs `pass show` or invents a name.
