# Tasks: Kernel contract extensions

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: KCONTRACT
Status: Approved
Date: 2026-11-02
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

**Goal:** Give the Kernel a per-Module config and a deadline wait handle that ROAD-8 can use without editing `crates/kernel`, keep a bad config value to itself, and pause the Journal sweep when retention cannot be trusted.

**Architecture:** `crates/kernel/src/scheduler.rs` holds a `Scheduler` (one watch channel, no task) and a `WaitHandle` a Module creates inside `run`. `navori-config` loads value by value and is the only crate that knows TOML; `navori-ffi` composes the Kernel and all user-facing wording. The Shell sends a wake pulse through `CoreStore` and shows the notice text it is given.

**Tech Stack:** Rust 2024, tokio 1.54.1 (`sync`, `time`; `test-util` in kernel tests), toml 1.1.8, serde_json, UniFFI 0.32, Swift (AppKit). No new third-party dependency. `serde_json` is added to `navori-config` from the workspace.

## Global Constraints

Source of truth (referenced, not copied — read before every task):

| Source | Content hash (`git hash-object`, first 12) |
|---|---|
| `docs/standards/INDEX.md` | `54e6a749595a` |
| `docs/standards/testing.md` | `b1a9b42fc1d3` |
| `docs/standards/errors-logging.md` | `c34f3f15f450` |
| `docs/standards/performance.md` | `b72898e354c6` |
| `docs/architecture/INDEX.md` (ARCH-N) | `2e318bd23ff5` |

Compact delta every task inherits:

- **ARCH-3** — `navori-kernel` gains no dependency on `navori-config` or `navori-macos`. `cargo xtask check-deps` is part of `cargo xtask ci`.
- **ARCH-9** — no new `unsafe` anywhere in this plan.
- **ARCH-13** — a bad config never stops the app.
- **ARCH-16** — the Scheduler spawns nothing. Timers are tokio's, owned by a handle local to `run`.
- **THR-3 at TB-6** — no raw config value and no `toml` or `serde` error text reaches a log, Event, Journal, notice or doctor check. `ConfigLoad` has a manual `impl Debug` that prints keys and counts only.
- **Testing** — test-first; tests in each crate's `tests/` through the public API; the clock is faked only through `WallClock`; test names are sentences in domain language; no requirement IDs in source or test names; secret-shaped fixtures are split with `concat!`.
- **Lints** — no `unwrap`/`expect` outside tests (`clippy.toml`); `missing_debug_implementations` is on, and `cargo xtask ci` denies warnings, so every new public type has `Debug`; a type holding `Arc<dyn WallClock>` or raw config values gets a manual `impl Debug`.
- **Commands** — task gate: `cargo nextest run -p <crate> --test <file>`; before each commit: `cargo xtask ci`. It is the only command that compiles the Swift Shell. Every commit builds.
- **Commits** — Conventional Commits, English, one logical change. Commit only when the user asked for the build.
- **Team** — Solo band (`docs/agents/project.md`): one contributor; no assignee or reviewer theater.

Consulted: `docs/codebase/modules.md` and `docs/codebase/dependencies.md` for placement. Nothing in them conflicts with the map below.

Retrieval (`load-subgraph`, schema 1.1, recipe `fsubr-1.2`, run once on the File Structure below; OWNS coverage 2/7): `blast_radius` → PERF on `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/Navori/App/AppDelegate.swift`, `docs/standards/performance.md`; PROCS on `docs/architecture/system.md`, `docs/codebase/modules.md`, `docs/ARCHITECTURE.md`. `cluster(KCONTRACT)` → focus only, because it had no OWNS before this file. Observations: none. Both neighbours are finished work, so this is no collision. CORE, SHELL, CLI and BUILD have no Files blocks, so this list does not prove nothing else owns these paths.

## File Structure

