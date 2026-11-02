# Requirements: Awake leases

Feature code: AWAKE
Status: Approved
Date: 2026-11-02

<!--
Rules:
- Every acceptance criterion gets a hierarchical ID: AWAKE-<story>.<criterion>.
- Criteria use EARS phrasing (WHEN / WHILE / IF…THEN / WHERE / ubiquitous).
- Guard requirements protect existing behavior: WHEN <condition> THE SYSTEM SHALL CONTINUE TO <behavior>.
- IDs are immutable once Status is Approved. Retire by strikethrough — never renumber.
-->

Roadmap item: ROAD-8 (MILE-2) · Tracker: #7 · Decisions: frame-change close package (2026-11-02), confirmed by the user.

Delivery order: stories 1–10 ship first and do not wait for MENU. Story 11 starts only after MENU (ROAD-34) merges.

Terms used below:
- **The system** — the Awake Module, `navori-ffi`, the `navori` CLI and the Shell code that shows Awake.
- **Lease** — as in `CONTEXT.md`: a request to keep the Mac awake from one source.
- **Manual Lease** — a Lease the user makes by hand: a duration, or until turned off.
- **Command Lease** — a Lease owned by a command run through `navori awake run`.
- **Session Lease** — a Lease owned by the Harness process of one Agent session, keyed by `session_id`.
- **First hold** — the time of the first session-hold Action for a `session_id` while it has a Lease.
- **Stop mark** — the time of the session-stop Action that started the grace period.
- **Grace** — 120 seconds after the Stop mark.
- **Recheck** — one reading of the Harness's descendants, every 60 seconds after grace.
- **Cap** — 2 hours after the Stop mark.
- **Session TTL** — 8 hours after the latest session-hold Action.
- **New descendant** — a live descendant of the Harness process whose start time is after the First hold.
- **System assertion**, **Display assertion** — `PreventUserIdleSystemSleep` and `PreventUserIdleDisplaySleep`.

## 1. Keep the Mac awake for a fixed time

**Story:** As the user, I want to keep the Mac awake for one hour from the terminal, so that a job I am watching is not stopped by idle sleep.

- **AWAKE-1.1** WHEN the user asks for a duration Lease THE SYSTEM SHALL create a Manual Lease whose deadline is the absolute wall-clock time at the end of that duration.
- **AWAKE-1.2** WHILE at least one Lease exists THE SYSTEM SHALL hold exactly one System assertion, however many Leases exist — verified by `pmset -g assertions` listing one for the Navori process.
- **AWAKE-1.3** WHEN a Lease's deadline passes THE SYSTEM SHALL end that Lease.
- **AWAKE-1.4** WHILE the menu is closed THE SYSTEM SHALL end an expired Lease no more than 30 seconds after its deadline, unless the Mac was asleep at the deadline.
- **AWAKE-1.5** WHEN the Mac sleeps during a duration Lease THE SYSTEM SHALL keep that Lease's deadline unchanged, so that sleep time is subtracted from the duration.
- **AWAKE-1.6** WHEN the Mac wakes after a Lease's deadline has passed THE SYSTEM SHALL end that Lease.
- **AWAKE-1.7** WHEN the last Lease ends THE SYSTEM SHALL release the System assertion.
- **AWAKE-1.8** THE SYSTEM SHALL use a fixed one-hour length for the menu's duration command.

## 2. Keep the Mac awake until I turn it off

**Story:** As the user, I want to keep the Mac awake until I turn it off, then turn it off with Release all, so that I decide when it ends.

- **AWAKE-2.1** WHEN the user asks for an until-off Lease THE SYSTEM SHALL create a Manual Lease with no deadline.
- **AWAKE-2.2** WHEN the user asks for Release all THE SYSTEM SHALL end every Lease.
- **AWAKE-2.3** WHEN the user asks for Release all THE SYSTEM SHALL delete every stored Lease row.
- **AWAKE-2.4** WHEN the app reopens after Release all THE SYSTEM SHALL NOT restore a Lease that Release all ended.
- **AWAKE-2.5** THE SYSTEM SHALL offer Release all as its only user command for ending Leases, apart from turning `Display on` off.
- **AWAKE-2.6** THE SYSTEM SHALL NOT have a Pause state.
- **AWAKE-2.7** THE SYSTEM SHALL NOT have an Action that puts the Mac to sleep.

