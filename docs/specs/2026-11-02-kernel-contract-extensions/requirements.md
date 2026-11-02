# Requirements: Kernel contract extensions

Feature code: KCONTRACT
Status: Implemented
Date: 2026-11-02

<!--
Rules:
- Every acceptance criterion gets a hierarchical ID: KCONTRACT-<story>.<criterion>.
- Criteria use EARS phrasing (WHEN / WHILE / IF…THEN / WHERE / ubiquitous).
- Guard requirements protect existing behavior: WHEN <condition> THE SYSTEM SHALL CONTINUE TO <behavior>.
- IDs are immutable once Status is Approved. Retire by strikethrough — never renumber.
-->

Roadmap item: ROAD-7 (MILE-2) · Tracker: #5 · Decisions: frame-change close package (2026-11-02), confirmed by the user.

Terms used below:
- **The system** — `navori-kernel`, `navori-config`, `navori-ffi` and the Shell code that reports panel visibility, wake and config errors.
- **Wait handle** — the object a Module's `run` holds to wait for a deadline. It lives inside `run` and is separate from `ModuleCtx`.
- **Deadline** — an absolute wall-clock time (`SystemTime`) that the Module computes, or `None` for no deadline.
- **Due** — the result a wait handle returns when the deadline has been reached.
- **Timer wake** — a wait handle waking because its timer ended, as opposed to a panel change or a wake pulse.
- **Wake pulse** — the one-way signal from the Shell, through FFI, into a counter, sent when macOS reports that the Mac did wake.
- **Bad value** — a value in a `config.toml` that is valid TOML, where the value fails to decode or decodes and then fails its later check. `log_level = 42`, `log_level = "loud"`, `retention_days = "365"` and `retention_days = 0` are bad values. `retention_days` belongs to `journal`, so a bad `retention_days` makes `journal` a bad value.
- **Whole-file fallback** — `config.toml` exists but cannot be read, or is not valid TOML. No value can be trusted, so none of it is a bad value in the sense above.
- **The sweep** — the `system` Module's periodic Journal deletion, at start and every 6 hours.

## 1. Give a Module its own config

**Story:** As the developer building Awake, I want to declare a config type on my Module and read it inside `run`, so that an `[awake]` section reaches Awake without any edit to `crates/kernel`.

- **KCONTRACT-1.1** WHEN a Module with a config type starts THE SYSTEM SHALL give its `run` the value decoded from the `config.toml` section named by the Module's ID.
- **KCONTRACT-1.2** IF that section is absent THEN THE SYSTEM SHALL give `run` the config type's default.
- **KCONTRACT-1.3** IF that section is a bad value THEN THE SYSTEM SHALL give that Module alone the config type's default.
- **KCONTRACT-1.4** WHEN the supervisor restarts a Module THE SYSTEM SHALL give the new `run` the same config value the Module had before the restart.
- **KCONTRACT-1.5** WHERE a Module takes no config THE SYSTEM SHALL ignore any `config.toml` section named by its ID.
- **KCONTRACT-1.6** THE SYSTEM SHALL offer the Module config and the wait handle through the public API of `navori-kernel` — verified by a test Module in `crates/kernel/tests` that uses both through that API only.

## 2. Keep a bad config value to itself

**Story:** As the user, I want a mistyped value in `config.toml` to change only that value and to be named, so that one typo does not reset the rest of my settings.