| Path | New / changed | Responsibility |
|---|---|---|
| `crates/kernel/src/scheduler.rs` | new | `Scheduler`, `WaitHandle`, `Wake`, `WallClock`, `SystemClock` |
| `crates/kernel/src/module.rs` | changed | `ModuleConfig`, `NoConfig`, `type Config`, `ctx.config()`, `ctx.wait_handle()` |
| `crates/kernel/src/runtime.rs` | changed | `Plumbing.scheduler`, `builder_with_clock`, `scheduler()`, `config_sections`, `config_issues`, `pause_journal_sweep`, launcher decodes config and republishes initial state |
| `crates/kernel/src/system.rs` | changed | `SweepPolicy`, `sweep_paused`, sweep on the wait handle |
| `crates/kernel/src/lib.rs` | changed | re-exports |
| `crates/kernel/tests/scheduler.rs` | new | wait handle behaviour with a fake wall clock and paused tokio time; `FakeClock` is defined here and again in `kernel.rs` |
| `crates/kernel/tests/kernel.rs` | changed | `Counter` takes `NoConfig`; Module config, supervision and sweep tests |
| `crates/config/src/lib.rs` | changed | `ConfigLoad` (manual `Debug`), `JournalSweep`, `ConfigReport`, `WholeFileReason`; `ConfigError` deleted |
| `crates/config/Cargo.toml` | changed | `serde_json.workspace = true` |
| `Cargo.lock` | changed | records that dependency edge |
| `crates/config/tests/config_file.rs` | changed | per-value load tests; replaces the two whole-file error tests |
| `crates/ffi/src/app.rs` | changed | `load_config`, `build_kernel`, `doctor`, `snapshot_pump`, `config_notice` |
| `crates/ffi/src/lib.rs` | changed | `ConfigNotice`, `CoreSnapshot.config_notice`, `system_did_wake`, `set_panel_visible` forwards |
| `crates/ffi/tests/core.rs` | changed | notice, sweep pause, panel coalescing and idempotent shutdown tests |
| `crates/ffi/tests/config_privacy.rs` | new | one test, in its own binary because it installs the global log subscriber |
| `apps/cli/tests/cli.rs` | changed | `doctor` fails on a bad value |
| `apps/macos/Navori/Core/CoreStore.swift` | changed | health reads `configNotice`; `systemDidWake()` |
| `apps/macos/Navori/Features/Panel/PanelView.swift` | changed | banner shows the notice |
| `apps/macos/Navori/Platform/WakeObserver.swift` | new | `NSWorkspace.didWakeNotification` observer |
| `apps/macos/Navori/App/AppDelegate.swift` | changed | owns the observer |
| `docs/standards/performance.md`, `docs/architecture/system.md`, `docs/architecture/runtime.md`, `docs/architecture/data.md`, `docs/codebase/modules.md`, `docs/ARCHITECTURE.md` | changed | wording and seeds the design names |

---

### Task 1: Wait for a wall-clock deadline inside `run`

**Files:**
- Create: `crates/kernel/src/scheduler.rs`
- Modify: `crates/kernel/src/module.rs`, `crates/kernel/src/runtime.rs`, `crates/kernel/src/lib.rs`
- Test: `crates/kernel/tests/scheduler.rs`

**Reuse:** none — new code (rung 7), because no existing type waits on a wall-clock deadline; built on rung 5 `tokio::sync::watch` and `tokio::time::sleep`

**Interfaces:**
- Consumes: `Plumbing`, `ModuleCtx`, `Kernel::builder` (existing)
- Produces: `pub trait WallClock: Send + Sync { fn now(&self) -> SystemTime; }`, `pub struct SystemClock`; `Kernel::builder_with_clock(store: Store, clock: Arc<dyn WallClock>) -> KernelBuilder`, with `Kernel::builder` delegating using `SystemClock`; `Kernel::scheduler(&self) -> &Scheduler`; `Scheduler::wait_handle(&self) -> WaitHandle`, `set_panel_visible(&self, bool)`, `panel_visible(&self) -> bool`, `wake_pulse(&self)`; `ModuleCtx::wait_handle(&self) -> WaitHandle`; `WaitHandle::wait_until(&mut self, Option<SystemTime>) -> Wake` (async, cancel-safe), `panel_visible(&self) -> bool`, `now(&self) -> SystemTime`; `pub enum Wake { Due, PanelChanged }`. This task implements loop steps 3, 4 (no pulse branch) and 6 of the design. `Inputs` already holds all three fields, and `PanelChanged` is not returned until Task 3.

**Depends-on:** none

- [x] **Step 1: Write failing tests** in `crates/kernel/tests/scheduler.rs` under `#[tokio::test(start_paused = true)]` with a test-local `FakeClock`, asserting "nothing yet" with `tokio::time::advance` and `futures::poll!`: `a_wait_returns_due_at_its_deadline`; `a_wait_with_no_deadline_stays_pending` (`tokio::time::advance` by 1 day, then `futures::poll!` is `Pending`; a `timeout` would auto-advance and prove nothing); `a_timer_that_ends_before_the_wall_deadline_arms_again` (wall clock held back, so no `Due` until it is advanced and tokio advances again); `a_wait_runs_in_the_same_select_as_recv` (probe Module answers a dispatched action while waiting, then still gets `Due`); `dropping_a_wait_and_waiting_again_still_returns_due`; `a_panicking_run_leaves_no_timer_armed_and_restarts_with_a_new_handle` (probe Module: after the panic, advancing past the old deadline gives no result from the old handle); `wait_handles_add_no_runtime_task` (`Handle::current().metrics().num_alive_tasks()` is equal with zero and with ten handles).
- [x] **Step 2: Run** `cargo nextest run -p navori-kernel --test scheduler` — expect: does not compile, `wait_handle` and `Wake` do not exist.
- [x] **Step 3: Implement** the Produces list. `Plumbing` and `ModuleCtx` each gain `scheduler: Scheduler`.
- [x] **Step 4: Run** the same command — expect: pass. `cargo nextest run -p navori-kernel` — expect: existing tests pass unchanged. That no timer exists on the `None` path cannot be observed through a public tokio API: confirm it by reading loop step 3, record that in the task report, and let Task 13's idle measurement back it.
- [x] **Step 5: Commit** `git commit -m "feat(kernel): let a module wait for a wall-clock deadline"`