## 3. Keep the Mac awake while a command runs

**Story:** As the user, I want to run `navori awake run -- <cmd>`, so that the Mac stays awake exactly as long as that command runs.

- **AWAKE-3.1** WHEN the user runs `navori awake run -- <cmd>` THE SYSTEM SHALL start `<cmd>` and ask the Core for a Command Lease with the pid of `<cmd>`.
- **AWAKE-3.2** THE SYSTEM SHALL resolve that pid to a process identity in the Module, not in the CLI.
- **AWAKE-3.3** WHEN the watched command exits THE SYSTEM SHALL end its Command Lease, with no release message from the CLI.
- **AWAKE-3.4** THE SYSTEM SHALL give a Command Lease no deadline, so that a Command Lease has no TTL.
- **AWAKE-3.5** WHEN `<cmd>` exits THE SYSTEM SHALL exit `navori awake run` with the exit code of `<cmd>`, including exit code 3.
- **AWAKE-3.6** IF the Core is not running when `navori awake run` is called THEN THE SYSTEM SHALL still run `<cmd>`.
- **AWAKE-3.7** IF no Lease could be made because the Core is not running THEN THE SYSTEM SHALL write one line on stderr saying there is no Lease because Navori is not running.
- **AWAKE-3.8** THE SYSTEM SHALL NOT open the app from the CLI.
- **AWAKE-3.9** IF the process has already exited when the Command Lease is asked for THEN THE SYSTEM SHALL end any Lease for it as soon as the exit is known.

## 4. Hold and end a Lease for one session

**Story:** As the developer building the hook, I want to hold and end a Lease for a `session_id` with short Actions through `navori dispatch`, so that the hook work needs no change to the Module.

- **AWAKE-4.1** WHEN the session-hold Action arrives with a `session_id`, a pid and a parent pid THE SYSTEM SHALL resolve the Harness and its process identity from those pids in the Module.
- **AWAKE-4.2** THE SYSTEM SHALL NOT take a Harness name or a process identity from the caller of a session Action.
- **AWAKE-4.3** WHEN the Harness is resolved THE SYSTEM SHALL create a Session Lease for that `session_id` with a Session TTL deadline.
- **AWAKE-4.4** WHEN the session-hold Action arrives for a `session_id` that already has a Lease THE SYSTEM SHALL keep one Lease for it.
- **AWAKE-4.5** WHEN the session-hold Action arrives for a `session_id` that already has a Lease THE SYSTEM SHALL move its deadline to a new Session TTL from now.
- **AWAKE-4.6** WHEN the session-hold Action arrives for a `session_id` that already has a Lease THE SYSTEM SHALL keep its First hold unchanged.
- **AWAKE-4.7** WHEN the session-end Action arrives with a `session_id` THE SYSTEM SHALL end that Session Lease, finding it by `session_id` alone with no process lookup.
- **AWAKE-4.8** WHEN the Harness process of a Session Lease exits THE SYSTEM SHALL end that Lease.
- **AWAKE-4.9** WHEN a Session Lease's Session TTL deadline passes THE SYSTEM SHALL end that Lease.
- **AWAKE-4.10** WHEN a session Action is answered THE SYSTEM SHALL not hold the request open for the length of grace or any later wait.

## 5. See a stopped session release or stay

**Story:** As the developer building the hook, I want to send the session-stop Action and see the Lease end after grace unless a new descendant is alive, so that background work keeps the Mac awake and a finished session does not.

