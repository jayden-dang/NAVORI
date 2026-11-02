# Design: Kernel contract extensions

Feature code: KCONTRACT
Status: Approved
Date: 2026-11-02
Requirements: ./requirements.md

## Context

The Kernel runs each Module as a supervised actor (`crates/kernel/src/runtime.rs:313-387`). A Module's `run` is a `select!` over `ctx.recv()` and whatever timers it creates itself; the only one today is the sweep's raw 6-hour `interval` (`crates/kernel/src/system.rs:90`). Modules receive no config. `Config::load` either returns a whole valid file or an error, and on error the composition root replaces the whole file with defaults (`crates/ffi/src/app.rs:156-161`). Default retention is 30 days and the sweep's first tick is immediate, so one mistyped `log_level` runs `DELETE` at 30 days (`crates/store/src/lib.rs:285`). Panel visibility is an `AtomicBool` only the snapshot pump reads (`crates/ffi/src/app.rs:275`). No layer observes sleep or wake. Digests: `.skills/KCONTRACT/scan.md`; clock and sleep facts: `.skills/research/2026-11-02-scheduler-clock-and-sleep-wake.md`; review: `.skills/KCONTRACT/design-review.md`.

The binding constraint is that ROAD-8 must add a Module without editing `crates/kernel`, while the idle budget stays at 0% CPU and ≤ 1 wakeup/s (GOAL-11, ARCH-16). That rules out a shared tick task or central job heap: both add a task, both need deregistration when a `run` dies, and neither has a consumer in this milestone. The design is therefore one small watch channel the Kernel owns, plus a handle a Module creates inside `run` and which dies with it. Timers stay in tokio's wheel, which parks with no timeout when it is empty (tokio 1.54.1 `src/runtime/time/mod.rs:245`). Idle wakeups are unmeasured; `cargo xtask perf` measures them.

The second constraint is that `navori-kernel` may not depend on `navori-macos` or `navori-config` for this. Sleep/wake therefore enters as a pulse from the Shell (it already has an AppKit run loop and needs no new `unsafe`), and config enters as decoded JSON sections handed over by the composition root.

Fresh retrieval (schema 1.1, recipe `fsubr-1.2`) for KCONTRACT: OWNS coverage 2/7. Neighbours PERF and PROCS are term-only with 0 shared paths; observations are empty. This neighbourhood is only as complete as the Files blocks, because CORE, SHELL, CLI, BUILD and KCONTRACT have no `tasks.md`. PROCS names ROAD-7 as the place consumers refresh from (`docs/specs/2026-11-01-platform-and-process-model/requirements.md:135`). No reuse candidate came from retrieval; the rungs below come from reading the code.

Relies on: ARCH-1, ARCH-2, ARCH-3, ARCH-6, ARCH-9, ARCH-10, ARCH-13, ARCH-16. No decision contradicts an invariant, and no decision meets the ADR gate: each is reversible inside this repo (Compat obligation None).

## Decisions

From the confirmed close package (`.skills/KCONTRACT/close-package.md`):

1. Each Module sleeps inside its own `run`, only while it has a deadline. No shared tick task, no central job list, no merging of wakeups. A handle's phase resets on restart.
2. The call takes an absolute deadline the Module computes; `None` means no timer. No `Cadence` type.
3. While the menu is closed, a handle does not fire a timer wake sooner than 30 s after its previous timer wake. Panel and wake-pulse wakes do not count.
4. `Deadline` is `SystemTime`. The handle parks through tokio for the remaining wall-clock duration, and re-arms when the park ends before the deadline.
5. Wake is a one-way pulse from the Shell, through FFI, into a counter. No asleep flag. Dark wake is not its own signal.
6. A bad value falls back to its own default; unreadable or non-TOML files fall back as a whole file. Reports name keys, never raw values or TOML source lines.
7. The sweep deletes only for a valid written retention or an absent one. Otherwise it is paused until the next process start.
8. ROAD-8 chooses JSON or a typed record for Module state. `CoreSnapshot` gains no Module state here.

