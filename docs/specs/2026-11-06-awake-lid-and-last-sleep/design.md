# Design: Awake with the lid closed, and the last sleep

Feature code: LID
Status: Draft
Date: 2026-11-06
Requirements: ./requirements.md

## Context

Awake (AWAKE, ROAD-8) keeps the Mac awake with one `PreventUserIdleSystemSleep` assertion while any Lease is held. The `Reconciler` holds the assertion and releases it on drop. The assertion never stops lid-close sleep: `IOPMLib.h` says the system "may still sleep for lid close", and on this Mac 4 of 5 lid closes slept with Leases held. A lid test on 2026-11-06 found three things:
- Selector 12 on the `IOPMrootDomain` user client works as uid 501. It is `kPMSetClamshellSleepState`, `IOPMLibDefs.h:43`.
- The setting outlives the process.
- Setting it back restores lid sleep.

The binding constraint is that **this setting is one global kernel bit that outlives Navori**. It is `kClamshellSleepDisablePowerd` in `clamshellSleepDisableMask` (research Q4). While it is set, the kernel keeps the Mac awake with the lid closed *even on battery*, until low-battery sleep. Every path that ends Navori's interest must give it back:
- the last Lease ending
- the switch turned off
- unplugging the adapter
- a watch failure
- a quit
- a module drop
- a crash, which is handled at the next launch

That rules out the obvious alternative, a fire-and-forget call made beside the assertion. It also rules out putting the lid logic in Swift, because Swift dies with the process and has no store (ARCH-1). The lid hold is therefore a guard with a Drop, owned by the same supervised actor that owns the Lease set. The actor writes a store marker before it sets the bit, and runs a once-per-process restore keyed on that marker.

The second constraint is the GOAL-11 budget and ARCH-16: no timers and no polling. The adapter is learned from the `kIOPSNotifyPowerSource` notify(3) key, which fires only on AC↔battery changes (`IOPowerSources.h:131-147`), through its file descriptor in the actor's `select!`. That adds no thread. The last sleep reaches the Core through the Shell's existing wake seam, which gains a will-sleep twin, so the menu reads only the Core snapshot (AWAKE-11.10).

## Decisions

1. **The Awake module owns the whole feature** (close package A1, agent-decided). `navori-macos` gets two platform traits, `LidSetting` and `PowerSource`, with fakes. All `unsafe` stays there (ARCH-9).
2. **The lid hold rule is `lid_on ∧ wants().system ∧ on_adapter`**, computed in the actor (A2, review r1 finding 2). Session Leases count only while Tracking agents is on.
3. **`LidGuard` mirrors `AssertionGuard`**: it has `release(self)`, and Drop clears and warns (A7, A13). A clean quit releases the guard and then deletes the marker before `run_loop` returns. An abort or a permanent failure drops the guard and leaves the marker for the next launch.
4. **Write order is fixed** (Lock L11):
   - Hold: store the `lid_held` marker, then set the bit. If the marker save fails, nothing is set.
   - Release: clear the bit, then delete the marker. If the clear fails, the marker stays and the clear is retried.
   - A refused set deletes the marker, because the bit never changed. It also turns the switch off (LID-1.9).
5. **The start-up restore runs once per process**, guarded by a `lid_restored` flag on the actor struct. The struct survives supervised restarts (`actor.rs:51`). It runs before `restore_if_needed`, which itself calls `turn` (`actor.rs:926`). No hold is wanted until `lid_restored` is true, so a refused restore is retried and never clears a live hold (LID-4.4; design review F3).
6. **The power watch is armed only while `Lid closed` is on** (LID-2.5, LID-2.7; changed in design). A Mac that never uses the feature pays nothing, and a broken watch cannot fail doctor when the switch is off. While the switch is on, a dead or failed watch reads as not on the adapter, raises a lid problem and is retried like the exits watcher (LID-2.6).
7. **The watch is a notify(3) fd driven by tokio `AsyncFd`** in the actor (close package A3).
   - `navori-macos` registers the key and returns a `PowerWatch` that owns the fd and the token. Its Drop calls `notify_cancel(token)` **only**, which also closes the fd (`notify.h:255-258`); it never calls `close()`, so there is no double close (design r1 finding 3).
   - `PowerWatch` exposes a safe `read_tokens()`, which returns `Err(WouldBlock)` once the fd is empty. The actor calls it inside `AsyncFd::readable()` → `guard.try_io(..)`, so tokio clears readiness on `WouldBlock` and the arm cannot spin (design review F2).
   - Arming registers the watch **first**, then reads `current()`, so a change between the two is never lost (design review F1).
   - No thread is added. tokio `net` is already a workspace feature (`Cargo.toml:68`).