_Requirements: KCONTRACT-4.1, KCONTRACT-4.2, KCONTRACT-4.3, KCONTRACT-4.4, KCONTRACT-4.5, KCONTRACT-4.6, KCONTRACT-4.7, KCONTRACT-4.8_

---

### Task 2: Keep timer wakes 30 seconds apart while the menu is closed

**Files:**
- Modify: `crates/kernel/src/scheduler.rs`
- Test: `crates/kernel/tests/scheduler.rs`

**Reuse:** none — new code (rung 7), because no existing type waits on a wall-clock deadline; built on rung 5 `tokio::sync::watch` and `tokio::time::sleep`

**Interfaces:**
- Consumes: `WaitHandle`, `Scheduler::set_panel_visible` (Task 1)
- Produces: `last_timer_wake: Option<tokio::time::Instant>` on `WaitHandle`, private; loop step 5

**Depends-on:** Task 1

- [x] **Step 1: Write failing tests:** `a_closed_menu_holds_timer_wakes_30_seconds_apart` (deadlines 1 s apart: second `Due` not before 30 s after the first, and at 30 s); `an_open_menu_applies_no_gap`; `the_first_timer_wake_is_not_delayed`; `dropping_a_wait_keeps_the_record_of_the_previous_timer_wake`.
- [x] **Step 2: Run** `cargo nextest run -p navori-kernel --test scheduler` — expect: the first test fails, `Due` arrives after 1 s.
- [x] **Step 3: Implement** loop step 5 with a 30-second constant.
- [x] **Step 4: Run** the same command — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(kernel): hold timer wakes 30 seconds apart while the menu is closed"`

_Requirements: KCONTRACT-4.5, KCONTRACT-4.9, KCONTRACT-4.10, KCONTRACT-4.11_

---

### Task 3: End a wait when the panel changes

**Files:**
- Modify: `crates/kernel/src/scheduler.rs`
- Test: `crates/kernel/tests/scheduler.rs`

**Reuse:** none — new code (rung 7), because no existing type waits on a wall-clock deadline; built on rung 5 `tokio::sync::watch` and `tokio::time::sleep`

**Interfaces:**
- Consumes: `WaitHandle`, `Scheduler::set_panel_visible` (Task 1), the gap (Task 2)
- Produces: `Wake::PanelChanged` from loop step 1; `WaitHandle::panel_visible` returning the current value

**Depends-on:** Task 2

- [x] **Step 1: Write failing tests:** `a_panel_change_ends_a_wait_with_a_deadline`; `a_panel_change_ends_a_wait_with_no_deadline`; `the_handle_reports_whether_the_panel_is_visible`; `a_panel_change_between_waits_is_not_lost`; `two_quick_panel_flips_still_end_the_wait`; `setting_the_same_visibility_changes_nothing`; `a_panel_change_does_not_count_toward_the_gap` (a timer wake, then a panel flip and back, then the next `Due` is still measured from the timer wake).
- [x] **Step 2: Run** `cargo nextest run -p navori-kernel --test scheduler` — expect: the first test fails, the wait does not end.
- [x] **Step 3: Implement** loop step 1 and the `panel_changes` counter.
- [x] **Step 4: Run** the same command — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(kernel): end a wait when the panel changes"`

_Requirements: KCONTRACT-5.1, KCONTRACT-5.2, KCONTRACT-5.3_

---

### Task 4: Return a deadline that passed during sleep when the Mac wakes

**Files:**
- Modify: `crates/kernel/src/scheduler.rs`
- Test: `crates/kernel/tests/scheduler.rs`

**Reuse:** none — new code (rung 7), because no existing type waits on a wall-clock deadline; built on rung 5 `tokio::sync::watch` and `tokio::time::sleep`

**Interfaces:**
- Consumes: `WaitHandle`, `Scheduler::wake_pulse` (Task 1), the gap (Task 2)
- Produces: loop steps 2 and 4 with the pulse branch. `seen_pulses` is recorded only once a deadline has answered the pulse.

**Depends-on:** Task 3

