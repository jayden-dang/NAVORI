# Tasks: Needs you

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: NEEDS
Status: Approved
Delivered-by: PR #37
Approved by the operator: "approve", 2026-11-08 17:37 +07, after reading the spec sheet at 270a07d (relayed by the conductor navori-47 in .skills/_pending-needs-you/auto/operator-reply.md). The ship-day Decision source stays an Open Question.
Date: 2026-11-08
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

**Goal:** Ship Needs you — Decisions any Module raises and the user answers on level 0, To-dos with a status-colored due-date pill, Updates from expired Decisions and a 30-day History in the submenu — on a kernel dispatcher that lets a Module dispatch an Action.

**Architecture:** The kernel gives every Module a non-blocking `ModuleDispatcher` with a Destructive grant held by the composition root (ADR 0011). Two new Modules, `needs_you` (Decisions, Updates) and `todo` (To-dos), keep their own tables and History; `crates/ffi` joins their State into `NeedsYouView`. The Swift Shell renders brief D: Decide rows on level 0 and a Needs you submenu with a text field and a date calendar in the cascade.

**Tech Stack:** Rust 2024 workspace (tokio, rusqlite through `navori-store`, serde/schemars, `time` 0.3, UniFFI), Swift 6 SwiftUI + AppKit (`NSTextField`, `NSDatePicker`, `NSSegmentedControl`), tests with `cargo nextest` and `swift test`.

## Global Constraints

- Engineering standards SSOT: `docs/standards/INDEX.md` (sha1 `63fedd67da98…`), `docs/standards/testing.md` (`1a12ccbfe8bf…`), `docs/standards/errors-logging.md` (`4f95955d9a37…`), `docs/standards/performance.md` (`3ca5b2163942…`).
- Architecture invariants: `docs/architecture/INDEX.md` (`cc538290d87a…`) — every task respects ARCH-1, 2, 3, 4, 5, 6, 9, 10, 12, 16.
- Commands (`docs/agents/project.md` verify commands): one test file `cargo nextest run -p <crate> --test <file>`; lint `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings`; Swift unit tests `swift test --arch arm64 --package-path apps/macos --filter <TestClass>` after `cargo xtask ci` or `cargo xtask run` has generated `apps/macos/Generated/`; wave gate `cargo xtask ci`; dev app `cargo xtask run`, drive with `docs/agents/verify.md`.
- Compat External: table names, column names, state/kind strings, Action names and payload fields written here are permanent once shipped. No task writes or runs a migration outside a test's in-memory store before the operator approves the spec.
- Requirement IDs live in task footers and the spec only — never in test names or source.
- No per-feature perf run or gate (NEEDS-14.1/14.2 are measured once, before release).
- Team: Solo — no assignees.

## File Structure

| Path | Responsibility |
|---|---|
| `crates/kernel/src/dispatcher.rs` (new) | `ModuleDispatcher`, `Pending`, the router, action-name parsing, the Destructive grant check |
| `crates/kernel/src/runtime.rs` | router in `Plumbing` filled by `start()` before launchers; `grant_destructive`; `DispatchError::Refused`; `Kernel::dispatch`/`actions` read the router |
| `crates/kernel/src/module.rs` | `ModuleCtx::dispatcher()` |
| `crates/kernel/src/lib.rs` | re-export `ModuleDispatcher`, `Pending` |
| `crates/kernel/tests/dispatcher.rs` (new) | dispatcher behavior with test Modules |
| `Cargo.toml` | workspace members and deps for the two Modules; `time` `parsing` feature |
| `modules/needs_you/Cargo.toml` (new) | crate `navori-mod-needs-you` |
| `modules/needs_you/src/lib.rs` (new) | `NeedsYou` Module: ID, State, `tray()`, `run` |
| `modules/needs_you/src/action.rs` (new) | `NeedsYouAction` enum, `risk()`, `catalog()` |
| `modules/needs_you/src/state.rs` (new) | `NeedsYouState`, `OpenDecision`, `UnviewedUpdate` |
| `modules/needs_you/src/stored.rs` (new) | Owner `needs_you`, migrations, row reads/writes |
| `modules/needs_you/src/raise.rs` (new) | raise validation, Shield, dedupe |
| `modules/needs_you/src/answer.rs` (new) | answer, in-flight replies, error classes, restart rule |
| `modules/needs_you/src/expiry.rs` (new) | deadlines, expiry, Updates |
| `modules/needs_you/src/history.rs` (new) | `history`, prune |
| `modules/needs_you/tests/common/mod.rs` (new) | harness: kernel builder, `FakeClock`, `Target` and `Producer` test Modules |
| `modules/needs_you/tests/raise.rs`, `answer.rs`, `expiry.rs`, `updates.rs`, `history.rs`, `producer.rs` (new) | behavior tests |
| `crates/macos/src/lib.rs`, `crates/macos/src/clock.rs` (new) | `local_utc_offset(SystemTime) -> i32` via `localtime_r` |
| `crates/macos/tests/clock.rs` (new) | offset agrees with the system zone |
| `modules/todo/Cargo.toml` (new) | crate `navori-mod-todo` |
| `modules/todo/src/lib.rs`, `action.rs`, `state.rs`, `stored.rs`, `calendar.rs`, `actor.rs` (new) | `Todo` Module, `TodoAction`, `TodoState`, Owner `todo`, `LocalCalendar`, run loop |
| `modules/todo/tests/common/mod.rs`, `todo.rs`, `day.rs`, `history.rs` (new) | behavior tests with a fake calendar and clock |
| `crates/ffi/Cargo.toml` | deps on both Modules; `navori-macos` as a normal dependency |
| `crates/ffi/src/app.rs` | `build_kernel` registers and grants; `dispatch_to_rpc` arm; `needs_you_view` |
| `crates/ffi/src/lib.rs` | `CoreSnapshot.needs_you`; `snapshot()`; `NavoriCore::needs_you_history()` |
| `crates/ffi/src/needs_you.rs` (new) | pure `build(..) -> Option<NeedsYouView>` and its records |
| `crates/ffi/tests/needs_you.rs` (new) | grant set, Navori AI refusal, history merge through `NavoriCore` |
| `crates/ffi/tests/needs_you_view.rs` (new) | pure join |
| `crates/ffi/tests/actions_risk.rs` | samples and arms for new Actions; `SAMPLED` |
| `apps/macos/NavoriPanel/NeedsYouText.swift` (new) | age text, VoiceOver strings, pill text and words, Destructive hint |
| `apps/macos/NavoriPanel/NeedsYouList.swift` (new) | filter, drawn-row set, key map |
| `apps/macos/NavoriPanel/CoreRowState.swift` | `LevelZeroTree.rows` gains Decide rows and `needs` |
| `apps/macos/NavoriPanel/MenuMetrics.swift` | `MenuHitMap` covers rows above `running` and Decide buttons |
| `apps/macos/Tests/NavoriPanelTests/NeedsYouTextTests.swift`, `NeedsYouListTests.swift` (new); `CoreRowStateTests.swift`, `MenuHitMapTests.swift` | Swift unit tests |
| `apps/macos/Navori/Features/Panel/CapsuleButton.swift` (new, moved) | button with a real action |
| `apps/macos/Navori/Features/Preview/PreviewSampleRows.swift` | drops its `CapsuleButton` definition |
| `apps/macos/Navori/Features/Panel/DecideRowView.swift` (new) | Decide row |
| `apps/macos/Navori/Features/Panel/PanelView.swift` | Decide block, `Needs you` row, order |
| `apps/macos/Navori/Core/CoreStore.swift` | `needsYou`, `answer`, `addTodo`, `tick`, `undo`, `clearUpdates`, `markViewed`, `loadHistory`, draft text |
| `apps/macos/Navori/StatusBar/StatusBarController.swift` | tree, submenu and activate cases, snapshot observation |
| `apps/macos/Navori/StatusBar/StatusPanel.swift` | ⌘⌫ routing; deferred `resignKey` |
| `apps/macos/Navori/StatusBar/CascadePanel.swift` | `acceptsTyping`, `becomesKeyOnlyIfNeeded`, text pass-through |
| `apps/macos/Navori/StatusBar/CascadeController.swift` | `onSubmenuClosed`, level-0 key on hide |
| `apps/macos/Navori/Features/Panel/NeedsYouSubmenuView.swift`, `TodoRowView.swift`, `UpdateRowView.swift`, `AddTodoField.swift`, `DueCalendarView.swift`, `HistorySubmenuView.swift` (new) | submenu views |
| `docs/specs/2026-11-02-menubar-visual-foundation/requirements.md`, `docs/specs/2026-11-02-awake-leases/requirements.md` | amendment notes on MENU-1.2, MENU-1.4, AWAKE-11.1 |
| `docs/design/2026-11-01-menubar-ui-brief.md` | amendment notes on rules 6 and 12 |
| `docs/architecture/data.md`, `docs/codebase/modules.md`, `docs/codebase/map.md` | real table and crate names |
| `docs/architecture/system.md` | Needs you row names the built modules |
| `docs/agents/verify.md` | how to drive Needs you |
| `docs/features/needs-you.md` | operator note |
| `docs/adr/0011-modules-dispatch-through-a-kernel-given-dispatcher.md` | dispatcher decision |
| `docs/roadmap/INDEX.md` | roadmap row for this feature |
| `CONTEXT.md` | glossary |
| `Cargo.lock` | lockfile for the new crates |
| `crates/ffi/tests/agent.rs` | catalog lock for the new Actions |
| `crates/kernel/tests/kernel.rs` | kernel drop closes module inboxes |