8. **The sleep reaches Awake through a new `slept {at_ms}` Action** (LID-5.9; added in design). `NavoriCore::system_will_sleep()` stamps `at_ms` synchronously, then spawns the dispatch, honouring `shut_down`. The Core reaches a Module only through dispatch (`crates/ffi/src/lib.rs:445-465`). The actor reads the `Last Sleep Reason` through the platform when it handles the Action. That value persists across wake (research Q2).
9. **Storage** (A4, A5, A9):
   - All new values go in kv namespace `awake`. There is no new migration (LID-7.6).
     - `lid`: JSON bool, default off (W1).
     - `lid_held`: JSON `true`.
     - `last_sleep`: JSON integer ms.
   - `Store::kv_delete(ns, key)` is added.
   - Journal kinds `awake.lid_changed {on}` and `awake.slept {at_ms, reason}` are permanent (Compat External).
10. **A lid problem stays out of `Problems::first`** (design review F4). It is published only as `AwakeState.lid_problem`. Putting it in `AwakeState.problem` would mark every project Unknown and turn off Stop (`crates/ffi/src/running.rs:319`), fail the `awake` doctor row and change the level-0 row. `CoreStore` gives it the menu's problem slot only after the Core's note and the Awake problem. Doctor reads it in `lid_check` (LID-6.2). It clears when its condition ends (LID-1.12). This answers the reviewer's ask about rank: it has no rank in `first()`, by design.
11. **`AwakeState` gains `#[serde(default)]` fields**: `lid`, `lid_held`, `on_adapter: Option<bool>` (None = not watched), `lid_marker`, `last_sleep_ms`, `lid_problem`. They are mirrored on `AwakeView` (add-only, AWAKE-12.3). The Swift fallback for `lid` is `?? false` (W1).
12. **An ADR records the clamshell-selector choice**: 0010, written via `define-domain`. It is surprising, hard to reverse because of the permanent keys and the global bit, and a real trade-off against root helpers.

**Fresh retrieval** (load-subgraph, schema 1.1, recipe fsubr-1.2, run after the design scan):
- registered 19; with OWNS 14 (LID has no `tasks.md` yet); observations [].
- Neighbors: AWAKE (owner), PROCS (platform fakes, exits watcher), PSTOP (Running view), ICON (status icon; LID-7.16 guard), HOOKS, PORTS.

**Spine:** ARCH-1, ARCH-2, ARCH-3, ARCH-4, ARCH-9, ARCH-10, ARCH-12, ARCH-16.
**Threat model:** TB-2, TB-3 (Approved `docs/security/threat-model.md`).

## Architecture

Vocabulary for every section below: **module**, **interface**, **implementation**,
**seam** (public surface where behavior is observable and substitutable). Prefer
deep modules — an interface much simpler than what it hides.

### 1. Lid setting and sleep reason (`crates/macos/src/lid.rs`, `fake/lid.rs`)

Satisfies: LID-1.11
Reuse: rung 2 — the `AssertionGuard` pattern in `crates/macos/src/power.rs:39-73` (a closure guard with release and a warning Drop), and the `FakePower` shape in `fake/power.rs`
Respects: ARCH-9
Interface:
- `trait LidSetting: Send + Sync`
  - `fn hold(&self) -> Result<LidGuard, LidError>` sets the bit.
  - `fn give_back(&self) -> Result<(), LidError>` clears the bit without a guard; used for the restore and the clear retry.
  - `fn sleep_reason(&self) -> Option<String>` reads `Last Sleep Reason`.
- `LidGuard` has `release(self) -> Result<(), LidError>`. Its `Drop` gives the bit back and warns on failure.
- `LidError { Refused { code }, Unavailable }`.
Depth: if this vanished, callers would still need to know only "set it, give it back, read the reason", three calls and one error. The service lookup, the `IOServiceOpen` connection, selector 12 and the CF property read stay inside.
Locality: new files in `crates/macos`, re-exported from `lib.rs`; `power.rs` is left as is.

`MacLid` is a unit struct.
- **Each call:** opens `IOPMrootDomain` (`IOServiceMatching` → `IOServiceGetMatchingService` → `IOServiceOpen`), calls `IOConnectCallScalarMethod(conn, 12, [1 or 0])` and closes the connection. The kernel bit is global and not tied to the connection (research Q4), so nothing is kept open.
- **`sleep_reason`:** `IORegistryEntryCreateCFProperty("Last Sleep Reason")`.
- Every `unsafe` block carries `// SAFETY:`.
- **`FakeLid`:**
  - Records an ordered call log (`Set`, `Clear`).
  - Has `refuse_set(code)`, `refuse_clear(code)` and `allow()`.
  - Reports the current bit and a scripted `reason`.
  - A guard dropped without `release` logs `Clear` (the drop test for LID-1.11).
  - Like `FakePower`, the fake keeps its state in `Arc<Mutex<_>>`, and its lock recovers from a poisoned mutex.

### 2. Power source (`crates/macos/src/power_source.rs`, `fake/power_source.rs`)

Satisfies: LID-2.3, LID-2.5
Reuse: rung 4/5 — the notify(3) key `kIOPSNotifyPowerSource` and IOPS from `objc2-io-kit`'s `ps` feature (already-installed dependency, feature widened)
Respects: ARCH-9, ARCH-16
Surface:
| Reader | Disposition |
|---|---|
| root `Cargo.toml:57` `objc2-io-kit` features `std, pwr_mgt, libc` | replace — add `ps` (and the CF features it pulls); the existing `pwr_mgt` users are unchanged |