Settled by this design (the close package left these as high-tweak):

9. **Handle result:** `enum Wake { Due, PanelChanged }`. A panel change must return so the Module can recompute; a wake pulse returns only as `Due`.
10. **Clock for the 30-second gap:** tokio's `Instant` (uptime). The gap limits how often the CPU is woken, which uptime measures, and a manual clock change cannot defeat it.
11. **Sections reach the Kernel as `serde_json::Map`**, set on the builder and decoded in `start()`, so builder call order does not matter. Only `navori-config` knows about TOML.
12. **`config_error: Option<String>` becomes `config_notice: Option<ConfigNotice { title, message }>`.** Rust composes all wording, so the Shell holds no logic (ARCH-1) and wording is testable without Swift.
13. **The wake notification is registered in a new `apps/macos/Navori/Platform/WakeObserver.swift`**, created by `AppDelegate`, which calls `CoreStore.systemDidWake()`. This matches the existing `Platform/` layout.
14. **Wall clock seam:** a `WallClock` trait with a production `SystemClock`, fixed when the builder is created so the built-in `system` Module sees it too. "No new clock type" rules out a second deadline clock, such as a monotonic one; this trait is the test seam for the one `SystemTime` (`docs/standards/testing.md:16`, which lists the clock as a boundary). Modules see `SystemTime` through `handle.now()`.
15. **The sweep uses the wait handle.** A paused sweep is `None`, so no timer is armed, and the `system` Module's periodic work goes through the Scheduler.

## Architecture

Vocabulary for every section below: **module**, **interface**, **implementation**, **seam** (public surface where behavior is observable and substitutable).

A Module using everything, as ROAD-8 will write it:

```rust
impl Module for Awake {
    const ID: &'static str = "awake";
    type Action = AwakeAction;
    type State = AwakeState;
    type Config = AwakeConfig;

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        let config = ctx.config();
        let mut wait = ctx.wait_handle();
        loop {
            let next = self.next_expiry(wait.now(), &config);   // Option<SystemTime>
            tokio::select! {
                req = ctx.recv() => { let Some(req) = req else { return Ok(()) }; /* … */ }
                wake = wait.wait_until(next) => match wake {
                    Wake::Due => self.expire(ctx, wait.now()),
                    Wake::PanelChanged => {}                     // loop recomputes `next`
                },
            }
        }
    }
}
```

### navori-config — per-value load

Satisfies: KCONTRACT-2.1, KCONTRACT-2.2, KCONTRACT-2.6, KCONTRACT-2.7, KCONTRACT-2.8, KCONTRACT-G5
Reuse: rung 2 — extends `Config::load` (`crates/config/src/lib.rs:125`); rung 5 — `toml::Table`, and `serde_json`, already a workspace dependency, added to this crate
Respects: ARCH-13
Surface:
- `crates/ffi/src/app.rs` `load_config` and `build_kernel` — replace
- `crates/config/tests/config_file.rs` `out_of_range_values_are_rejected`, `malformed_toml_is_rejected_with_the_file_path` and the `unwrap()` calls on `load` — replace
- `ConfigError` (only these readers) — replace; the type is deleted
- `apps/cli/src/main.rs` imports only `Paths` and `Profile` — frozen by not touching them; no change
- `docs/codebase/modules.md:18`, `docs/architecture/data.md:21,23` — replace
Interface:
```rust
impl Config { pub fn load(path: &Path) -> ConfigLoad }        // infallible (ARCH-13)
pub struct ConfigLoad {
    pub config: Config,                                       // effective values
    pub sweep: JournalSweep,                                  // KeepDays(u32) | Paused
    pub sections: serde_json::Map<String, Value>,             // every other top-level key
    pub report: ConfigReport,                                 // Clean | BadValues(Vec<String>) | WholeFile(WholeFileReason)
}
pub enum WholeFileReason { Unreadable, NotToml { position: Option<(usize, usize)> } }   // line, column
```
Depth: n/a — extends `Config::load`.
Locality: `extend` `crates/config/src/lib.rs`; `leave` `Paths` and `Profile`.