- [x] **Step 1: Write failing tests.** Sleep is simulated by advancing the fake wall clock without advancing tokio. `a_pulse_after_the_deadline_passed_in_sleep_returns_due`; `a_pulse_returns_due_without_waiting_for_the_gap`; `a_pulse_before_the_deadline_returns_nothing_and_waits_the_remaining_wall_time`; `a_pulse_with_no_deadline_keeps_waiting` (`futures::poll!` is `Pending`); `a_pulse_seen_with_no_deadline_answers_the_next_deadline_without_the_gap`; `with_no_pulse_a_deadline_across_sleep_returns_at_a_later_timer_wake`; `a_pulse_with_no_handle_is_recorded_without_error`; `a_wall_clock_moved_back_defers_due_and_moved_forward_brings_it_on`; `a_pulse_does_not_count_toward_the_gap`.
- [x] **Step 2: Run** `cargo nextest run -p navori-kernel --test scheduler` — expect: the first test fails, no `Due` until tokio time advances.
- [x] **Step 3: Implement** the pulse branch and `wake_pulse`.
- [x] **Step 4: Run** the same command — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(kernel): return an overdue deadline on a wake pulse"`

_Requirements: KCONTRACT-5.3, KCONTRACT-6.2, KCONTRACT-6.3, KCONTRACT-6.4, KCONTRACT-6.5, KCONTRACT-6.6, KCONTRACT-6.7_

---

### Task 5: Prove a wait never returns Due early

**Files:**
- Test: `crates/kernel/tests/scheduler.rs`

**Reuse:** existing — seeded `xorshift64` test generator pattern from `crates/macos/tests/procargs.rs` (rung 2), written inside the test file with no new test dependency

**Interfaces:**
- Consumes: `WaitHandle`, `Scheduler`, `FakeClock` (test-local)
- Produces: nothing

**Depends-on:** Task 4

- [x] **Step 1: Write the test** `due_is_never_returned_before_the_deadline_by_the_wall_clock`: a fixed seed, 1,000 runs, each a random sequence of tokio advances, fake wall advances and retreats, pulses and panel flips. Whenever `wait_until` returns `Due`, assert `clock.now() >= deadline`; on `PanelChanged`, wait again.
- [x] **Step 2: Run** `cargo nextest run -p navori-kernel --test scheduler due_is_never` — expect: pass. To check the test can fail, temporarily remove the step 6 wall-clock comparison, expect a failure, and restore it.
- [x] **Step 3: Commit** `git commit -m "test(kernel): prove a wait never returns due before its deadline"`

_Requirements: KCONTRACT-7.3_

---

### Task 6: Give a Module its own config

**Files:**
- Modify: `crates/kernel/src/module.rs`, `crates/kernel/src/runtime.rs`, `crates/kernel/src/system.rs`, `crates/kernel/src/lib.rs`
- Test: `crates/kernel/tests/kernel.rs`, `crates/kernel/tests/scheduler.rs`

**Reuse:** rung 2 — extends `Module`, `ModuleCtx` and `KernelBuilder` (`crates/kernel/src/module.rs:34,85`, `runtime.rs:77`)

**Interfaces:**
- Consumes: `ModuleCtx::wait_handle` (Task 1)
- Produces: `pub trait ModuleConfig: DeserializeOwned + Default + Send + Sync + 'static { fn check(&self) -> Result<(), &'static str> { Ok(()) } }`; `pub struct NoConfig` (decodes from any value through `serde::de::IgnoredAny`); `Module::Config: ModuleConfig`; `ModuleCtx::config(&self) -> Arc<M::Config>`; `KernelBuilder::config_sections(self, serde_json::Map<String, Value>) -> Self`; `Kernel::config_issues(&self) -> &[&'static str]`; launcher decodes in `start()`

**Depends-on:** Task 5, because it edits the probe Modules in `crates/kernel/tests/scheduler.rs`

- [x] **Step 1: Write failing tests** with a test Module in `crates/kernel/tests/kernel.rs` using config and a wait handle through the public API only: `a_module_outside_the_kernel_uses_config_and_a_wait_handle`; `a_module_reads_its_config_section`; `an_absent_section_gives_the_default`; `an_undecodable_section_gives_that_module_alone_the_default_and_is_listed` (one test Module stays configured); `a_section_failing_the_check_gives_the_default_and_is_listed`; `a_restarted_module_sees_the_same_config`; `a_module_with_no_config_ignores_a_section_with_its_id` (`config_issues` is empty for a `system` section holding `3`); `config_sections_may_be_set_after_register`; `a_restart_keeps_the_module_fields_state_and_queued_requests` (nothing asserts this today).
- [x] **Step 2: Run** `cargo nextest run -p navori-kernel --test kernel` — expect: does not compile, `ModuleConfig` does not exist.
- [x] **Step 3: Implement** the Produces list. `Counter`, `System` and the probe Modules in `crates/kernel/tests/scheduler.rs` gain `type Config = NoConfig;`. Decode errors are dropped, not logged.
- [x] **Step 4: Run** `cargo nextest run -p navori-kernel` — expect: pass, with no existing assertion changed, including `shutdown_stops_every_module`, `dispatch_routes_an_envelope_to_the_module_and_returns_its_answer`, `state_changes_bump_the_revision` and `a_module_that_keeps_crashing_is_marked_failed_and_unavailable`.
- [x] **Step 5: Commit** `git commit -m "feat(kernel): give each module its own config section"`