- **KCONTRACT-2.1** IF `log_level` or `journal` is a bad value THEN THE SYSTEM SHALL replace that value alone with its own default.
- **KCONTRACT-2.2** IF `config.toml` exists but cannot be read or is not valid TOML THEN THE SYSTEM SHALL use the defaults for the whole file.
- **KCONTRACT-2.3** WHEN one or more values are bad THE SYSTEM SHALL name `config.toml` and each bad key in the panel banner, the status-item health text and `navori doctor`, which the Settings Diagnostics tab also shows.
- **KCONTRACT-2.4** WHILE only some values are bad THE SYSTEM SHALL NOT say in the banner, the health text or `navori doctor` that the whole file is on defaults.
- **KCONTRACT-2.5** WHEN `config.toml` falls back as a whole file THE SYSTEM SHALL say in the banner, the health text and `navori doctor` that the whole file is on defaults, naming the file.
- **KCONTRACT-2.6** (guard) WHEN `config.toml` is missing THE SYSTEM SHALL CONTINUE TO use the defaults without reporting an error.
- **KCONTRACT-2.7** (guard) WHEN `config.toml` sets only some keys THE SYSTEM SHALL CONTINUE TO use the defaults for the keys it does not set.
- **KCONTRACT-2.8** (guard) WHEN `config.toml` holds unknown keys THE SYSTEM SHALL CONTINUE TO ignore them without reporting an error.
- **KCONTRACT-2.9** (guard) WHEN `config.toml` holds a bad value THE SYSTEM SHALL CONTINUE TO start and keep running.
- **KCONTRACT-2.10** (guard) WHEN `config.toml` holds a bad value THE SYSTEM SHALL CONTINUE TO fail the `config` check of `navori doctor`.

## 3. Pause the sweep when retention cannot be trusted

**Story:** As the user, I want the sweep to pause rather than delete when my retention setting is broken, so that a typo cannot erase my Journal history.

- **KCONTRACT-3.1** (guard) WHEN `retention_days` is a valid number the user wrote THE SYSTEM SHALL CONTINUE TO have the sweep delete Events older than that many days.
- **KCONTRACT-3.2** WHEN `retention_days` is absent — no file, no `journal`, or a `[journal]` table without the key — THE SYSTEM SHALL have the sweep delete Events older than 30 days.
- **KCONTRACT-3.3** IF `journal` is a bad value, or `config.toml` falls back as a whole file, THEN THE SYSTEM SHALL pause the sweep so it deletes nothing.
- **KCONTRACT-3.4** WHILE the sweep is paused THE SYSTEM SHALL report that in the `system` Module's state, readable through `Kernel::state`.
- **KCONTRACT-3.5** WHILE the sweep is paused THE SYSTEM SHALL say so in the panel banner and `navori doctor`.
- **KCONTRACT-3.6** WHILE the sweep is paused THE SYSTEM SHALL keep every other Module running.
- **KCONTRACT-3.7** WHILE the process runs THE SYSTEM SHALL keep a paused sweep paused, even if the file is fixed — verified by a Kernel test that advances past the next 6-hour sweep with nothing deleted, and a Core test that fixes the file and still sees the pause reported.
- **KCONTRACT-3.8** WHEN the process starts with a valid or absent `retention_days` THE SYSTEM SHALL run the sweep as KCONTRACT-3.1 and KCONTRACT-3.2 say.
- **KCONTRACT-3.9** (guard) WHEN `PruneJournal` is dispatched with an explicit day count THE SYSTEM SHALL CONTINUE TO delete Events older than that count, including while the sweep is paused.

## 4. Wait for a deadline inside `run`

**Story:** As the developer building Awake, I want to wait inside `run` until a Lease's expiry time, so that the Lease ends on time without my own timer or any polling.

- **KCONTRACT-4.1** WHEN `run` waits with a deadline THE SYSTEM SHALL return Due once wall time is at or past that deadline.
- **KCONTRACT-4.2** WHEN `run` waits with no deadline THE SYSTEM SHALL arm no timer.
- **KCONTRACT-4.3** WHEN the timer ends and wall time is still before the deadline THE SYSTEM SHALL arm again for the remaining wall-clock duration and SHALL NOT return Due.
- **KCONTRACT-4.4** THE SYSTEM SHALL let `run` wait on the wait handle and on `ctx.recv()` in the same `select!`.
- **KCONTRACT-4.5** WHEN a wait is dropped before it finishes and `run` waits again THE SYSTEM SHALL keep the handle's record of its previous timer wake.
- **KCONTRACT-4.6** WHEN a `run` ends, by returning or by a panic, THE SYSTEM SHALL leave no timer of that run's wait handle armed.
- **KCONTRACT-4.7** WHEN a Module restarts THE SYSTEM SHALL start its new wait handle with no record of a previous timer wake.
- **KCONTRACT-4.8** THE SYSTEM SHALL add no task, thread or list of armed waits shared across Modules — verified by a test that counts the runtime's live tasks with and without wait handles.
- **KCONTRACT-4.9** WHILE the menu is closed THE SYSTEM SHALL NOT let a wait handle return from a timer wake sooner than 30 seconds after its previous timer wake, so a Due may be up to 30 seconds late.
- **KCONTRACT-4.10** WHILE the menu is open THE SYSTEM SHALL apply no minimum gap between timer wakes.
- **KCONTRACT-4.11** WHEN a wait handle has no previous timer wake THE SYSTEM SHALL NOT delay its first timer wake.