Interface:
- `trait PowerSource: Send + Sync`
  - `fn current(&self) -> Result<PowerKind, PowerSourceError>`
  - `fn watch(&self) -> Result<PowerWatch, PowerSourceError>`
- `enum PowerKind { Adapter, Battery }`. `AC Power` maps to Adapter. `Battery Power` and `UPS Power` map to Battery (LID-2.3).
- `PowerWatch` implements `AsRawFd` and has two variants: `Notify { fd, token }` (Mac; Drop = `notify_cancel(token)` only) and `Stream(UnixStream)` (fake, from `PowerWatch::from_stream`; Drop drops the stream). `read_tokens(&self) -> io::Result<usize>` reads every pending 4-byte token. It returns `Ok(n > 0)` when there was a change, `Ok(0)` on EOF (the watch is closed), and `Err(WouldBlock)` when the fd is empty. Callers must go through `AsyncFd::try_io`, so readiness is cleared.
Depth: callers still need to know only "what powers the Mac now" and "an fd that turns readable on a change". The notify registration, the non-blocking flag, the token and cancel, IOPS lookup and CF release stay inside.
Locality: new files in `crates/macos`; nothing existing changes except the feature list.

`MacPowerSource` details:
- **`current`:** `IOPSCopyPowerSourcesInfo` → `IOPSGetProvidingPowerSourceType`.
- **`watch`:**
  - Declares `notify_register_file_descriptor` and `notify_cancel` in a hand-written `extern "C"` block. They are not in `libc` (research Q3); this is the crate's first such block.
  - Sets `O_NONBLOCK` on the fd.
  - `read_tokens` reads the 4-byte tokens until `EAGAIN`. It surfaces `EAGAIN` as `WouldBlock` once nothing was read, and returns 0 on EOF.
- **`FakePowerSource`:**
  - Scripted with `set(kind)`, `fail_current()`, `fail_watch()` and `read_count()`.
  - `fire()` writes to one end of a `UnixStream::pair` that `PowerWatch::from_stream` wraps.
  - `kill()` drops the writer, which gives `Closed`.

### 3. Store delete (`crates/store/src/lib.rs`)

Satisfies: LID-7.6
Reuse: rung 2 — extends `Store` beside `kv_set`/`kv_get` (`lib.rs:200,212`)
Respects: ARCH-12
Interface: `pub fn kv_delete(&self, ns: &str, key: &str) -> Result<()>`. Deleting a missing key is `Ok`.
Depth: n/a — extends `Store`.
Locality: extend; one method and its test. No migration and no schema version change.

### 4. Lid hold in the Awake actor (`modules/awake`)

Satisfies: LID-1.1, LID-1.2, LID-1.3, LID-1.4, LID-1.5, LID-1.6, LID-1.7, LID-1.8, LID-1.9, LID-1.10, LID-1.12, LID-2.1, LID-2.2, LID-2.6, LID-2.7, LID-4.1, LID-4.2, LID-4.3, LID-4.4, LID-7.1, LID-7.3, LID-7.4, LID-7.11, LID-8.1, LID-8.4
Reuse: rung 2 — the actor's `turn` / `note_*` / `needs_retry` / `schedule_retry` loop, the exits-watcher dead-and-retry pattern (`actor.rs:588-614`), `Platform` injection (`infra/platform.rs:47-83`) and kv helpers in `infra/stored.rs`
Respects: ARCH-3, ARCH-10, ARCH-16
Surface:
| Reader | Disposition |
|---|---|
| `Platform::new(power, processes, exits)`: `crates/ffi/tests/awake.rs:31`, `hook_session.rs:25`, `modules/awake/tests/common/mod.rs:135,167` | replace (signature kept). Builders `.with_lid(Arc<dyn LidSetting>)` and `.with_power_source(Arc<dyn PowerSource>)` are added. Defaults fail safe: lid `Unsupported` (refuses) and power `Unsupported` (Battery, no watch). The existing call sites compile unchanged |
| `Platform::mac()`: `crates/ffi/src/app.rs:270`, `modules/awake/tests/real_os.rs:16,76` | replace — wires `MacLid` and `MacPowerSource`. `real_os.rs` never turns `Lid closed` on (charter hard stop) |
| `Problems::first()` (`actor.rs:40`) and its readers: `AwakeState.problem` → `crates/ffi/src/running.rs:319` `unknown_reason`, doctor `awake_check`, level-0 `AwakeRowState` | frozen — untouched; workaround: the lid problem is published apart as `lid_problem` (Decision 10) |
| `needs_retry` (`actor.rs:678`) | replace — also true for a pending clear, a pending start-up restore, a failed marker save while a hold is wanted, or a dead watch while `Lid closed` is on. Never true for a refused or unavailable set, or a failed save of `lid = false` after one |
| `run_loop` quit path (`actor.rs:114`, `ctx.recv() == None`) | replace — releases the lid, then deletes the marker, before returning `Ok(())` |
| `Wants` (`domain/table.rs:109`), `Reconciler`, `Held` | frozen — untouched; the lid is computed beside them so assertions behave as today (LID-7.1) |
| `tray()` (`lib.rs:34-53`) | frozen — reads `leases` and `system_held` only (LID-7.16) |