_Requirements: KCONTRACT-1.1, KCONTRACT-1.2, KCONTRACT-1.3, KCONTRACT-1.4, KCONTRACT-1.5, KCONTRACT-1.6, KCONTRACT-G1, KCONTRACT-G2, KCONTRACT-G3, KCONTRACT-G9_

---

### Task 7: Pause the sweep as a real state, on the wait handle

**Files:**
- Modify: `crates/kernel/src/system.rs`, `crates/kernel/src/runtime.rs`, `crates/kernel/src/lib.rs`
- Test: `crates/kernel/tests/kernel.rs`

**Reuse:** rung 2 — extends `System` (`crates/kernel/src/system.rs:55`) and the builder

**Interfaces:**
- Consumes: `WaitHandle`, `Kernel::builder_with_clock` (Task 1), launcher in `start()` (Task 6)
- Produces: `KernelBuilder::pause_journal_sweep(self) -> Self`; `SystemState { last_prune_removed, sweep_paused: bool }`; private `SweepPolicy::{KeepDays(u32), Paused}` in `Arc<Mutex<_>>`, default `KeepDays(30)`; launcher republishes `module.initial_state()` before it spawns

**Depends-on:** Task 6

- [x] **Step 1: Write failing tests:** `a_paused_sweep_deletes_nothing_at_start_or_after_six_hours` (fake clock and paused tokio advanced past 6 h; Event still present); `a_paused_sweep_is_reported_in_state_as_soon_as_start_returns`; `an_unset_builder_sweeps_at_30_days`; `a_retention_below_one_is_raised_to_one`; `a_later_sweep_runs_six_hours_after_the_last`; `one_long_sleep_gives_one_catch_up_sweep` (advance the fake wall clock 3 days, send a pulse, see the sweep; then seed another old Event and see it survive until the wall clock passes 6 more hours); `prune_journal_with_a_day_count_still_deletes_while_paused`; `a_failed_sweep_keeps_the_module_running` (drop `events` through `Store::with_conn` before start, then `ping` still answers). `System::prune` takes its cutoff from the real clock (`bus::now_ms`), so seeded Events are dated from the real clock. `the_builtin_system_module_answers_ping` and `the_system_module_prunes_the_journal_using_the_configured_retention` stay unchanged.
- [x] **Step 2: Run** `cargo nextest run -p navori-kernel --test kernel` — expect: does not compile, `pause_journal_sweep` does not exist.
- [x] **Step 3: Implement** the Produces list, and `run` keeps `next_sweep: Option<SystemTime>`: `Some(wait.now())` for `KeepDays`, `None` for `Paused`, and `wait.now()` plus 6 hours after each sweep. Keep the failed-sweep warning as it is.
- [x] **Step 4: Run** `cargo nextest run -p navori-kernel` — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(kernel): run the journal sweep on the wait handle and let it pause"`

_Requirements: KCONTRACT-3.1, KCONTRACT-3.2, KCONTRACT-3.4, KCONTRACT-3.7, KCONTRACT-3.9, KCONTRACT-G4, KCONTRACT-G10, KCONTRACT-G11_

---

### Task 8: Load config value by value

**Files:**
- Modify: `crates/config/src/lib.rs`, `crates/config/Cargo.toml`, `Cargo.lock`, `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs`
- Test: `crates/config/tests/config_file.rs`

**Reuse:** rung 2 — extends `Config::load` (`crates/config/src/lib.rs:125`); rung 5 — `toml::Table`, and `serde_json`, already a workspace dependency, added to this crate

**Interfaces:**
- Consumes: nothing new
- Produces: `Config::load(&Path) -> ConfigLoad`; `pub struct ConfigLoad { config, sweep: JournalSweep, sections: serde_json::Map<String, Value>, report: ConfigReport }` with a manual `Debug` that prints keys and counts only; `pub enum JournalSweep { KeepDays(u32), Paused }`; `pub enum ConfigReport { Clean, BadValues(Vec<String>), WholeFile(WholeFileReason) }`; `pub enum WholeFileReason { Unreadable, NotToml { position: Option<(usize, usize)> } }`. `ConfigError` is deleted. Until Task 9, `load_config` turns `report` into the existing `config_error` string, naming `config.toml` and keys only.

**Depends-on:** none