## 5. End a wait when the panel changes

**Story:** As the developer building a periodic Module, I want a wait in progress to end with a result other than Due when the panel changes, with the handle reporting whether the panel is visible, so that a Module can compute a new deadline, as Ports will for its 3-second and 30-second polls.

- **KCONTRACT-5.1** WHEN panel visibility changes THE SYSTEM SHALL end every wait in progress, with or without a deadline, with a result distinct from Due.
- **KCONTRACT-5.2** WHEN `run` asks its wait handle THE SYSTEM SHALL report whether the panel is currently visible.
- **KCONTRACT-5.3** WHEN a panel change or a wake pulse wakes a wait handle THE SYSTEM SHALL NOT count that wake toward the 30-second gap.
- **KCONTRACT-5.4** (guard) WHEN the Shell reports panel visibility THE SYSTEM SHALL CONTINUE TO coalesce snapshot pushes at 33 ms while the panel is open and 250 ms while it is closed.

## 6. See a deadline that passed during sleep

**Story:** As the developer building Awake, I want a deadline that passed while the Mac slept to return as soon as the Mac wakes, so that a one-hour Lease does not run on for the time the Mac was asleep.

- **KCONTRACT-6.1** WHEN macOS reports that the Mac did wake THE SYSTEM SHALL send one wake pulse from the Shell, through `CoreStore` and FFI, into a counter.
- **KCONTRACT-6.2** WHEN a wake pulse arrives and wall time is at or past the deadline THE SYSTEM SHALL return Due without waiting for the 30-second gap.
- **KCONTRACT-6.3** WHEN a wake pulse arrives and wall time is before the deadline THE SYSTEM SHALL arm again for the remaining wall-clock duration and return nothing to `run`.
- **KCONTRACT-6.4** WHEN a wake pulse arrives and a wait has no deadline THE SYSTEM SHALL keep waiting and arm no timer.
- **KCONTRACT-6.5** IF no wake pulse arrives after a sleep THEN THE SYSTEM SHALL still return Due at a later timer wake, so the deadline is late and not lost.
- **KCONTRACT-6.6** WHEN a wake pulse arrives and no wait handle exists THE SYSTEM SHALL record it in the counter without error.
- **KCONTRACT-6.7** IF the system clock is changed by hand THEN THE SYSTEM SHALL judge the deadline against the new wall time at its next check.

## 7. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** **KCONTRACT-7.1** WHEN `cargo xtask perf` runs on a build with this feature THE SYSTEM SHALL stay within the GOAL-11 budgets, including `idle_wakeups_per_s` ≤ 1/s, first menu open ≤ 80 ms and later menu opens ≤ 50 ms — verified by that run. The comparison against a baseline applies only once `perf-baseline.json` exists.
- **Security:** **KCONTRACT-7.2** THE SYSTEM SHALL NOT write the raw text of a bad value, or a TOML source snippet, to logs, Events, the Journal, the banner, the health text or `navori doctor` (THR-3 at TB-6) — verified by a test that plants a secret-shaped value in a bad section and in malformed TOML and searches every one of those outputs.
- **Reliability:** **KCONTRACT-7.3** THE SYSTEM SHALL NOT return Due while wall time at the moment of return is before the deadline — verified by a property test with a fake clock over random sequences of timer ends, wake pulses, panel changes and clock changes.
- **Accessibility:** None — no standing target (`docs/standards/accessibility.md` is absent), and this feature only changes the text inside the existing Banner and doctor output.

## Files touched and guards