Interface: no new public API. New `AwakeState` fields (section 5) carry the result. The lid is reconciled inside `turn` between `apply` and `publish`.
Depth: n/a — extends the Awake actor.
Locality: extend `actor.rs` with roughly five private fns (`reconcile_lid`, `restore_lid_once`, `arm_power`, `note_power_dead`, `release_lid_on_quit`), a `power: Option<AsyncFd<PowerWatch>>` and `lid: Option<LidGuard>` field, and the `lid_restored` flag. Extend `stored.rs` with load/save/delete helpers. `Platform` gets two fields. `reconciler.rs` and the `Wants` rule are left alone. `table.rs` changes only where `snapshot()` builds an `AwakeState` literal (section 5).

**Flow, every `turn`:**
1. `want = lid_restored ∧ lid_on ∧ table.wants().system ∧ on_adapter == Some(true)`.
2. If `want` and no guard and no pending clear:
   - Save the marker. On failure: lid problem, retry, stop (LID-1.6).
   - Call `lid.hold()`. Ok → keep the guard and clear the lid problem (LID-1.12).
   - On `Refused` or `Unavailable`: lid problem; set `lid_on = false` in memory; save `lid = false`; emit `lid_changed {on: false}`; delete the marker (LID-1.9). A failed save here is only logged, and nothing is retried, so nothing spins.
3. If not `want` and holding: `guard.release()`.
   - Ok → delete the marker.
   - Err → keep the marker and raise the lid problem (LID-1.7, LID-1.8).
4. A **pending clear** is derived, not stored as a flag: the in-memory marker mirror is set and there is no guard (after `lid_restored`). It retries `give_back()` and then deletes the marker. Because nothing extra is kept, a panic and restart cannot orphan it (design review F7).

**Start-up, in `run_loop` after `load_tracking`, once per process:**
1. Load `lid` (LID-3.5, 3.6). If the marker is present, call `give_back()`, then delete the marker, and set `lid_restored`. If `give_back()` is refused, keep the marker, raise the lid problem and retry (LID-4.1 to 4.4). If the marker read fails, skip the restore (LID-3.13) but set `lid_restored`.
2. If `lid` is on, arm the power watch: `watch()` **first**, then one `current()` read (design review F1).

This runs before `restore_if_needed` and the first `turn` (`actor.rs:93,926`). `want` stays false until `lid_restored` (design review F3).

**The `select!` gets a fourth arm** that awaits `power.readable()`:
- `Ok(n > 0)` → read `current()` once and set `on_adapter`. `publish` pushes only if the value changed (LID-8.1).
- `Ok(0)`, a read error, or a failed `current()` → `note_power_dead`:
  - drop the watch;
  - set `on_adapter = Some(false)`, which releases the lid in the same `turn` (LID-2.1, LID-2.6);
  - raise the lid problem;
  - schedule a retry.

The arm sits after the exits arm and before `ctx.recv()`. It is pending forever while not armed (LID-2.7).

**On quit and on drop:**
- Quit: when `ctx.recv()` returns `None`, release the lid and then delete the marker (LID-1.10).
- Drop of the actor, after a permanent failure (`crates/kernel/src/runtime.rs:435-451`) or an abort: drops `LidGuard`, which clears the bit with the marker left (LID-1.11, LID-8.4).

### 5. `Lid closed` switch, state and events (`modules/awake/src/{action,state,event,lib}.rs`, `infra/stored.rs`)

Satisfies: LID-3.1, LID-3.2, LID-3.3, LID-3.4, LID-3.5, LID-3.6, LID-3.12, LID-3.13, LID-7.2, LID-8.3
Reuse: rung 2 — `AwakeAction::Tracking` and `set_tracking` (`actor.rs:253-274`), `load_tracking`/`save_tracking` (`stored.rs:198-207`), `TRACKING_CHANGED` and its summary helper (`event.rs`)
Respects: ARCH-4
Security: TB-2, TB-3
Surface:
| Reader | Disposition |
|---|---|
| `AwakeAction` catalog order and `risk()` match (`action.rs:63-85`) | replace — `lid` added after `tracking`, `slept` after `lid`, both `Risk::Write` |
| `modules/awake/tests/actions.rs` `sample()` (`:28-52`), ordered names (`:301-319`), spec compare (`:325-337`), decode-all (`:344-352`) | replace — sample payloads and names added |
| `modules/awake/tests/manual.rs:~255-275` and `display.rs:~289` "not ending manual" lists | replace — `lid`, `slept` added |
| `crates/ffi/tests/actions_risk.rs:64` `SAMPLED = 24`, `sample()` `:66-98` | replace — 26, with two samples |
| `AwakeState` JSON readers: exact-JSON tests in `modules/awake/tests/{duration,manual,session,idle,display}.rs`, literal in `tests/tray.rs:25` | replace — new fields added to the expected objects |
| `AwakeState` deserialized by `awake_view` (`crates/ffi/src/app.rs:323`) and the CLI `--json` | replace — `#[serde(default)]` keeps older state readable |
| `AwakeState {` literals: `LeaseTable::snapshot` (`domain/table.rs:692`), `AwakeState::idle` (`state.rs:13`) | replace — new fields set (table defaults; the actor fills them in `publish`) |
| Journal kinds `awake.lease_started`, `awake.lease_ended`, `awake.tracking_changed` | frozen — untouched; two new kinds added beside them (permanent, Compat External) |
| kv `awake/tracking` | frozen — untouched; new keys `lid`, `lid_held`, `last_sleep` |