- [x] **Step 1: Write failing tests** in `crates/config/tests/config_file.rs`, replacing `out_of_range_values_are_rejected` and `malformed_toml_is_rejected_with_the_file_path`. One table-driven test `a_bad_value_falls_back_alone`: `log_level = "loud"` and `log_level = 42` each give `log_level` `info`, with `retention_days = 365` kept, `sweep` `KeepDays(365)` and `report` naming `log_level`; `retention_days = 0` and `retention_days = "365"` each name `journal.retention_days` with `log_level = "debug"` kept, retention 30 and `sweep` `Paused`; `journal = 3` names `journal` with `sweep` `Paused`. Also `invalid_toml_falls_back_as_a_whole_file` (`log_level = \n`: line 1, column not asserted, `sweep` `Paused`); `an_unreadable_file_falls_back_as_a_whole_file` (a directory at the path, and non-UTF-8 bytes; `sweep` `Paused`); `a_journal_table_without_the_key_counts_as_absent` (`[journal]` with the key commented out: `KeepDays(30)`, `Clean`); `module_sections_reach_the_map_and_core_keys_do_not`.
- [x] **Step 2: Rewrite** `missing_file_yields_defaults`, `partial_file_overrides_only_what_it_sets` and `unknown_keys_are_tolerated_for_forward_compatibility`: they drop `.unwrap()`, keep their value assertions on `.config`, and add `report` is `Clean`. The missing-file test also asserts `KeepDays(30)`.
- [x] **Step 3: Run** `cargo nextest run -p navori-config --test config_file` — expect: does not compile, `ConfigLoad` does not exist.
- [x] **Step 4: Implement** `navori-config` per the design, dropping `toml` and `serde` error text. A `NotToml` position comes from `Error::span()` over the raw text, with `None` when there is no span. Adapt `app::load_config` and the log line at `crates/ffi/src/lib.rs` (line 167).
- [x] **Step 5: Run** `cargo xtask ci` — expect: `✓ ci passed`; `an_invalid_config_falls_back_to_defaults_and_is_reported` still passes.
- [x] **Step 6: Commit** `git commit -m "feat(config): load config value by value"`

_Requirements: KCONTRACT-2.1, KCONTRACT-2.2, KCONTRACT-2.6, KCONTRACT-2.7, KCONTRACT-2.8, KCONTRACT-3.2, KCONTRACT-3.3, KCONTRACT-G5_

---

### Task 9: Name each bad value in the banner, health text and doctor

**Files:**
- Modify: `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs`, `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/Navori/Features/Panel/PanelView.swift`
- Test: `crates/ffi/tests/core.rs`, `crates/ffi/tests/config_privacy.rs`, `apps/cli/tests/cli.rs`

**Reuse:** rung 2 — extends `load_config`, `build_kernel`, `doctor`, `snapshot_pump` (`crates/ffi/src/app.rs`)

**Interfaces:**
- Consumes: `ConfigLoad` (Task 8); `KernelBuilder::config_sections`, `Kernel::config_issues` (Task 6)
- Produces: `pub fn config_notice(report: &ConfigReport, sweep: &JournalSweep, module_issues: &[&str], file: &Path) -> Option<ConfigNotice>` defined in `app.rs`, with `pub use app::config_notice;` in `lib.rs` and no UniFFI export; `#[derive(uniffi::Record)] pub struct ConfigNotice { title: String, message: String }`; `CoreSnapshot.config_notice`; Swift `snapshot.configNotice`. `config_error` is deleted, including `crates/ffi/tests/core.rs` (line 71).

**Depends-on:** Task 6, Task 8

- [x] **Step 1: Write failing tests** in `crates/ffi/tests/core.rs`: `a_bad_value_is_named_and_does_not_claim_the_whole_file` (notice holds `config.toml` and `log_level`, not `is on defaults`); `a_bad_module_section_is_named` (no Module in the Core has config yet, so this calls `navori_ffi::config_notice` directly; the path from a real section arrives with ROAD-8, and Task 6 proves `config_issues`); `invalid_toml_says_the_whole_file_is_on_defaults_and_names_it`, with and without a position; `an_unreadable_file_says_it_cannot_be_read`; `doctor_fails_the_config_check_on_a_bad_value`; `a_bad_value_does_not_stop_the_core`; `an_invalid_config_falls_back_to_defaults_and_is_reported` reads `config_notice`.
- [x] **Step 2: Write a failing CLI test** `doctor_fails_on_a_bad_config_value` in `apps/cli/tests/cli.rs`, adding a `running_core` variant that writes `config.toml` before the Core starts. `doctor_passes_on_a_healthy_core` stays.
- [x] **Step 3: Write a failing test** `a_secret_in_a_bad_value_reaches_no_report` in `crates/ffi/tests/config_privacy.rs`, in its own binary because `logging::init` is once per process. One home with `log_to_file: true`, and a `concat!`-split fixture first in a bad section, then, after shutdown and restart in the same home, in malformed TOML. The appender is non-blocking, so poll the `navori.*.log` file under `Library/Logs/Navori Dev` until the known line `core stopped` appears, then assert the secret is absent from the log, `recent_events`, the notice and `doctor`.
- [x] **Step 4: Run** `cargo nextest run -p navori-ffi` — expect: does not compile, `config_notice` does not exist.
- [x] **Step 5: Implement** `config_sections`, `config_notice` with the design's wording, `doctor` with `ok = false`, and `tracing::warn!` after `build_kernel` logging the notice text.
- [x] **Step 6: Implement the Shell:** `CoreStore.health` and `PanelView` read `configNotice`, keeping health's order (failed, then restarting, then config) and adding no wording of their own.
- [x] **Step 7: Run** `cargo xtask ci` — expect: `✓ ci passed`, including the Swift build.
- [x] **Step 8: Manual Shell check:** `cargo xtask run` with `log_level = "loud"` in the dev `config.toml`: the banner and health text name `log_level`, and Settings → Diagnostics shows the same `config` check. With broken TOML: they say the whole file is on defaults. Health order and the absence of Swift wording are confirmed by reading `CoreStore.health`. Record this in the task report.
- [x] **Step 9: Commit** `git commit -m "feat(ffi): name each bad config value in the notice and doctor"`