`load` parses to `toml::Table` first, which only fails on syntax. A missing file gives defaults, `KeepDays(30)`, `Clean`. `log_level` is decoded and checked on its own, then `journal`; a failure records the key and uses that value's default, so a failing `journal` also gives `Paused`. A valid or absent `retention_days` gives `KeepDays(n)`, including a `[journal]` table with no key. The key reported is `journal.retention_days` when `journal` is a table holding that key, else `journal`. Errors from `toml` and `serde` are dropped, because their text can hold the raw value. `ConfigLoad` holds raw section values, so its `Debug` is manual and prints keys and counts only (`missing_debug_implementations` is denied in CI). `NotToml` line and column come from `toml::de::Error::span()` over the raw text, never from its `Display`, which prints the source line. `span()` returns an `Option` (toml 1.1.8 `src/de/error.rs:69`); when it is `None`, the notice names the file and says it is not valid TOML, still without using `Display`. Remaining top-level keys convert to JSON for the Kernel; unknown ones are simply never looked up.

### Kernel — Module config and supervision

Satisfies: KCONTRACT-1.1, KCONTRACT-1.2, KCONTRACT-1.3, KCONTRACT-1.4, KCONTRACT-1.5, KCONTRACT-1.6, KCONTRACT-G1, KCONTRACT-G2, KCONTRACT-G3, KCONTRACT-G9
Reuse: rung 2 — extends `Module`, `ModuleCtx` and `KernelBuilder` (`crates/kernel/src/module.rs:34,85`, `runtime.rs:77`)
Respects: ARCH-3, ARCH-10, ARCH-13
Surface:
- `impl Module for Counter` (`crates/kernel/tests/kernel.rs:75`) — replace: adds `type Config = NoConfig;`, assertions unchanged
- `impl Module for System` (`crates/kernel/src/system.rs:80`) — replace: same line
- `Launcher` type and `start()` (`runtime.rs:58,181`) — replace
- `crates/kernel/src/lib.rs:15` re-exports — replace
Interface:
```rust
pub trait ModuleConfig: DeserializeOwned + Default + Send + Sync + 'static {
    fn check(&self) -> Result<(), &'static str> { Ok(()) }    // &'static str cannot hold a raw value
}
pub struct NoConfig;                                          // decodes from anything
trait Module { type Config: ModuleConfig; /* … */ }
impl ModuleCtx<M> { pub fn config(&self) -> Arc<M::Config> }
impl KernelBuilder { pub fn config_sections(self, Map<String, Value>) -> Self }
impl Kernel { pub fn config_issues(&self) -> &[&'static str] } // IDs of Modules on their default
```
Depth: n/a — extends `Module`.
Locality: `extend` `module.rs`, `runtime.rs`; `leave` `bus.rs`, dispatch and supervision logic.

`start()` hands each launcher the sections. The launcher finds the section named `M::ID`, decodes it, runs `check`, and on failure uses `M::Config::default()` and records the ID. The serde error is dropped. `ModuleCtx` holds the `Arc`, and the supervisor reuses the same `ctx` on restart, so a restarted `run` sees the same value with no reload path. `NoConfig` implements `Deserialize` through `serde::de::IgnoredAny`, so a `[system]` section is ignored rather than reported.

### Kernel — Scheduler and wait handle