Retrieval (load-subgraph, schema 1.1, fsubr-1.2): owns_coverage 15/20 (0.75); observations `[]`. blast_radius names ICON and KCONTRACT on the kernel files; ASK, AWAKE, HOOKS, ICON, KCONTRACT, LID, PORTS, PSTOP (and LOOP on `lib.rs`) on the ffi files; MENU, PSTOP, AWAKE, PERF, ICON, LID on the Shell files. `cluster(NEEDS)` holds NEEDS only — NEEDS has no OWNS until this file exists.

---

### Task 1: Spike — typing and a calendar in a cascade panel (throwaway)

**Files:**
- Create: a scratch SwiftPM package under `/tmp/navori-spike-cascade-key/` (outside the repo, deleted at the end)
- Test: the spike's own manual run; evidence to `.skills/_pending-needs-you/auto/spike-cascade-key.md` in the main checkout

**Reuse:** 4 — AppKit `NSPanel.becomesKeyOnlyIfNeeded` and `NSView.needsPanelToBecomeKey` (research note); `StatusPanel.routeKey`'s text-field pass-through (`StatusPanel.swift:290-293`)

**Interfaces:**
- Consumes: none
- Produces: a yes/no per fact for design decision 9

**Depends-on:** none

- [x] **Step 1:** Copy the shape of `StatusPanel` (`.nonactivatingPanel`, `canBecomeKey` true, `resignKey` closes) and of `CascadePanel` (`canBecomeKey` false) into the scratch package; add a child panel with `canBecomeKey { acceptsTyping }`, `becomesKeyOnlyIfNeeded = true`, an `NSTextField` hosted through `NSViewRepresentable`, and a depth-2 panel with a graphical `NSDatePicker`.
- [x] **Step 2:** Run it (`swift run`) and check, writing each result: (a) clicking the field makes the child key and typing lands in it; (b) with `resignKey` deferred and checking "key still inside my panels", the parent stays open; (c) the date picker takes a first click and arrow keys; (d) Return in the field reaches the field, Esc closes.
- [x] **Step 3:** Write the four results with screenshots to the evidence file, then `rm -rf /tmp/navori-spike-cascade-key`. Any "no" stops the plan and reopens design decision 9.

_Requirements: NEEDS-8.2, NEEDS-13.11, NEEDS-13.13_

---

### Task 2: Kernel — a Module dispatches an Action

**Files:**
- Create: `crates/kernel/src/dispatcher.rs`
- Modify: `crates/kernel/src/runtime.rs` (`Plumbing`, `Registered`, `start()`, `Kernel::dispatch`, `Kernel::actions`, `DispatchError`, `KernelBuilder::grant_destructive`), `crates/kernel/src/module.rs` (`ModuleCtx` field and `dispatcher()`), `crates/kernel/src/lib.rs`, `crates/ffi/src/app.rs` (one `Refused` arm in `dispatch_to_rpc`, lines 562-570)
- Test: `crates/kernel/tests/dispatcher.rs`

**Reuse:** 2 — extends the existing `SendFn` table (`crates/kernel/src/runtime.rs:60,167-178`, already a non-blocking `try_send` returning a `oneshot::Receiver`) and `Kernel::dispatch`'s timeout and error mapping (`:330-343`)

**Interfaces:**
- Consumes: `navori_protocol::{Envelope, Risk}`, `ActionSpec`
- Produces: `ModuleCtx::dispatcher(&self) -> ModuleDispatcher`; `ModuleDispatcher::send(&self, Envelope) -> Result<Pending, DispatchError>`; `ModuleDispatcher::risk_of(&self, &Envelope) -> Option<Risk>`; `Pending: Future<Output = Result<Value, DispatchError>>`; `KernelBuilder::grant_destructive(self, &'static str) -> Self`; `DispatchError::Refused { module: String, action: String }`

**Depends-on:** none

- [x] **Step 1: Failing tests** in `crates/kernel/tests/dispatcher.rs`, with two test Modules (`Asker`, which on a request dispatches an Envelope through `ctx.dispatcher()` and reports the outcome, and `Target`, with a `Write` action `poke`, a `Destructive` action `wipe` and a `stall` action that never replies): `a_module_reaches_another_and_gets_its_reply`; `send_returns_before_the_target_handles_it` (the Asker answers a second request while `Target` is stalled); `a_module_can_dispatch_to_itself`; `unknown_module_and_timeout_report_dispatch_errors` (`UnknownModule`, `Timeout` with a 100 ms builder timeout); `a_destructive_action_is_refused_without_a_grant` (`Refused`, `wipe` never runs); `a_granted_module_runs_a_destructive_action`; `risk_of_reads_the_catalog_for_string_and_object_actions`; `kernel_dispatch_is_unchanged_for_destructive_actions`.
  Run: `cargo nextest run -p navori-kernel --test dispatcher` — expect: compile error, `dispatcher` not found on `ModuleCtx`.
- [x] **Step 2: Implement** the router (`Arc<OnceLock<Router>>` in `Plumbing`, filled in `start()` before the first launcher runs), `ModuleDispatcher`, `Pending` (wraps the receiver in `tokio::time::timeout(dispatch_timeout)`, maps like `Kernel::dispatch`), the grant set (a grant naming no registered Module is logged and dropped at `start()`), `Refused`, and the `dispatch_to_rpc` arm mapping `Refused` to `rpc::code::INVALID_PARAMS`.
  Run: `cargo nextest run -p navori-kernel` — expect: pass (new and existing kernel tests).
- [x] **Step 3: Commit** `git commit -m "feat(kernel): give Modules a dispatcher with a Destructive grant"`

_Requirements: NEEDS-2.1, NEEDS-2.2, NEEDS-2.3, NEEDS-2.4, NEEDS-2.5, NEEDS-2.6, NEEDS-13.1_

---

### Task 3: Needs you Module — raise a Decision