_Requirements: KCONTRACT-2.3, KCONTRACT-2.4, KCONTRACT-2.5, KCONTRACT-2.9, KCONTRACT-2.10, KCONTRACT-7.2, KCONTRACT-G7, KCONTRACT-G12_

---

### Task 10: Pause the sweep when the file says retention cannot be trusted

**Files:**
- Modify: `crates/ffi/src/app.rs`
- Test: `crates/ffi/tests/core.rs`

**Reuse:** rung 2 — extends `load_config`, `build_kernel`, `doctor`, `snapshot_pump` (`crates/ffi/src/app.rs`)

**Interfaces:**
- Consumes: `ConfigLoad.sweep`, `config_notice` (Tasks 8, 9); `KernelBuilder::journal_retention_days`, `pause_journal_sweep` (Task 7)
- Produces: `build_kernel` applies `JournalSweep`; the pause line in the notice and in `doctor`

**Depends-on:** Task 7, Task 9

- [x] **Step 1: Write failing tests** in `crates/ffi/tests/core.rs`. Each seeds an Event dated 40 days ago through `navori_store::Store` before start. A positive control first, `absent_retention_deletes_events_older_than_30_days`: no file, no `journal`, and `[journal]` with the key commented out each delete it within 1 second. Then a table-driven `a_broken_retention_or_file_keeps_old_events_and_says_the_sweep_is_paused`: `retention_days = 0`, `retention_days = "365"`, `journal = 3` and `log_level = \n` each keep the Event for that same 1 second, with a notice and a `doctor` check containing `Journal cleanup is paused`. The start-up sweep is not observable from here, so the deterministic proof that a paused sweep deletes nothing is Task 7's; this test proves the wiring. Also `a_valid_retention_keeps_what_it_says` with `retention_days = 90`; `a_bad_log_level_alone_does_not_pause_the_sweep`; `a_paused_sweep_leaves_other_modules_running` (`doctor`'s `modules` check passes, `ping` answers); `fixing_the_file_keeps_the_pause_until_the_next_start` (the notice still reports the pause after the file is fixed; after shutdown and restart there is no notice and the Event is deleted).
- [x] **Step 2: Run** `cargo nextest run -p navori-ffi --test core` — expect: the table test fails, the Event is deleted.
- [x] **Step 3: Implement** `build_kernel` calling `journal_retention_days` for `KeepDays` and `pause_journal_sweep` for `Paused`, and the pause line.
- [x] **Step 4: Run** `cargo xtask ci` — expect: `✓ ci passed`.
- [x] **Step 5: Manual Shell check:** `cargo xtask run` with `retention_days = 0`: the banner says cleanup is paused. Record this in the task report.
- [x] **Step 6: Commit** `git commit -m "feat(ffi): pause the journal sweep when retention cannot be trusted"`

_Requirements: KCONTRACT-3.2, KCONTRACT-3.3, KCONTRACT-3.5, KCONTRACT-3.6, KCONTRACT-3.7, KCONTRACT-3.8_

---

### Task 11: Send panel visibility and a wake pulse to the Scheduler

**Files:**
- Create: `apps/macos/Navori/Platform/WakeObserver.swift`
- Modify: `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs`, `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/Navori/App/AppDelegate.swift`
- Test: `crates/ffi/tests/core.rs`

**Reuse:** rung 2 — extends `load_config`, `build_kernel`, `doctor`, `snapshot_pump` (`crates/ffi/src/app.rs`); rung 2 — the `setPanelVisible` path (`apps/macos/Navori/Core/CoreStore.swift:57`); rung 4 — `NSWorkspace.shared.notificationCenter`