Satisfies: KCONTRACT-4.1, KCONTRACT-4.2, KCONTRACT-4.3, KCONTRACT-4.4, KCONTRACT-4.5, KCONTRACT-4.6, KCONTRACT-4.7, KCONTRACT-4.8, KCONTRACT-4.9, KCONTRACT-4.10, KCONTRACT-4.11, KCONTRACT-5.1, KCONTRACT-5.2, KCONTRACT-5.3, KCONTRACT-6.2, KCONTRACT-6.3, KCONTRACT-6.4, KCONTRACT-6.5, KCONTRACT-6.6, KCONTRACT-6.7, KCONTRACT-7.1, KCONTRACT-7.3
Reuse: none — new code (rung 7), because no existing type waits on a wall-clock deadline; built on rung 5 `tokio::sync::watch` and `tokio::time::sleep`
Respects: ARCH-10, ARCH-16
Interface:
```rust
impl Kernel { pub fn scheduler(&self) -> &Scheduler }
impl Scheduler {
    pub fn wait_handle(&self) -> WaitHandle;
    pub fn set_panel_visible(&self, visible: bool);
    pub fn panel_visible(&self) -> bool;
    pub fn wake_pulse(&self);
}
impl ModuleCtx<M> { pub fn wait_handle(&self) -> WaitHandle }
impl WaitHandle {
    pub async fn wait_until(&mut self, deadline: Option<SystemTime>) -> Wake;   // cancel-safe
    pub fn panel_visible(&self) -> bool;
    pub fn now(&self) -> SystemTime;
}
pub enum Wake { Due, PanelChanged }
pub trait WallClock: Send + Sync { fn now(&self) -> SystemTime; }
impl Kernel { pub fn builder_with_clock(store: Store, clock: Arc<dyn WallClock>) -> KernelBuilder }
```
Depth: if this vanished, every Module would have to carry its own re-arm loop against the wall clock, the 30-second gap, pulse handling and cancel-safe bookkeeping; it would still need to know only its own next deadline.
Locality: new `crates/kernel/src/scheduler.rs`; `extend` `Plumbing` with the `Scheduler` (which holds the clock) and `ModuleCtx` with the `Scheduler` and the config; `leave` everything else.

`Scheduler` wraps an `Arc<watch::Sender<Inputs { panel_visible, panel_changes: u64, wake_pulses: u64 }>>` and spawns nothing (4.8, ADR-free). Counters rather than a bare bool mean two quick flips still end a wait (5.1). `set_panel_visible` uses `send_if_modified`, so a repeated value changes nothing. `wake_pulse` increments even with no receiver (6.6).

`WaitHandle` owns a clone of the `Scheduler`, a `watch::Receiver`, the last seen counters and `last_timer_wake: Option<tokio::time::Instant>`. Each `wait_until` loops:

1. `panel_changes` moved: record it, return `PanelChanged` (5.1). This also reports a change that happened between waits, so none is lost.
2. Note whether `wake_pulses` moved, without recording it yet.
3. `None`: await the channel and go to 1. No timer exists (4.2, 6.4). A pulse stays unrecorded here, so a deadline given by the next call can still answer it.
4. `Some(d)`: `remaining` is `d` minus `clock.now()`, zero if past. Record the pulse count. If a pulse had moved and `remaining` is zero, return `Due` without touching `last_timer_wake` (6.2, 5.3).
5. `park` is the larger of `remaining` and, only while the panel is closed and a previous timer wake exists, what is left of 30 s since it (4.9, 4.10, 4.11).
6. `select!` `sleep(park)` against the channel. Channel first: go to 1 (6.3). Sleep first: if `clock.now()` is at or past `d`, set `last_timer_wake` and return `Due`; otherwise go to 1 (4.3, 6.5, 6.7).

Dropping the future loses nothing (4.4, 4.5): the panel count is recorded in the poll that returns it, `last_timer_wake` in the poll that returns `Due`, and a pulse only once a deadline has answered it. `watch::changed` is documented cancel-safe. `run` may hold `ctx` in another `select!` arm the same way `crates/kernel/src/system.rs:94-108` does today. The handle is a local of `run`: it is created fresh after a restart (4.7), and the `Sleep` drops with the future when `catch_unwind` finishes (4.6). `Due` is only returned after comparing against `clock.now()` in that same poll, which is what 7.3 tests.

