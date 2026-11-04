# Tasks: Awake leases

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: AWAKE
Status: Approved
Date: 2026-11-02
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md
Delivered-by: af477352183df3e30c91594d737f2a958658cab3

**Goal:** Let the user keep the Mac awake by hand from the terminal and the menu, for a time, until turned off or for a command's life, and give the hook work a session Lease Action that needs no Module change.

**Architecture:** A new Module crate, `navori-mod-awake`, keeps a pure `LeaseTable`, a `Reconciler` over `PowerAssertions`, stored rows for process-owned Leases and a lazy `Platform`, all as fields of `self`, in one `select!` over the inbox, kqueue exits and one computed deadline. `navori-ffi` registers it, carries a typed `AwakeView` on `CoreSnapshot` and adds one `awake` doctor row. The CLI drives it over IPC; Swift renders it on MENU's cascade.

**Tech Stack:** Rust 2024, tokio 1.54.1 (`test-util` in tests), rusqlite 0.41 through `Store::with_conn`, schemars, clap 4, UniFFI 0.32, Swift (AppKit, SwiftUI). No new third-party dependency.

**Delivery:** Tasks 1–17 are Wave 1. Tasks 18–19 are Wave 2. MENU merged at `030774d`, so nothing waits on it; Task 18 needs only Task 4.

## Global Constraints

Source of truth (referenced, not copied — read before every task):

| Source | Content hash (`git hash-object`, first 12) |
|---|---|
| `docs/standards/INDEX.md` | `f83e09733ee4` |
| `docs/standards/testing.md` | `b1a9b42fc1d3` |
| `docs/standards/errors-logging.md` | `c34f3f15f450` |
| `docs/standards/performance.md` | `ebf1670ff150` |
| `docs/architecture/INDEX.md` (ARCH-N) | `2e318bd23ff5` |

Compact delta every task inherits:

- **Locks** — `.skills/AWAKE/close-package.md`, locks 1–19, and `design.md` Decisions 1–14 are settled. The `## Not locked` and `## Must measure` lists in `requirements.md` are not facts: do not build on them and do not write them into code comments as facts.
- **ARCH-3** — `navori-mod-awake` depends on no other Module. `cargo xtask check-deps` is part of `cargo xtask ci`.
- **ARCH-9** — no new `unsafe` in this plan.
- **ARCH-2** — no new Swift file imports `NavoriCore`. Only `CoreStore.swift` and `NavoriBanner` import it, and that stays so.
- **ARCH-16** — one `WaitHandle` per `run` entry; no `tokio::time::interval`. Time comes only from `wait.now()`.
- **THR-3, ARCH-6** — no argv, environment or cwd in a row, an assertion name, or a log. Event `<source>` is the Harness name, `1 hour`, `until off`, `display` or the executable name, never a project or branch name.
- **Testing** — test-first. Tests go in each crate's `tests/` through the public API, with real `Store::open_in_memory`. Fake only `PowerAssertions`, `ProcessSource`, `ExitSource` and `WallClock`. Test names are sentences in domain language. No requirement IDs in source or test names.
- **Lints** — no `unwrap`/`expect` outside tests. `missing_debug_implementations` is on, and CI denies warnings: every new public type has `Debug`, by hand when it holds `dyn` fields. Rust doc comments on UniFFI items never contain `*/`.
- **Commands** — task gate: `cargo nextest run -p <crate> --test <file>`. Before each commit: `cargo xtask ci`. It is the only command that compiles the Swift Shell, and it runs `swift test`.
- **Commits** — Conventional Commits, English, one logical change. Commit only when the user asked for the build.
- **Team** — Solo band (`docs/agents/project.md`): one contributor; no assignee or reviewer theater.

Consulted: `docs/codebase/map.md` (Approved) for layout, so the crate follows its `lib.rs`, `actor.rs`, `action.rs`, `state.rs`, `event.rs`, `domain/`, `infra/` shape. `design.md` Locality lines match. `context.rs` is omitted (lock 19). `docs/codebase/dependencies.md` allows every new edge.

Retrieval (`load-subgraph`, schema 1.1, recipe `fsubr-1.2`, run on the File Structure below at `c69c895`; OWNS coverage 4/9, since CORE, SHELL, CLI and BUILD have no `tasks.md`): `blast_radius` → KCONTRACT on `apps/cli/tests/cli.rs`, `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs`, `crates/ffi/tests/core.rs`, `CoreStore.swift`, `PanelView.swift` and docs; MENU on `CoreStore.swift`, `MenuRowView.swift`, `PanelView.swift`, `SettingsView.swift`, `StatusBarController.swift`, `docs/ARCHITECTURE.md`, `docs/design/2026-11-01-menubar-ui-brief.md`; PROCS on `crates/macos/src/fake/exits.rs`, `fake/power.rs`, `fake/processes.rs` and docs; PERF on `CoreStore.swift`, `StatusBarController.swift`. `cluster(AWAKE)` overlaps the same four. Observations: none. Advisory.

## File Structure

| Path | New / changed | Responsibility | Task |
|---|---|---|---|
| `crates/macos/src/fake/power.rs` | changed | `refuse_kind`, `allow`, `fail_release`, `panic_on_hold` | 1 |
| `crates/macos/src/fake/processes.rs` | changed | `list_calls`, `fail_list` | 1 |
| `crates/macos/src/fake/exits.rs` | changed | `close`, so a test can end the exit source | 1 |
| `crates/macos/tests/fakes.rs` | new | the new fake behavior; `refuse` still refuses every kind | 1 |
| `Cargo.toml`, `Cargo.lock` | changed | workspace `members`, `navori-mod-awake` path dependency | 2, 4 |
| `modules/awake/Cargo.toml` | new | crate manifest, `[lints] workspace = true` | 2 |
| `modules/awake/src/lib.rs` | new | `Awake`, `impl Module`, public exports | 2 |
| `modules/awake/src/actor.rs` | new | the `run` loop, settle, apply, publish | 2, 6, 8, 9, 10, 12, 13, 14, 15 |
| `modules/awake/src/action.rs` | new | `AwakeAction`, `impl ModuleAction` | 2, 3, 6, 8, 9, 11, 14 |
| `modules/awake/src/state.rs` | new | `AwakeState`, `LeaseState` | 2 |
| `modules/awake/src/event.rs` | new | Event kinds and summaries | 2, 9, 14 |
| `modules/awake/src/domain/mod.rs` | new | exports | 2 |
| `modules/awake/src/domain/table.rs` | new | `LeaseTable`, `LeaseSource`, `Lease`, `LeaseKey`, `EndReason`, `Ended`, `Settled`, `Reading`, `Wants`, `Timing` | 2, 3, 6, 8, 9, 11 |
| `modules/awake/src/domain/reconciler.rs` | new | `Reconciler`, `Held` | 2, 11, 12 |
| `modules/awake/src/infra/mod.rs` | new | exports | 2 |
| `modules/awake/src/infra/platform.rs` | new | `Platform`, lazy `ProcessSource` and exit source, `identify`, `resolve_session`, `read_descendants` | 2, 6, 8, 9 |
| `modules/awake/src/infra/stored.rs` | new | `awake_leases`, `migrate`, `load`, `upsert`, `delete`, `clear` | 10 |
| `modules/awake/tests/common/mod.rs` | new | `Rig`: fakes, `FakeClock`, Kernel, `dispatch`, `advance`, `reopen`, shorter `Timing`, fast `RestartPolicy` | 2, 9, 10, 13 |
| `modules/awake/tests/duration.rs` | new | story 1 | 2 |
| `modules/awake/tests/manual.rs` | new | story 2 | 3 |
| `modules/awake/tests/command.rs` | new | Command Lease | 6 |
| `modules/awake/tests/session.rs` | new | story 4 | 8 |
| `modules/awake/tests/stop.rs` | new | story 5 | 9 |
| `modules/awake/tests/restore.rs` | new | story 6 | 10 |
| `modules/awake/tests/display.rs` | new | story 7 | 11 |
| `modules/awake/tests/refusal.rs` | new | story 8 | 12 |
| `modules/awake/tests/restart.rs` | new | restart and permanent failure | 13 |
| `modules/awake/tests/actions.rs` | new | story 9 | 14 |
| `modules/awake/tests/idle.rs` | new | idle and timer rules | 15 |
| `modules/awake/tests/real_os.rs` | new | real `pmset`, `ProcWatcher` + `KqueueExits` | 6, 16 |
| `crates/ffi/Cargo.toml` | changed | `navori-mod-awake`; `navori-macos` with `fake` under `[dev-dependencies]` | 4 |
| `crates/ffi/src/app.rs` | changed | register Awake, `awake_view`, `awake` doctor row | 4, 12, 13 |
| `crates/ffi/src/lib.rs` | changed | `AwakeView`, `LeaseView`, `CoreSnapshot.awake`, re-exports `awake_view`, `awake_check` | 4 |
| `crates/ffi/tests/core.rs` | changed | Awake in a real core | 4 |
| `crates/ffi/tests/awake.rs` | new | `awake_view` and the doctor row with a fake Awake | 12, 13 |
| `apps/cli/src/main.rs` | changed | `navori awake hold`, `release-all`, `status`, `run` | 5, 7 |
| `apps/cli/tests/cli.rs` | changed | the `awake` commands against a real core | 5, 7 |
| `docs/ARCHITECTURE.md` | changed | seed: `ffi` feature text, "exactly one", "app exits", old names | 17 |
| `docs/architecture/integrations.md` | changed | at most one assertion per kind | 17 |
| `docs/architecture/data.md` | changed | what persists across a restart | 17 |
| `docs/architecture/system.md` | changed | Awake module row is Built | 17 |
| `docs/codebase/modules.md`, `docs/codebase/map.md`, `docs/codebase/dependencies.md` | changed | `navori-mod-awake` and `docs/features/` are Built | 17 |
| `docs/agents/verify.md`, `README.md` | changed | doctor rows include `awake`; `navori awake` commands | 17 |
| `docs/features/awake.md` | new | the user-facing note (DoD item 9) | 17, 19 |
| `.skills/AWAKE/measurements.md` | new, git-ignored | results for the Must-measure list | 16 |
| `apps/macos/NavoriPanel/AwakeRowState.swift` | new | pure label, dot and Submenu rules | 18 |
| `apps/macos/Tests/NavoriPanelTests/AwakeRowStateTests.swift` | new | those rules | 18 |
| `apps/macos/NavoriPanel/AwakeSubmenuState.swift` | new | pure status block and Holding row rules | 19 |
| `apps/macos/Tests/NavoriPanelTests/AwakeSubmenuStateTests.swift` | new | those rules | 19 |
| `apps/macos/Navori/Core/CoreStore.swift` | changed | plain `AwakeRowInput`, Awake Action calls | 18, 19 |
| `apps/macos/Navori/Features/Panel/PanelView.swift` | changed | Awake row above the Core row | 18 |
| `apps/macos/Navori/Features/Panel/MenuRowView.swift` | changed | `MenuRow.isDimmed`, defaulting to `false` | 19 |
| `apps/macos/Navori/StatusBar/StatusBarController.swift` | changed | `menuTree()`, the `submenu` closure, `activate`, `replaceTree` when the row's Submenu appears or goes | 18, 19 |
| `apps/macos/Navori/Features/Panel/AwakeSubmenuView.swift` | new | commands, status block, Holding | 19 |
| `apps/macos/Navori/Features/Settings/SettingsView.swift` | changed | doctor help text names `awake` | 19 |
| `docs/specs/2026-11-02-menubar-visual-foundation/requirements.md` | changed | MENU-1.2, MENU-1.4, "Comes back if" answered | 18 |
| `docs/design/2026-11-01-menubar-ui-brief.md` | changed | Surface 3 and level 0 as built | 19 |