- **AWAKE-5.1** WHEN the session-stop Action arrives for a Session Lease with no Stop mark THE SYSTEM SHALL set its Stop mark to now.
- **AWAKE-5.2** WHEN the session-stop Action arrives for a Session Lease with a Stop mark THE SYSTEM SHALL keep the existing Stop mark.
- **AWAKE-5.3** IF the session-stop Action arrives for a `session_id` with no Lease THEN THE SYSTEM SHALL change no Lease.
- **AWAKE-5.4** WHEN grace ends THE SYSTEM SHALL read the Harness's descendants once.
- **AWAKE-5.5** IF that reading holds no New descendant THEN THE SYSTEM SHALL end the Lease.
- **AWAKE-5.6** IF that reading holds a New descendant THEN THE SYSTEM SHALL keep the Lease and read again at the next Recheck.
- **AWAKE-5.7** THE SYSTEM SHALL NOT count a descendant whose start time is at or before the First hold as busy.
- **AWAKE-5.8** THE SYSTEM SHALL NOT use CPU time to decide whether a session is busy.
- **AWAKE-5.9** WHEN the Cap is reached THE SYSTEM SHALL end the Lease, whatever the reading holds.
- **AWAKE-5.10** WHEN the Cap ends a Lease THE SYSTEM SHALL emit an Event whose summary states the Cap as the reason.
- **AWAKE-5.11** WHEN the session-hold Action arrives for a Session Lease with a Stop mark THE SYSTEM SHALL clear the Stop mark.
- **AWAKE-5.12** THE SYSTEM SHALL use 120 seconds for grace, 60 seconds for Recheck, 2 hours for the Cap and 8 hours for the Session TTL as constants in the Module.
- **AWAKE-5.13** THE SYSTEM SHALL read no `[awake]` config section, and a stray `[awake]` table SHALL NOT raise a config error.
- **AWAKE-5.14** WHERE the code is test code THE SYSTEM SHALL accept shorter timing values passed to the Module's constructor.

## 6. Keep a job's Lease across an app reopen

**Story:** As the user, I want to quit and reopen Navori while a watched process runs and see the assertion come back, so that an app restart does not let the Mac sleep through that work.

- **AWAKE-6.1** WHEN a Session Lease or a Command Lease changes THE SYSTEM SHALL store it with its pid, the process start time, its First hold when it has one, its Stop mark when it has one, and its deadline when it has one.
- **AWAKE-6.2** THE SYSTEM SHALL NOT store a Manual Lease.
- **AWAKE-6.3** WHEN the app closes THE SYSTEM SHALL end every Manual Lease.
- **AWAKE-6.4** WHEN the app opens with a stored Lease whose process still has the stored start time THE SYSTEM SHALL restore that Lease.
- **AWAKE-6.5** WHEN a Lease is restored THE SYSTEM SHALL hold the assertions it needs.
- **AWAKE-6.6** WHEN the app opens with a stored Lease whose process is gone, or whose pid now has another start time THE SYSTEM SHALL delete that Lease.
- **AWAKE-6.7** WHEN the app opens with a stored Session Lease whose deadline has passed THE SYSTEM SHALL end that Lease.
- **AWAKE-6.8** WHEN a Session Lease with a Stop mark is restored THE SYSTEM SHALL count grace and the Cap from the stored Stop mark, not from the time the app opened.
- **AWAKE-6.9** WHEN the app opens with a stored Session Lease whose Cap has passed THE SYSTEM SHALL end it and emit the Event in AWAKE-5.10.
- **AWAKE-6.10** WHEN a restored Lease's process exits THE SYSTEM SHALL end that Lease.

## 7. Keep the display on for a Lease that asks for it

**Story:** As the user, I want to press `Display on` and see the display stay on until I turn it off, so that the screen stays readable while the Mac is held awake.

- **AWAKE-7.1** WHILE a Lease asks for the display THE SYSTEM SHALL hold exactly one Display assertion in addition to the System assertion.
- **AWAKE-7.2** WHEN no Lease asks for the display any longer THE SYSTEM SHALL release the Display assertion.
- **AWAKE-7.3** WHEN the Display assertion is released while a Lease exists THE SYSTEM SHALL keep the System assertion, with no moment without it.
- **AWAKE-7.4** IF macOS refuses the Display assertion THEN THE SYSTEM SHALL keep the System assertion.
- **AWAKE-7.5** IF macOS refuses the Display assertion THEN THE SYSTEM SHALL report that the display is not held.