**Interfaces:**
- Consumes: `Kernel::scheduler`, `Scheduler::set_panel_visible`, `panel_visible`, `wake_pulse` (Task 1)
- Produces: `NavoriCore::system_did_wake(&self)`, which calls `wake_pulse` and logs one `info` line; `set_panel_visible` keeps its signature and forwards; `Inner.panel_visible` deleted, and `snapshot_pump` reads `kernel.scheduler().panel_visible()`; Swift `CoreStore.systemDidWake()`, `WakeObserver(onWake:)` registering on `NSWorkspace.shared.notificationCenter` with `queue: .main`

**Depends-on:** Task 1, Task 10

- [x] **Step 1: Write failing tests:** `snapshot_pushes_coalesce_at_33ms_with_the_panel_open_and_250ms_closed`: wait for quiet first (over 250 ms with no snapshot, because startup also bumps the revision), then change state through `prune_journal`; the listener sees it under 200 ms open and not before 240 ms closed. `a_system_wake_with_a_running_core_is_harmless` (a smoke test: the pulse's effect is proven in Task 4, and the Shell's half is manual); `calling_shutdown_twice_is_harmless`.
- [x] **Step 2: Run** `cargo nextest run -p navori-ffi --test core` — expect: does not compile, `system_did_wake` does not exist.
- [x] **Step 3: Implement** the Rust side, deleting `Inner.panel_visible`.
- [x] **Step 4: Implement** the Swift side. `AppDelegate` creates `WakeObserver` after `store.start()`, calling `store.systemDidWake()`, and drops it in `applicationWillTerminate`. `git diff --stat` shows `StatusPanel.swift` unchanged, so the panel path still goes through `CoreStore.setPanelVisible`.
- [x] **Step 5: Run** `cargo xtask ci` — expect: `✓ ci passed`.
- [x] **Step 6: Manual Shell check:** `cargo xtask run`, sleep the Mac and wake it, then confirm one wake log line in `navori.*.log` under `~/Library/Logs/Navori Dev` at the time `pmset -g log` shows the full `Wake`. Compare any `DarkWake` entries: this is evidence for the dark-wake open question, not a pass or fail. Record this in the task report.
- [x] **Step 7: Commit** `git commit -m "feat(macos): send panel visibility and wake pulses to the scheduler"`

_Requirements: KCONTRACT-5.4, KCONTRACT-6.1, KCONTRACT-G6, KCONTRACT-G8_

---

### Task 12: Bring the docs in line

**Files:**
- Modify: `docs/standards/performance.md`, `docs/architecture/system.md`, `docs/architecture/runtime.md`, `docs/architecture/data.md`, `docs/codebase/modules.md`, `docs/ARCHITECTURE.md`

**Reuse:** existing docs, edited in place

**Interfaces:**
- Consumes: the final names from Tasks 1–11
- Produces: nothing

**Depends-on:** Task 11

- [x] **Step 1: Edit** `docs/standards/performance.md` (line 41) and `docs/architecture/system.md` (line 60) to replace "paused during sleep" with what now holds: a timer parks on uptime, and an overdue timer may fire during dark wake. In `system.md`, set the Scheduler row to Built.
- [x] **Step 2: Edit** `docs/architecture/runtime.md` (line 54) for the sweep's pause, `docs/architecture/data.md` (lines 21 and 23) for per-value fallback and the pause, and `docs/codebase/modules.md` (lines 18 and 22) for the new surface.
- [x] **Step 3: Edit** the `docs/ARCHITECTURE.md` kernel seed (lines 70–103): delete the Scheduler bullet with its battery wording and the `Config` line, and correct `run(self, ctx)` and `ActionMeta` to the code. Keep `context()`, `tray()` and `ModuleHandle` for MILE-3. Add `system_did_wake` to the FFI surface near line 122.
- [x] **Step 4: Commit** `git commit -m "docs: record the scheduler, per-value config and sweep pause"`

_Requirements: none of its own; documentation for the IDs in Tasks 1–11._

---

### Task 13: Check the perf budget on the finished branch

**Files:**
- Test: `cargo xtask perf` (no file changes)

**Reuse:** existing — `cargo xtask perf` (rung 2)

**Interfaces:**
- Consumes: the finished branch
- Produces: the measured values for the task report

**Depends-on:** Task 12

- [x] **Step 1: Run** `cargo xtask perf` — expect: every gated line within budget, including `idle_wakeups_per_s` ≤ 1/s, `menu_open_after_launch_ms` ≤ 80 and `menu_reopen_ms` ≤ 50. `regression check skipped: no baseline` is expected while `perf-baseline.json` is absent.
- [x] **Step 2: Record** the printed values in the task report. A missed budget stops the plan and goes to the user: a budget is never loosened here.

_Requirements: KCONTRACT-7.1_