| File | Existing behavior | Guard |
|---|---|---|
| `crates/kernel/src/module.rs` | Restart keeps `self`, the State and the queued inbox; `ctx.recv()` yields `None` at shutdown | KCONTRACT-G1, KCONTRACT-G2 below |
| `crates/kernel/src/runtime.rs` | `register`, dispatch, `Kernel::state`, supervision with backoff, a permanently failed Module drops its inbox, shutdown within 5 s; builder default retention and its minimum of 1 | KCONTRACT-G1, KCONTRACT-G3, KCONTRACT-G9, KCONTRACT-G10 |
| `crates/kernel/src/system.rs` | `Ping`; `PruneJournal`; sweep at start and every 6 h; a failed sweep only logs a warning | KCONTRACT-3.1, KCONTRACT-3.9, KCONTRACT-G4, KCONTRACT-G11 |
| `crates/kernel/src/lib.rs` | Re-exports only | no behavior to guard |
| `crates/kernel/tests/kernel.rs` | Assertions stay as they are; a test Module may gain a declaration that it takes no config | KCONTRACT-G1 |
| `crates/config/src/lib.rs` | Missing file, partial file, unknown keys, malformed TOML names the file. Today's messages hold the raw value and the TOML source line; KCONTRACT-7.2 removes that on purpose | KCONTRACT-2.6, KCONTRACT-2.7, KCONTRACT-2.8, KCONTRACT-G5, KCONTRACT-7.2 |
| `crates/config/tests/config_file.rs` | `out_of_range_values_are_rejected` asserts today's whole-file error; KCONTRACT-2.1 replaces that behavior on purpose | KCONTRACT-2.1 |
| `crates/ffi/src/app.rs` | `load_config`, `doctor`, `build_kernel`, `snapshot_pump` | KCONTRACT-2.9, KCONTRACT-2.10, KCONTRACT-5.4 |
| `crates/ffi/tests/core.rs` | `an_invalid_config_falls_back_to_defaults_and_is_reported` asserts a report that names `config.toml`; KCONTRACT-2.3 keeps that, and the test changes only if the field's type changes | KCONTRACT-2.3, KCONTRACT-2.9 |
| `crates/ffi/src/lib.rs` | `CoreSnapshot` module views unchanged; `config_error` field changes to carry KCONTRACT-2.3; `set_panel_visible`; `shutdown` idempotent | KCONTRACT-5.4, KCONTRACT-G6 |
| `crates/protocol/src/lib.rs` | Touched only if `design-solution` needs a shared type; wire types stay compatible with the CLI | KCONTRACT-G7 |
| `apps/macos/Navori/Core/CoreStore.swift` | `setPanelVisible`; health text and its order (failed, then restarting, then config) | KCONTRACT-G8, KCONTRACT-G12 |
| `apps/macos/Navori/Features/Panel/PanelView.swift` | Config banner, whose fixed title "config.toml is invalid — using defaults" KCONTRACT-2.4 replaces on purpose for a partial fallback | KCONTRACT-2.3, KCONTRACT-2.4 |
| `apps/macos/Navori/StatusBar/StatusPanel.swift` | Panel show and close report visibility | KCONTRACT-5.4 |
| The Swift file that registers the wake notification | Chosen in `design-solution`; no existing wake behavior | no behavior to guard |
| `docs/standards/performance.md:41`, `docs/architecture/system.md:60`, `docs/ARCHITECTURE.md:102` and the seeds this feature lifts | Wording only | no behavior to guard |

Guards named above:

- **KCONTRACT-G1** (guard) WHEN a Module crashes THE SYSTEM SHALL CONTINUE TO restart it with backoff, keeping `self`, its State and its queued inbox.
- **KCONTRACT-G2** (guard) WHEN the Kernel shuts down THE SYSTEM SHALL CONTINUE TO make `ctx.recv()` yield `None`.
- **KCONTRACT-G3** (guard) WHEN a caller dispatches an Envelope to a running Module THE SYSTEM SHALL CONTINUE TO return that Module's reply, and `Kernel::state` SHALL CONTINUE TO return the Module's State as JSON.
- **KCONTRACT-G4** (guard) WHEN `Ping` is dispatched THE SYSTEM SHALL CONTINUE TO answer `pong`.
- **KCONTRACT-G5** (guard) WHEN `config.toml` is malformed THE SYSTEM SHALL CONTINUE TO name the file in the error.
- **KCONTRACT-G6** (guard) WHEN `shutdown` is called more than once THE SYSTEM SHALL CONTINUE TO treat it as idempotent.
- **KCONTRACT-G7** (guard) WHEN the CLI talks to the Core over IPC THE SYSTEM SHALL CONTINUE TO serve `status`, `doctor`, `events`, `actions` and `dispatch`.
- **KCONTRACT-G8** (guard) WHEN the Shell reports panel visibility THE SYSTEM SHALL CONTINUE TO make that FFI call through `CoreStore.setPanelVisible`.
- **KCONTRACT-G9** (guard) WHEN a Module exhausts its restarts THE SYSTEM SHALL CONTINUE TO mark it failed and answer later callers with `Unavailable`.
- **KCONTRACT-G10** (guard) WHEN `Kernel::builder` is used with no retention setting THE SYSTEM SHALL CONTINUE TO sweep at 30 days, and `journal_retention_days` SHALL CONTINUE TO raise a value below 1 to 1.
- **KCONTRACT-G11** (guard) WHEN a sweep fails THE SYSTEM SHALL CONTINUE TO log a warning and keep the Module running.
- **KCONTRACT-G12** (guard) WHEN several problems exist at once THE SYSTEM SHALL CONTINUE TO put a failed or restarting Module ahead of a config problem in the health text.

## Out of Scope

- `context()` and `tray()`. `context()` waits for MILE-3.
- Battery mode. The Scheduler has two inputs: panel visibility and sleep/wake.
- A `Cadence` type. Periodic Modules write their own next deadline.
- A shared tick task, a central job list, merging wakeups across Modules, and a job list in `navori doctor`.
- An asleep flag, and registering dark wake as its own signal. An overdue timer may still return Due if the process runs during dark wake.
- Wake pulses or panel visibility over the IPC socket or the CLI.
- Module state in `CoreSnapshot`, and the choice of JSON or a typed record. ROAD-8 starts by answering that.
- Reloading config while the app is running.
- Saving a last-good retention. Not now, which is not never; see below.
- Staying awake with the lid closed (`docs/ARCHITECTURE.md:307`).
- Imports of `NavoriCore` outside `CoreStore`.
- Lease, Reconciler, TTL and grace values — ROAD-8.

## Comes back if

These conditions reopen a decision. They are not requirements.

- A menu row cannot be built from `Kernel::state` without a kernel edit: state crossing returns to ROAD-7.
- ROAD-8 defines the hour as time the machine was awake: the deadline switches to `Instant`.
- A real Lease goes wrong because the system clock was changed: a monotonic clock.
- ROAD-8 must list armed jobs: the central list.
- ROAD-8 cannot accept a Lease ending 30 seconds late: the gap.
- A second periodic Module copies the same next-deadline logic: a `Cadence` helper, without changing this call.
- The Journal grows enough while the sweep is paused to hit the disk-write or footprint budget: a saved retention.
- A Module needs another Module's config, or the app must reload config while running: the per-value split.
- Measured `idle_wakeups_per_s` exceeds the baseline by more than 0.2/s because timers stagger: this wait model.
- A measured dark wake runs Module work and the battery cost is clear: an asleep flag.
- `didWake` does not fire after a full wake: the IOKit path.

`kIOPMAssertionTimeoutKey` is not the Lease clock.

## Open Questions

- Whether the ROAD-7 surface line in `docs/roadmap/INDEX.md` is edited — Jayden Đặng — 2026-11-02 — forbid-guess.
- Dark-wake spike: whether a timer runs during dark wake and whether `didWake` fires then; needs one real sleep — Jayden Đặng — 2026-11-02 — forbid-guess.
- `perf-baseline.json` is missing, so the regression check in KCONTRACT-7.1 and the wait-model trigger above cannot fire yet (PERF Task 12) — Jayden Đặng — 2026-11-02 — forbid-guess.

Not run yet: the delete path against a real Journal, a timer during dark wake, and a row built from `Kernel::state`. The criteria above are how they get verified; they are not results.