**Files:**
- Create: `modules/needs_you/Cargo.toml`, `modules/needs_you/src/lib.rs`, `modules/needs_you/src/action.rs`, `modules/needs_you/src/state.rs`, `modules/needs_you/src/stored.rs`, `modules/needs_you/src/raise.rs`, `modules/needs_you/tests/common/mod.rs`, `modules/needs_you/tests/raise.rs`
- Modify: `Cargo.toml` (members, `[workspace.dependencies] navori-mod-needs-you`), `crates/ffi/Cargo.toml`, `crates/ffi/src/app.rs` (`build_kernel` registers `NeedsYou::new(*sweep)` (`JournalSweep` is `Copy`)), `crates/ffi/tests/actions_risk.rs` (sample for `raise`, a `needs_you` arm in the module match, bump `SAMPLED` by 1)
- Test: `modules/needs_you/tests/raise.rs`, `crates/ffi/tests/actions_risk.rs`
- Crate deps (`modules/needs_you/Cargo.toml`): `anyhow`, `navori-kernel`, `navori-store`, `navori-protocol`, `navori-redact`, `navori-config`, `rusqlite`, `schemars`, `serde`, `serde_json`, `tokio`, `tracing`; dev: `tokio` with `test-util`

**Reuse:** 2 — the Module pattern of `modules/awake` (`stored.rs` migrations and `with_conn` idiom, `src/infra/stored.rs:7-124`; externally tagged `snake_case` Action enum with `risk()`/`catalog()`, `src/action.rs:4-86`); `navori_redact::redact_text` (`crates/redact/src/lib.rs:120`)

**Interfaces:**
- Consumes: `ModuleDispatcher::risk_of` (Task 2), `navori_config::JournalSweep`, `navori_redact::{redact_text, redact_json}`
- Produces: `NeedsYou::new(JournalSweep) -> NeedsYou` (ID `"needs_you"`); `NeedsYouAction::Raise { key, title, reason, expires_at_ms: Option<u64>, primary: Choice, secondary: Choice }`; `Choice { label: String, envelope: Option<Envelope> }`; reply `{ "id": i64, "revision": u32 }`; `NeedsYouState { decisions: Vec<OpenDecision>, updates: Vec<UnviewedUpdate> }`; `OpenDecision { id, key, revision, title, reason, raised_at_ms, expires_at_ms, primary: ChoiceState, secondary: ChoiceState, state: DecisionState }`; table `needs_you_decisions` as design section B (no `CHECK`, no `FOREIGN KEY`); test harness `common::{harness, FakeClock, Target, Producer}`

**Depends-on:** Task 2

- [x] **Step 1: Failing tests** in `raise.rs` (kernel with `NeedsYou` + `Target`, in-memory store, `FakeClock`): `a_new_key_adds_an_open_decision`; `a_repeat_key_replaces_and_bumps_revision`; `each_choice_records_its_catalog_risk`; table-driven `refused_raises_add_nothing` (rows: empty key, blank title, blank label, Envelope to an unknown module, unknown action, a `needs_you` action, an Envelope with a `token` field, a past `expires_at_ms`); `title_reason_and_labels_pass_the_shield` (an `sk-…` key in the reason is stored redacted); `open_decisions_survive_a_restart`; `a_raise_while_answering_is_refused` (mark the row `answering` through `stored` and raise again). Also add the `raise` sample to `actions_risk.rs`.
  Run: `cargo nextest run -p navori-mod-needs-you` — expect: compile error, crate has no `NeedsYou`.
- [x] **Step 2: Implement** the crate (`[lints] workspace = true`), migration v1 for `needs_you_decisions` with its partial unique index, `raise` validation in the order of design section B, redaction, insert/replace in one transaction, reload State at the top of `run`, and register in `build_kernel`.
  Run: `cargo nextest run -p navori-mod-needs-you && cargo nextest run -p navori-ffi --test actions_risk` — expect: pass.
- [x] **Step 3: Acceptance** — `cargo xtask run`, then `"build/dev/Navori Dev.app/Contents/Helpers/navori" --profile dev dispatch needs_you '{"raise":{"key":"k1","title":"Try it","reason":"from the CLI","primary":{"label":"Ok"},"secondary":{"label":"Later"}}}'` returns an id; `cargo xtask check-deps` passes.
- [x] **Step 4: Commit** `git commit -m "feat(needs-you): raise a Decision through needs_you.raise"`

_Requirements: NEEDS-1.1, NEEDS-1.2, NEEDS-1.3, NEEDS-1.4, NEEDS-1.5, NEEDS-1.6, NEEDS-1.7, NEEDS-1.8, NEEDS-1.9, NEEDS-1.10, NEEDS-1.11, NEEDS-1.12, NEEDS-5.3, NEEDS-13.8, NEEDS-14.4_

---

### Task 4: Needs you Module — answer a Decision, at most once

**Files:**
- Create: `modules/needs_you/src/answer.rs`, `modules/needs_you/tests/answer.rs`, `modules/needs_you/tests/producer.rs`, `crates/ffi/tests/needs_you.rs`
- Modify: `modules/needs_you/src/lib.rs` (run loop arm, `tray()`), `modules/needs_you/src/action.rs` (`Answer`), `modules/needs_you/src/stored.rs`, `crates/kernel/src/runtime.rs` (`Kernel::destructive_grants()`), `crates/ffi/src/app.rs` (`.grant_destructive("needs_you")` and an in-crate `#[cfg(test)]` test of the grant set — `mod app` is private, `crates/ffi/src/lib.rs:11`), `crates/ffi/tests/actions_risk.rs` (`answer` sample, bump `SAMPLED`)
- Test: `modules/needs_you/tests/answer.rs`, `modules/needs_you/tests/producer.rs`, `crates/ffi/tests/needs_you.rs`, `crates/ffi/src/app.rs` unit test

**Reuse:** 2 — section A's `ModuleDispatcher`; Navori AI's existing gate (`ai/agent/src/dispatch.rs:33-42`) for NEEDS-4.11 with no edit; the ports actor's in-flight arm (`modules/ports/src/actor.rs:35-44,151-166`); `Module::tray` (`crates/kernel/src/module.rs:75`)

**Interfaces:**
- Consumes: `ModuleDispatcher::{send, risk_of}`, `Pending` (Task 2); `NeedsYouState`, `stored` (Task 3)
- Produces: `Kernel::destructive_grants(&self) -> Vec<&'static str>`; `NeedsYouAction::Answer { id: i64, revision: u32, choice: Pick }`, `Pick::{Primary, Secondary}` (`"primary"`/`"secondary"`), Risk `Destructive`; `DecisionState::{Open, Answering { choice }, Failed { choice, error }}`; attention clause `"N decisions waiting"`

**Depends-on:** Task 3

- [x] **Step 1: Failing tests** in `answer.rs`: `answering_runs_the_chosen_envelope_and_moves_it_to_history` (`Target` records one `poke`); `a_choice_without_an_envelope_closes_the_decision`; `a_failed_dispatch_keeps_it_open_with_the_error`; `a_timeout_says_it_may_already_have_run`; `retry_answers_again_with_the_failed_choice`; table-driven `refused_answers_dispatch_nothing` (rows: unknown id, answered Decision, one already answering, stale revision after a re-raise, answer at or after `expires_at_ms`); `a_restart_mid_answer_marks_it_failed_and_never_reruns` (kill the Module between the `answering` write and the reply, restart, assert `Failed` with `Navori stopped before this finished — it may already have run` and `Target` saw no second `poke`); `a_vanished_action_fails_the_answer`; `risk_is_read_from_the_catalog_at_start`; `attention_is_on_only_while_a_decision_is_open`. In `producer.rs`: `a_module_raises_through_its_dispatcher_and_its_envelope_runs_on_answer` (`Producer` raises via `ctx.dispatcher()`, the test answers, `Target` records the Envelope). In `crates/ffi/src/app.rs` `#[cfg(test)]`: `only_needs_you_holds_the_destructive_grant` (`build_kernel(..).destructive_grants() == ["needs_you"]`). In `crates/ffi/tests/needs_you.rs`, with a `Kernel` built in the test as `crates/ffi/tests/awake.rs:23-42` does (`Kernel::builder(Store::open_in_memory().unwrap()).pause_journal_sweep().register(NeedsYou::new(JournalSweep::Paused)).grant_destructive("needs_you").start()`): `navori_ai_cannot_answer_a_decision` — `navori_agent::Dispatcher::new(Duration::from_secs(5)).call(&kernel, &navori_agent::Toolset::from_specs(&kernel.actions()), &navori_llm::ToolCall { id: "1".into(), name: "needs_you_answer".into(), arguments: <answer JSON> })` returns `is_error: true` with the not-read-only message, and the Decision stays open.
  Run: `cargo nextest run -p navori-mod-needs-you --test answer` — expect: compile error, no `Answer` variant.