## 8. See it when macOS refuses to keep the Mac awake

**Story:** As the user, I want `navori doctor` and Awake's status to say when a Lease exists but the System assertion is not held, so that Navori never claims the Mac is held when it is not.

- **AWAKE-8.1** IF macOS refuses the System assertion THEN THE SYSTEM SHALL keep the Lease.
- **AWAKE-8.2** IF macOS refuses the System assertion THEN THE SYSTEM SHALL record the problem in Awake's state.
- **AWAKE-8.3** WHILE a Lease exists with no System assertion THE SYSTEM SHALL try to hold it again about every 60 seconds.
- **AWAKE-8.4** WHEN a retry succeeds THE SYSTEM SHALL clear the problem from Awake's state.
- **AWAKE-8.5** WHILE no Lease exists THE SYSTEM SHALL NOT retry.
- **AWAKE-8.6** WHILE a Lease exists with no System assertion THE SYSTEM SHALL NOT report the System assertion as held in the CLI, the snapshot or `navori doctor`.
- **AWAKE-8.7** WHEN the user runs `navori doctor` THE SYSTEM SHALL show one fixed `awake` row, which fails while the problem exists.
- **AWAKE-8.8** IF an explicit release of an assertion fails THEN THE SYSTEM SHALL record the problem in Awake's state.
- **AWAKE-8.9** WHILE the Awake Module is `failed` THE SYSTEM SHALL NOT report any assertion as held in the CLI, the snapshot or `navori doctor`.

## 9. Find Awake's Actions and Events

**Story:** As the user, I want `navori actions` to list Awake's Actions and `navori events` to show what Awake did, so that any caller can see how to drive it and what happened.

- **AWAKE-9.1** THE SYSTEM SHALL expose every Awake capability as an Action with a JSON schema, a description written for people and LLMs, and a Risk.
- **AWAKE-9.2** THE SYSTEM SHALL mark Awake's status Action `read`.
- **AWAKE-9.3** THE SYSTEM SHALL mark Awake's other Actions `write`.
- **AWAKE-9.4** THE SYSTEM SHALL NOT mark an Awake Action `destructive`.
- **AWAKE-9.5** WHEN a Lease starts or ends THE SYSTEM SHALL emit an Event with a one-sentence summary.
- **AWAKE-9.6** THE SYSTEM SHALL offer `navori awake` commands for a duration Lease, an until-off Lease, Release all and the status.

## 10. Read Awake from the Core snapshot

**Story:** As the developer building the Awake row, I want a typed Awake record on `CoreSnapshot`, so that Swift reads Awake without decoding JSON.

- **AWAKE-10.1** THE SYSTEM SHALL declare the Awake record in `navori-ffi`.
- **AWAKE-10.2** THE SYSTEM SHALL carry the Awake record on `CoreSnapshot`.
- **AWAKE-10.3** THE SYSTEM SHALL build the Awake record through the Module's Rust state type.
- **AWAKE-10.4** THE SYSTEM SHALL keep `modules/awake` free of a dependency on `uniffi`.
- **AWAKE-10.5** THE SYSTEM SHALL NOT decode Awake's state from JSON in Swift.
- **AWAKE-10.6** WHEN Awake's state changes THE SYSTEM SHALL carry the change in the next snapshot.

## 11. Keep the Mac awake from the menu

**Story:** As the user, I want to open the Awake row, press `Keep awake for 1 hour`, see the Lease under Holding and press `Release all`, so that I do all of this from the menubar.

This story starts only after MENU merges. It changes three things in MENU's spec: MENU-1.2 gains the Awake row before the Core row; MENU-1.4 no longer lists the Awake row among what Release code hides; and MENU's "The Awake row ships: decide whether the Core row stays" is answered: it stays.