Interface:
- `AwakeAction::Lid { on: bool }`, JSON `{"lid":{"on":true}}`.
- `AwakeAction::Slept { at_ms: i64 }` (section 6).
- `AwakeState` gains `lid: bool`, `lid_held: bool`, `on_adapter: Option<bool>`, `lid_marker: bool`, `last_sleep_ms: Option<i64>` and `lid_problem: Option<String>`.
- `pub const LID_CHANGED = "awake.lid_changed"` and `SLEPT = "awake.slept"`.
Depth: n/a — extends the Awake Action and state.
Locality: extend the same five files.

`set_lid` copies `set_tracking`:
1. Unchanged value → `Ok(None)` (LID-3.12).
2. Save, and on failure return `ActionError::Failed`, changing nothing (LID-3.2, 3.3).
3. Set `lid_on`.
4. Arm or disarm the power watch.
5. Emit `LID_CHANGED {on}` with a one-sentence summary (LID-3.4).

Read failures at start (`Corrupt` or `Sqlite` from `kv_get`), each raising a lid problem (LID-3.13):
- An unreadable `lid` reads as off.
- An unreadable `last_sleep` leaves `last_sleep_ms` as None, so there is no row.
- Only an unreadable `lid_held` marker skips the start-up restore (§4 Start-up, step 1).

### 6. Last sleep (`modules/awake` handling, `crates/ffi/src/lib.rs` forwarder)

Satisfies: LID-5.1, LID-5.2, LID-5.3, LID-5.8, LID-5.9, LID-7.5
Reuse: rung 2 — `NavoriCore::system_did_wake` and the spawned-dispatch shape in `NavoriCore::dispatch` (`lib.rs:445-487`), the Swift `WakeObserver` token pattern
Respects: ARCH-1, ARCH-2, ARCH-4
Surface:
| Reader | Disposition |
|---|---|
| UniFFI bindings (`NavoriCore` gains `system_will_sleep()`), imported only in `CoreStore` and `NavoriBanner` | replace — bindings regenerated; `system_did_wake` unchanged (LID-7.5) |

Interface:
- `NavoriCore::system_will_sleep(&self)` is synchronous and returns nothing.
- The Action `slept {at_ms}`.
- State field `last_sleep_ms`.
Depth: n/a — extends the wake seam and the Awake Action set.
Locality: extend `crates/ffi/src/lib.rs` (one method) and the actor (one match arm and `record_sleep`); `stored.rs` gets the `last_sleep` helpers.

**`system_will_sleep`:**
1. `at_ms = SystemTime::now()` in ms, taken first.
2. If `shut_down` is set, return.
3. Spawn `kernel.dispatch({module:"awake", action:{"slept":{"at_ms":at_ms}}})`, logging an error if the dispatch fails.

The Mac may sleep before the task runs. The stamp is already taken, so the record may land at wake with the right time (LID-5.1).

**`record_sleep(at_ms)`:**
1. `reason = platform.lid.sleep_reason()`.
2. Save kv `last_sleep = at_ms`. On failure, record the problem and still publish in memory.
3. Set `last_sleep_ms`.
4. Emit `SLEPT {at_ms, reason}` (LID-5.2, 5.3).

Every `slept` call counts (LID-5.8); there is no reason filter. If the dispatch lands only after wake and a dark wake slept again in between, `reason` may describe that later sleep while `at_ms` stays right. This is accepted: the reason is never shown, and appears only in the Journal (design review F10). `sleep_reason` lives on `LidSetting` because both read `IOPMrootDomain`; a third trait would add a seam for one getter.

### 7. Core view and doctor (`crates/ffi/src/{lib,app}.rs`)