- [x] **Step 2: Implement** `answer` per design section C: checks, Risk re-read, `answering` write before `Ok(())`, `send`, `FuturesUnordered` arm, definite vs uncertain errors, `redact_text` on error text, restart rule at the top of `run`, `tray()`; add the grant and the `answer` sample.
  Run: `cargo nextest run -p navori-mod-needs-you && cargo nextest run -p navori-ffi --test needs_you --test actions_risk` — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(needs-you): answer a Decision once, from the Core"`

_Requirements: NEEDS-2.7, NEEDS-3.4, NEEDS-3.8, NEEDS-4.3, NEEDS-4.4, NEEDS-4.5, NEEDS-4.7, NEEDS-4.8, NEEDS-4.9, NEEDS-4.10, NEEDS-4.11, NEEDS-4.12, NEEDS-11.9, NEEDS-13.5_

---

### Task 5: Needs you Module — a Decision expires into an Update

**Files:**
- Create: `modules/needs_you/src/expiry.rs`, `modules/needs_you/tests/expiry.rs`
- Modify: `modules/needs_you/src/lib.rs` (deadline arm), `modules/needs_you/src/stored.rs` (migration v2: `needs_you_updates`), `modules/needs_you/src/state.rs` (`UnviewedUpdate`)
- Test: `modules/needs_you/tests/expiry.rs`

**Reuse:** 2 — the Scheduler's `WaitHandle::wait_until(Option<SystemTime>) -> Wake` (`crates/kernel/src/scheduler.rs:112`), wake- and panel-aware with the closed-menu 30 s gap (KCONTRACT-4.9, 5.1, 6.1–6.3; `scheduler.rs:91,118-171`); `System`'s loop as template (`crates/kernel/src/system.rs:107-137`)

**Interfaces:**
- Consumes: `ctx.wait_handle()`, `WaitHandle::now()`; `stored` (Task 3)
- Produces: table `needs_you_updates` as design section B; `UnviewedUpdate { id, kind: "decision_expired", title, created_at_ms }`

**Depends-on:** Task 4

- [x] **Step 1: Failing tests** (`#[tokio::test(start_paused = true)]`, advance both `FakeClock` and tokio time): `a_decision_without_expiry_stays_open`; `an_expired_decision_becomes_an_expired_update` (title `Expired: <title>`, row gone from State); `a_decision_answering_does_not_expire`; `a_failed_decision_expires`; `a_past_deadline_expires_at_start`; `a_wake_after_the_deadline_expires_it_at_once` (`scheduler.wake_pulse()`); `no_action_posts_an_update` (the catalog lists no such action); `updates_never_turn_on_attention`; `migration_v2_keeps_v1_decisions_and_backs_up` (a store at v1 with a row, upgraded: row kept, `MigrationReport.backup` is `Some`).
  Run: `cargo nextest run -p navori-mod-needs-you --test expiry` — expect: fail, Decision still open after the deadline.
- [x] **Step 2: Implement** migration v2 and the deadline logic of design section D.
  Run: `cargo nextest run -p navori-mod-needs-you` — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(needs-you): expire a Decision into an Update"`

_Requirements: NEEDS-5.1, NEEDS-5.2, NEEDS-5.4, NEEDS-5.5, NEEDS-11.9, NEEDS-14.4_

---

### Task 6: Needs you Module — seen and cleared Updates

**Files:**
- Create: `modules/needs_you/tests/updates.rs`
- Modify: `modules/needs_you/src/action.rs` (`MarkViewed`, `ClearUpdates`), `modules/needs_you/src/expiry.rs`, `modules/needs_you/src/stored.rs`, `crates/ffi/tests/actions_risk.rs` (two samples, bump `SAMPLED`)
- Test: `modules/needs_you/tests/updates.rs`

**Reuse:** 2 — the Scheduler's `WaitHandle::wait_until(Option<SystemTime>) -> Wake` (`crates/kernel/src/scheduler.rs:112`), wake- and panel-aware with the closed-menu 30 s gap (KCONTRACT-4.9, 5.1, 6.1–6.3; `scheduler.rs:91,118-171`); `System`'s loop as template (`crates/kernel/src/system.rs:107-137`)

**Interfaces:**
- Consumes: `needs_you_updates` (Task 5)
- Produces: `NeedsYouAction::MarkViewed { ids: Vec<i64> }` (Write), `NeedsYouAction::ClearUpdates` (Write)

**Depends-on:** Task 5

- [x] **Step 1: Failing tests:** `marking_viewed_hides_exactly_those_updates`; `an_update_never_marked_stays`; `clear_updates_resolves_every_update`; `viewed_and_cleared_updates_stay_in_history_rows` (resolution `viewed`/`cleared`, `resolved_at_ms` set).
  Run: `cargo nextest run -p navori-mod-needs-you --test updates` — expect: compile error, no `MarkViewed`.
- [x] **Step 2: Implement** both actions; add their samples to `actions_risk.rs`.
  Run: `cargo nextest run -p navori-mod-needs-you && cargo nextest run -p navori-ffi --test actions_risk` — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(needs-you): mark Updates viewed and clear them"`

_Requirements: NEEDS-6.3, NEEDS-7.2_

---

### Task 7: Needs you Module — History and its 30 days

**Files:**
- Create: `modules/needs_you/src/history.rs`, `modules/needs_you/tests/history.rs`
- Modify: `modules/needs_you/src/action.rs` (`History`, Read), `modules/needs_you/src/lib.rs` (prune deadline), `crates/ffi/tests/actions_risk.rs` (`history` sample, bump `SAMPLED`)
- Test: `modules/needs_you/tests/history.rs`

**Reuse:** 2 — the Scheduler's `WaitHandle::wait_until(Option<SystemTime>) -> Wake` (`crates/kernel/src/scheduler.rs:112`), wake- and panel-aware with the closed-menu 30 s gap (KCONTRACT-4.9, 5.1, 6.1–6.3; `scheduler.rs:91,118-171`); `System`'s loop as template (`crates/kernel/src/system.rs:107-137`)

**Interfaces:**
- Consumes: `JournalSweep` from `NeedsYou::new` (Task 3)
- Produces: `NeedsYouAction::History` (Read) → `[{ "kind": "decision" | "update", "title", "chosen_label"?, "resolved_at_ms" }]`

**Depends-on:** Task 6

- [x] **Step 1: Failing tests:** `history_lists_answered_decisions_and_resolved_updates_newest_first`; `history_has_no_answer_for_a_decision` (no request string, no action accepts its id); `items_older_than_retention_are_pruned_at_start_and_daily`; `a_paused_sweep_prunes_nothing` (`JournalSweep::Paused`).
  Run: `cargo nextest run -p navori-mod-needs-you --test history` — expect: compile error, no `History`.