- **AWAKE-11.1** THE SYSTEM SHALL lay out level 0 in this order: any banners, the Awake row, the Core row, a separator, `Settings…`, `About Navori`, `Quit Navori`.
- **AWAKE-11.2** WHILE no Lease exists THE SYSTEM SHALL show `Off` on the Awake row.
- **AWAKE-11.3** WHILE a Lease exists with the System assertion held THE SYSTEM SHALL show `On` and the names of the sources holding on the Awake row.
- **AWAKE-11.4** WHILE a Lease exists with no System assertion THE SYSTEM SHALL NOT show `On` on the Awake row.
- **AWAKE-11.5** WHEN the Awake row opens THE SYSTEM SHALL show `Keep awake for 1 hour`, `Keep awake until I turn it off`, the status block, `Display on`, `Release all` and a Holding row for each current Lease.
- **AWAKE-11.6** THE SYSTEM SHALL NOT show `Pause 15m`, `Let my Mac sleep now`, an Agent sessions row, an Integrations row or a per-Lease release button in the Awake Submenu.
- **AWAKE-11.7** THE SYSTEM SHALL write menu text about Navori's own Leases only.
- **AWAKE-11.8** THE SYSTEM SHALL NOT say in the menu that the Mac will sleep.
- **AWAKE-11.9** THE SYSTEM SHALL NOT say in the menu that another app is holding the Mac awake.
- **AWAKE-11.10** WHEN the app opens the Awake Submenu THE SYSTEM SHALL NOT read the process table or the system's assertions.

## 12. Keep what exists working

**Story:** As the user, I want every existing command and menu row to behave as before, so that adding Awake breaks nothing I rely on.

Files this feature touches, with what they guard:

| File | Guard |
|---|---|
| `modules/awake/` (new crate) | no behavior to guard |
| `docs/features/awake.md` (new, at delivery) | no behavior to guard |
| `Cargo.toml`, `crates/ffi/Cargo.toml` | no behavior to guard |
| `crates/ffi/src/app.rs` | AWAKE-12.1, AWAKE-12.2, AWAKE-12.4 |
| `crates/ffi/src/lib.rs` | AWAKE-12.3 |
| `crates/macos/src/fake/power.rs`, `crates/macos/src/fake/processes.rs` (gain per-kind refusal, lifting a refusal, a failing release and a `list` counter, so AWAKE-7.4, 8.4, 8.8 and 13.3 can be tested) | AWAKE-12.12 |
| `apps/cli/src/main.rs` | AWAKE-12.5, AWAKE-12.6 |
| `apps/macos/Navori/Core/CoreStore.swift` | AWAKE-12.7 |
| `apps/macos/Navori/Features/Panel/`, `apps/macos/Navori/StatusBar/`, `apps/macos/NavoriPanel/` | AWAKE-12.8, AWAKE-12.9, from the MENU merge |
| `docs/ARCHITECTURE.md`, `docs/architecture/integrations.md`, `docs/design/2026-11-01-menubar-ui-brief.md`, MENU's `requirements.md` | no behavior to guard |

- **AWAKE-12.1** (guard) WHEN `navori doctor` runs THE SYSTEM SHALL CONTINUE TO report the `config`, `database`, `ipc`, `modules` and `logs` checks.
- **AWAKE-12.2** (guard) WHEN the Kernel is built THE SYSTEM SHALL CONTINUE TO run the `system` Module with its Journal sweep.
- **AWAKE-12.3** (guard) WHEN a snapshot is pushed THE SYSTEM SHALL CONTINUE TO carry every field `CoreSnapshot` has today.
- **AWAKE-12.4** (guard) WHEN snapshots are pushed THE SYSTEM SHALL CONTINUE TO coalesce them at 33 ms while the panel is visible and 250 ms while it is not.
- **AWAKE-12.5** (guard) WHEN a `navori` command other than `awake run` finds the app not running THE SYSTEM SHALL CONTINUE TO exit 3.
- **AWAKE-12.6** (guard) WHEN `cargo xtask check-deps` runs THE SYSTEM SHALL CONTINUE TO allow the CLI only `navori-config`, `navori-ipc` and `navori-protocol`.
- **AWAKE-12.7** THE SYSTEM SHALL NOT import the generated bindings in any new Awake Swift file, reading Awake through `CoreStore` (ARCH-2). Three existing files outside `CoreStore` import them today; that is out of scope here.
- **AWAKE-12.8** (guard) WHERE MENU has merged, WHEN the Core row is shown THE SYSTEM SHALL CONTINUE TO open its Submenu with module states and at most 8 recent Events.
- **AWAKE-12.9** (guard) WHERE MENU has merged and the app runs the dev profile THE SYSTEM SHALL CONTINUE TO show the `DEV` chip on the Core row.
- **AWAKE-12.10** (guard) WHEN a Module fails and restarts THE SYSTEM SHALL CONTINUE TO restart it with backoff, keeping `self`, its State and its queued requests.
- **AWAKE-12.11** (guard) WHILE two Profiles run THE SYSTEM SHALL CONTINUE TO keep their databases, sockets and logs separate.
- **AWAKE-12.12** (guard) WHEN an existing test calls `FakePower::refuse` THE SYSTEM SHALL CONTINUE TO refuse every assertion kind.