Satisfies: LID-6.1, LID-6.2, LID-7.7, LID-7.8
Reuse: rung 2 — `AwakeView`, `From<AwakeState>`, `awake_view`, `awake_check`, `check()` (`app.rs:156,301-338,426-440`)
Respects: ARCH-2
Surface:
| Reader | Disposition |
|---|---|
| `AwakeView` (`lib.rs:117-124`) → Swift `CoreStore`, `crates/ffi/tests/running.rs:37` literal, `tests/awake.rs` | replace — six fields appended; the test literal is updated; existing fields unchanged (LID-7.8) |
| `awake_view` module-down blanking (`app.rs:323-338`) | replace — also blanks `lid_held` and `on_adapter` |
| Doctor order: `Inner::doctor` (`app.rs:72-152`), `ffi/tests/integrations.rs:64-119` and `tests/core.rs:772-790` ordered lists, `tests/core.rs:423` all-ok | replace — `lid` appended after `ports`; both ordered lists gain `lid`; it passes on a fresh start with `Lid closed` off |
| `SettingsView.swift:135` doctor sentence | replace — see section 9 (LID-6.3) |

Interface: `pub fn lid_check(view: Option<&AwakeView>) -> Check`, next to `awake_check`.
- No view → fail, "Awake's state could not be read."
- `lid_problem` present → fail with its text (LID-6.2).
- Otherwise → pass, with the four things always in the detail: `Lid closed on|off · held|not held · adapter|battery|not watched · marker|no marker` (LID-6.1). With the switch off it reads `Lid closed off · not held · not watched · no marker` (plan review F5).
Depth: n/a — extends the doctor.
Locality: extend two files.

### 8. CLI (`apps/cli/src/main.rs`)

Satisfies: LID-3.10, LID-3.11, LID-7.9, LID-7.12
Reuse: rung 2 — `AwakeCommand` and `dispatch_awake` (`main.rs:81-101,269-285`), `print_awake_status` (`:467-491`)
Surface:
| Reader | Disposition |
|---|---|
| `print_awake_status` output, read by people and `apps/cli/tests/cli.rs` | replace — a `Lid closed:` line appended after `Tracking agents:`, which stays (LID-7.12); missing fields read as off for an older core |
| `docs/features/awake.md:22` (Action catalog), `docs/features/awake.md:36` (lists the lid as unsupported), `docs/agents/verify.md:48` (awake commands) | replace — add the `lid` and `slept` Actions, `navori awake lid on\|off`, and adapter-only lid support |

Interface:
- `navori awake lid on|off`: clap `ValueEnum` `OnOff`, sending `{"lid":{"on":bool}}`.
- The status line reads `on`, `off` or `on (needs adapter)` from `lid` and `on_adapter`.
Depth: n/a — extends the CLI.
Locality: extend one file. Dependencies stay `navori-config`, `navori-ipc` and `navori-protocol` (LID-7.9); everything is plain JSON.

### 9. Shell: will-sleep forwarder and Running submenu (`apps/macos`)

Satisfies: LID-2.4, LID-3.7, LID-3.8, LID-3.9, LID-5.4, LID-5.5, LID-5.6, LID-5.7, LID-6.3, LID-7.10, LID-7.13, LID-7.14, LID-7.15, LID-7.16, LID-7.17, LID-8.2, LID-8.5
Reuse: rung 2:
- `WakeObserver` token pattern
- `AwakeSubmenuState.tracking*`
- the `RunningSubmenuItem` / `items` builder
- `MenuRow` / `MenuRowView`
- the static row in `CommandRowView` (`RunningSubmenuView.swift:~136-163`)
- the cascade nodes and `activate` in `StatusBarController.swift:301-327`
- `clockText`'s fixed-locale formatter (`AwakeLeaseMap.swift:44-53`)

Respects: ARCH-1, ARCH-2
Surface:
| Reader | Disposition |
|---|---|
| `WakeObserver` (`Platform/WakeObserver.swift`), `App/AppDelegate.swift:15-20` | replace — adds an `onSleep` token for `willSleepNotification`, wired in the same `!perfFlagWithoutHome` block → `CoreStore.systemWillSleep()` |
| `RunningSubmenuItem` (`RunningState.swift:281`) and its exhaustive `switch` in `RunningSubmenuView.row` (`:24-91`) | replace — `.lastSleep(LastSleepText)` and `.lidClosed` added and drawn |
| `RunningSubmenuState.init(running:problem:commands:)` callers: `CoreStore.swift:224-228`, `RunningStateTests`, `RunningCommandRowsTests` | replace — new `lastSleep:` and `lid:` params defaulted to `nil`, so existing tests compile; ordered-item tests gain `.lidClosed` |
| `AwakeSubmenuState.init(...)` callers: `CoreStore.swift:202-210`, `AwakeSubmenuStateTests:15-30`, `CoreStartupBannerTests:22`, `LeaseLabelFixtureTests:27`, `AwakeLeaseMapTests:54` | replace — `lid: Bool = false` and `onAdapter: Bool? = nil` defaulted; `CoreStore` passes `awake?.lid ?? false` (W1) |
| `AwakeCommand` (`KeepAwakeChoice.swift:41-52`) and `AwakeActionFailure.achieved` (`AwakeRowState.swift:~55-64`) exhaustive switches | replace — `.lid(Bool)` added, emitting `{"lid":{"on":…}}` |
| `StatusBarController.runningCommands` and `activate` (`:301-327`) | replace — `CascadeNode(id:"awake.lid", isActionable:true)` after tracking, and `case "awake.lid"`. `Last sleep` gets no node, so it is not keyboard-reachable or actionable (LID-5.7) |
| `MenuRow` (`MenuRowView.swift`) | replace — optional `valueNote: String?` drawn in secondary colour after `value`; nil for every existing row |
| `AwakeHeldMarks` (`CoreStore.swift:178`, `AwakeRowStateTests.swift:114`) | frozen — untouched; workaround: the lid is not a held mark, it lives on `AwakeSubmenuState` |
| `CoreStore.runningSubmenu` problem slot (`CoreStore.swift:224-228`) | replace — passes `awake.problem ?? awake.lidProblem`; the Core's note still wins inside `RunningSubmenuState` (Decision 10, LID-7.17) |
| `SettingsView.swift:135` sentence | replace — "…, logs, ports and lid." (LID-6.3) |
| Running row dot (`RunningRowState`) and status icon (`tray`) | frozen — untouched (LID-7.16) |
| Note-versus-problem slot (`RunningState.swift:307-309`) | frozen — the Core's note still wins (LID-7.17) |

