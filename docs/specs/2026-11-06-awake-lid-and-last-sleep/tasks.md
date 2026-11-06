# Tasks: Awake with the lid closed, and the last sleep

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: LID
Status: Approved
Approved by the operator's command `/shape #34 --auto --go` (relayed by the conductor navori-e7 in .skills/_pending-awake-lid/auto/operator-reply.md, 2026-11-06 11:47 +07).
Approval: the operator's /shape --go, before reading the spec
Date: 2026-11-06
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

<!--
This plan was written in an unattended run under `.skills/_pending-awake-lid/auto/charter.md`.
- requirements.md, design.md and this file were approved under the operator's `/shape #34 --auto --go`, before they read the spec (see the Status lines).
- Per the charter, plan-tasks publishes no issue (ticket #34 is the only ticket) and offers no execution route in this run.
-->

**Goal:** Let `Lid closed` keep the Mac awake with the lid shut while a Lease holds and the adapter is in, give lid sleep back on every exit path, and show when the Mac last slept at the top of the Running submenu.

**Architecture:**
- `navori-macos` gains two platform seams with fakes: `LidSetting` (selector 12 on `IOPMrootDomain`, a `LidGuard` and `Last Sleep Reason`) and `PowerSource` (IOPS plus a notify(3) fd).
- The Awake actor owns the switch, the `lid_held` marker, the hold, the start-up restore, the power watch (`AsyncFd` in its `select!`) and the last sleep. It publishes them on `AwakeState`.
- `navori-ffi` mirrors them on `AwakeView`, forwards will-sleep and adds a `lid` doctor row. The CLI gets `navori awake lid on|off`. The Swift Shell draws a `Lid closed` row and a static `Last sleep` row from the snapshot.

**Tech Stack:**
- Rust 2024 (pinned toolchain), with crates already in use: `objc2-io-kit` 0.3.2 (adding feature `ps`), `objc2-core-foundation`, `libc`, `tokio` (`net` → `AsyncFd`), `serde`, `serde_json`, `schemars`, `thiserror`, `tracing`, `rusqlite` (via `navori-store`).
- Swift 6 (AppKit + SwiftUI, Swift Testing), UniFFI.
- No new dependency.
- Test commands: `cargo test -p <crate>`, `cargo xtask ci`.

**Source classes:** every value in `requirements.md` and `design.md` is a Lock, a `(Weigh, fork 1)`, `agent-decided` or Unverified (Open Questions). A task never promotes one to the operator's choice, and no code comment says an `agent-decided` value was decided by the user.

## Global Constraints

Source of truth (referenced, not copied; read before every task):

| Source | Content hash (`git hash-object`, first 12) |
|---|---|
| `docs/standards/INDEX.md` | `38798e7207e4` |
| `docs/standards/testing.md` | `1accd637fdb0` |
| `docs/standards/errors-logging.md` | `68aadebff863` |
| `docs/standards/performance.md` | `48a8ee8a879f` |
| `docs/architecture/INDEX.md` (ARCH-N) | `2e318bd23ff5` |
| `docs/security/threat-model.md` (TB-2, TB-3) | `69375a2ce55c` |

Compact delta every task inherits:

- **Test-first at the seams agreed in `design.md` "Seams for testing".**
  - Test names are sentences in domain language. No requirement ID goes in test or production source. Expected values are literals.
  - Fakes only at system boundaries: `FakePower`, `FakeLid`, `FakePowerSource`, `FakeClock`, `FakeProcesses`, `FakeExitsHandle`.
- **Charter hard stop.**
  - No test calls the real lid setting (`MacLid::hold`, `MacLid::give_back`, selector 12).
  - No test built through `NavoriCore::start_with` / `Platform::mac()` dispatches `lid {on: true}` or plants a `lid_held` marker.
  - `modules/awake/tests/real_os.rs` gets no lid test.
  - Reading `IOPSGetProvidingPowerSourceType` or `Last Sleep Reason` is read-only and allowed.
- **Build limits:** no step writes outside the worktree or a temporary directory. No step runs `pmset` with a setting, or `sudo`.
- **These ARCH invariants hold in every task: ARCH-1, 2, 3, 4, 9, 10, 12, 16.**
  - `unsafe` appears only in `navori-macos`. Each block has a `// SAFETY:` comment, and the crate's own lint table stays as `xtask check-deps` requires.
  - No `unwrap` or `expect` outside tests.
  - Only `CoreStore` and `NavoriBanner` import the bindings. No module imports another. There are no timers and no polling.
- **Gates:**
  - Every Rust task runs `cargo fmt --all`, then `cargo clippy --workspace --all-targets -- -D warnings`, then its own `cargo test -p <crate>`.
  - Before the Swift tasks, run `cargo xtask ci` once to build the core and bindings.
  - Swift test command: `swift test --arch arm64 --package-path apps/macos --filter <Suite>`.
  - `cargo xtask ci` is the push gate (`docs/standards/INDEX.md` lines 25 and 33). It is not required per commit.
  - Perf is not gated per feature (`docs/standards/INDEX.md` line 27).
- **Permanent names (Compat External).** These are not renamed after the first commit:
  - kv `awake/lid`, `awake/lid_held` and `awake/last_sleep`
  - Journal kinds `awake.lid_changed {on}` and `awake.slept {at_ms, reason}`
  - Actions `lid {on}`, `slept {at_ms}` and `woke`
- **Shared files.** From load-subgraph on 2026-11-06 (advisory; `owns_coverage` 14/19, and `cluster(LID)` is empty until this file is indexed):
  - `crates/ffi/src/{lib,app}.rs` is shared with ASK, AWAKE, HOOKS, ICON, KCONTRACT, LOOP, PORTS and PSTOP.
  - `CoreStore.swift` and `StatusBarController.swift` are shared with AWAKE, ICON, MENU, PERF and PSTOP.
  - `crates/macos/src/lib.rs` is shared with PORTS, PROCS and PSTOP.
  - None of these is In-progress. Rebase on `main` before editing them if `main` moved.
- **Commits:** Conventional Commits, one logical change per commit, no `git push`.
- **These stay Unverified, and no task asserts them** (requirements Open Questions):
  - silence on dark wake
  - sleep when the bit clears with the lid closed
  - a reboot reset
  - powerd sharing the bit
  - an unplug during sleep

## File Structure

| Path | New/Changed | Responsibility |
|---|---|---|
| `Cargo.toml` | changed | `objc2-io-kit` features gain `ps` (and the CF features it needs) |
| `crates/store/src/lib.rs` | changed | `Store::kv_delete` |
| `crates/store/tests/kv_and_journal.rs` | changed | kv delete tests |
| `crates/macos/src/lid.rs` | new | `LidSetting`, `LidGuard`, `LidError`, `MacLid` (selector 12, `Last Sleep Reason`) |
| `crates/macos/src/power_source.rs` | new | `PowerSource`, `PowerKind`, `PowerWatch`, `PowerSourceError`, `MacPowerSource` (IOPS, notify(3)) |
| `crates/macos/src/fake/lid.rs` | new | `FakeLid` |
| `crates/macos/src/fake/power_source.rs` | new | `FakePowerSource` |
| `crates/macos/src/lib.rs`, `crates/macos/src/fake/mod.rs` | changed | module lines and re-exports |
| `crates/macos/tests/fakes.rs` | changed | fake and guard tests |
| `crates/macos/tests/power_source.rs` | new | `PowerWatch::from_stream` readiness contract; a read-only `MacPowerSource::current` test |
| `modules/awake/src/infra/platform.rs` | changed | `Platform.with_lid` / `with_power_source`, fail-safe defaults, `mac()` wiring |
| `modules/awake/src/infra/stored.rs` | changed | load, save and delete for `lid`, `lid_held`, `last_sleep` |
| `modules/awake/src/state.rs`, `domain/table.rs` | changed | new `AwakeState` fields; `snapshot()` and `idle()` literals |
| `modules/awake/src/action.rs` | changed | `AwakeAction::Lid`, `AwakeAction::Slept`, risk, catalog |
| `modules/awake/src/event.rs`, `lib.rs` | changed | `LID_CHANGED`, `SLEPT`, summaries, exports |
| `modules/awake/src/actor.rs` | changed | `set_lid`, `reconcile_lid`, `restore_lid_once`, `arm_power`, `note_power_dead`, the quit release, the `select!` power arm, `record_sleep` |
| `modules/awake/tests/common/mod.rs` | changed | the rig gains `FakeLid` and `FakePowerSource` |
| `modules/awake/tests/{actions,manual,display,duration,session,idle,tray}.rs` | changed | catalog lists and exact `AwakeState` JSON |
| `modules/awake/tests/lid.rs` | new | switch, hold, marker order, refusal, adapter, power watch, quit, drop |
| `modules/awake/tests/lid_restore.rs` | new | start-up restore and restart |
| `modules/awake/tests/sleep.rs` | new | `slept` Action, last sleep, Journal |
| `crates/ffi/src/lib.rs` | changed | `AwakeView` fields, `NavoriCore::system_will_sleep` |
| `crates/ffi/src/app.rs` | changed | `From<AwakeState>`, `awake_view` blanking, `lid_check`, doctor order |
| `crates/ffi/tests/{awake,core,integrations,running,actions_risk,agent}.rs` | changed | view literal, doctor order, `lid_check`, will-sleep, `SAMPLED`, the pinned agent catalog |
| `apps/cli/src/main.rs`, `apps/cli/tests/cli.rs` | changed | `navori awake lid on\|off`, `Lid closed:` status line |
| `docs/features/awake.md`, `docs/agents/verify.md` | changed | lid Action, CLI verb, adapter-only support |
| `modules/awake/tests/restart.rs` | read only | `restart.rs` line 89 already covers in-memory Leases kept on restart (LID-7.4) |
| `apps/macos/NavoriPanel/LastSleepText.swift` | new | pure `lastSleepText(ms:now:calendar:)` |
| `apps/macos/NavoriPanel/{RunningState,AwakeSubmenuState,KeepAwakeChoice,AwakeRowState}.swift` | changed | `.lastSleep`, `.lidClosed`, lid state, `.lid(Bool)` |
| `apps/macos/Navori/Features/Panel/{RunningSubmenuView,MenuRowView}.swift` | changed | the two rows; `MenuRow.valueNote` |
| `apps/macos/Navori/StatusBar/StatusBarController.swift` | changed | `awake.lid` cascade node and `activate` case |
| `apps/macos/Navori/Core/CoreStore.swift` | changed | `?? false`, `lidProblem` slot, `systemWillSleep()` |
| `apps/macos/Navori/Platform/WakeObserver.swift`, `App/AppDelegate.swift` | changed | will-sleep token and wiring |
| `apps/macos/Navori/Features/Settings/SettingsView.swift` | changed | doctor sentence gains `lid` |
| `apps/macos/Tests/NavoriPanelTests/{RunningStateTests,AwakeSubmenuStateTests,AwakeRowStateTests}.swift` | changed | rows, order, lid state |
| `apps/macos/Tests/NavoriPanelTests/LastSleepTextTests.swift` | new | time buckets and spoken text |

---

### Task 1: The Store can delete a kv key

**Files:**
- Modify: `crates/store/src/lib.rs`
- Test: `crates/store/tests/kv_and_journal.rs`

**Reuse:** rung 2 — extends `Store` beside `kv_set`/`kv_get` (`lib.rs:200,212`)

**Interfaces:**
- Consumes: `Store::with_conn`, `StoreError`
- Produces: `pub fn kv_delete(&self, ns: &str, key: &str) -> navori_store::Result<()>`

**Depends-on:** none

- [x] **Step 1: Write failing tests:**
  - `a_deleted_key_reads_as_absent`
  - `deleting_a_missing_key_is_not_an_error`
  - `deleting_one_key_leaves_its_neighbours`, where `awake/tracking` survives a delete of `awake/lid_held`
- [x] **Step 2: Run** `cargo test -p navori-store --test kv_and_journal`. Expect: no method `kv_delete`.
- [x] **Step 3: Implement** `DELETE FROM kv WHERE ns = ?1 AND key = ?2` through `with_conn`. No migration and no schema version change.
- [x] **Step 4: Run** `cargo test -p navori-store`. Expect: pass, with `migrations.rs` unchanged.
- [x] **Step 5: Commit** `feat(store): delete a kv key`

_Requirements: LID-7.6_

---

### Task 2: A lid-setting seam in `navori-macos`

**Files:**
- Create: `crates/macos/src/lid.rs`, `crates/macos/src/fake/lid.rs`
- Modify: `crates/macos/src/lib.rs`, `crates/macos/src/fake/mod.rs`
- Test: `crates/macos/tests/fakes.rs`

**Reuse:** rung 2 — the `AssertionGuard` pattern in `crates/macos/src/power.rs:39-73` (a closure guard with release and a warning Drop), and the `FakePower` shape in `fake/power.rs`

**Interfaces:**
- Produces:
  - `pub trait LidSetting: Send + Sync`, with methods:
    - `fn hold(&self) -> Result<LidGuard, LidError>`
    - `fn give_back(&self) -> Result<(), LidError>`
    - `fn sleep_reason(&self) -> Option<String>`
  - `pub struct LidGuard`, built with `LidGuard::new(Box<dyn FnOnce() -> Result<(), LidError> + Send>)` and released with `release(self) -> Result<(), LidError>`. Its Drop gives the setting back and calls `tracing::warn!` on failure.
  - `pub enum LidError { Refused { code: i32 }, Unavailable }` (thiserror). Messages name the call and the code, never a path.
  - `pub struct MacLid`
  - `pub struct FakeLid` (feature `fake`), with:
    - `new()`, `refuse_set(code)`, `refuse_clear(code)`, `allow()`
    - `set_reason(Option<&str>)`
    - `calls() -> Vec<LidCall>`, where `LidCall` is `Set` or `Clear`
    - `is_set() -> bool`
    - `on_call(Box<dyn Fn(LidCall) + Send + Sync>)`

**Depends-on:** none

- [x] **Step 1: Write failing tests in `fakes.rs`:**
  - `a_held_lid_is_set_until_released`
  - `releasing_a_lid_guard_clears_the_setting`
  - `dropping_a_lid_guard_clears_the_setting`, where the log ends in `Clear`
  - `a_refused_set_leaves_the_setting_clear`
  - `a_refused_clear_keeps_the_setting`
  - `give_back_clears_without_a_guard`
  - `the_probe_sees_every_call_in_order`
  - `the_scripted_sleep_reason_is_returned`
- [x] **Step 2: Run** `cargo test -p navori-macos --features fake --test fakes`. Expect: unresolved `FakeLid`.
- [x] **Step 3: Implement `MacLid`:**
  - each call: `IOServiceMatching("IOPMrootDomain")` → `IOServiceGetMatchingService` → `IOServiceOpen(task, 0)`, where `task` is read from `mach_task_self_`, declared in the crate's own `extern "C"` block. `libc`'s `mach_task_self()` and `mach_task_self_` are both `#[deprecated]` in 0.2.191 (`unix/bsd/apple/mod.rs` lines 5326 and 5451), so using them fails `clippy -D warnings` → `IOConnectCallScalarMethod(conn, 12, [1 or 0])` → `IOServiceClose` and `IOObjectRelease`
  - `sleep_reason`: `IORegistryEntryCreateCFProperty("Last Sleep Reason")`
  - each `unsafe` block carries `// SAFETY:`
  - **no test calls `MacLid`**
- [x] **Step 4: Implement `FakeLid`** on `Arc<Mutex<State>>`, with a lock that recovers from a poisoned mutex (copy `fake/power.rs`). A guard's closure shares the state. Add the module and `pub use` lines.
- [x] **Step 5: Run** `cargo test -p navori-macos --features fake` and `cargo xtask check-deps`. Expect: pass.
- [x] **Step 6: Commit** `feat(macos): a lid-setting seam with a guard and a fake`

_Requirements: LID-1.11_

---

### Task 3: A power-source seam in `navori-macos`

**Files:**
- Create: `crates/macos/src/power_source.rs`, `crates/macos/src/fake/power_source.rs`, `crates/macos/tests/power_source.rs`
- Modify: `Cargo.toml` (the `objc2-io-kit` features on line 57), `crates/macos/src/lib.rs`, `crates/macos/src/fake/mod.rs`
- Test: `crates/macos/tests/power_source.rs`, `crates/macos/tests/fakes.rs`

**Reuse:** rung 4/5 — the notify(3) key `kIOPSNotifyPowerSource` and IOPS from `objc2-io-kit`'s `ps` feature (already-installed dependency, feature widened)

**Interfaces:**
- Produces:
  - `pub trait PowerSource: Send + Sync`, with methods:
    - `fn current(&self) -> Result<PowerKind, PowerSourceError>`
    - `fn watch(&self) -> Result<PowerWatch, PowerSourceError>`
  - `pub enum PowerKind { Adapter, Battery }`. Only `"AC Power"` maps to `Adapter`; `"Battery Power"` and `"UPS Power"` map to `Battery`.
  - `pub struct PowerWatch` with two variants:
    - `Notify { fd, token }`: Drop calls `notify_cancel(token)` **only**
    - `Stream(UnixStream)`, built by `PowerWatch::from_stream(UnixStream)`
  - `impl AsRawFd for PowerWatch`
  - `pub fn read_tokens(&self) -> io::Result<usize>`, which returns:
    - `Ok(n > 0)` after a change
    - `Ok(0)` on EOF
    - `Err(WouldBlock)` when empty
  - `pub enum PowerSourceError`
  - `pub struct MacPowerSource`
  - `pub struct FakePowerSource` (feature `fake`), with:
    - `new(PowerKind)`, `set(PowerKind)`
    - `fail_current()`, `fail_watch()`
    - `fire()`, `kill()`
    - `read_count() -> usize`, `watch_count() -> usize`, `drain_count() -> usize`

**Depends-on:** none

- [x] **Step 1: Write failing tests:**
  - In `power_source.rs`:
    - `an_empty_watch_would_block`
    - `a_fired_watch_reads_tokens_then_would_block`
    - `a_closed_watch_reads_zero`
    - `the_mac_power_source_names_a_kind`, which is read-only and asserts only that `current()` returns `Ok`
  - In `fakes.rs`:
    - `the_fake_counts_current_reads`
    - `a_failed_watch_returns_an_error`
- [x] **Step 2: Run** `cargo test -p navori-macos --features fake --test power_source`. Expect: unresolved names.
- [x] **Step 3: Add** `ps` to the `objc2-io-kit` features in `Cargo.toml`, plus any CF feature the build asks for. Implement `MacPowerSource::current` with `IOPSCopyPowerSourcesInfo` → `IOPSGetProvidingPowerSourceType`.
- [x] **Step 4: Implement `watch`:**
  - `notify_register_file_descriptor` and `notify_cancel` in the same crate-local `extern "C"` block that Task 2 starts
  - `fcntl(O_NONBLOCK)` on the fd
  - `read_tokens` reads 4-byte tokens until `EAGAIN`, returns `WouldBlock` when nothing was read, and returns `0` on EOF
  - every `unsafe` block carries `// SAFETY:`
- [x] **Step 5: Implement `FakePowerSource`** over `UnixStream::pair()`. `fire` writes 4 bytes; `kill` drops the writer.
  - `PowerWatch::from_stream` calls `set_nonblocking(true)`.
  - With feature `fake`, `from_stream` takes an `Arc<AtomicUsize>` probe that counts `read_tokens` calls, exposed as `FakePowerSource::drain_count()`.
  - Every new public type derives or implements `Debug` (lint `missing_debug_implementations`).
- [x] **Step 6: Run** `cargo test -p navori-macos --features fake` and `cargo xtask check-deps`. Expect: pass.
- [x] **Step 7: Commit** `feat(macos): a power-source seam with a notify(3) watch`

_Requirements: LID-2.3_

---

### Task 4: The `Lid closed` switch, its stored value and the new Awake state

**Files:**
- Modify:
  - `modules/awake/src/infra/platform.rs`, `infra/stored.rs`, `state.rs`, `domain/table.rs`, `action.rs`, `event.rs`, `lib.rs`, `actor.rs`
  - `crates/ffi/src/lib.rs`, `crates/ffi/src/app.rs`
  - `modules/awake/tests/common/mod.rs`, `tests/actions.rs`, `tests/manual.rs`, `tests/display.rs`, `tests/duration.rs`, `tests/session.rs`, `tests/idle.rs`, `tests/tray.rs`
  - `crates/ffi/tests/running.rs`, `crates/ffi/tests/actions_risk.rs`, `crates/ffi/tests/awake.rs`, `crates/ffi/tests/agent.rs`
- Create: `modules/awake/tests/lid.rs`
- Test: `modules/awake/tests/lid.rs`, `modules/awake/tests/actions.rs`, `crates/ffi/tests/actions_risk.rs`, `crates/ffi/tests/awake.rs`

**Reuse:**
- rung 2 — `AwakeAction::Tracking` and `set_tracking` (`actor.rs:253-274`), `load_tracking`/`save_tracking` (`stored.rs:198-207`), `TRACKING_CHANGED` and its summary helper (`event.rs`)
- rung 2 — `AwakeView`, `From<AwakeState>`, `awake_view`, `awake_check`, `check()` (`app.rs:156,301-338,426-440`), for the view fields only

**Interfaces:**
- Consumes: `LidSetting`, `FakeLid` (Task 2); `PowerSource`, `FakePowerSource` (Task 3)
- Produces:
  - `Platform::with_lid(self, Arc<dyn LidSetting>) -> Self` and `Platform::with_power_source(self, Arc<dyn PowerSource>) -> Self`. The defaults are an unsupported lid that refuses, and an unsupported power source that reads `Battery` and has no watch. `Platform::mac()` wires `MacLid` and `MacPowerSource`.
  - `AwakeAction::Lid { on: bool }`, `Risk::Write`, in the catalog after `tracking`.
  - `AwakeAction::Slept { at_ms: i64 }` and `AwakeAction::Woke`, both `Risk::Write`, in the catalog after `lid` in that order. Their handlers land in Task 9; until then the actor answers `ActionError::Failed`.
  - `AwakeState` fields, all `#[serde(default)]`:
    - `lid: bool`
    - `lid_held: bool`
    - `on_adapter: Option<bool>`
    - `lid_marker: bool`
    - `last_sleep_ms: Option<i64>`
    - `lid_problem: Option<String>`
  - The same six fields on `AwakeView`.
  - `pub const LID_CHANGED: &str = "awake.lid_changed"` and `pub const SLEPT: &str = "awake.slept"`.
  - `stored::{load_lid, save_lid, load_lid_held, save_lid_held, delete_lid_held, load_last_sleep, save_last_sleep}`.
  - The test rig's `assemble` takes a `FakeLid` and a `FakePowerSource`; existing callers get defaults.

**Depends-on:** Task 2, Task 3

- [x] **Step 1: Write failing tests in `lid.rs`:**
  - `lid_closed_is_off_on_a_fresh_store`
  - `a_stored_non_boolean_reads_as_off`
  - `turning_lid_closed_on_stores_it_and_journals_the_change`, where the event payload is `{"on": true}` and the summary is `Lid closed is on: the lid may close without sleep while a Lease holds on the adapter.` (`Lid closed is off: closing the lid sleeps this Mac.` when off). Wording is agent-decided, in the style of `event.rs` line 15
  - `asking_for_the_current_value_changes_nothing`, with no event and no store write
  - `a_failed_store_leaves_lid_closed_unchanged_and_fails_the_action`, using an SQLite trigger on `kv` for key `lid` through `Store::with_conn`
  - `the_stored_value_is_applied_after_a_relaunch`
  - `an_unreadable_lid_value_reads_as_off_and_raises_a_lid_problem`
  - `setting_tracking_still_stores_first_and_journals_tracking_changed`

  Add `lid`, `slept` and `woke` to `actions.rs`: `sample()`, the ordered names list and `every_catalog_name_decodes_as_an_action`. Add them to the not-ending lists in `manual.rs` and `display.rs`. In `actions_risk.rs`, set `SAMPLED = 27`, with samples asserting `Write` for all three. In `crates/ffi/tests/agent.rs` `no_new_action_appears` (line 606), insert `"awake lid write"`, `"awake slept write"` and `"awake woke write"` after `"awake tracking write"` (line 634); LOOP-10.5 allows additions.
- [x] **Step 2: Run** `cargo test -p navori-mod-awake --test lid --test actions`. Expect: unknown variant `lid`.
- [x] **Step 3: Implement** the Platform builders and defaults, the stored helpers (kv namespace `awake`), the `AwakeState` fields (set in `LeaseTable::snapshot` and `AwakeState::idle`, filled by the actor in `publish`), the three Action variants (`lid`, `slept`, `woke`) with risk and catalog, and the event constants and summaries.
- [x] **Step 4: Implement `set_lid`** by copying `set_tracking`: unchanged → `Ok(None)`; save first; set `lid_on`; emit `LID_CHANGED`. Load `lid` on the first `run_loop` entry.
- [x] **Step 5: Mirror the six fields** on `AwakeView`, its `From` and the `awake_view` module-down blanking (`lid_held = false`, `on_adapter = None`). Update the exact-JSON expectations and literals in the seven Awake test files and `crates/ffi/tests/running.rs`.
- [x] **Step 6: Run** `cargo test -p navori-mod-awake && cargo test -p navori-ffi`. Expect: pass, with every pre-existing test unchanged apart from the added fields.
- [x] **Step 7: Commit** `feat(awake): a Lid closed switch with its stored value and state`

_Requirements: LID-3.1, LID-3.2, LID-3.3, LID-3.4, LID-3.5, LID-3.6, LID-3.12, LID-3.13, LID-7.2, LID-7.8, LID-8.3_

---

### Task 5: Watch the power source while `Lid closed` is on

**Files:**
- Modify: `modules/awake/src/actor.rs`
- Test: `modules/awake/tests/lid.rs`

**Reuse:** rung 2 — the actor's `turn` / `note_*` / `needs_retry` / `schedule_retry` loop, the exits-watcher dead-and-retry pattern (`actor.rs:588-614`), `Platform` injection (`infra/platform.rs:47-83`) and kv helpers in `infra/stored.rs`

**Interfaces:**
- Consumes: `PowerSource::watch`, `PowerSource::current`, `PowerWatch::read_tokens` (Task 3); `Platform.with_power_source` and `AwakeState.on_adapter` / `lid_problem` (Task 4)
- Produces (private): the actor fields `power: Option<tokio::io::unix::AsyncFd<PowerWatch>>` and `on_adapter: Option<bool>`; `arm_power`, `disarm_power`, `note_power_dead`, `rebuild_power_now`; and a fourth `select!` arm placed after the exits arm.

**Depends-on:** Task 4

- [x] **Step 1: Write failing tests in `lid.rs`:**
  - `nothing_reads_the_power_source_while_lid_closed_is_off`, where `read_count` is 0 and `watch_count` is 0
  - `turning_lid_closed_on_watches_first_then_reads_once`
  - `a_power_change_reads_the_source_once_and_publishes_only_a_real_change`
  - `unplugging_publishes_not_on_the_adapter`
  - `a_dead_watch_reads_as_battery_raises_a_lid_problem_and_retries`, where the watch is re-armed after the 60 s retry on `FakeClock` and the problem clears
  - `a_failed_watch_at_arming_reads_as_battery_and_retries`
  - `turning_lid_closed_off_drops_the_watch`
  - `an_idle_watch_does_not_spin`: after arming and `rig.idle()` ten times on a paused clock, `drain_count()` is at most 1
- [x] **Step 2: Run** `cargo test -p navori-mod-awake --test lid`. Expect: the new tests fail.
- [x] **Step 3: Implement** arming on turn-on and at start with `lid` on: `watch()` **first**, then one `current()`. On turn-off, disarm and set `on_adapter = None`.
- [x] **Step 4: Implement the arm.** It loops on `readable().await`, then `guard.try_io(|w| w.get_ref().read_tokens())`. `Ok(n > 0)` → `current()` once. `Ok(0)`, an error, or a failed `current()` → `note_power_dead`: drop the watch, set `on_adapter = Some(false)`, raise the lid problem, `schedule_retry`. `needs_retry` is true while the watch is dead and `lid` is on.
- [x] **Step 5: Run** `cargo test -p navori-mod-awake`. Expect: pass.
- [x] **Step 6: Commit** `feat(awake): watch the power source while Lid closed is on`

_Requirements: LID-2.5, LID-2.6, LID-2.7, LID-8.1_

---

### Task 6: Hold the lid while a Lease holds on the adapter, and give it back

**Files:**
- Modify: `modules/awake/src/actor.rs`
- Test: `modules/awake/tests/lid.rs`

**Reuse:** rung 2 — the actor's `turn` / `note_*` / `needs_retry` / `schedule_retry` loop, the exits-watcher dead-and-retry pattern (`actor.rs:588-614`), `Platform` injection (`infra/platform.rs:47-83`) and kv helpers in `infra/stored.rs`

**Interfaces:**
- Consumes: `LidSetting::hold`, `LidGuard::release`, `LidSetting::give_back`, `FakeLid::on_call` (Task 2); `Store::kv_delete` (Task 1); `stored::{save_lid_held, delete_lid_held}`, `AwakeState.lid_held` / `lid_marker` / `lid_problem` (Task 4); `on_adapter` (Task 5)
- Produces (private):
  - actor fields `lid: Option<LidGuard>`, `lid_marker: bool` (the in-memory mirror) and `lid_restored: bool`. This task initialises it to `true`, as a stand-in until Task 8 adds the restore.
  - `reconcile_lid(ctx, now)`, called in `turn` between `note_assertions` and `publish`

**Depends-on:** Task 1, Task 5

- [x] **Step 1: Write failing tests in `lid.rs`.** Every test uses `FakeLid` with an `on_call` probe that records `kv_get("awake","lid_held")` at each call.
  - `a_held_lease_on_the_adapter_with_lid_closed_on_sets_the_lid`
  - `the_marker_is_stored_before_the_lid_is_set`
  - `the_marker_is_deleted_only_after_the_lid_is_cleared`
  - `the_last_lease_ending_gives_the_lid_back`
  - `turning_lid_closed_off_gives_the_lid_back`
  - `with_tracking_agents_off_a_session_lease_alone_does_not_hold_the_lid`
  - `a_manual_lease_holds_the_lid_with_tracking_agents_off`
  - `unplugging_gives_the_lid_back_and_plugging_in_holds_it_again`
  - `a_failed_marker_store_sets_nothing_and_raises_a_lid_problem`, using an SQLite trigger on key `lid_held`
  - `a_refused_set_turns_lid_closed_off_stores_it_deletes_the_marker_and_does_not_retry`
  - `a_refused_clear_keeps_the_marker_raises_a_lid_problem_and_retries_until_cleared`
  - `the_lid_problem_clears_when_the_lid_call_succeeds`
  - `the_lid_problem_clears_when_the_marker_saves_again`, after the trigger is dropped
  - `the_lid_problem_clears_when_the_user_sets_lid_closed`
  - `a_dead_watch_while_holding_gives_the_lid_back_and_raises_a_lid_problem`, using `FakePowerSource::kill()` while `FakeLid::is_set()`; the probe shows the marker present at `Clear` and deleted after
  - `the_lid_problem_never_becomes_the_awake_problem`, where `AwakeState.problem` stays `None`
  - `the_system_assertion_is_held_whatever_the_lid`
  - `a_refused_system_assertion_still_refuses_every_kind`
  - `lease_events_keep_their_key_and_kind_payload`
- [x] **Step 2: Run** `cargo test -p navori-mod-awake --test lid`. Expect: the new tests fail.
- [x] **Step 3: Implement `reconcile_lid`** to the design's §4 flow:
  - `want = lid_restored ∧ lid_on ∧ wants().system ∧ on_adapter == Some(true)`
  - hold: marker, then `hold()`
  - refused or unavailable: off, save, emit, delete the marker; no retry
  - release: `release()`, then delete the marker
  - pending clear (derived from the marker mirror with no guard): `give_back()`, then delete
- [x] **Step 4: Extend `needs_retry`** for a pending clear and for a failed marker save while a hold is wanted, never for a refusal. Publish `lid_held`, `lid_marker` and `lid_problem`. `Problems::first()` stays untouched.
- [x] **Step 5: Run** `cargo test -p navori-mod-awake`. Expect: pass.
- [x] **Step 6: Commit** `feat(awake): hold the lid while a Lease holds on the adapter`

_Requirements: LID-1.1, LID-1.2, LID-1.3, LID-1.4, LID-1.5, LID-1.6, LID-1.7, LID-1.8, LID-1.9, LID-1.12, LID-2.1, LID-2.2, LID-2.6, LID-7.1, LID-7.3, LID-7.11, LID-8.4_

---

### Task 7: Give the lid back on quit and when the module is dropped

**Files:**
- Modify: `modules/awake/src/actor.rs`
- Test: `modules/awake/tests/lid.rs`

**Reuse:** rung 2 — the actor's `turn` / `note_*` / `needs_retry` / `schedule_retry` loop, the exits-watcher dead-and-retry pattern (`actor.rs:588-614`), `Platform` injection (`infra/platform.rs:47-83`) and kv helpers in `infra/stored.rs`

**Interfaces:**
- Consumes: `reconcile_lid`, the `lid` guard field (Task 6); the `LidGuard` Drop (Task 2)
- Produces (private): `release_lid_on_quit(ctx)`, called where `ctx.recv()` returns `None` (`actor.rs` line 114) before `run_loop` returns `Ok(())`

**Depends-on:** Task 6

- [x] **Step 1: Write failing tests:**
  - `a_clean_shutdown_gives_the_lid_back_then_deletes_the_marker`, through kernel shutdown
  - `a_module_that_fails_permanently_gives_the_lid_back_and_keeps_the_marker`. Copy `permanent_failure_releases_every_assertion` (`modules/awake/tests/restart.rs` lines 243-262): `Rig::with_policy(restarts(0))`, hold the lid (Lease + adapter + switch on), then `rig.power.panic_on_hold(1)` and dispatch `display_on()`; wait for `awake_failed`
  - `dropping_the_kernel_while_holding_gives_the_lid_back`
- [x] **Step 2: Run** `cargo test -p navori-mod-awake --test lid`. Expect: the quit test fails, because the marker stays.
- [x] **Step 3: Implement** the quit release. Drop paths need no code beyond the `LidGuard` Drop; the tests prove them.
- [x] **Step 4: Run** `cargo test -p navori-mod-awake`. Expect: pass.
- [x] **Step 5: Commit** `feat(awake): give the lid back on quit and on drop`

_Requirements: LID-1.10, LID-1.11, LID-8.4_

---

### Task 8: Restore lid sleep at start when the marker says Navori held it

**Files:**
- Modify: `modules/awake/src/actor.rs`
- Create: `modules/awake/tests/lid_restore.rs`
- Test: `modules/awake/tests/lid_restore.rs` (and the existing `modules/awake/tests/restart.rs`, run unchanged)

**Reuse:** rung 2 — the actor's `turn` / `note_*` / `needs_retry` / `schedule_retry` loop, the exits-watcher dead-and-retry pattern (`actor.rs:588-614`), `Platform` injection (`infra/platform.rs:47-83`) and kv helpers in `infra/stored.rs`

**Interfaces:**
- Consumes: `LidSetting::give_back` (Task 2); `stored::{load_lid_held, delete_lid_held}` (Task 4); `lid_restored`, `reconcile_lid` (Task 6)
- Produces (private): `restore_lid_once(ctx, now)`, run on the first `run_loop` entry after `load_tracking` and before `restore_if_needed` (`actor.rs` lines 92-93)

**Depends-on:** Task 6

- [x] **Step 1: Write failing tests in `lid_restore.rs`:**
  - `a_stored_marker_at_start_gives_the_lid_back_then_deletes_it`
  - `no_marker_at_start_makes_no_lid_call`
  - `the_restore_runs_before_any_hold_in_that_process`, with a marker, a Lease, the adapter and the switch on; the call order is `Clear` then `Set`
  - `a_refused_restore_keeps_the_marker_raises_a_lid_problem_and_holds_nothing_until_it_succeeds`
  - `a_supervised_restart_does_not_restore_again_and_keeps_a_live_hold`
  - `an_unreadable_marker_skips_the_restore_and_raises_a_lid_problem`
  - `an_unreadable_last_sleep_does_not_skip_the_restore`

  LID-7.4's guard is already covered by the existing restart test (`modules/awake/tests/restart.rs` line 89); keep it green.
- [x] **Step 2: Run** `cargo test -p navori-mod-awake --test lid_restore`. Expect: fail.
- [x] **Step 3: Implement `restore_lid_once`.** Set `lid_restored` on success or on an unreadable marker. On refusal, keep the marker, raise the problem and retry; `want` stays false. Change the initial value of `lid_restored` to `false`. With an empty store (no marker), the restore succeeds at once and sets it to `true`, so Task 6's tests still pass unchanged.
- [x] **Step 4: Run** `cargo test -p navori-mod-awake`. Expect: pass.
- [x] **Step 5: Commit** `feat(awake): give lid sleep back at start when Navori held it`

_Requirements: LID-3.13, LID-4.1, LID-4.2, LID-4.3, LID-4.4, LID-7.4_

---

### Task 9: Record when the Mac went to sleep

**Files:**
- Modify: `modules/awake/src/actor.rs`, `crates/ffi/src/lib.rs`
- Create: `modules/awake/tests/sleep.rs`
- Test: `modules/awake/tests/sleep.rs`, `modules/awake/tests/lid.rs`, `crates/ffi/tests/core.rs`

**Reuse:** rung 2 — `NavoriCore::system_did_wake` and the spawned-dispatch shape in `NavoriCore::dispatch` (`lib.rs:445-487`), the Swift `WakeObserver` token pattern

**Interfaces:**
- Consumes: `AwakeAction::Slept`, `SLEPT`, `stored::{save_last_sleep, load_last_sleep}`, `AwakeState.last_sleep_ms` (Task 4); `LidSetting::sleep_reason`, `FakeLid::set_reason` (Task 2)
- Produces:
  - private `record_sleep(ctx, at_ms)`
  - `#[uniffi::export] fn system_will_sleep(&self)` on `NavoriCore`, which:
    1. stamps `SystemTime::now()` in ms first
    2. returns if `shut_down`
    3. spawns a dispatch of `{"slept":{"at_ms":..}}` to `awake` and logs an error
  - `system_did_wake` always sends its wake pulse as today. After that it spawns a dispatch of `"woke"` to `awake`, unless `shut_down` is set; only that dispatch is skipped, never the pulse.
  - private `on_woke(ctx)`: while the power watch is armed, one `current()` read through the change path

**Depends-on:** Task 4, Task 8 (all of Tasks 5–9 edit `actor.rs`, so they run in order)

- [ ] **Step 1: Write failing tests:**
  - In `sleep.rs`:
    - `a_recorded_sleep_is_published_and_journaled_with_its_reason`, where the payload is `{"at_ms": 1793931600000, "reason": "Clamshell Sleep"}` (2026-11-06 09:20 +07), with the summary `The Mac went to sleep.`
    - `an_unreadable_reason_is_journaled_as_null`
    - `the_last_sleep_survives_a_relaunch`
    - `every_slept_call_counts`, where two calls give two events and the latest is published
    - `a_failed_store_still_publishes_the_sleep_and_raises_a_lid_problem`
  - In `lid.rs`:
    - `a_wake_rereads_the_power_source_while_lid_closed_is_on`, where `read_count` goes up by one
    - `an_unplug_during_sleep_is_seen_at_wake_and_gives_the_lid_back`: hold the lid, `FakePowerSource::set(Battery)` with no `fire()`, dispatch `woke`; `FakeLid` logs `Clear`
    - `a_wake_while_lid_closed_is_off_reads_nothing`
  - In `core.rs` (real platform; `Lid closed` stays off):
    - `will_sleep_records_a_time_taken_at_the_call`, where `last_sleep_ms` is between the before and after stamps
    - `the_wake_pulse_still_runs_on_wake`
    - `will_sleep_after_shutdown_records_nothing_and_does_not_panic`
    - `did_wake_after_shutdown_dispatches_nothing_and_does_not_panic`
- [ ] **Step 2: Run** `cargo test -p navori-mod-awake --test sleep`. Expect: the `slept` handler fails.
- [ ] **Step 3: Implement `record_sleep`:** reason → save → set → emit `SLEPT` (one-sentence summary).
- [ ] **Step 4: Implement `system_will_sleep`** next to `system_did_wake`. Extend `system_did_wake` to dispatch `woke` after its unchanged wake pulse, and implement `on_woke` in the actor.
- [ ] **Step 5: Run** `cargo test -p navori-mod-awake && cargo test -p navori-ffi`. Expect: pass.
- [ ] **Step 6: Commit** `feat(awake): record when the Mac went to sleep`

_Requirements: LID-2.8, LID-5.1, LID-5.2, LID-5.3, LID-5.8, LID-5.9, LID-7.5_

---

### Task 10: `navori doctor` reports the lid

**Files:**
- Modify: `crates/ffi/src/app.rs`, `apps/macos/Navori/Features/Settings/SettingsView.swift`
- Test: `crates/ffi/tests/awake.rs`, `crates/ffi/tests/integrations.rs`, `crates/ffi/tests/core.rs`

**Reuse:** rung 2 — `AwakeView`, `From<AwakeState>`, `awake_view`, `awake_check`, `check()` (`app.rs:156,301-338,426-440`)

**Interfaces:**
- Consumes: the `AwakeView` lid fields (Task 4)
- Produces: `pub fn lid_check(view: Option<&AwakeView>) -> Check`, appended after `ports_check` in `Inner::doctor`

**Depends-on:** Task 4, Task 9 (both edit `crates/ffi/tests/core.rs`)

- [ ] **Step 1: Write failing tests in `awake.rs`:**
  - `the_lid_check_passes_with_lid_closed_off`, detail `Lid closed off · not held · not watched · no marker`
  - `the_lid_check_reports_held_adapter_and_marker`, detail `Lid closed on · held · adapter · marker`
  - `the_lid_check_reports_battery_while_on_and_unplugged`, detail `Lid closed on · not held · battery · no marker`
  - `the_lid_check_fails_with_the_lid_problem_text`
  - `the_lid_check_fails_when_awake_cannot_be_read`

  Add `lid` last in the ordered doctor lists in `integrations.rs` (lines 64-119) and `core.rs` (lines 772-790). Keep `doctor_reports_a_healthy_core` (`core.rs` line 423) as is.
- [ ] **Step 2: Run** `cargo test -p navori-ffi --test awake`. Expect: no `lid_check`.
- [ ] **Step 3: Implement** `lid_check` and append it after `ports`. In `SettingsView.swift`, change the doctor sentence to "…, logs, ports and lid."
- [ ] **Step 4: Run** `cargo test -p navori-ffi`. Expect: pass; existing check names and order unchanged. Acceptance for LID-6.3: `grep -n 'logs, ports and lid' apps/macos/Navori/Features/Settings/SettingsView.swift` matches.
- [ ] **Step 5: Commit** `feat(ffi): a lid row in navori doctor`

_Requirements: LID-6.1, LID-6.2, LID-6.3, LID-7.7_

---

### Task 11: `navori awake lid on|off` and the status line

**Files:**
- Modify: `apps/cli/src/main.rs`, `docs/features/awake.md`, `docs/agents/verify.md`
- Test: `apps/cli/tests/cli.rs`

**Reuse:** rung 2 — `AwakeCommand` and `dispatch_awake` (`main.rs:81-101,269-285`), `print_awake_status` (`:467-491`)

**Interfaces:**
- Consumes: the Action JSON `{"lid":{"on":bool}}` and the state fields `lid` and `on_adapter` (Task 4)
- Produces: `AwakeCommand::Lid { state: OnOff }`, where `OnOff` is a clap `ValueEnum` with values `on` and `off`; and a `Lid closed:` line after `Tracking agents:`

**Depends-on:** Task 4

- [ ] **Step 1: Write failing tests in `cli.rs`.** Every lid test runs against a stub `UnixListener` server that records the request JSON and answers a fixture state. Copy the stubs at `apps/cli/tests/cli.rs` lines 535, 1072 and 1419. **Never** use the real in-process `NavoriCore` (`cli.rs` line 55, `Platform::mac()`), because a `lid {on: true}` there would set the real lid (charter hard stop).
  - `awake_lid_on_dispatches_the_lid_action`
  - `awake_lid_off_dispatches_the_lid_action`
  - `status_prints_lid_closed_after_tracking_agents`, covering `off`, `on`, `on (held)` and `on (needs adapter)` from fixture states (`lid`, `lid_held`, `on_adapter`)
  - `status_reads_a_missing_lid_field_as_off`
  - `status_json_carries_the_lid_fields`
- [ ] **Step 2: Run** `cargo test -p navori-cli --test cli`. Expect: unknown subcommand `lid`.
- [ ] **Step 3: Implement** the verb and the status line with plain `serde_json::Value` (no new dependency). In `docs/features/awake.md`, update the CLI paragraph (line 20), the doctor line (line 22), the menu rows (line 30) and the "does not do" list (line 36), where the lid becomes adapter-only support. In `docs/agents/verify.md`, update the awake commands (lines 66, 100 and 151-156).
- [ ] **Step 4: Run** `cargo test -p navori-cli && cargo xtask check-deps`. Expect: pass.
- [ ] **Step 5: Commit** `feat(cli): navori awake lid on|off`

_Requirements: LID-3.10, LID-3.11, LID-7.9, LID-7.12_

---

### Task 12: The `Lid closed` row in the Running submenu

**Files:**
- Modify:
  - `apps/macos/NavoriPanel/AwakeSubmenuState.swift`, `RunningState.swift`, `KeepAwakeChoice.swift`, `AwakeRowState.swift`
  - `apps/macos/Navori/Features/Panel/RunningSubmenuView.swift`, `apps/macos/Navori/Features/Panel/MenuRowView.swift`
  - `apps/macos/Navori/StatusBar/StatusBarController.swift`
  - `apps/macos/Navori/Core/CoreStore.swift`
- Test: `apps/macos/Tests/NavoriPanelTests/AwakeSubmenuStateTests.swift`, `RunningStateTests.swift`, `AwakeRowStateTests.swift`

**Reuse:** rung 2:
- `WakeObserver` token pattern
- `AwakeSubmenuState.tracking*`
- the `RunningSubmenuItem` / `items` builder
- `MenuRow` / `MenuRowView`
- the static row in `CommandRowView` (`RunningSubmenuView.swift:~136-163`)
- the cascade nodes and `activate` in `StatusBarController.swift:301-327`
- `clockText`'s fixed-locale formatter (`AwakeLeaseMap.swift:44-53`)

**Interfaces:**
- Consumes: `AwakeView.lid`, `onAdapter`, `lidProblem` (Task 4, through the regenerated bindings in `CoreStore` only)
- Produces:
  - `AwakeSubmenuState`: `init(..., lid: Bool = false, onAdapter: Bool? = nil)`, plus `lidOn`, `lidValue` (`On`/`Off`), `lidDot` (`.green` when on), `lidNote` (`"Needs adapter"` when on and `onAdapter != true`) `lidSpoken` (`Lid closed, stay awake, on`, plus `, needs adapter`) and `lidToggle: AwakeCommand` (`.lid(!lidOn)`)
  - `RunningSubmenuItem.lidClosed`
  - `AwakeCommand.lid(Bool)`, which sends `{"lid":{"on":…}}`
  - `MenuRow.valueNote: String?`, defaulting to `nil`
  - the `CascadeNode(id: "awake.lid", isActionable: true)` and the `activate` case `"awake.lid"`

**Depends-on:** Task 4

- [ ] **Step 1: Run** `cargo xtask ci` once to build the core and the bindings. Write failing tests:
  - In `AwakeSubmenuStateTests`:
    - `lid_closed_reads_off_by_default`
    - `lid_closed_on_shows_on_and_a_green_dot`
    - `lid_closed_on_without_the_adapter_keeps_on_and_adds_needs_adapter`
    - `the_spoken_label_reads_the_switch_state`
    - `the_lid_action_json_carries_on`
  - In `RunningStateTests.swift`, the order tests in `RunningSubmenuStateTests` (lines 195-235) and `RunningCommandRowsTests` (lines 633-650) gain `.lidClosed` after `.trackingAgents`. The `kinds` mapping at line 658 gains a case for the new item. Also add to `RunningSubmenuStateTests`:
    - `the_core_note_still_takes_the_slot_before_any_problem`
    - `a_lid_problem_shows_only_when_no_note_or_awake_problem_does`
    - `project_rows_and_stop_all_are_unchanged`
  - In `AwakeRowStateTests`:
    - `a_lid_action_failure_is_reported`
  - In `AwakeSubmenuStateTests`, also add `the_lid_row_toggles_to_the_opposite_value`, which tests a pure `AwakeSubmenuState.lidToggle: AwakeCommand` that `StatusBarController.activate` calls (LID-3.9)
- [ ] **Step 2: Run** `swift test --arch arm64 --package-path apps/macos --filter AwakeSubmenuStateTests`. Expect: compile errors.
- [ ] **Step 3: Implement** the state, the item, the command, the `AwakeActionFailure.achieved` case and `MenuRow.valueNote`, drawn in secondary colour after `value`. Draw `.lidClosed` with `MenuRowView`: `id "awake.lid"`, icon `laptopcomputer`.
- [ ] **Step 4: Wire `StatusBarController`:** the node after `awake.tracking`, and `case "awake.lid": store.awakeSubmenu.lidToggle`. In `CoreStore`, pass `awake?.lid ?? false` and `awake?.onAdapter`, and set the problem slot to `awake.problem ?? awake.lidProblem`. Leave `AwakeHeldMarks`, the Running row dot and `tray` untouched.
- [ ] **Step 5: Run** `swift test --arch arm64 --package-path apps/macos --filter "AwakeSubmenuStateTests|RunningSubmenuStateTests|RunningCommandRowsTests|AwakeRowStateTests"`, then `cargo xtask ci`. Expect: pass.
- [ ] **Step 6: Commit** `feat(menu): a Lid closed row in the Running submenu`

_Requirements: LID-2.4, LID-3.8, LID-3.9, LID-7.10, LID-7.13, LID-7.15, LID-7.16, LID-7.17, LID-8.5_

---

### Task 13: The `Last sleep` row and the will-sleep forwarder

**Files:**
- Create: `apps/macos/NavoriPanel/LastSleepText.swift`, `apps/macos/Tests/NavoriPanelTests/LastSleepTextTests.swift`
- Modify:
  - `apps/macos/NavoriPanel/RunningState.swift`
  - `apps/macos/Navori/Features/Panel/RunningSubmenuView.swift`
  - `apps/macos/Navori/Core/CoreStore.swift`
  - `apps/macos/Navori/Platform/WakeObserver.swift`
  - `apps/macos/Navori/App/AppDelegate.swift`
- Test: `apps/macos/Tests/NavoriPanelTests/LastSleepTextTests.swift`, `RunningStateTests.swift`

**Reuse:** rung 2:
- `WakeObserver` token pattern
- `AwakeSubmenuState.tracking*`
- the `RunningSubmenuItem` / `items` builder
- `MenuRow` / `MenuRowView`
- the static row in `CommandRowView` (`RunningSubmenuView.swift:~136-163`)
- the cascade nodes and `activate` in `StatusBarController.swift:301-327`
- `clockText`'s fixed-locale formatter (`AwakeLeaseMap.swift:44-53`)

**Interfaces:**
- Consumes: `AwakeView.lastSleepMs` (Task 4); `NavoriCore.systemWillSleep()` (Task 9); `.lidClosed` (Task 12)
- Produces:
  - `public struct LastSleepText: Equatable, Sendable { value: String; spoken: String }`
  - `public func lastSleepText(ms: Int64, now: Date, calendar: Calendar) -> LastSleepText`, using static cached `DateFormatter`s (`en_US_POSIX`, gregorian, the calendar's time zone)
  - `RunningSubmenuItem.lastSleep(LastSleepText)` and `RunningSubmenuItem.actionId: String?`
  - `RunningSubmenuState.init(..., lastSleep: LastSleepText? = nil)`
  - `WakeObserver(onWake:onSleep:)`
  - `CoreStore.systemWillSleep()`

**Depends-on:** Task 9, Task 12

- [ ] **Step 1: Write failing tests:**
  - In the new `@Suite struct LastSleepTextTests`, one table-driven test, `the_last_sleep_reads_by_calendar_day`. Its rows use a fixed `now` of 2026-11-06 10:00 in `Asia/Ho_Chi_Minh`:

    | Sleep time | `value` | `spoken` |
    |---|---|---|
    | 09:20 today | `09:20` | `today at 09:20` |
    | yesterday 12:03 | `Yesterday 12:03` | `yesterday at 12:03` |
    | 2026-11-04 11:43 | `Wed 11:43` | `Wednesday at 11:43` |
    | 2026-10-30 08:00 | `30 Oct` | — |
    | 2025-10-30 08:00 | `30 Oct 2025` | — |

  - In `RunningSubmenuStateTests` (`RunningStateTests.swift`):
    - `a_recorded_sleep_leads_the_submenu_with_a_separator`
    - `no_recorded_sleep_shows_no_row_and_no_separator`
    - `the_full_order_runs_from_last_sleep_to_stop_all`, with exactly the order in design §9
    - `the_last_sleep_item_has_no_action_id`, testing a new `RunningSubmenuItem.actionId: String?` (`.lastSleep` → `nil`, `.lidClosed` → `"awake.lid"`, `.trackingAgents` → `"awake.tracking"`); `StatusBarController.runningCommands` builds its nodes only for non-nil ids (LID-5.7)
- [ ] **Step 2: Run** `swift test --arch arm64 --package-path apps/macos --filter "LastSleepTextTests|RunningSubmenuStateTests"`. Expect: compile errors.
- [ ] **Step 3: Implement** `lastSleepText` and the `.lastSleep` item. Draw it as a static row on the `CommandRowView` pattern: icon `moon`, value in secondary label colour, no Button, no hover, no cascade node, `.accessibilityElement(children: .ignore)` and `accessibilityLabel` `Last sleep, <spoken>`.
- [ ] **Step 4: Wire the forwarder.** Add an `onSleep` token for `NSWorkspace.willSleepNotification` in `WakeObserver`, wired in `AppDelegate`'s existing `!perfFlagWithoutHome` block to `store.systemWillSleep()`, which calls `core?.systemWillSleep()`. `CoreStore` passes `lastSleep` from `awake?.lastSleepMs` with `Date()` and `.current`.
- [ ] **Step 5: Run** the Swift suites above, then `cargo xtask ci`. Expect: pass.
- [ ] **Step 6: Acceptance check for the menu-open path (LID-8.2, LID-7.14).** Run `grep -rnE 'import IOKit|IOPS|notify_register|IORegistry' apps/macos/NavoriPanel apps/macos/Navori/Features/Panel`. Expect: no match. `CoreStore` builds both submenus only from the last pushed `CoreSnapshot`.
- [ ] **Step 7: Commit** `feat(menu): show when the Mac last slept`

_Requirements: LID-3.7, LID-5.4, LID-5.5, LID-5.6, LID-5.7, LID-7.14, LID-8.2, LID-8.5_

---

## Coverage

| Story | IDs → Task |
|---|---|
| 1 | 1.1–1.9, 1.12 → 6; 1.10 → 7; 1.11 → 2, 7 |
| 2 | 2.1, 2.2 → 6; 2.3 → 3; 2.4 → 12; 2.5, 2.7 → 5; 2.6 → 5, 6; 2.8 → 9 |
| 3 | 3.1–3.6, 3.12 → 4; 3.13 → 4, 8; 3.7 → 13; 3.8, 3.9 → 12; 3.10, 3.11 → 11 |
| 4 | 4.1–4.4 → 8 |
| 5 | 5.1–5.3, 5.8, 5.9 → 9; 5.4–5.7 → 13 |
| 6 | 6.1–6.3 → 10 |
| 7 | 7.1, 7.3, 7.11 → 6; 7.2, 7.8 → 4; 7.4 → 8; 7.5 → 9; 7.6 → 1; 7.7 → 10; 7.9, 7.12 → 11; 7.10, 7.13, 7.15–7.17 → 12; 7.14 → 13 |
| 8 | 8.1 → 5; 8.2 → 13; 8.3 → 4; 8.4 → 6, 7; 8.5 → 12, 13 |

No ID is unmapped. Manual acceptance belongs to the operator's lid run (requirements Open Questions), not to these tasks.