## 13. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** **AWAKE-13.1** WHEN `cargo xtask perf` runs with Awake registered and no Lease THE SYSTEM SHALL stay inside the GOAL-11 budget in `docs/standards/performance.md`: idle 0% CPU, ≤ 1 wakeup/s, ≤ 30 MB footprint, cold start ≤ 200 ms, first menu open ≤ 80 ms and later opens ≤ 50 ms.
- **Performance:** **AWAKE-13.2** WHILE no Lease exists THE SYSTEM SHALL give the wait handle no deadline — verified by a Module test on paused tokio time: advancing one hour causes no process read and no assertion call.
- **Performance:** **AWAKE-13.3** WHEN the panel opens THE SYSTEM SHALL NOT take a process snapshot — verified by a test counting `list` calls on the fake process source.
- **Performance:** **AWAKE-13.4** WHILE the menu is closed THE SYSTEM SHALL wake the Awake Module's timer no sooner than 30 seconds after its previous timer wake (ARCH-16), waiting on one wait handle for the life of `run` — verified by a Module test on paused tokio time with a fake `WallClock`.
- **Security:** **AWAKE-13.5** WHEN a hook payload is forged or repeated THE SYSTEM SHALL keep at most one Lease per `session_id`, ending by process exit or Session TTL (THR-6) — verified by Module tests with fakes.
- **Security:** **AWAKE-13.6** THE SYSTEM SHALL NOT store argv, environment or a working directory in a Lease row, or put them in an assertion name (ARCH-6, THR-3) — verified by a test reading the stored row and the fake assertion's name.
- **Security:** **AWAKE-13.7** THE SYSTEM SHALL build `modules/awake` as `navori-mod-awake` with `unsafe_code = forbid` inherited from the workspace (ARCH-9) — verified by `cargo xtask check-deps`.
- **Security:** **AWAKE-13.11** THE SYSTEM SHALL add no hand-written `unsafe` anywhere for this feature — verified in review of the diff, since `navori-ffi` has no lint forbidding it.
- **Reliability:** **AWAKE-13.8** WHEN the Awake Module restarts after a failure THE SYSTEM SHALL keep its Leases and their held assertions — verified by a test that fails the Module with `FakePower` in place.
- **Reliability:** **AWAKE-13.9** WHEN the Awake Module fails permanently THE SYSTEM SHALL release its assertions — verified with `FakePower`.
- **Accessibility:** **AWAKE-13.10** WHEN the Awake Submenu is shown THE SYSTEM SHALL make every command reachable by keyboard and read every row and button as one sentence by VoiceOver, following MENU's rules. No `docs/standards/accessibility.md` exists — verified by a manual walk with VoiceOver on.

## Out of Scope

- Installing hooks and `navori hook` — ROAD-9.
- Where the Integrations row sits — ROAD-9 chooses.
- A background scan for Harness processes.
- The Agent sessions row, the session list and `Show terminal`. Later work, not ROAD-9. No roadmap item yet; the buttons are not locked.
- `Pause 15m`, `Let my Mac sleep now` and a real sleep command.
- Reading which other apps hold the Mac awake.
- Timing options: no `[awake]` section and no Settings tab.
- Staying awake with the lid closed.
- **DoD item 5, a stated gap:** the context fragment and the eval case are owned by MILE-3. `context()` waits for MILE-3 (KCONTRACT Out of Scope) and no AI runtime or eval runner exists. Item 5 is therefore only partly met when ROAD-8 closes.