---

### Task 1: Give the fakes per-kind refusal, recovery and failure

**Files:**
- Modify: `crates/macos/src/fake/power.rs`, `crates/macos/src/fake/processes.rs`, `crates/macos/src/fake/exits.rs`
- Test: `crates/macos/tests/fakes.rs`

**Reuse:** rung 2 — extends `FakePower` and `FakeProcesses`.

**Interfaces:**
- Consumes: `PowerAssertions`, `AssertionKind`, `PowerError`, `ProcessSource`
- Produces: `FakePower::refuse_kind(&self, AssertionKind, i32)`, `FakePower::allow(&self)`, `FakePower::fail_release(&self, i32)`, `FakePower::panic_on_hold(&self, usize)`, `FakeProcesses::list_calls(&self) -> usize`, `FakeProcesses::fail_list(&self, bool)`, `FakeExitsHandle::close(&self)`, which drops the sink so `ProcWatcher::next` returns `None`

**Depends-on:** none

- [x] **Step 1: Write failing tests:** `refuse_still_refuses_every_kind`, `a_refused_kind_leaves_the_other_kind_held`, `allow_lifts_a_refusal`, `a_failing_release_returns_its_code_and_still_counts_the_release`, `panic_on_hold_panics_that_many_times_then_holds`, `list_calls_counts_each_list`, `fail_list_makes_a_snapshot_fail`, `close_makes_the_watcher_return_none`.
Run: `cargo nextest run -p navori-macos --test fakes` — expect: no such method.
- [x] **Step 2: Implement.** `refuse` keeps its meaning. A release closure returns `PowerError::ReleaseFailed { code }` while `fail_release` is set.
Run: `cargo nextest run -p navori-macos` — expect: pass, including `crates/macos/tests/power.rs` unedited.
- [x] **Step 3: Commit** `git commit -m "test(macos): let the fakes refuse one kind, recover, and fail a release"`

_Requirements: AWAKE-12.12_

---

### Task 2: Keep the Mac awake for a fixed time

**Files:**
- Modify: `Cargo.toml`, `Cargo.lock`
- Create: `modules/awake/Cargo.toml`, `modules/awake/src/lib.rs`, `modules/awake/src/actor.rs`, `modules/awake/src/action.rs`, `modules/awake/src/state.rs`, `modules/awake/src/event.rs`, `modules/awake/src/domain/mod.rs`, `modules/awake/src/domain/table.rs`, `modules/awake/src/domain/reconciler.rs`, `modules/awake/src/infra/mod.rs`, `modules/awake/src/infra/platform.rs`
- Test: `modules/awake/tests/common/mod.rs`, `modules/awake/tests/duration.rs`

**Reuse:** rung 2 — `Module` and the `select!` shape of `System` (`crates/kernel/src/system.rs:107-138`), `WaitHandle`, `ctx.emit`. Also rung 2 — `PowerAssertions`, `AssertionGuard`, `AssertionKind` (`crates/macos/src/power.rs:29`), for the holding. The policy on top is new.

**Interfaces:**
- Consumes: `FakePower`, `FakeProcesses`, `FakeExitsHandle`, `Kernel::builder_with_clock`, `KernelBuilder::register`, `Kernel::dispatch`, `Envelope`, `Store::open_in_memory`, `WallClock`
- Produces: `Awake::new(Platform, impl Into<String>, Timing)`; `Platform::mac()`; `Platform::new(Arc<dyn PowerAssertions>, ProcessFactory, ExitsFactory)`, where `ProcessFactory = Box<dyn FnMut() -> Result<Arc<dyn ProcessSource>, PlatformError> + Send>` and `ExitsFactory = Box<dyn FnMut(ExitSink) -> Result<Box<dyn ExitSource>, PlatformError> + Send>`; `Timing { grace, recheck, cap, session_ttl }`, `Timing::FIXED`; `AwakeAction::{Status, HoldFor { seconds }, ReleaseAll}`; `AwakeState`, `LeaseState` as in `design.md`, deriving `Serialize, Deserialize, Clone, PartialEq, Debug`; `LeaseTable`, `LeaseTable::settle(now) -> Settled`, `Wants`, `Reconciler::new`, `Reconciler::apply(Wants) -> Held`; `Timing::FIXED` as 120 s, 60 s, 2 h, 8 h; Event kinds `awake.lease_started`, `awake.lease_ended`, emitted from here on through `event.rs`. Test helper `Rig { kernel, power, processes, exits, clock }` with `Rig::new()`, `Rig::dispatch(action: Value) -> Result<Value, DispatchError>`, `Rig::advance(Duration)`, and `FakeClock`, copied in shape from `crates/kernel/tests/scheduler.rs:38-72`. `ExitsFactory` in the Rig clones the handle per build: `move |sink| exits.clone().factory()(sink)`. `common/mod.rs` opens with `#![allow(dead_code, clippy::unwrap_used)]`, because each test binary uses part of it.

**Depends-on:** Task 1