Interface:
- `func lastSleepText(ms: Int64, now: Date, calendar: Calendar) -> LastSleepText` returns `{ value, spoken }`. It is pure and lives in NavoriPanel, with static cached `DateFormatter`s (`en_US_POSIX`, gregorian, the calendar's time zone).
- `AwakeSubmenuState` gains `lidOn`, `lidValue` (`On`/`Off`), `lidDot` (green when on), `lidNote` (`Needs adapter` when on and `onAdapter != true`) and `lidSpoken`.
- `CoreStore.systemWillSleep()`.
Depth: n/a — extends the shell surfaces.
Locality: extend the files in the table; one new pure file, `NavoriPanel/LastSleepText.swift`. The Swift side does only what the Core published: it reads `last_sleep_ms`, `lid` and `on_adapter` from the snapshot and calls no system API when the menu opens (LID-7.14, LID-8.2).

`lastSleepText` buckets by calendar day relative to `now` (LID-5.6):

| Bucket | `value` | `spoken` |
|---|---|---|
| same day | `HH:mm` | `today at HH:mm` |
| previous day | `Yesterday HH:mm` | `yesterday at …` |
| 2 to 6 days before | `EEE HH:mm` | `Wednesday at …` |
| older, same year | `d MMM` | — |
| older, earlier year | `d MMM yyyy` | — |

Item order (LID-3.7, LID-5.4, LID-5.5):
1. `.lastSleep` and `.separator`, only when `lastSleep != nil`
2. `.keepAwake`, `.trackingAgents`, `.lidClosed`
3. the note or problem slot, as today
4. `.separator`, then the projects or `.nothingRunning`
5. `.separator` and the commands, when there are any
6. `.separator` and `.stopAll`

## UI design

Grounding: lifted 1:1 from the locked brief `docs/design/2026-11-06-awake-lid-and-last-sleep-ui-brief.md` (Surface 2 amended; Surface 3 unchanged). Tokens are the macOS system semantics of the 2026-11-01 brief. No new color.

### Running submenu (Surface 2, amended)

Layout:
- Order: `Last sleep` row · separator · `Keep awake` · `Tracking agents` · `Lid closed` · problem line only when something is wrong · separator · project rows · separator · command rows · separator · `Stop all ports…`.
- Rows are 26 pt, in a panel about 272–300 pt wide.

Components:
- `Last sleep`: rung 2, the `CommandRowView` static-row pattern. Icon `moon`, the time as its value, no Button, no hover pill, no cascade node.
- `Lid closed`: rung 2, `MenuRowView`. Icon `laptopcomputer`, value `On`/`Off`, green dot while on. This is the brief's kit fallback (agent-decided).
- `Needs adapter`: rung 2, a new optional `MenuRow.valueNote` drawn after the value.
- Problem line: `AwakeProblemView`, unchanged.

States:
- `Last sleep` absent: no row and no separator.
- `Last sleep` values: `09:20` · `Yesterday 12:03` · `Wed 11:43` · `30 Oct` · `30 Oct 2025`.
- `Lid closed` off: `Off`, no dot.
- `Lid closed` on, adapter: `On` + green dot.
- `Lid closed` on, battery or not yet known: `On` + green dot + grey `Needs adapter`.
- `Lid closed` refused: the problem line, when neither the Core's note nor the Awake problem takes the slot, and the switch flips to `Off`.
- `Lid closed` hover or keyboard selection: the standard row pill, as `Tracking agents`.

Type & color: as Surface 1 of the 2026-11-01 brief.
- `Last sleep` value and `Needs adapter`: `secondaryLabelColor`.
- Dot: system green.

A11y:
- `Last sleep` is static text, `.accessibilityElement(children: .ignore)`, read as `Last sleep, today at 09:20`.
- `Lid closed` is reached by keyboard through its cascade node and read as `Lid closed, stay awake, on`, plus `, needs adapter` on battery (LID-8.5).

## Seams for testing

Charter hard stop: no test may call the real lid setting.
- Lid-on behavior is tested only through `FakeLid` and `FakePowerSource`, wired with `Platform::new(..).with_lid(..).with_power_source(..)`.
- No test built through `NavoriCore::start_with` / `Platform::mac()` may dispatch `lid {on: true}` or plant a `lid_held` marker.
- `modules/awake/tests/real_os.rs` is excluded from lid tests.

| Seam | Kind | Covers |
|---|---|---|
| Awake Module through the kernel (`modules/awake/tests/common` rig: `dispatch` + `FakeClock` + `FakePower` + `FakeLid` + `FakePowerSource` + in-memory Store) — existing seam, extended | integration | LID-1.1–1.10, 1.12, LID-2.1, 2.2, 2.5–2.7, LID-3.1–3.6, 3.12, 3.13, LID-4.1–4.4, LID-5.2, 5.3, 5.8, 5.9, LID-7.1–7.4, 7.11, LID-8.1, 8.3, 8.4 |
| Awake actor drop (rig: drop the kernel or module while holding) — existing rig | integration | LID-1.11, LID-8.4 |
| `FakeLid` / `FakePowerSource` / `LidGuard` / `PowerWatch::from_stream` (`crates/macos/tests/fakes.rs`) — existing file | unit | LID-1.11, LID-2.3 |
| `Store` (`crates/store` tests) — existing | unit | LID-7.6 |
| `NavoriCore::start_with` on a temp profile (`crates/ffi/tests/core.rs` pattern; real `Platform::mac()` with `Lid closed` off and no marker, so the lid setting is never called. `system_will_sleep()` reads only `Last Sleep Reason`) — existing | integration | LID-5.1, LID-7.5, 7.7, 7.8 |
| `lid_check` / `awake_view` pure functions (`crates/ffi/tests/awake.rs` pattern, as `awake_check`) — existing | unit | LID-6.1, 6.2 |
| CLI binary against a test socket (`apps/cli/tests/cli.rs`) — existing | integration | LID-3.10, 3.11, LID-7.9, 7.12 |
| NavoriPanel pure state (`RunningStateTests`, `AwakeSubmenuStateTests`, new `LastSleepTextTests`) — existing target | unit | LID-2.4, LID-3.7–3.9, LID-5.4–5.7, LID-6.3 (string), LID-7.13, 7.15–7.17, LID-8.5 |
| Menu-open path (`CoreStore` builds `RunningSubmenuState` from the last pushed `CoreSnapshot` only). Verified by a NavoriPanel test that builds every row from a snapshot value, and by the import rule: NavoriPanel and `Features/Panel` import no IOKit; `check-deps` / grep) — existing | unit + integration | LID-8.2 |
| `cargo xtask check-deps` and the Swift import rule — existing | integration | LID-7.9, LID-7.10, LID-7.14 |