### Kernel — journal sweep

Satisfies: KCONTRACT-3.1, KCONTRACT-3.2, KCONTRACT-3.4, KCONTRACT-3.7, KCONTRACT-3.9, KCONTRACT-G4, KCONTRACT-G10, KCONTRACT-G11
Reuse: rung 2 — extends `System` (`crates/kernel/src/system.rs:55`) and the builder
Respects: ARCH-6, ARCH-16
Surface:
- `System.retention_days` (`system.rs:57,63,105`) and `KernelBuilder.retention_days` (`runtime.rs:83,103,232`) — replace with one shared policy
- `SystemState` (`system.rs:49`) read through `Kernel::state` and `crates/kernel/tests/kernel.rs` — replace: gains `sweep_paused: bool`
- `the_system_module_prunes_the_journal_using_the_configured_retention` (`kernel.rs:377`) — frozen: must pass unchanged
- `docs/architecture/runtime.md:54` — replace
Interface:
```rust
impl KernelBuilder {
    pub fn journal_retention_days(self, days: u32) -> Self;   // unchanged, still raises to ≥ 1
    pub fn pause_journal_sweep(self) -> Self;
}
SystemState { last_prune_removed, sweep_paused }
```
Depth: n/a — extends `System`.
Locality: `extend` `system.rs`, `runtime.rs` builder; `leave` `Store::journal_prune` and the `PruneJournal` action.

The shared `AtomicU32` becomes `Arc<Mutex<SweepPolicy>>` with `SweepPolicy::{KeepDays(u32), Paused}`, default `KeepDays(30)` (G10). A pause is therefore a state and not a number, and it is not sent through `journal_retention_days`. `initial_state` runs at registration (`runtime.rs:123`), before the builder options apply, so each launcher in `start()` republishes `module.initial_state()` through the State channel before it spawns. `sweep_paused` is therefore right as soon as `start()` returns, with no window. `run` keeps `next_sweep: Option<SystemTime>`, set to `Some(wait.now())` for `KeepDays` (so the sweep still runs at start) and `None` when paused, and sets it to `wait.now()` plus 6 hours after each sweep, so a long sleep gives one catch-up sweep. Today's `MissedTickBehavior::Delay` also fires no burst of missed ticks: it schedules the next tick one period after `tick()` was called. Nothing can turn a paused sweep back on, because config is read once per process (3.7). `PruneJournal` still takes its own day count and does not look at the policy (3.9).

### navori-ffi — composition root and report

Satisfies: KCONTRACT-2.3, KCONTRACT-2.4, KCONTRACT-2.5, KCONTRACT-2.9, KCONTRACT-2.10, KCONTRACT-3.3, KCONTRACT-3.5, KCONTRACT-3.6, KCONTRACT-3.8, KCONTRACT-5.4, KCONTRACT-7.2, KCONTRACT-G6, KCONTRACT-G7
Reuse: rung 2 — extends `load_config`, `build_kernel`, `doctor`, `snapshot_pump` (`crates/ffi/src/app.rs`)
Respects: ARCH-1, ARCH-2, ARCH-6, ARCH-13
Security: TB-6, THR-3
Surface:
- `CoreSnapshot.config_error` (`crates/ffi/src/lib.rs:91,348`) and generated Swift — replace with `config_notice`
- `Inner.config_error` (`app.rs:28,59`, `lib.rs:192`) — replace
- `CoreStore.health` (`apps/macos/Navori/Core/CoreStore.swift:91-92`), `PanelView` banner (`Features/Panel/PanelView.swift:57-58`) — replace
- `an_invalid_config_falls_back_to_defaults_and_is_reported` (`crates/ffi/tests/core.rs:160-168`), `crates/ffi/tests/core.rs:71` — replace: read `config_notice`, still assert `config.toml`
- `Inner.panel_visible` (`app.rs:31,275`, `lib.rs:194,298`) — replace: the pump reads `kernel.scheduler().panel_visible()`, so there is one source of truth
- Settings Diagnostics tab and `navori doctor` read the `doctor` checks — frozen shape: `Check` is unchanged, only its text changes
- `docs/codebase/modules.md:22`, `docs/ARCHITECTURE.md:122` (seed list of the FFI surface) — replace
Interface:
```rust
#[derive(uniffi::Record)] pub struct ConfigNotice { pub title: String, pub message: String }
CoreSnapshot { .., config_notice: Option<ConfigNotice> }
impl NavoriCore { pub fn system_did_wake(&self); }              // new; set_panel_visible keeps its signature
```
Depth: n/a — extends `navori-ffi`.
Locality: `extend` `app.rs`, `lib.rs`; `leave` `session.rs`, `logging.rs`, the IPC bridge.