## Not locked

Recorded so that nobody reads these as requirements.

- `Null` state is not drawn as an empty Lease list.
- Release all does not kill the command.
- Grok's `Stop` and `StopCancelled` stay in the hook command.
- The right-aligned time text on the menu.
- Heavy work only in a process born before the First hold, spawning no child, is missed by the busy rule.
- A turn over 8 hours raises only the Session TTL.
- After ROAD-9, the next prompt makes a new Lease; Release all does not block it.
- Release all in the status block or among the quick commands.
- If the numbers need changing while running, a Settings tab that applies at once.
- A static sentence that other apps may hold the Mac, only if `Off` is misread.

## Must measure

These are not known facts. No requirement here assumes them unless it says how it is verified.

- The OS reclaiming an assertion with `navori-macos` when the process dies.
- Two assertions at once on `pmset`.
- The Display assertion alone stopping idle sleep.
- Recovery time after an app reopen.
- A Lease sleeping through its deadline.
- Timers during dark wake.
- A real `Stop` trace through this flow, and `caffeinate` at `Stop`.
- Background work after `Stop`.
- `watch` across an app reopen, and `kill -9` on the CLI.
- Whether the real session ids of Claude Code, Codex and Grok fit `[A-Za-z0-9._:-]`, 1 to 128 bytes.
- The cost of `Snapshot::take` against the menu budget.

## Comes back if

These conditions reopen a decision. They are not requirements.

- A Harness the user runs has no hooks: reopen the problem statement, and whether the 1-hour button may be the only holder while the app can reopen unattended.
- ROAD-9 runs and the Mac must sleep while an agent keeps running: Pause.
- The user often needs to sleep from this menu, or often cannot tell why the Mac stays awake after ending a Lease: sleep-now, the other-apps line.
- A real hook event cannot be expressed as a command by `session_id`: the Action shape.
- A `Stop` trace shows a helper not exiting so the Lease is not released, or a real job runs only in an old process and the Mac sleeps mid-way: the busy rule.
- The Cap Event hits real work, or the Mac stays awake overnight because of a leftover process: the timing numbers.
- A real Lease goes wrong because the system clock changed, or the hour is clearly shorter because the Mac slept: the clock.
- Dogfood edits a timing constant two or more times, or the app is given to someone else: timing options.
- `Display on` is not pressed once after a few days of use: Display support.
- A second Module must copy nearly the same mapper, or a Module field changing with CI green becomes unacceptable: the record in `navori-ffi`.
- The Core row is not opened once in 3 days of use, or level 0 gets tight: the Core row.
- A hung command keeps the Mac awake overnight unnoticed: the Command Lease TTL.
- A long command is slept through because nobody saw the stderr line: `awake run` with the Core off.
- Dogfood needs a terminal to see which session runs, or ROAD-9 slips clearly: the Agent sessions row.
- macOS refusal never occurs in dogfood, or retrying never helps: the retry path.

## Open Questions

- Action fields, column names, status block English, Event text and stderr text: `design-solution` proposes, the user approves — Jayden Đặng — 2026-11-02 — forbid-guess.
- How `Display on` enters the model, and whether a Session Lease may ask for the display — Jayden Đặng — 2026-11-02 — forbid-guess.
- How the session-hold Action answers `NotRunning` and `NotAHarness` — Jayden Đặng — 2026-11-02 — forbid-guess.
- Whether `Snapshot::take` runs in `spawn_blocking` — Jayden Đặng — 2026-11-02 — forbid-guess.
- What a second duration Lease or until-off Lease does while a Manual Lease exists — Jayden Đặng — 2026-11-02 — forbid-guess.
- The exit code of `navori awake run` when `<cmd>` cannot start or dies by a signal — Jayden Đặng — 2026-11-02 — forbid-guess.