- [x] **Step 2: Implement** `history` and prune per design section D (small `DELETE` statements).
  Run: `cargo nextest run -p navori-mod-needs-you && cargo nextest run -p navori-ffi --test actions_risk` — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(needs-you): keep 30 days of History"`

_Requirements: NEEDS-12.2, NEEDS-12.3, NEEDS-12.4_

---

### Task 8: Platform — the local UTC offset

**Files:**
- Create: `crates/macos/src/clock.rs`, `crates/macos/tests/clock.rs`
- Modify: `crates/macos/src/lib.rs`
- Test: `crates/macos/tests/clock.rs`

**Reuse:** 2 — the same Module pattern as B; env injection like `Ports::new(PortsEnv)` (`crates/ffi/src/app.rs:271-273`); `time` 0.3 (rung 5, workspace dep) for `Date` arithmetic; `navori-macos` (rung 2, existing platform crate) gains one function

**Interfaces:**
- Consumes: none
- Produces: `navori_macos::local_utc_offset(at: SystemTime) -> i32` (seconds east of UTC, from `localtime_r` `tm_gmtoff`)

**Depends-on:** none

- [x] **Step 1: Failing test** `offset_matches_the_system_zone`: compare with `date +%z` run through `std::process::Command` for `SystemTime::now()`.
  Run: `cargo nextest run -p navori-macos --test clock` — expect: compile error, no `local_utc_offset`.
- [x] **Step 2: Implement** with `libc::localtime_r`.
  Run: same command — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(macos): read the local UTC offset"`

_Requirements: NEEDS-9.2_

---

### Task 9: To-do Module — add a To-do and its status by day

**Files:**
- Create: `modules/todo/Cargo.toml`, `modules/todo/src/lib.rs`, `modules/todo/src/action.rs`, `modules/todo/src/state.rs`, `modules/todo/src/stored.rs`, `modules/todo/src/calendar.rs`, `modules/todo/src/actor.rs`, `modules/todo/tests/common/mod.rs`, `modules/todo/tests/todo.rs`, `modules/todo/tests/day.rs`
- Modify: `Cargo.toml` (members, `navori-mod-todo`, `time` features `parsing`), `crates/ffi/Cargo.toml` (`navori-mod-todo`, `navori-macos` under `[dependencies]`), `crates/ffi/src/app.rs` (`build_kernel` registers `Todo::new(TodoEnv { calendar }, *sweep)` with a `LocalCalendar` backed by `navori_macos::local_utc_offset`), `crates/ffi/tests/actions_risk.rs`
- Test: `modules/todo/tests/todo.rs`, `modules/todo/tests/day.rs`
- Crate deps (`modules/todo/Cargo.toml`): as Task 3 plus `time.workspace = true` (workspace features `formatting`, `macros`, `parsing`); `crates/ffi/tests/actions_risk.rs`: `add` sample, a `todo` arm, bump `SAMPLED`

**Reuse:** 2 — the same Module pattern as B; env injection like `Ports::new(PortsEnv)` (`crates/ffi/src/app.rs:271-273`); `time` 0.3 (rung 5, workspace dep) for `Date` arithmetic; `navori-macos` (rung 2, existing platform crate) gains one function

**Interfaces:**
- Consumes: `navori_macos::local_utc_offset` (Task 8), `JournalSweep`
- Produces: `Todo::new(TodoEnv, JournalSweep) -> Todo` (ID `"todo"`); `TodoEnv { calendar: Arc<dyn LocalCalendar> }`; `trait LocalCalendar: Send + Sync { fn utc_offset(&self, at: SystemTime) -> i32 }`; `TodoAction::Add { title: String, due_on: Option<String> }` (Write) → `{ "id": i64 }`; `TodoState { today: String, open: Vec<OpenTodo> }`; `OpenTodo { id, title, due_on: Option<String>, status: TodoStatus }`; `TodoStatus::{Overdue, Today, Soon, Later, None}`; table `todo_items` as design section E

**Depends-on:** Task 8, Task 7 (shares `Cargo.toml`, `crates/ffi/Cargo.toml`, `build_kernel` and `actions_risk.rs` with Tasks 3–7)

- [x] **Step 1: Failing tests** (fake calendar with a fixed offset, `FakeClock`): `adding_a_todo_shows_it_at_once`; `a_blank_title_adds_nothing`; `the_title_passes_the_shield`; `todos_survive_a_restart`; table-driven `status_follows_the_local_day` (rows: yesterday → Overdue, today → Today, +1 and +3 → Soon, +4 → Later, none → None); `status_changes_at_local_midnight_and_after_a_wake` (advance past midnight; and a `wake_pulse` after a long sleep); `todos_never_turn_on_attention`.
  Run: `cargo nextest run -p navori-mod-todo` — expect: compile error, crate has no `Todo`.
- [x] **Step 2: Implement** the crate, migration v1, `add`, status from `wait.now()` plus `calendar.utc_offset(now)`, the midnight deadline that calls `update_state` only on change, and registration; add the `add` sample.
  Run: `cargo nextest run -p navori-mod-todo && cargo nextest run -p navori-ffi --test actions_risk && cargo xtask check-deps` — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(todo): add a To-do with a status by local day"`

_Requirements: NEEDS-8.4, NEEDS-8.5, NEEDS-8.6, NEEDS-8.7, NEEDS-8.8, NEEDS-9.2, NEEDS-9.4, NEEDS-11.9, NEEDS-14.3_

---

### Task 10: To-do Module — tick, undo and History

**Files:**
- Create: `modules/todo/tests/history.rs`
- Modify: `modules/todo/src/action.rs` (`Tick`, `Undo`, `History`), `modules/todo/src/actor.rs` (prune deadline), `modules/todo/src/stored.rs`, `crates/ffi/tests/actions_risk.rs` (three samples, bump `SAMPLED`)
- Test: `modules/todo/tests/history.rs`

**Reuse:** 2 — the same Module pattern as B; env injection like `Ports::new(PortsEnv)` (`crates/ffi/src/app.rs:271-273`); `time` 0.3 (rung 5, workspace dep) for `Date` arithmetic; `navori-macos` (rung 2, existing platform crate) gains one function

**Interfaces:**
- Consumes: `TodoState`, `stored` (Task 9)
- Produces: `TodoAction::Tick { id: i64 }`, `TodoAction::Undo { id: i64 }` (Write); `TodoAction::History` (Read) → `[{ "id", "title", "due_on"?, "done_at_ms" }]`

**Depends-on:** Task 9

- [x] **Step 1: Failing tests:** `ticking_moves_a_todo_to_history`; `undo_returns_it_with_title_and_date`; `tick_and_undo_are_write_actions` (catalog); `old_done_todos_are_pruned`; `a_paused_sweep_prunes_nothing`.
  Run: `cargo nextest run -p navori-mod-todo --test history` — expect: compile error, no `Tick`.
- [x] **Step 2: Implement** per design section E; add samples.
  Run: `cargo nextest run -p navori-mod-todo && cargo nextest run -p navori-ffi --test actions_risk` — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(todo): tick, undo and keep 30 days of History"`

_Requirements: NEEDS-10.1, NEEDS-10.2, NEEDS-12.3, NEEDS-12.4, NEEDS-12.5, NEEDS-12.6_

---

### Task 11: Core view — `NeedsYouView` in the snapshot

**Files:**
- Create: `crates/ffi/src/needs_you.rs`, `crates/ffi/tests/needs_you_view.rs`
- Modify: `crates/ffi/src/lib.rs` (`CoreSnapshot.needs_you`, `snapshot()`, `NavoriCore::needs_you_history()`), `crates/ffi/src/app.rs` (`needs_you_view`), `crates/ffi/tests/needs_you.rs`
- Test: `crates/ffi/tests/needs_you_view.rs`, `crates/ffi/tests/needs_you.rs`