`load_config` returns `ConfigLoad`. `build_kernel` applies `journal_retention_days` or `pause_journal_sweep` from `sweep`, and `config_sections` from `sections`. After the Kernel starts, one function merges `report` with `kernel.config_issues()` into `Option<ConfigNotice>`. All text the user sees comes from there:

| Case | Title | Message |
|---|---|---|
| Some values bad | `config.toml has bad values` | `log_level, awake use their defaults. Fix them and reopen Navori.` plus the pause line if paused |
| Whole file | `config.toml is on defaults` | `Not valid TOML at line 3, column 7.`, `Not valid TOML.` (no span) or `It cannot be read.` plus the pause line |
| Pause line | | `Journal cleanup is paused until the file is fixed and Navori is reopened.` |

This fits `docs/standards/errors-logging.md:10`, which asks for what went wrong and how to fix it, with the path or value involved: the path and key are enough. The `tracing::warn!` at `lib.rs:167` moves after `build_kernel` and logs the same notice text. `doctor` emits it in the `config` check with `ok = false`, and the `— running with defaults` suffix goes away (2.4). `system_did_wake` calls `wake_pulse` and logs one `info` line, which also gives the dark-wake spike a timestamp to compare with `pmset -g log`.

### Shell — wake observer and notice

Satisfies: KCONTRACT-6.1, KCONTRACT-G8, KCONTRACT-G12
Reuse: rung 2 — the `setPanelVisible` path (`apps/macos/Navori/Core/CoreStore.swift:57`); rung 4 — `NSWorkspace.shared.notificationCenter`
Respects: ARCH-1, ARCH-2
Surface:
- `AppDelegate.applicationDidFinishLaunching`, `applicationWillTerminate` (`apps/macos/Navori/App/AppDelegate.swift:11,25`) — replace: own the observer
- `StatusPanel.swift:84,114` calling `setPanelVisible` — frozen by not touching them; no change
Interface: `CoreStore.systemDidWake()`; `WakeObserver(onWake:)`, which removes its observer on deinit.
Depth: n/a — a thin adapter over a framework notification.
Locality: new `Platform/WakeObserver.swift`; `extend` `CoreStore.swift`, `AppDelegate.swift`. `leave` the other `NavoriCore` imports (out of scope).

`WakeObserver` registers for `NSWorkspace.didWakeNotification` on `NSWorkspace.shared.notificationCenter` with `queue: .main`. A stuck main thread makes the pulse late, which the design accepts. The notice replaces `configError` in `CoreStore.health`, in the same place in the order (G12), and `PanelView` shows `title` and `message` with no wording of its own. Apple's pages do not state which thread a notification arrives on without a queue, so the explicit `.main` removes the question; the research note marks dark-wake behaviour unverified.

### Documentation updates (infrastructure — no Satisfies)

- `docs/standards/performance.md:41`, `docs/architecture/system.md:60`: replace "paused during sleep" with what now holds: a timer parks on uptime, and an overdue timer may fire during dark wake.
- `docs/architecture/system.md:60`: Scheduler to Built; `docs/architecture/runtime.md:54`; `docs/architecture/data.md:21,23`; `docs/codebase/modules.md:18,22`.
- `docs/ARCHITECTURE.md:70-103` seed: delete the Scheduler bullet (including battery at `:102`) and the `Config` line, and correct `run(self, ctx)` and `ActionMeta` to the code. `context()`, `tray()` and `ModuleHandle` stay in the seed for MILE-3.