How LID-8.4 is tested (design review F6):
- `FakeLid` takes an `on_call` probe. The test passes a closure that reads `kv_get("awake","lid_held")`, so every `Set` and `Clear` records whether the marker was stored at that instant.
- Per-key store failures come from an SQLite trigger installed through `Store::with_conn` on the in-memory store, for example `BEFORE INSERT ON kv WHEN NEW.key='lid_held' … RAISE(ABORT)`.

There are no new seams: every row extends an existing test rig or target.

## Coverage check

- Every requirement ID appears in exactly one `Satisfies:` line:
  - §1: 1.11
  - §2: 2.3, 2.5
  - §3: 7.6
  - §4: 1.1–1.10, 1.12, 2.1, 2.2, 2.6, 2.7, 4.1–4.4, 7.1, 7.3, 7.4, 7.11, 8.1, 8.4
  - §5: 3.1–3.6, 3.12, 3.13, 7.2, 8.3
  - §6: 5.1–5.3, 5.8, 5.9, 7.5
  - §7: 6.1, 6.2, 7.7, 7.8
  - §8: 3.10, 3.11, 7.9, 7.12
  - §9: 2.4, 3.7–3.9, 5.4–5.7, 6.3, 7.10, 7.13–7.17, 8.2, 8.5
- None is deliberately unmapped.
- Requirements changed in design (upstream sync-back, re-surfaced for approval with this design):
  - LID-1.9: also deletes the marker.
  - LID-2.5: the watch is armed only while on.
  - LID-2.6: scoped to while on.
  - LID-2.7: new; no watch while off.
  - LID-5.9: new; the `slept` Action.
  - LID-6.1: `not watched`.
  - LID-8.1: reads counted from arming.
  - LID-8.3: covers `slept`.
  - After the independent design review (`RUN/design-review.md`):
    - LID-1.12: new; when a lid problem clears.
    - LID-4.4: no hold until a restore succeeds.
    - LID-3.13: only an unreadable marker skips the restore.
    - The "problem line" term: the lid problem is published apart from the Awake problem.