**Reuse:** 2 — the `running.rs` pure-function join (`crates/ffi/src/running.rs:96`) and `awake_view`'s state read with `module_is_down` overlay (`crates/ffi/src/app.rs:344-400`); pre-built request strings as `RunningPort.stop_request` (`running.rs:43,349`)

**Interfaces:**
- Consumes: `NeedsYouState` (Tasks 3–6), `TodoState` (Tasks 9–10)
- Produces: `NeedsYouView { decisions: Vec<DecideRow>, todos: Vec<TodoRow>, updates: Vec<UpdateRow>, today: String, needs_count: u32, any_overdue: bool, all_caught_up: bool, down: bool }`; `DecideRow { id, title, reason, raised_at_ms, state: DecideState, primary: ChoiceView, secondary: ChoiceView }`; `DecideState::{Open, Answering { choice: String }, Failed { choice: String, error: String }}`; `ChoiceView { label, risk: Option<String>, action: Option<String>, answer_request: String }`; `TodoRow { id, title, due_on: Option<String>, status: TodoStatus, tick_request: String }`; `UpdateRow { id, kind, title, created_at_ms }`; `HistoryView { entries: Vec<HistoryEntry> }`, `HistoryEntry { kind, title, outcome: Option<String>, resolved_at_ms, undo_request: Option<String> }`; all `#[derive(Debug, Clone, PartialEq, uniffi::Record)]` or `uniffi::Enum`, like `AwakeView` (`crates/ffi/src/lib.rs:125`)

**Depends-on:** Task 7, Task 10

- [x] **Step 1: Failing tests** in `needs_you_view.rs` (pure `build`): `decisions_order_by_expiry_then_age`; `todos_order_overdue_first_and_no_date_last`; `updates_order_newest_first`; `needs_count_is_open_todos_plus_unviewed_updates`; `any_overdue_flags_an_overdue_todo`; `all_caught_up_only_when_nothing_is_left`; `the_all_list_is_todos_then_updates`; `no_decision_appears_in_the_submenu_lists`; `answer_requests_carry_id_revision_and_choice`; `choice_risk_and_action_are_carried_into_the_view`. In `needs_you.rs`: `history_merges_both_modules_newest_first`.
  Run: `cargo nextest run -p navori-ffi --test needs_you_view` — expect: compile error, no `needs_you::build`.
- [x] **Step 2: Implement** `build`, the record types, the snapshot field, `needs_you_history()`.
  Run: `cargo nextest run -p navori-ffi` — expect: pass (all ffi tests, including `core.rs`, `running.rs`).
- [x] **Step 3: Commit** `git commit -m "feat(ffi): join Needs you and To-dos into NeedsYouView"`

_Requirements: NEEDS-3.1, NEEDS-3.7, NEEDS-3.8, NEEDS-6.1, NEEDS-9.1, NEEDS-11.2, NEEDS-11.3, NEEDS-11.4, NEEDS-11.6, NEEDS-12.1_

---

### Task 12: Shell logic — text, lists and the level-0 tree (NavoriPanel)

**Files:**
- Create: `apps/macos/NavoriPanel/NeedsYouText.swift`, `apps/macos/NavoriPanel/NeedsYouList.swift`, `apps/macos/Tests/NavoriPanelTests/NeedsYouTextTests.swift`, `apps/macos/Tests/NavoriPanelTests/NeedsYouListTests.swift`
- Modify: `apps/macos/NavoriPanel/CoreRowState.swift` (`LevelZeroTree.rows`), `apps/macos/NavoriPanel/MenuMetrics.swift` (`MenuHitMap`), `apps/macos/Tests/NavoriPanelTests/CoreRowStateTests.swift`, `apps/macos/Tests/NavoriPanelTests/MenuHitMapTests.swift`
- Test: the four test files above

**Reuse:** 2 — `MenuRowView`/`MenuRow` (`apps/macos/Navori/Features/Panel/MenuRowView.swift:5-133`) for the `Needs you` row; `CapsuleButton` moved out of `#if DEBUG` (`Features/Preview/PreviewSampleRows.swift:40-73`) into `Features/Panel/`; `LevelZeroTree.rows` (`apps/macos/NavoriPanel/CoreRowState.swift:84-102`); `CoreStore.perform` pattern (`CoreStore.swift:441-454`); new `DecideRowView` (rung 7 — `MenuRowView` has no inline action slot, brief :33)

**Interfaces:**
- Consumes: plain values (no NavoriCore import: `NavoriPanel` has no dependency on the bindings)
- Produces: `NeedsYouText.age(raisedAtMs:nowMs:) -> String`; `NeedsYouText.decisionSpoken(title:reason:age:primary:secondary:) -> String`; `NeedsYouText.pill(dueOn:today:) -> String`; `NeedsYouText.pillSpoken(status:dueOn:) -> String`; `NeedsYouText.destructiveHint(module:action:) -> String`; `NeedsYouList.filter(_ tab: NeedsYouTab, todos:updates:)`; `NeedsYouList.drawnIds(rowFrames:[Int64: CGRect], viewport: CGRect) -> Set<Int64>`; `DecideKey.from(keyCode:modifiers:) -> DecideKey?` (`.primary` Return, `.secondary` ⌘⌫); `LevelZeroTree.rows(decisions:needs:running:core:status:inDevProfile:)`

**Depends-on:** none

- [x] **Step 1: Failing tests:** table-driven `testPillText` with today = Sunday 2026-11-08 (`Today` for 11-08, `Tomorrow` for 11-09, `Tue` for 11-10, `Fri` for 11-13, `Sat` for 11-14, `Sun 15` for 11-15, `Thu 5` for 11-05, `Sat 7` for 11-07, `No date` for none); table-driven `testPillSpoken` (`Overdue, Thursday 5 November`, `Due today`, `Due soon, Sunday 8 November`, `Due later, Friday 13 November`, `No due date`); `testDecisionSpoken` (`Decision: Review PR #41 in zroute, Minh requested your review, 25 minutes ago, Open, Later`); `testDestructiveHint` (`Runs ports.kill — changes things and cannot be undone`); `testCheckboxSpokenIsMarkDone`; `testFilterTabs`; `testDrawnIdsAreOnlyRowsInsideTheViewport`; `testDecideKeys`; `testLevelZeroOrderPutsDecideAndNeedsBeforeRunning`; `testHitMapFindsRowsAboveRunningAndDecideButtons`.
  Run: `swift test --arch arm64 --package-path apps/macos --filter NeedsYou` — expect: build failure, `NeedsYouText` not found.
- [x] **Step 2: Implement** the helpers and the tree and hit-map changes.
  Run: `swift test --arch arm64 --package-path apps/macos` — expect: pass (all NavoriPanel tests).
- [x] **Step 3: Commit** `git commit -m "feat(panel): Needs you text, lists and level-0 tree"`

_Requirements: NEEDS-3.6, NEEDS-3.10, NEEDS-4.2, NEEDS-6.2, NEEDS-9.3, NEEDS-9.5, NEEDS-10.3, NEEDS-11.1_

---

### Task 13: Shell — Decide rows and the `Needs you` row on level 0

**Files:**
- Create: `apps/macos/Navori/Features/Panel/CapsuleButton.swift`, `apps/macos/Navori/Features/Panel/DecideRowView.swift`
- Modify: `apps/macos/Navori/Features/Preview/PreviewSampleRows.swift` (remove `CapsuleButton`; `InboxRowView` stays), `apps/macos/Navori/Features/Panel/PanelView.swift`, `apps/macos/Navori/Core/CoreStore.swift` (`needsYou`, `answer(_:)`), `apps/macos/Navori/StatusBar/StatusBarController.swift` (`menuTree`, `onSnapshotChange`, `observeSnapshot`, `activate`), `apps/macos/Navori/StatusBar/StatusPanel.swift` (`routeKey` ⌘⌫, `activateRow(at:)` buttons)
- Test: build + NavoriPanel tests (Task 12) + acceptance below