- [x] **Step 1: Write failing tests** on paused tokio time with `FakePower`: `a_one_hour_lease_holds_one_system_assertion_with_the_given_name`, `two_duration_requests_keep_one_lease_and_restart_the_hour`, `the_lease_ends_at_its_deadline_and_releases_the_assertion`, `with_the_menu_closed_a_lease_ends_within_30_seconds_of_its_deadline`, `sleeping_past_the_deadline_ends_the_lease_on_wake` (a wall-clock jump plus `Scheduler` wake pulse), `sleep_time_is_subtracted_from_the_duration` (a jump before the deadline leaves the absolute deadline unchanged), `a_zero_or_over_7_day_duration_is_invalid`, `status_answers_the_state`.
Run: `cargo nextest run -p navori-mod-awake --test duration` — expect: crate does not exist.
- [x] **Step 2: Implement** the crate. `actor.rs` computes `let deadline = self.deadline();` before `select!`, ignores `Wake::PanelChanged`, runs `settle(now)`, then `apply`, at the top of every loop turn, and calls `update_state` only when `AwakeState` changed. `Reconciler::apply` computes from its guard fields each call, and uses explicit `release()`. Add `"modules/awake"` to workspace `members` and `navori-mod-awake` to `[workspace.dependencies]`.
Run: `cargo nextest run -p navori-mod-awake --test duration` — expect: pass.
- [x] **Step 3: Run** `cargo xtask check-deps` — expect: pass with no `xtask` edit.
- [x] **Step 4: Commit** `git commit -m "feat(awake): keep the Mac awake for a fixed time"`

_Requirements: AWAKE-1.1, AWAKE-1.2, AWAKE-1.3, AWAKE-1.4, AWAKE-1.5, AWAKE-1.6, AWAKE-1.7_

---

### Task 3: Keep the Mac awake until turned off, then Release all

**Files:**
- Modify: `modules/awake/src/action.rs`, `modules/awake/src/domain/table.rs`
- Test: `modules/awake/tests/manual.rs`

**Reuse:** none — new code (rung 7). No existing type models a Lease.

**Interfaces:**
- Consumes: `Rig`, `AwakeAction`
- Produces: `AwakeAction::HoldUntilOff`

**Depends-on:** Task 2

- [x] **Step 1: Write failing tests:** `an_until_off_lease_has_no_deadline_and_holds_past_any_hour`, `until_off_replaces_a_duration_lease`, `a_duration_request_replaces_an_until_off_lease`, `release_all_ends_every_lease_and_releases_the_assertion`, `the_action_catalog_has_no_pause_and_no_sleep_action` (reads `AwakeAction::catalog()` names), `release_all_is_the_only_catalog_action_for_ending_manual_leases` (the `display` Action joins that list in Task 11).
Run: `cargo nextest run -p navori-mod-awake --test manual` — expect: no such variant.
- [x] **Step 2: Implement.**
Run: same command — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(awake): hold until turned off and release everything"`

_Requirements: AWAKE-2.1, AWAKE-2.2, AWAKE-2.6, AWAKE-2.7_

---

### Task 4: Register Awake and read it from the snapshot

**Files:**
- Modify: `Cargo.lock`, `crates/ffi/Cargo.toml`, `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs`
- Test: `crates/ffi/tests/core.rs`

**Reuse:** rung 2 — `Kernel::state`, `Kernel::modules`, `snapshot()`, `Inner::doctor`.

**Interfaces:**
- Consumes: `Awake::new`, `Platform::mac`, `Timing::FIXED`, `AwakeState`, `LeaseState`
- Produces: `CoreSnapshot.awake: Option<AwakeView>`; `AwakeView { leases: Vec<LeaseView>, system_held: bool, display_held: bool, since_ms: Option<i64>, problem: Option<String> }`; `LeaseView { key, kind, harness: Option<String>, label: Option<String>, pid: Option<i32>, stopped: bool, ends_at_ms: Option<i64> }`; `pub fn awake_view(&Kernel) -> Option<AwakeView>` and `pub fn awake_check(Option<&AwakeView>) -> Check` in `app.rs`, re-exported from `crates/ffi/src/lib.rs`, because `mod app` is private (`lib.rs:10`); `build_kernel` gains a `name: &str` argument

**Depends-on:** Task 2