## Seams for testing

One new seam: the wait handle, reached through the Kernel's public API with a fake `WallClock` and `tokio::time::pause`. Advancing the fake wall clock without advancing tokio reproduces real sleep, where uptime stands still. The rest are existing seams. `Kernel::builder_with_clock` is what lets a test advance past the 6-hour sweep (3.7), because `tokio::time::pause` alone never moves `SystemTime`.

| Seam | Kind | Covers |
|---|---|---|
| `Config::load` (`crates/config/tests/config_file.rs`) | integration | KCONTRACT-2.1, 2.2, 2.6, 2.7, 2.8, G5; `sweep` values for 3.2, 3.3 |
| Kernel public API, test Module with config (`crates/kernel/tests/kernel.rs`) | integration | KCONTRACT-1.1 – 1.6, G1, G2, G3, G4, G9, G10, G11, 3.1, 3.4, 3.7, 3.9 |
| Wait handle through the Kernel, fake clock (`crates/kernel/tests/scheduler.rs`, new) | integration | KCONTRACT-4.1 – 4.11, 5.1 – 5.3, 6.2 – 6.7 |
| Same seam, property test using a seeded xorshift generator, with no new test dependency (same approach as PROCS, `docs/specs/2026-11-01-platform-and-process-model/design.md:94`) | property | KCONTRACT-7.3 |
| `NavoriCore` (`crates/ffi/tests/core.rs`) | integration | KCONTRACT-2.3 – 2.5, 2.9, 2.10, 3.2, 3.3, 3.5, 3.6, 3.8, 5.4, 7.2, G6 |
| `navori` CLI over IPC (`apps/cli/tests`) | integration | KCONTRACT-G7, and the doctor side of 2.10 |
| Shell, manual per `docs/standards/testing.md` (Swift AppKit exempt) | manual | KCONTRACT-6.1, G8, G12, and the banner side of 2.3 – 2.5, 3.5 |
| `cargo xtask perf` | e2e | KCONTRACT-7.1 |

## Coverage check

Every requirement ID appears in exactly one `Satisfies:` line above, walked top to bottom:

- 1.1 – 1.6, G1, G2, G3, G9: Module config and supervision.
- 2.1, 2.2, 2.6, 2.7, 2.8, G5: per-value load.
- 2.3, 2.4, 2.5, 2.9, 2.10, 3.3, 3.5, 3.6, 3.8, 5.4, 7.2, G6, G7: composition root.
- 3.1, 3.2, 3.4, 3.7, 3.9, G4, G10, G11: sweep.
- 4.1 – 4.11, 5.1, 5.2, 5.3, 6.2 – 6.7, 7.1, 7.3: Scheduler and wait handle.
- 6.1, G8, G12: Shell.

Deliberately unmapped: none. No requirement is delivered through a browser-rendered surface, so the UI design section is removed; the only UI change is text in the existing native Banner.

## Still open

- A Module can still call `tokio::time::interval` directly. This design does not block that path, and that stays open.
- A Module that returns an already-past deadline repeatedly while the panel is open spins, because 4.10 applies no gap. That is a Module bug the wait handle does not guard against.
- Handoff notes to ROAD-8 stay notes: a Failed Module still exposes its last JSON, and a serialize failure becomes `Null` with no log (`crates/kernel/src/runtime.rs:155`). Two more from review: a TOML date-time in a Module section reaches the Module as a private-key JSON object, and a Module ID of `journal` or `log_level` can never receive a section.
- The requirements' three Open Questions stay open: the roadmap surface line, the dark-wake spike and `perf-baseline.json`.
- Nothing in this design has been compiled or run.