**Reuse:** 2 — `MenuRowView`/`MenuRow` (`apps/macos/Navori/Features/Panel/MenuRowView.swift:5-133`) for the `Needs you` row; `CapsuleButton` moved out of `#if DEBUG` (`Features/Preview/PreviewSampleRows.swift:40-73`) into `Features/Panel/`; `LevelZeroTree.rows` (`apps/macos/NavoriPanel/CoreRowState.swift:84-102`); `CoreStore.perform` pattern (`CoreStore.swift:441-454`); new `DecideRowView` (rung 7 — `MenuRowView` has no inline action slot, brief :33)

**Interfaces:**
- Consumes: `NeedsYouView`, `DecideRow`, `ChoiceView` (Task 11); `NeedsYouText`, `DecideKey`, `LevelZeroTree.rows`, `MenuHitMap` (Task 12)
- Produces: `CoreStore.needsYou: NeedsYouView?`; `CoreStore.answer(_ request: String) async`; `DecideRowView(row:cascade:store:)`; `CapsuleButton(title:prominent:selected:action:)` with `action` defaulting to `{}` so the preview's calls (`PreviewSampleRows.swift:105,108`) still compile

**Depends-on:** Task 11, Task 12

- [x] **Step 1: Implement** `CapsuleButton` with a real action, `DecideRowView` (hover/focus swap, spinner, `Retry`, tooltip/accessibility value, Destructive hint), the Decide block, the `Needs you` row (count, red dot, chevron), `✓ You're all caught up`, the new level-0 order, Release visibility, ⌘⌫ routing and button hit-testing.
  Run: `cargo xtask ci` — expect: pass (Swift build and `swift test`).
- [x] **Step 2: Acceptance** — `cargo xtask run`; start a throwaway listener (`python3 -m http.server 8765`) and raise two Decisions from the CLI — one whose primary is a `ports.kill` Envelope (`modules/ports/src/action.rs:21-26`) naming only that listener's pid, one with no Envelopes; check one-line rows, hover and focus swap, Return and ⌘⌫, spinner, `Retry` after making the target fail, the Destructive tooltip, attention on and off, `All caught up`, VoiceOver string; then `cargo xtask bundle --profile release` (builds `build/release/Navori.app` in the worktree; nothing is installed) and `strings build/release/Navori.app/Contents/MacOS/Navori | grep -c NAVORI-PREVIEW-SAMPLE` — expect `0`; in the dev app, no Ask field, Ports or Pull Requests rows outside the preview.
- [x] **Step 3: Commit** `git commit -m "feat(panel): Decide rows and the Needs you row on level 0"`

_Requirements: NEEDS-3.2, NEEDS-3.3, NEEDS-3.5, NEEDS-3.9, NEEDS-4.1, NEEDS-4.6, NEEDS-11.5, NEEDS-13.7, NEEDS-13.10, NEEDS-13.12_

---

### Task 14: Shell — typing in the cascade

**Files:**
- Modify: `apps/macos/Navori/StatusBar/CascadePanel.swift` (`acceptsTyping`, `canBecomeKey`, `becomesKeyOnlyIfNeeded`, `routeKey`), `apps/macos/Navori/StatusBar/StatusPanel.swift` (`resignKey` deferred check), `apps/macos/Navori/StatusBar/CascadeController.swift` (`onSubmenuClosed`, level-0 key on hide)
- Test: build + acceptance below

**Reuse:** 4 — AppKit `NSPanel.becomesKeyOnlyIfNeeded` and `NSView.needsPanelToBecomeKey` (research note); `StatusPanel.routeKey`'s text-field pass-through (`StatusPanel.swift:290-293`)

**Interfaces:**
- Consumes: Task 1's spike results
- Produces: `CascadePanel.acceptsTyping: Bool`; `CascadeController.onSubmenuClosed: ((String) -> Void)?`

**Depends-on:** Task 1, Task 13 (both edit `StatusPanel.swift`)

- [x] **Step 1: Implement** the three changes of design section I exactly as the spike proved them.
  Run: `cargo xtask ci` — expect: pass.
- [x] **Step 2: Acceptance** — `cargo xtask run`; open `Running` and Core submenus: arrows, Return and Esc behave as before and no submenu takes key without VoiceOver; click another app's window with the menu open: the menu closes; with VoiceOver on, cascade navigation is unchanged.
- [x] **Step 3: Commit** `git commit -m "feat(panel): let a submenu take typing without closing the menu"`

_Requirements: NEEDS-13.11, NEEDS-13.13_

---

### Task 15: Shell — the Needs you submenu, To-dos and the calendar

**Files:**
- Create: `apps/macos/Navori/Features/Panel/NeedsYouSubmenuView.swift`, `apps/macos/Navori/Features/Panel/TodoRowView.swift`, `apps/macos/Navori/Features/Panel/UpdateRowView.swift`, `apps/macos/Navori/Features/Panel/AddTodoField.swift`, `apps/macos/Navori/Features/Panel/DueCalendarView.swift`
- Modify: `apps/macos/Navori/Core/CoreStore.swift` (`addTodo(title:dueOn:)`, `tick(_:)`, `clearUpdates()`, `markViewed(_:)`, draft text and date), `apps/macos/Navori/StatusBar/StatusBarController.swift` (`needs`, `needs.date` cases; `onSubmenuClosed` wiring)
- Test: build + acceptance below

**Reuse:** 2 — `RunningSubmenuView` as template (`Features/Panel/RunningSubmenuView.swift:5-23`, `CappedScroll`, `MenuDivider`); the `awake.keep` depth-2 child precedent for the calendar and History (`StatusBarController.swift:301-309`); a new `UpdateRowView` drawn on the shape of the preview's `InboxRowView` (`PreviewSampleRows.swift:75-143`) — `InboxRowView` itself stays in the preview, which still reads it (`PreviewMenu.swift:126`), and keeps `PreviewSampleRows.marker` (`:142`), which must never reach Release code; AppKit `NSSegmentedControl`, `NSTextField`, `NSDatePicker` (rung 4) through `NSViewRepresentable`; new `TodoRowView`, `DuePill` (rung 7 — no kit row has a trailing pill, brief :49-50)

**Interfaces:**
- Consumes: `NeedsYouView`, `TodoRow`, `UpdateRow` (Task 11); `NeedsYouText`, `NeedsYouList` (Task 12); `CascadePanel.acceptsTyping`, `CascadeController.onSubmenuClosed` (Task 14)
- Produces: `NeedsYouSubmenuView(store:)`, `TodoRowView`, `DuePill`, `UpdateRowView`, `AddTodoField`, `DueCalendarView`

**Depends-on:** Task 13, Task 14

- [x] **Step 1: Implement** the submenu layout, filter, field and date chip, the `needs.date` calendar with quick picks, To-do rows with the pill and the 0.45 s strike-through hold, Update rows, `Clear updates`, empty tabs, drawn-row tracking and `markViewed` on close.
  Run: `cargo xtask ci` — expect: pass.
- [x] **Step 2: Acceptance** — `cargo xtask run`; type a To-do, pick `Tomorrow`, press Return: it shows at once with a yellow pill and the field resets; blank Return adds nothing; tick it: struck, gone after ~0.45 s; raise a Decision with a 1-minute `expires_at_ms`, wait: an `Expired:` Update shows, clicking it does nothing; close the submenu with it drawn and reopen: gone; `Clear updates` on the Updates tab only; quit with the submenu open and relaunch: the Update is still there; the filter reads as a tab group and the field precedes the list.
- [x] **Step 3: Commit** `git commit -m "feat(panel): the Needs you submenu with To-dos, Updates and a calendar"`