- [x] **Step 1: Write failing tests** in `crates/ffi/tests/core.rs`: `the_snapshot_carries_awake_after_a_hold`, `an_idle_core_reports_a_passing_awake_row_and_keeps_the_five_others`, `a_stray_awake_table_in_config_raises_no_notice`, `an_idle_core_holds_no_assertion` (the snapshot's `system_held` is false and status answers no Leases), `the_system_module_still_runs_beside_awake`, `a_lease_in_the_dev_profile_is_absent_from_the_release_profile` (two cores on one home, each with its own socket and database).
Run: `cargo nextest run -p navori-ffi --test core` — expect: no field `awake`.
- [x] **Step 2: Implement.** `navori-macos` goes under `[dev-dependencies]` with `fake`; `Platform::mac()` lives in the Module. `From<AwakeState> for AwakeView` destructures with no `..`. `awake_view` deserializes `Kernel::state("awake")` and returns `None` when that fails. Name the assertion `Navori: keeping this Mac awake` or `Navori Dev: keeping this Mac awake` by Profile. Add the `awake` row after `modules`.
Run: `cargo nextest run -p navori-ffi` and `cargo nextest run -p navori-cli` — expect: pass, with no other test edited.
- [x] **Step 3: Verify** `cargo tree -p navori-mod-awake | grep -c uniffi` — expect: `0`. `crates/kernel/tests/kernel.rs` still passes unedited, which guards restart with backoff.
- [x] **Step 4: Commit** `git commit -m "feat(ffi): register Awake and carry its record on the snapshot"`

_Requirements: AWAKE-5.13, AWAKE-10.1, AWAKE-10.2, AWAKE-10.3, AWAKE-10.4, AWAKE-10.6, AWAKE-12.1, AWAKE-12.2, AWAKE-12.3, AWAKE-12.4, AWAKE-12.10, AWAKE-12.11_

---

### Task 5: Drive Awake from the terminal

**Files:**
- Modify: `apps/cli/src/main.rs`
- Test: `apps/cli/tests/cli.rs`

**Reuse:** rung 2 — `Client`, `Failure`, the clap `Command` enum (`apps/cli/src/main.rs:28`); rung 3 — `std::process::Command`, so no new tokio feature.

**Interfaces:**
- Consumes: `AwakeAction` JSON shapes as literals, since the CLI cannot import the Module: `"status"`, `{"hold_for":{"seconds":N}}`, `"hold_until_off"`, `"release_all"`
- Produces: `navori awake hold [--for <Ns|Nm|Nh>]`, `navori awake release-all`, `navori awake status [--json]`

**Depends-on:** Task 3, Task 4

- [x] **Step 1: Write failing tests:** `awake_hold_for_an_hour_shows_in_status`, `awake_hold_with_no_flag_is_until_off`, `awake_release_all_ends_the_lease`, `a_bad_duration_is_a_usage_error` (exit 2), `awake_status_with_the_app_down_exits_3`.
Run: `cargo nextest run -p navori-cli --test cli` — expect: unrecognized subcommand.
- [x] **Step 2: Implement** the subcommands and a small `parse_duration`, with no new dependency. Status prints the Lease count, then one line per Lease, and says when a Lease exists with `system_held` false.
Run: same command — expect: pass.
- [x] **Step 3: Run** `cargo xtask check-deps` — expect: pass, with the CLI still on `navori-config`, `navori-ipc`, `navori-protocol`.
- [x] **Step 4: Commit** `git commit -m "feat(cli): add navori awake hold, release-all and status"`

_Requirements: AWAKE-9.6, AWAKE-12.5, AWAKE-12.6_

---

### Task 6: Hold a Lease for the life of a process

**Files:**
- Modify: `modules/awake/src/action.rs`, `modules/awake/src/actor.rs`, `modules/awake/src/domain/table.rs`, `modules/awake/src/infra/platform.rs`
- Test: `modules/awake/tests/command.rs`, `modules/awake/tests/real_os.rs`

**Reuse:** rung 2 — `Snapshot`, `harness_of`, `resolve_location`, `ProcWatcher`, `KqueueExits`, `MacProcesses` (`crates/procs/src/`, `crates/macos/src/`). The real `ProcWatcher` plus `KqueueExits` pairing has never run together in the repo, so Task 6 proves it.

**Interfaces:**
- Consumes: `ProcWatcher::new`, `watch`, `unwatch`, `next`, `ProcessSource::entry`, `ProcId`, `StartTime`
- Produces: `AwakeAction::HoldCommand { pid: i32 }`; `Platform::identify(i32) -> Result<Option<ProcId>, PlatformError>`; `LeaseTable::leases_on(ProcId) -> usize`; `LeaseTable::on_exit(ProcId) -> Vec<Ended>`

**Depends-on:** Task 3

- [x] **Step 1: Write failing tests** with `FakeProcesses` and `FakeExitsHandle`: `a_command_lease_holds_until_its_process_exits`, `a_command_lease_has_no_deadline`, `a_gone_process_answers_ok_with_no_lease`, `a_zombie_process_answers_ok_with_no_lease`, `a_process_of_another_user_is_refused`, `a_non_positive_pid_is_invalid`, `kqueue_rejecting_the_watch_answers_failed_with_no_lease`, `two_leases_on_one_process_unwatch_only_at_the_last`, `no_exit_source_is_built_until_a_process_lease_exists`, `a_dead_exit_source_records_a_problem_and_does_not_spin_then_rebuilds_on_the_timer` (through `FakeExitsHandle::close`).
Run: `cargo nextest run -p navori-mod-awake --test command` — expect: no such variant.
- [x] **Step 2: Implement.** `watcher_next` is pending while `self.watcher` is `None`. On `None` from `next`, drop the watcher, record the `exits` problem and rebuild on the 60-second timer. Filter `entry` by uid and zombie.
Run: same command — expect: pass.
- [x] **Step 3: Write and run a real-OS test** in `real_os.rs`, `a_real_child_exit_reaches_the_module`: `Platform::mac()`, spawn `sleep 1`, `hold_command`, wait for `leases` to empty. This is the first time real `ProcWatcher` and `KqueueExits` run together; a failure here is a design finding, so stop and report.
Run: `cargo nextest run -p navori-mod-awake --test real_os` — expect: pass.
- [x] **Step 4: Commit** `git commit -m "feat(awake): hold a Lease for the life of a process"`

_Requirements: AWAKE-3.2, AWAKE-3.3, AWAKE-3.4, AWAKE-3.9_

---

### Task 7: Run a command under a Lease

**Files:**
- Modify: `apps/cli/src/main.rs`
- Test: `apps/cli/tests/cli.rs`

**Reuse:** rung 2 — `Client`, `Failure`, the clap `Command` enum (`apps/cli/src/main.rs:28`); rung 3 — `std::process::Command`, so no new tokio feature.

**Interfaces:**
- Consumes: `{"hold_command":{"pid":N}}`, `Client::connect`, `tokio::time::timeout`
- Produces: `navori awake run -- <cmd> [args...]`, returning its own `ExitCode` before the `Client::connect` at `apps/cli/src/main.rs:127`

**Depends-on:** Task 5, Task 6

- [x] **Step 1: Write failing tests:** `awake_run_holds_while_the_command_runs` (checks `status` from a second call while `sleep` runs), `awake_run_exits_with_the_commands_code` (table: 0, 3, 7), `awake_run_with_the_app_down_still_runs_and_warns_once_on_stderr` (asserts a marker file, exit code and exactly one stderr line `navori: no Lease held because Navori is not running`), `awake_run_with_a_missing_command_exits_127`, `awake_run_on_a_non_executable_file_exits_126`, `awake_run_exits_128_plus_the_signal`, `awake_run_with_the_app_down_creates_no_socket`. That the CLI never opens the app is also checked in Step 3.
Run: `cargo nextest run -p navori-cli --test cli` — expect: unrecognized subcommand.
- [x] **Step 2: Implement.** Spawn with inherited stdio, connect, dispatch under a 5-second timeout, wait. Arguments are `Vec<OsString>` with `#[arg(last = true, required = true)]`. Install no signal handler. Any other dispatch failure writes `navori: no Lease held: <reason>`.
Run: same command — expect: pass.
- [x] **Step 3: Verify** `grep -n '"open"\|NSWorkspace\|launchctl' apps/cli/src/main.rs` — expect: no match.
- [x] **Step 4: Commit** `git commit -m "feat(cli): add navori awake run"`

_Requirements: AWAKE-3.1, AWAKE-3.5, AWAKE-3.6, AWAKE-3.7, AWAKE-3.8_

---

### Task 8: Hold and end a Lease for one session

**Files:**
- Modify: `modules/awake/src/action.rs`, `modules/awake/src/actor.rs`, `modules/awake/src/domain/table.rs`, `modules/awake/src/infra/platform.rs`
- Test: `modules/awake/tests/session.rs`

**Reuse:** rung 2 — `Snapshot`, `harness_of`, `resolve_location`, `ProcWatcher`, `KqueueExits`, `MacProcesses` (`crates/procs/src/`, `crates/macos/src/`). The real `ProcWatcher` plus `KqueueExits` pairing has never run together in the repo, so Task 6 proves it.

**Interfaces:**
- Consumes: `harness_of`, `HarnessLookup`, `Snapshot::take`, `Snapshot::cwd`, `resolve_location`
- Produces: `AwakeAction::HoldSession { session_id, pid, ppid }`, `AwakeAction::EndSession { session_id }`; `Platform::resolve_session(i32, i32) -> Result<(Harness, ProcId, Option<String>), Rejected>`; `Rejected::{NotRunning, NotAHarness, Platform}`; session key `session:<session_id>`

**Depends-on:** Task 6

- [x] **Step 1: Write failing tests**, with a chain `claude(100) -> sh(101) -> navori(102)`, as in `crates/procs/tests/harness_of.rs:36-39`: `a_session_lease_watches_the_harness_not_the_hook`, `a_dead_hook_pid_falls_back_to_its_parent`, `a_hold_from_a_process_outside_a_harness_is_invalid_with_no_lease`, `a_hold_with_both_pids_gone_is_invalid_with_no_lease`, `a_repeated_hold_keeps_one_lease_moves_the_deadline_and_keeps_the_first_hold`, `end_session_ends_it_by_session_id_with_no_process_read` (asserts `list_calls` is unchanged), `the_harness_exiting_ends_the_lease`, `the_session_ttl_ends_the_lease_8_hours_after_the_latest_hold`, `a_session_id_outside_the_allowed_characters_is_invalid` (table: empty, 129 bytes, a space, a slash), `a_hold_answers_without_waiting_for_grace`.
Run: `cargo nextest run -p navori-mod-awake --test session` — expect: no such variant.
- [x] **Step 2: Implement.** Stamp First hold from `wait.now()` on the Action's arrival, truncated to microseconds, before `spawn_blocking`. Try `pid`, then `ppid`.
Run: same command — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(awake): hold and end a Lease for one session"`

_Requirements: AWAKE-4.1, AWAKE-4.2, AWAKE-4.3, AWAKE-4.4, AWAKE-4.5, AWAKE-4.6, AWAKE-4.7, AWAKE-4.8, AWAKE-4.9, AWAKE-4.10, AWAKE-13.5_

---

### Task 9: Release a stopped session, or keep it while work remains

**Files:**
- Modify: `modules/awake/src/action.rs`, `modules/awake/src/actor.rs`, `modules/awake/src/event.rs`, `modules/awake/src/domain/table.rs`, `modules/awake/src/infra/platform.rs`, `modules/awake/tests/common/mod.rs`
- Test: `modules/awake/tests/stop.rs`

**Reuse:** none — new code (rung 7). No existing type models a Lease.

**Interfaces:**
- Consumes: `Snapshot::descendants`, `Process.id.start`, `StartTime`
- Produces: `AwakeAction::StopSession { session_id }`; `Platform::read_descendants(ProcId, SystemTime) -> Reading`; `LeaseTable::resolve_check`; the Cap's Event summary in `event.rs`

**Depends-on:** Task 8

- [x] **Step 1: Write failing tests:** `only_old_descendants_end_the_lease_after_grace`, `a_child_started_at_the_first_hold_is_not_busy`, `a_new_descendant_keeps_the_lease_and_it_is_read_again_60_seconds_on`, `the_lease_ends_at_the_next_read_after_the_new_descendant_exits`, `the_cap_ends_the_lease_with_a_new_descendant_alive`, `the_cap_event_states_the_reason`, `a_repeated_stop_keeps_the_stop_mark`, `a_stop_for_an_unknown_session_changes_nothing`, `a_hold_during_grace_clears_the_stop_mark_and_the_lease_stays`, `a_stop_hold_stop_sequence_counts_the_cap_from_the_second_stop`, `a_snapshot_that_cannot_be_taken_keeps_the_lease_until_the_cap` (through `FakeProcesses::fail_list`), `no_cpu_time_is_read` (asserts `FakeProcesses::reads` holds no `cpu`), `tests_can_pass_shorter_timing`, `the_fixed_timing_is_120_60_7200_and_28800_seconds`.
Run: `cargo nextest run -p navori-mod-awake --test stop` — expect: no such variant.
- [x] **Step 2: Implement.** A `Timing` shorter than 30 seconds is floored by the closed-menu gap, so tests of it call `kernel.scheduler().set_panel_visible(true)`.
Run: same command — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(awake): release a stopped session unless new work remains"`

_Requirements: AWAKE-5.1, AWAKE-5.2, AWAKE-5.3, AWAKE-5.4, AWAKE-5.5, AWAKE-5.6, AWAKE-5.7, AWAKE-5.8, AWAKE-5.9, AWAKE-5.10, AWAKE-5.11, AWAKE-5.12, AWAKE-5.14_

---

### Task 10: Keep a job's Lease across an app reopen

**Files:**
- Create: `modules/awake/src/infra/stored.rs`
- Modify: `modules/awake/src/actor.rs`, `modules/awake/src/infra/mod.rs`, `modules/awake/tests/common/mod.rs`
- Test: `modules/awake/tests/restore.rs`

**Reuse:** rung 2 — `Store::migrate`, `Store::with_conn` (`crates/store/src/lib.rs:119,128`).

**Interfaces:**
- Consumes: `Store`, `Migration`, `ctx.store`, `ProcessSource::entry`, `ProcWatcher::watch`
- Produces: Owner `awake`, table `awake_leases` as in `design.md`; `stored::{migrate, load, upsert, delete, clear}`, `StoredLease`. Test helper `Rig::reopen()`: shuts the Kernel down and starts a new one on the same `Store` and fakes.

**Depends-on:** Task 9

- [x] **Step 1: Write failing tests:** `a_session_lease_row_stores_pid_start_first_hold_stop_mark_and_deadline`, `a_command_lease_row_has_no_end_time`, `a_manual_lease_writes_no_row_and_is_gone_after_reopen`, `closing_the_app_ends_manual_leases_and_releases_the_assertion` (`Kernel::shutdown`, then `FakePower::held` is empty), `a_live_process_is_restored_and_the_assertion_returns`, `a_gone_process_deletes_its_row_and_never_holds`, `a_reused_pid_deletes_its_row_and_never_holds`, `a_zombie_deletes_its_row`, `an_expired_session_ends_on_reopen`, `a_passed_cap_ends_on_reopen_with_the_cap_event`, `grace_counts_from_the_stored_stop_mark_after_reopen`, `a_restored_lease_ends_when_its_process_exits`, `release_all_deletes_every_row_and_reopen_restores_nothing`, `a_failed_release_all_delete_changes_nothing_and_answers_failed`, `a_failed_row_write_keeps_the_lease_and_retries_on_the_timer`, `an_empty_table_takes_no_snapshot_on_open`, `a_row_holds_no_argv_environment_or_cwd` (reads every column, with `FakeProcess` argv and cwd set), `the_assertion_name_holds_no_session_text`.
Run: `cargo nextest run -p navori-mod-awake --test restore` — expect: no module `stored`.
- [x] **Step 2: Implement.** Create `wait` first, since time comes only from it. Migrate on every `run` entry. `restored` on `self`, set after the last row. Insert by key. `release_all` runs the `DELETE` before any memory change. Failed upserts go in a dirty set, flushed on the 60-second timer. Platform build failure leaves rows on disk and records the `platform` problem. For the store failure tests, drop the table through `Store::with_conn`.
Run: same command — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(awake): keep process-owned Leases across an app reopen"`

_Requirements: AWAKE-2.3, AWAKE-2.4, AWAKE-6.1, AWAKE-6.2, AWAKE-6.3, AWAKE-6.4, AWAKE-6.5, AWAKE-6.6, AWAKE-6.7, AWAKE-6.8, AWAKE-6.9, AWAKE-6.10, AWAKE-13.6_

---

### Task 11: Keep the display on with its own Lease

**Files:**
- Modify: `modules/awake/src/action.rs`, `modules/awake/src/domain/table.rs`, `modules/awake/src/domain/reconciler.rs`
- Test: `modules/awake/tests/display.rs`

**Reuse:** rung 2 — `PowerAssertions`, `AssertionGuard`, `AssertionKind` (`crates/macos/src/power.rs:29`), for the holding. The policy on top is new.

**Interfaces:**
- Consumes: `FakePower::refuse_kind`, `Rig`
- Produces: `AwakeAction::Display { on: bool }`; `LeaseSource::Display`, key `display`

**Depends-on:** Task 10

- [x] **Step 1: Write failing tests:** `display_on_holds_one_display_and_one_system_assertion`, `display_off_with_another_lease_keeps_system_with_no_gap` (`FakePower::releases` is 1 with System still in `held`), `display_on_with_no_other_lease_holds_system_too`, `display_off_with_no_other_lease_releases_both`, `a_refused_display_keeps_system_and_reports_display_not_held`, `a_refused_system_still_tries_display`, `release_all_ends_the_display_lease`, `display_writes_no_row`, `display_off_is_the_only_other_action_that_ends_a_manual_lease`.
Run: `cargo nextest run -p navori-mod-awake --test display` — expect: no such variant.
- [x] **Step 2: Implement.** System holds before Display; Display releases before System.
Run: same command — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(awake): keep the display on with its own Lease"`

_Requirements: AWAKE-7.1, AWAKE-7.2, AWAKE-7.3, AWAKE-7.4, AWAKE-7.5_

---

### Task 12: Say when macOS refuses to keep the Mac awake

**Files:**
- Modify: `modules/awake/src/actor.rs`, `modules/awake/src/domain/reconciler.rs`, `crates/ffi/src/app.rs`
- Test: `modules/awake/tests/refusal.rs`, `crates/ffi/tests/awake.rs`

**Reuse:** rung 2 — `Kernel::state`, `Kernel::modules`, `snapshot()`, `Inner::doctor`.

**Interfaces:**
- Consumes: `FakePower::refuse`, `allow`, `fail_release`, `awake_view`, `awake_check`
- Produces: internal `Problems { system, display, release, store, platform, exits }`, surfaced as `AwakeState.problem`; the retry time joins `self.deadline()`

**Depends-on:** Task 4, Task 11

- [x] **Step 1: Write failing tests:** in `refusal.rs`: `a_refused_system_keeps_the_lease_and_records_the_problem`, `a_hold_answers_with_system_held_false`, `refusal_retries_about_every_60_seconds`, `a_successful_retry_clears_only_that_problem`, `no_retry_with_no_lease`, `a_failed_explicit_release_records_the_problem`, `a_store_problem_survives_a_system_recovery`. In `crates/ffi/tests/awake.rs`, on a Kernel with a fake Awake: `the_awake_row_fails_while_system_is_refused`, `the_view_never_reports_held_while_refused`.
Run: `cargo nextest run -p navori-mod-awake --test refusal` and `cargo nextest run -p navori-ffi --test awake` — expect: fail on the assertions.
- [x] **Step 2: Implement.**
Run: same commands — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(awake): report and retry a refused assertion"`

_Requirements: AWAKE-8.1, AWAKE-8.2, AWAKE-8.3, AWAKE-8.4, AWAKE-8.5, AWAKE-8.6, AWAKE-8.7, AWAKE-8.8_

---

### Task 13: Survive a restart and a permanent failure

**Files:**
- Modify: `modules/awake/src/actor.rs`, `modules/awake/tests/common/mod.rs`, `crates/ffi/src/app.rs`
- Test: `modules/awake/tests/restart.rs`, `crates/ffi/tests/awake.rs`

**Reuse:** rung 2 — `Module` and the `select!` shape of `System` (`crates/kernel/src/system.rs:107-138`), `WaitHandle`, `ctx.emit`.

**Interfaces:**
- Consumes: `FakePower::panic_on_hold`, `Kernel::modules`, `ModuleState`
- Produces: `awake_view` reports nothing held, with problem `The Awake module stopped.`, while the lifecycle is `failed` or `stopped`

**Depends-on:** Task 12

- [x] **Step 1: Write failing tests:** `a_panic_keeps_the_leases_and_the_held_assertion`, `a_panic_between_a_table_change_and_reconcile_is_repaired_on_reentry` (one Lease held, `panic_on_hold(1)`, Display on, the dispatch answers `Unavailable`, then assert both held after restart), `a_panic_mid_restore_restores_every_row_on_reentry`, `permanent_failure_releases_every_assertion` (a fast `RestartPolicy` through `KernelBuilder::restart_policy`). In `crates/ffi/tests/awake.rs`: `a_failed_module_is_never_reported_as_holding`, `the_awake_row_fails_when_the_module_failed`.
Run: `cargo nextest run -p navori-mod-awake --test restart` and `cargo nextest run -p navori-ffi --test awake` — expect: fail on the assertions.
- [x] **Step 2: Implement.** Every `run` entry: migrate, restore unless restored, settle, reconcile, flush dirty rows, publish.
Run: same commands — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(awake): repair after a restart and never claim a hold after failing"`

_Requirements: AWAKE-8.9, AWAKE-13.8, AWAKE-13.9_

---

### Task 14: Describe Awake's Actions and record what it did

**Files:**
- Modify: `modules/awake/src/action.rs`, `modules/awake/src/actor.rs`, `modules/awake/src/event.rs`
- Test: `modules/awake/tests/actions.rs`

**Reuse:** rung 2 — `Module` and the `select!` shape of `System` (`crates/kernel/src/system.rs:107-138`), `WaitHandle`, `ctx.emit`.

**Interfaces:**
- Consumes: `ActionDoc`, `Risk`, `ctx.emit`, `Kernel::actions`
- Produces: every summary from the table in `design.md`

**Depends-on:** Task 13

- [x] **Step 1: Write failing tests:** `every_action_has_a_description_a_schema_and_a_risk`, `every_catalog_name_decodes_as_an_action` (checks each name against a sample payload, since nothing ties them today), `status_is_read_and_the_others_are_write`, `no_action_is_destructive`, `a_started_lease_emits_one_sentence` and `an_ended_lease_emits_one_sentence` (table: deadline, process exit, grace, Cap, Release all, literal strings from `design.md`), `no_event_names_a_project_or_a_branch`.
Run: `cargo nextest run -p navori-mod-awake --test actions` — expect: fail on the assertions.
- [x] **Step 2: Implement.**
Run: same command — expect: pass.
- [x] **Step 3: Verify** `cargo xtask check-deps` — expect: pass. `grep -rn 'unsafe' modules/awake/src` — expect: no match. `git diff main -- crates/ffi apps/cli | grep -n '^+.*unsafe'` — expect: no match.
- [x] **Step 4: Commit** `git commit -m "feat(awake): describe its Actions and record its Events"`

_Requirements: AWAKE-9.1, AWAKE-9.2, AWAKE-9.3, AWAKE-9.4, AWAKE-9.5, AWAKE-13.7, AWAKE-13.11_

---

### Task 15: Cost nothing while idle

**Files:**
- Modify: `modules/awake/src/actor.rs`, only for a defect these tests find
- Test: `modules/awake/tests/idle.rs`

**Reuse:** rung 2 — `Module` and the `select!` shape of `System` (`crates/kernel/src/system.rs:107-138`), `WaitHandle`, `ctx.emit`.

**Interfaces:**
- Consumes: `FakeProcesses::list_calls`, `Rig`, `Scheduler::set_panel_visible`
- Produces: none

**Depends-on:** Task 14

- [x] **Step 1: Write tests:** `an_hour_with_no_lease_reads_nothing_and_calls_nothing`, `opening_the_panel_takes_no_snapshot` (with a session Lease held: `list_calls` unchanged across a panel toggle), `with_the_menu_closed_two_timer_wakes_are_30_seconds_apart`. Fix any defect they find in `modules/awake/src/actor.rs`.
Run: `cargo nextest run -p navori-mod-awake --test idle` — expect: pass.
- [x] **Step 2: Run** `cargo xtask perf` with no Lease held — expect: every GOAL-11 budget passes, including `submenu_open_ms` ≤ 16 ms on the Core Submenu. No `perf-baseline.json` exists unless PERF Task 12 ran, so the regression check may skip and print why; report that. Do not record a baseline.
- [x] **Step 3: Commit** `git commit -m "test(awake): prove the idle Module reads nothing and arms nothing"`

_Requirements: AWAKE-13.1, AWAKE-13.2, AWAKE-13.3, AWAKE-13.4_

---

### Task 16: Measure against real macOS

**Files:**
- Test: `modules/awake/tests/real_os.rs`
- Create: `.skills/AWAKE/measurements.md`

**Reuse:** rung 2 — `PowerAssertions`, `AssertionGuard`, `AssertionKind` (`crates/macos/src/power.rs:29`), for the holding. The policy on top is new.

**Interfaces:**
- Consumes: `Platform::mac()`, `pmset -g assertions`, the built `navori` binary
- Produces: `.skills/AWAKE/measurements.md`, one line per Must-measure item: result, or who still owes it. Real-OS tests take real IOKit assertions, so they need a normal login session, not a sandbox.

**Depends-on:** Task 7, Task 13, Task 14

- [x] **Step 1: Write real-OS tests:** `pmset_lists_one_system_assertion_for_a_lease`, `pmset_lists_system_and_display_together` (first real test of two at once), `releasing_display_leaves_system_listed`, `ending_the_last_lease_clears_pmset`. Filter by the test's own unique assertion name, because other apps hold assertions too.
Run: `cargo nextest run -p navori-mod-awake --test real_os` — expect: pass.
- [x] **Step 2: Measure by hand on `cargo xtask run`, and record each:** assertion gone after `kill -9` of the app · a running `awake run` Lease across an app quit and reopen, with recovery time · `kill -9` on the CLI with the child alive · `Snapshot::take` cost on this Mac in a release build · real session ids from Claude Code, Codex and Grok against `[A-Za-z0-9._:-]`, from each tool's own docs or a harmless hook-free source.
- [x] **Step 3: Record what stays open, and who owes it:** Display alone stopping idle sleep · a Lease sleeping through its deadline · timers in dark wake (each needs a real sleep; the user) · a real `Stop` trace, `caffeinate` at `Stop`, background work after `Stop` (ROAD-9, which installs hooks). Do not invent results.
- [x] **Step 4: Commit** the test only: `git commit -m "test(awake): prove assertions against pmset"`

_Requirements: AWAKE-1.2, AWAKE-1.7, AWAKE-7.1, AWAKE-7.3_

---

### Task 17: Bring the docs in line

**Files:**
- Create: `docs/features/awake.md`
- Modify: `docs/ARCHITECTURE.md`, `docs/architecture/integrations.md`, `docs/architecture/data.md`, `docs/architecture/system.md`, `docs/codebase/modules.md`, `docs/codebase/map.md`, `docs/codebase/dependencies.md`, `docs/agents/verify.md`, `README.md`

**Reuse:** none — new code (rung 7). Docs have no reusable target.

**Interfaces:**
- Consumes: the built commands and Events
- Produces: none

**Depends-on:** Task 16

- [x] **Step 1: Edit.** `integrations.md:26` and `ARCHITECTURE.md:296` become at most one assertion per kind. `ARCHITECTURE.md:297` says the assertion follows the process, and rows for process-owned Leases survive. `ARCHITECTURE.md:154` describes the record in `navori-ffi`. Replace `ProcessTable`, `ProjectResolver` and `HarnessCatalog` at `ARCHITECTURE.md:284-286` and `:307` with `Snapshot`, `resolve_location` and `harness_of`. `data.md:33` states what persists. `system.md:64`, `modules.md`, `map.md` and `dependencies.md` say Built. `verify.md` and `README.md:27` list `awake`, and `verify.md` gains a Drive entry for `navori awake`. Do not touch `docs/standards/errors-logging.md`: its line 30 already says every Module has a doctor check.
- [x] **Step 2: Write** `docs/features/awake.md`: what it does, the commands, what it does not do, the DoD item 5 gap in the wording of `requirements.md`, and which Must-measure items stay open, since `.skills/AWAKE/measurements.md` is git-ignored.
- [x] **Step 3: Verify** `grep -rnE "exactly one|ProcessTable|HarnessCatalog|ProjectResolver" docs/ARCHITECTURE.md docs/architecture/integrations.md` — expect: no match on an assertion line, and none of the three old names. `grep -n "Comes back if" -c docs/features/awake.md` — expect: 0.
- [x] **Step 4: Commit** `git commit -m "docs(awake): align the seed, spine and feature note with what shipped"`

_Requirements: AWAKE-12.1_

---

### Task 18: Show the Awake row at level 0

Wave 2.

**Files:**
- Create: `apps/macos/NavoriPanel/AwakeRowState.swift`
- Modify: `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/Navori/Features/Panel/PanelView.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`, `docs/specs/2026-11-02-menubar-visual-foundation/requirements.md`
- Test: `apps/macos/Tests/NavoriPanelTests/AwakeRowStateTests.swift`

**Reuse:** rung 2 — `MenuRow`, `CascadeNode`, `CascadeContent`, `CoreRowState` pattern, `CoreStore.dispatch`; new (rung 7): `AwakeRowState` in `NavoriPanel`, `AwakeSubmenuView`, `StatusBlockView`.

**Interfaces:**
- Consumes: `CoreSnapshot.awake`, `AwakeView`, `MenuRow { id, icon, title, value, dot, opensSubmenu, shortcut }`, `CascadeNode(id:isActionable:children:)`, `StatusDot`
- Produces: `AwakeRowState(status: AwakeStatus)` with `label`, `dot`, `hasSubmenu`; `AwakeStatus` as `.off`, `.on(sources: [String])`, `.notHeld`, `.unavailable`; `CoreStore.awakeStatus: AwakeStatus`; row id `awake`, icon `cup.and.saucer`, title `Awake`

**Depends-on:** Task 4

- [x] **Step 1: Write failing tests:** table over the four statuses: `Off` with no dot; green dot and `On · Claude Code, 1 hour`; orange dot and `Not held`; `Unavailable` with no Submenu.
Run `cargo xtask build-core` first, so `apps/macos/Generated` carries `AwakeView`.
Run: `swift test --package-path apps/macos --filter AwakeRowStateTests` — expect: no such type.
- [x] **Step 2: Implement** the state and `CoreStore.awakeStatus`, mapping `snapshot?.awake` only inside `CoreStore`. Add the row directly above `coreRow` in `PanelView`. Add `awake` at the head of `menuTree()`, with `store.hasSnapshot ? children : []` as `core` does, and matching ids. Change the `submenu` closure's `guard id == "core"` (`StatusBarController.swift:19-22`) to a switch returning `EmptyView()` for `awake` until Task 19. Call `panel.cascade.replaceTree()` when the row's `hasSubmenu` flips, beside the one-shot `didReplaceCoreTree`. No new file imports `NavoriCore`. Do not edit `Features/Preview/`.
- [x] **Step 3: Amend MENU's spec in place, keeping its IDs,** so its `tasks.md` citations stay live: MENU-1.2's order gains the Awake row, noting `amended by AWAKE-11.1`; MENU-1.4 stops listing the Awake row, noting the same; answer "The Awake row ships" with: the Core row stays. Leave MENU's `Status: Implemented` and its ticked Task 18, which records what was true at delivery.
- [x] **Step 4: Verify** `swift test --package-path apps/macos --filter AwakeRowStateTests` — expect: pass. `cargo xtask ci` — expect: pass. `grep -rln 'import NavoriCore' apps/macos --include='*.swift' | grep -v Generated | grep -v Tests` — expect: only `Core/CoreStore.swift` and `NavoriBanner/CoreStartupBanner.swift`. By hand on `cargo xtask run`: level 0 order is banners, Awake, Core, separator, Settings, About, Quit; `DEV` is on the Core row; the Core Submenu still shows modules, now including `awake`, and at most 8 Events. `cargo xtask perf` — expect: `submenu_open_ms` ≤ 16 ms, still opening `core`, and later opens ≤ 50 ms with the extra row.
- [x] **Step 5: Commit** `git commit -m "feat(menu): show the Awake row above the Core row"`

_Requirements: AWAKE-11.1, AWAKE-11.2, AWAKE-11.3, AWAKE-11.4, AWAKE-12.7, AWAKE-12.8, AWAKE-12.9_

---

### Task 19: Keep the Mac awake from the menu

Wave 2.

**Files:**
- Create: `apps/macos/Navori/Features/Panel/AwakeSubmenuView.swift`, `apps/macos/NavoriPanel/AwakeSubmenuState.swift`
- Modify: `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/Navori/Features/Panel/MenuRowView.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/Navori/Features/Settings/SettingsView.swift`, `docs/design/2026-11-01-menubar-ui-brief.md`, `docs/features/awake.md`
- Test: `apps/macos/Tests/NavoriPanelTests/AwakeSubmenuStateTests.swift`

**Reuse:** rung 2 — `MenuRow`, `CascadeNode`, `CascadeContent`, `CoreRowState` pattern, `CoreStore.dispatch`; new (rung 7): `AwakeRowState` in `NavoriPanel`, `AwakeSubmenuView`, `StatusBlockView`.

**Interfaces:**
- Consumes: `CoreStore.dispatch(module:actionJSON:)`, `AwakeView`, `LeaseView`
- Produces: `MenuRow.isDimmed: Bool = false`; tree ids `awake.hold_hour`, `awake.hold_until_off`, `awake.display`, `awake.release_all`; `CoreStore.holdOneHour()`, `holdUntilOff()`, `setDisplay(on:)`, `releaseAll()`; constant `AwakeSubmenuState.menuHoldSeconds = 3600`; `AwakeSubmenuState(status:leases:displayHeld:displayWanted:)` with `statusTitle`, `statusDetail`, `releaseAllEnabled`, `displayLabel`, `holdingRows: [HoldingRow]`; `HoldingRow { line1, line2, accessibilitySentence }`, built from a plain `AwakeLeaseInput` that `CoreStore` maps from `LeaseView`

**Depends-on:** Task 5, Task 12, Task 17, Task 18

- [x] **Step 1: Write failing tests:** `the_menu_hold_is_one_hour`; a table over each status block state and Holding row in `design.md`, with its exact text; `release_all_is_disabled_with_no_lease`; `no_text_says_the_mac_will_sleep_or_names_another_app`.
Run: `swift test --package-path apps/macos --filter AwakeSubmenuStateTests` — expect: no such type.
- [x] **Step 2: Implement** the Submenu from `design.md` `## UI design`, with its exact text. Commands are `MenuRowView` rows, using `@Environment(CascadeController.self)` as `PanelView` does, and the four ids are the `awake` node's children, each `isActionable: true`. `activate` gains four cases that call `CoreStore` in a `Task` and leave the Cascade open, logging a failed dispatch through `CoreStore`'s `log`. `Release all` is always shown; with no Lease it is dimmed and its `activate` case does nothing. Holding rows are not tree nodes. The `awake` case of the `submenu` closure returns `AwakeSubmenuView`. Nothing here reads the process table or the system's assertions.
- [x] **Step 3: Update** the brief's Surface 3 and level 0 to what shipped, removing `Pause 15m`, `Let my Mac sleep now`, per-Lease release and the two level 3 rows. Add `awake` to the doctor help text at `SettingsView.swift:129`. Add the menu to `docs/features/awake.md`.
- [x] **Step 4: Verify** `swift test --package-path apps/macos --filter AwakeSubmenuStateTests` — expect: pass. `cargo xtask ci` — expect: pass. `grep -nE "will sleep|Pause|sleep now|another app|other app" apps/macos/Navori/Features/Panel/AwakeSubmenuView.swift apps/macos/NavoriPanel/AwakeRowState.swift apps/macos/NavoriPanel/AwakeSubmenuState.swift` — expect: no match. In review of the diff, no Awake Swift code decodes JSON: state comes from `AwakeView`, and `dispatch` answers are ignored. By hand on `cargo xtask run`: run story 11's demo; ↑ and ↓ reach all four commands; VoiceOver reads each row and button as one sentence; each state in `design.md` shows, using `navori dispatch` to reach it.
- [x] **Step 5: Commit** `git commit -m "feat(menu): keep the Mac awake from the Awake Submenu"`

_Requirements: AWAKE-10.5, AWAKE-11.6, AWAKE-11.7, AWAKE-11.8, AWAKE-11.9, AWAKE-11.10, AWAKE-13.10_

### Task 20: Show the locked Awake menu

Amendment after delivery, from `ui-brief.md`.

**Files:**
- Create: `apps/macos/Navori/Features/Panel/HarnessMarkImage.swift`, `docs/specs/2026-11-02-awake-leases/ui-brief.md`, `docs/specs/2026-11-02-awake-leases/ui-brief.png`
- Modify: `apps/macos/NavoriPanel/AwakeRowState.swift`, `apps/macos/NavoriPanel/AwakeSubmenuState.swift`, `apps/macos/Navori/Features/Panel/AwakeSubmenuView.swift`, `apps/macos/Navori/Features/Panel/MenuRowView.swift`, `apps/macos/Navori/Core/CoreStore.swift`, `docs/design/2026-11-01-menubar-ui-brief.md`, `docs/features/awake.md`
- Test: `apps/macos/Tests/NavoriPanelTests/AwakeRowStateTests.swift`, `apps/macos/Tests/NavoriPanelTests/AwakeSubmenuStateTests.swift`, `apps/macos/Tests/NavoriPanelTests/AwakeLeaseMapTests.swift`, `apps/macos/Tests/NavoriBannerTests/CoreStartupBannerTests.swift`

**Reuse:** rung 2 — `MenuRowView`, `AwakeRowState`, `AwakeSubmenuState`; new (rung 7): `HoldingList`, `HoldingLine`, `HoldingMarkView`, `AwakeProblemView`, `HarnessMarkImage`.

**Interfaces:**
- Consumes: `CoreStore.awakeStatus`, `awakeLeases`, `displayHeld`, `awakeActionMessage`
- Produces: `MenuRow.isChecked: Bool = false`; `AwakeSubmenuState(status:leases:displayHeld:actionFailure:)` with `problem: AwakeProblem?`, `releaseAllEnabled`, `hourChecked`, `hourUntil`, `untilOffChecked`, `displayChecked`, `holdingGroups: [HoldingGroup]`; `HoldingRow { mark, name, word, showsDot, isQuiet, help, accessibilitySentence }`. Removes `AwakeBlockStatus`, `StatusBlockView` and `CoreStore.awakeBlockStatus`.

**Depends-on:** Task 18, Task 19

- [x] **Step 1: Write failing tests** for the counted level-0 sources and `Needs attention`, then for the problem line, checks, group order, row order, marks and row text. Run: `swift test --package-path apps/macos --filter AwakeRowStateTests` — expect: the new expectations fail.
- [x] **Step 2: Implement** both states and the view from `design.md` `## UI design`.
- [x] **Step 3: Update** the menubar brief, `docs/features/awake.md` and this spec to what shipped.
- [x] **Step 4: Verify** `cargo xtask ci` — expect: pass. `grep -nE "will sleep|Pause|sleep now|another app|other app"` over the three Awake UI files — expect: no match.
- [ ] **Step 5: Verify by hand** on `cargo xtask run`: ↑ and ↓ reach all four commands, pressing `Display on` twice ends its Lease, VoiceOver reads each row as one sentence, and each state in `design.md` shows.
- [x] **Step 6: Commit** `git commit -m "feat(menu): group Awake's Holding list by project with Harness marks"`

_Requirements: AWAKE-11.11, AWAKE-11.12, AWAKE-11.13, AWAKE-11.14, AWAKE-11.15, AWAKE-11.17, AWAKE-11.18_

### Task 21: Show the Awake row as a dot

Amendment after delivery, from `ui-brief.md` amendment 6.

**Files:**
- Modify: `apps/macos/NavoriPanel/AwakeRowState.swift`, `apps/macos/NavoriPanel/AwakeSubmenuState.swift`, `apps/macos/Navori/Features/Panel/MenuRowView.swift`, `apps/macos/Navori/Features/Panel/PanelView.swift`, `docs/design/2026-11-01-menubar-ui-brief.md`, `docs/features/awake.md`
- Test: `apps/macos/Tests/NavoriPanelTests/AwakeRowStateTests.swift`, `apps/macos/Tests/NavoriPanelTests/AwakeSubmenuStateTests.swift`

**Reuse:** rung 2 — `MenuRow`, `AwakeRowState`.

**Interfaces:**
- Consumes: `CoreStore.awakeStatus`, `awakeActionMessage`
- Produces: `MenuRow.spokenValue: String?`; `AwakeStatus.problem(String, held: Bool)`. `AwakeRowState.label` is now read only by VoiceOver.

**Depends-on:** Task 20

- [x] **Step 1: Write failing tests** for the dot in each status, including a problem with and without a held Lease, and a failed action. Run: `swift build --package-path apps/macos --build-tests` — expect: `held` does not exist yet.
- [x] **Step 2: Implement** the dot rule and `spokenValue`; the Awake row passes no `value`.
- [x] **Step 3: Update** the design, both briefs and `docs/features/awake.md`.
- [x] **Step 4: Verify** `cargo xtask ci` — expect: pass.
- [ ] **Step 5: Verify by hand** on `cargo xtask run`: the row shows a green dot with a Lease and nothing after `Release all`; VoiceOver reads the state in words.
- [x] **Step 6: Commit** `git commit -m "feat(menu): show the Awake row as a dot"`

_Requirements: AWAKE-11.2, AWAKE-11.3, AWAKE-11.4, AWAKE-11.17, AWAKE-11.18, AWAKE-11.21_

### Task 22: Keep awake lengths and Tracking agents

Amendment after delivery, from stories 14 and 15 and `ui-brief.md` amendment 7.

**Files:**
- Create: `apps/macos/NavoriPanel/KeepAwakeChoice.swift`
- Modify: `modules/awake/src/action.rs`, `modules/awake/src/actor.rs`, `modules/awake/src/domain/table.rs`, `modules/awake/src/event.rs`, `modules/awake/src/infra/stored.rs`, `modules/awake/src/lib.rs`, `modules/awake/src/state.rs`, `crates/ffi/src/lib.rs`, `crates/ffi/src/app.rs`, `apps/cli/src/main.rs`, `apps/macos/NavoriPanel/AwakeRowState.swift`, `apps/macos/NavoriPanel/AwakeSubmenuState.swift`, `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/Navori/Features/Panel/AwakeSubmenuView.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/Navori/StatusBar/CascadePanel.swift`, `apps/macos/NavoriPanel/CascadePlacement.swift`, `docs/design/2026-11-01-menubar-ui-brief.md`, `docs/features/awake.md`, `docs/agents/verify.md`, `CONTEXT.md`
- Test: `modules/awake/tests/keep.rs`, `modules/awake/tests/tracking.rs`, `crates/ffi/tests/core.rs`, `apps/cli/tests/cli.rs`, `apps/macos/Tests/NavoriPanelTests/AwakeSubmenuStateTests.swift`, `apps/macos/Tests/NavoriPanelTests/AwakeRowStateTests.swift`, `apps/macos/Tests/NavoriPanelTests/CascadeStateTests.swift`, `apps/macos/Tests/NavoriPanelTests/CascadePlacementTests.swift`

**Reuse:** rung 2 — `LeaseTable`, `Store::kv_set`, `MenuRowView`, `CascadeNode`; new (rung 7): `KeepAwakeChoice`, `AwakeCommand`, `KeepAwakeSubmenuView`.

**Interfaces:**
- Consumes: `CoreStore.awakeStatus`, `awakeLeases`, `awakeActionMessage`
- Produces: Actions `keep_for`, `keep_indefinitely`, `keep_off`, `tracking`; `AwakeState.tracking`, `AwakeView.tracking`; `AwakeSubmenuState(status:leases:displayHeld:tracking:actionFailure:)` with `keepValue`, `trackingOn`, `trackingValue`, `trackingDot`, `isChecked(_:)`, `command(picking:)`; `CoreStore.awakeSubmenu`, `CoreStore.perform(_:)`. Removes `releaseAllEnabled`, `hourChecked`, `hourUntil`, `untilOffChecked`, `displayChecked`, `menuHoldSeconds`, `AwakeActionFailure.Request`, `AwakeHeldMarks.hasLease`.

**Depends-on:** Task 21

- [x] **Step 1: Write the tests** for the choices, the checks, the setting, a followed session and the third cascade level.
- [x] **Step 2: Implement** the Module, FFI, CLI and Shell changes from `design.md` Decisions 15–20.
- [x] **Step 3: Update** the design, both briefs and the feature docs.
- [x] **Step 4: Verify** `cargo xtask ci` — expect: pass.
- [ ] **Step 5: Verify by hand** on `cargo xtask run`: the third level opens and each length checks and shows `until <time>`; pressing a check ends it; `Tracking agents` off lets `pmset -g assertions` drop a session's hold with a real agent running, and on brings it back at once; VoiceOver reads each row.
- [x] **Step 6: Commit** `git commit -m "feat(awake): add Keep awake lengths and a Tracking agents toggle"`

_Requirements: AWAKE-14.1, AWAKE-14.2, AWAKE-14.3, AWAKE-14.4, AWAKE-14.5, AWAKE-14.6, AWAKE-14.7, AWAKE-14.8, AWAKE-14.9, AWAKE-14.10, AWAKE-14.11, AWAKE-15.1, AWAKE-15.2, AWAKE-15.3, AWAKE-15.4, AWAKE-15.5, AWAKE-15.6, AWAKE-15.7, AWAKE-15.8, AWAKE-15.9, AWAKE-15.10, AWAKE-15.11, AWAKE-15.12, AWAKE-15.13, AWAKE-15.14, AWAKE-15.15, AWAKE-15.16_