_Requirements: NEEDS-6.1, NEEDS-6.2, NEEDS-6.4, NEEDS-6.5, NEEDS-7.1, NEEDS-8.1, NEEDS-8.2, NEEDS-8.3, NEEDS-8.4, NEEDS-8.5, NEEDS-10.1, NEEDS-11.6, NEEDS-11.7, NEEDS-11.8_

---

### Task 16: Shell — History

**Files:**
- Create: `apps/macos/Navori/Features/Panel/HistorySubmenuView.swift`
- Modify: `apps/macos/Navori/Core/CoreStore.swift` (`loadHistory()`, `undo(_:)`), `apps/macos/Navori/StatusBar/StatusBarController.swift` (`needs.history` case)
- Test: build + acceptance below

**Reuse:** 2 — `RunningSubmenuView` as template (`Features/Panel/RunningSubmenuView.swift:5-23`, `CappedScroll`, `MenuDivider`); the `awake.keep` depth-2 child precedent for the calendar and History (`StatusBarController.swift:301-309`); a new `UpdateRowView` drawn on the shape of the preview's `InboxRowView` (`PreviewSampleRows.swift:75-143`) — `InboxRowView` itself stays in the preview, which still reads it (`PreviewMenu.swift:126`), and keeps `PreviewSampleRows.marker` (`:142`), which must never reach Release code; AppKit `NSSegmentedControl`, `NSTextField`, `NSDatePicker` (rung 4) through `NSViewRepresentable`; new `TodoRowView`, `DuePill` (rung 7 — no kit row has a trailing pill, brief :49-50)

**Interfaces:**
- Consumes: `NavoriCore.needsYouHistory() -> HistoryView` (Task 11)
- Produces: `HistorySubmenuView(store:)`; `CoreStore.history: HistoryView?`

**Depends-on:** Task 15

- [x] **Step 1: Implement** the History submenu (loading, empty, error states; `Undo` on To-do entries only; answered Decisions read-only).
  Run: `cargo xtask ci` — expect: pass.
- [x] **Step 2: Acceptance** — `cargo xtask run`; after ticking a To-do, answering a Decision and clearing an Update, `History ›` lists all three newest first; `Undo` returns the To-do with its date; the Decision entry has no answer control.
- [x] **Step 3: Commit** `git commit -m "feat(panel): History of the last 30 days"`

_Requirements: NEEDS-12.1, NEEDS-12.2, NEEDS-12.5_

---

### Task 17: Documents and amendments

**Files:**
- Modify: `docs/specs/2026-11-02-menubar-visual-foundation/requirements.md` (MENU-1.2, MENU-1.4), `docs/specs/2026-11-02-awake-leases/requirements.md` (AWAKE-11.1), `docs/design/2026-11-01-menubar-ui-brief.md` (rules 6 and 12), `docs/architecture/data.md` (line 37), `docs/codebase/modules.md` (line 50), `docs/codebase/map.md` (line 44)
- Test: `audit-trace` clean; `rg -n 'inbox_\*|navori-mod-inbox|modules/inbox' docs` returns only historical notes

**Reuse:** 2 — the "(amended by …)" convention (`docs/specs/2026-11-02-menubar-visual-foundation/requirements.md:33-35`)

**Interfaces:**
- Consumes: none
- Produces: none

**Depends-on:** Task 13

- [x] **Step 1:** Append `(amended by NEEDS-11.1)` to MENU-1.2 and AWAKE-11.1, `(amended by NEEDS-3.9)` to MENU-1.4; add amendment notes to brief rules 6 and 12 naming NEEDS-3.1 and NEEDS-3.9; replace the `inbox_*` row with `needs_you_decisions`, `needs_you_updates` (Owner `needs_you`) and `todo_items` (Owner `todo`); rename the planned crate and directory lines.
- [x] **Step 2:** Run the `rg` check above and `audit-trace` — expect: clean.
- [x] **Step 3: Commit** `git commit -m "docs: record what Needs you amends"`

_Requirements: NEEDS-13.10, NEEDS-13.12_

---

### Task 18: Guards and the dogfood walk

**Files:**
- Create: the dogfood guide written by `write-dogfood` under `.skills/NEEDS/` (main checkout)
- Test: the whole suite; the dogfood run

**Reuse:** 2 — the existing suites and `docs/agents/verify.md` (Launch / Doctor / Drive / Evidence)

**Interfaces:**
- Consumes: Tasks 2–17
- Produces: a dogfood guide and run file

**Depends-on:** Task 16, Task 17

- [x] **Step 1:** `cargo xtask ci` — expect: pass, including the unchanged suites `crates/kernel/tests/{kernel,scheduler,tray}.rs` (the restart test keeps queued requests), `crates/kernel/tests/system*`, `modules/awake/tests/`, `ai/agent/tests/`, `crates/ffi/tests/core.rs`, `running.rs`.
- [x] **Step 2:** Run `write-dogfood` for NEEDS, then `run-dogfood` in the dev app: every Shell-only ID of the design's dogfood row, a timed Keep awake Lease ending as today with no Decision, the keyboard-and-VoiceOver walk of every Needs you control, and the snapshot cadence unchanged (`navori --profile dev status` while open and closed).
- [x] **Step 3:** Record that NEEDS-14.1 and NEEDS-14.2 are left to the release perf run (`cargo xtask perf`, once, before release) — no per-feature perf run.
- [x] **Step 4: Commit** `git commit -m "test(needs-you): dogfood guide and run"`

_Requirements: NEEDS-13.2, NEEDS-13.3, NEEDS-13.4, NEEDS-13.6, NEEDS-13.9, NEEDS-14.1, NEEDS-14.2, NEEDS-14.5_

---

### Task 19: Amendment 2026-11-09 — menu polish

**Files:**
- Modify: `apps/macos/Navori/Features/Panel/{PanelView,NeedsYouSubmenuView,AddTodoField,CapsuleButton,TodoRowView}.swift`, `apps/macos/NavoriPanel/{MenuMetrics,NeedsYouText}.swift`, `apps/macos/Navori/StatusBar/{CascadeContent,StatusBarController,StatusPanel}.swift`, `apps/macos/Navori/Features/Preview/PreviewMenu.swift`
- Test: `apps/macos/Tests/NavoriPanelTests/{NeedsYouTextTests,MenuHitMapTests}.swift`

**Reuse:** 2 — `MenuRowView` for `Clear updates`, `CapsuleButton` for the date chip and the filter's look.

**Interfaces:**
- Produces: `NeedsYouText.filterLabels(todoCount:updateCount:) -> [String]`; `MenuHitMap.hit(at:width:tree:decideButtons:)` without the `caughtUp` offset.

**Depends-on:** Task 15

- [x] **Step 1: Failing tests:** `filter_labels_show_a_count_only_when_there_is_one`; `with_no_decision_needs_is_the_top_row_and_nothing_sits_above_it` (replaces the caught-up hit test).
- [x] **Step 2: Implement** no level-0 caught-up line and no `caughtUp` plumbing; capsule filter tabs; no submenu header; one rounded field with `+` lead, chip with `calendar` icon, accent border on focus; lead icons for `History` and `Clear updates`; the To-do checkbox no longer stretches across the row.
- [x] **Step 3: Acceptance** — `cargo xtask run`; open the panel: no caught-up line on level 0; the submenu shows the tabs, the field (focus border on click), an empty tab's `You're all caught up`, and rows whose text starts in one column.

_Requirements: NEEDS-3.9, NEEDS-8.1, NEEDS-11.4, NEEDS-11.5, NEEDS-11.7, NEEDS-11.10_
