# Requirements: Awake leases

Feature code: AWAKE
Status: Implemented
Date: 2026-11-02

<!--
Rules:
- Every acceptance criterion gets a hierarchical ID: AWAKE-<story>.<criterion>.
- Criteria use EARS phrasing (WHEN / WHILE / IF…THEN / WHERE / ubiquitous).
- Guard requirements protect existing behavior: WHEN <condition> THE SYSTEM SHALL CONTINUE TO <behavior>.
- IDs are immutable once Status is Approved. Retire by strikethrough — never renumber.
-->

Roadmap item: ROAD-8 (MILE-2) · Tracker: #7 · Decisions: frame-change close package (2026-11-02), confirmed by the user.

Delivery order: stories 1–10 ship first. Story 11 builds on MENU (ROAD-34), which merged at `030774d`. Stories 14 and 15 amend the shipped menu (2026-11-04, user's request after using `Release all`).

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
- **Keep awake lease** — the Manual Lease the menu's `Keep awake` controls: a duration, until-off or display Lease.
- **Tracking agents** — the on/off setting that decides whether Session Leases keep the Mac awake. While it is off, Navori still follows each session, but the session holds nothing.
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
- ~~**AWAKE-1.8**~~ superseded by AWAKE-14.1 (the menu offers several lengths).

## 2. Keep the Mac awake until I turn it off

**Story:** As the user, I want to keep the Mac awake until I turn it off, then turn it off with Release all, so that I decide when it ends.

- **AWAKE-2.1** WHEN the user asks for an until-off Lease THE SYSTEM SHALL create a Manual Lease with no deadline.
- **AWAKE-2.2** WHEN the user asks for Release all THE SYSTEM SHALL end every Lease.
- **AWAKE-2.3** WHEN the user asks for Release all THE SYSTEM SHALL delete every stored Lease row.
- **AWAKE-2.4** WHEN the app reopens after Release all THE SYSTEM SHALL NOT restore a Lease that Release all ended.
- ~~**AWAKE-2.5**~~ superseded by AWAKE-14.6, AWAKE-14.8 and AWAKE-15.1 (the menu ends a Keep awake lease and stops sessions holding; Release all stays outside the menu).
- **AWAKE-2.6** THE SYSTEM SHALL NOT have a Pause state. *(Amended 2026-11-04: Tracking agents off, story 15, answers this file's "Comes back if" line. It has no timer and touches no Lease except by keeping Session Leases from holding; no timed Pause exists.)*
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

**Story:** As the user, I want to press `Display on` and see the display stay on until I turn it off, so that the screen stays readable while the Mac is held awake. *(Amended 2026-11-04: the menu reaches the display Lease through `Keep awake` → `Indefinitely`, AWAKE-14.3.)*

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

**Story:** As the user, I want to open the Awake row, press `Keep awake for 1 hour`, see the Lease under Holding and press `Release all`, so that I do all of this from the menubar. *(Amended 2026-11-04: the commands are now stories 14 and 15; the Holding list stays.)*

It changes three things in MENU's spec: MENU-1.2 gains the Awake row before the Core row; MENU-1.4 no longer lists the Awake row among what Release code hides; and MENU's "The Awake row ships: decide whether the Core row stays" is answered: it stays.

- **AWAKE-11.1** THE SYSTEM SHALL lay out level 0 in this order: any banners, the Awake row, the Core row, a separator, `Settings…`, `About Navori`, `Quit Navori`. *(Amended 2026-11-04 by PSTOP: the Awake row is now the `Running` row, with the same place in the order and a dot and a chevron but no text, PSTOP-4.8; the Awake Module, its Actions and the name `Awake` keep their names, PSTOP-4.18.)*
- **AWAKE-11.2** WHILE no Lease exists THE SYSTEM SHALL show no dot on the Awake row. *(Amended 2026-11-04: the row shows a dot and no text.)*
- **AWAKE-11.3** WHILE a Lease exists with the System assertion held THE SYSTEM SHALL show a green dot on the Awake row, with VoiceOver reading `On` and the names of the sources holding. *(Amended 2026-11-04: the row shows a dot and no text.)*
- **AWAKE-11.4** WHILE a Lease exists with no System assertion THE SYSTEM SHALL NOT show a green dot on the Awake row. *(Amended 2026-11-04: the row shows a dot and no text.)*
- ~~**AWAKE-11.5**~~ superseded by AWAKE-14.1 and AWAKE-15.2.
- **AWAKE-11.6** THE SYSTEM SHALL NOT show `Pause 15m`, `Let my Mac sleep now`, an Agent sessions row, an Integrations row or a per-Lease release button in the Awake Submenu.
- **AWAKE-11.7** THE SYSTEM SHALL write menu text about Navori's own Leases only. *(Amended 2026-11-04 by PSTOP: the Running submenu also writes about the user's ports, containers and projects, and about whether `Stop` is on, PSTOP-4.11 and 4.21; a sentence about Leases still speaks of Navori's own Leases only.)* *(Amended 2026-11-06 by LID (approved under the operator's /shape --go): the Running submenu also shows when the Mac last slept and the `Lid closed` switch, LID-3.7 and LID-5.4.)*
- **AWAKE-11.8** THE SYSTEM SHALL NOT say in the menu that the Mac will sleep.
- **AWAKE-11.9** THE SYSTEM SHALL NOT say in the menu that another app is holding the Mac awake.
- **AWAKE-11.10** WHEN the app opens the Awake Submenu THE SYSTEM SHALL NOT read the process table or the system's assertions.
- **AWAKE-11.11** WHILE no problem exists THE SYSTEM SHALL show no status text in the Awake Submenu.
- **AWAKE-11.12** IF the System assertion or the Display assertion is refused, a menu action fails, or Awake records a problem THEN THE SYSTEM SHALL state that problem between the commands and the Holding rows.
- **AWAKE-11.13** THE SYSTEM SHALL group Holding rows in this order: `Held by you`, `Commands`, then one group per session project in case-insensitive name order, where the project is the Lease label before its first `/`. *(Amended 2026-11-04 by PSTOP: the Running submenu replaces the Holding list. It shows one row per project, with the project's sessions and ports in its child menu, PSTOP-4.11 to 4.13; command Leases are listed as their own rows beside the project rows, and the `Keep awake` row's value shows the manual hold.)*
- **AWAKE-11.14** THE SYSTEM SHALL lead a session's Holding row with the monochrome mark of its Harness, for Claude Code, Codex and Grok, and with a terminal symbol for any other Harness.
- **AWAKE-11.15** THE SYSTEM SHALL show a working session with a green dot and the word `working`, and a stopped session dimmed with the word `stopped`. *(Amended 2026-11-04: AWAKE-16.5 says which sessions read as working.)* *(Amended 2026-11-04 by PSTOP: a session row in a project's child menu keeps the dot and the words `working` and `stopped`; the project row itself shows only a dot, a green dot or an orange ring, PSTOP-4.13.)*
- ~~**AWAKE-11.16**~~ superseded by AWAKE-14.4 and AWAKE-14.5.
- **AWAKE-11.17** WHEN one source holds more than one Lease THE SYSTEM SHALL have VoiceOver name it once on the Awake row with its count, as `Claude Code ×8`. *(Amended 2026-11-04: the row shows a dot and no text.)* *(Amended 2026-11-04 by PSTOP: VoiceOver reads the `Running` row as `Running, active` or `Running, nothing running`, PSTOP-4.9; a source's count is no longer spoken on level 0.)*
- **AWAKE-11.18** IF a menu action fails or Awake records a problem other than a refused System assertion or a stopped Module THEN THE SYSTEM SHALL have VoiceOver read `Needs attention` on the Awake row. *(Amended 2026-11-04: the row shows a dot and no text.)*
- ~~**AWAKE-11.19**~~ superseded by AWAKE-14.7.
- ~~**AWAKE-11.20**~~ superseded by AWAKE-14.6.
- **AWAKE-11.21** THE SYSTEM SHALL show no text value on the Awake row, in any state.

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
| `apps/macos/Navori/Features/Panel/`, `apps/macos/Navori/StatusBar/`, `apps/macos/NavoriPanel/` | AWAKE-12.8, AWAKE-12.9 |
| `docs/ARCHITECTURE.md`, `docs/architecture/integrations.md`, `docs/design/2026-11-01-menubar-ui-brief.md`, MENU's `requirements.md` | no behavior to guard |

- **AWAKE-12.1** (guard) WHEN `navori doctor` runs THE SYSTEM SHALL CONTINUE TO report the `config`, `database`, `ipc`, `modules` and `logs` checks.
- **AWAKE-12.2** (guard) WHEN the Kernel is built THE SYSTEM SHALL CONTINUE TO run the `system` Module with its Journal sweep.
- **AWAKE-12.3** (guard) WHEN a snapshot is pushed THE SYSTEM SHALL CONTINUE TO carry every field `CoreSnapshot` has today.
- **AWAKE-12.4** (guard) WHEN snapshots are pushed THE SYSTEM SHALL CONTINUE TO coalesce them at 33 ms while the panel is visible and 250 ms while it is not.
- **AWAKE-12.5** (guard) WHEN a `navori` command other than `awake run` finds the app not running THE SYSTEM SHALL CONTINUE TO exit 3.
- **AWAKE-12.6** (guard) WHEN `cargo xtask check-deps` runs THE SYSTEM SHALL CONTINUE TO allow the CLI only `navori-config`, `navori-ipc` and `navori-protocol`.
- **AWAKE-12.7** (guard) WHEN Swift reads Awake THE SYSTEM SHALL CONTINUE TO import the generated bindings only in `CoreStore` and `NavoriBanner` (ARCH-2).
- **AWAKE-12.8** (guard) WHEN the Core row is shown THE SYSTEM SHALL CONTINUE TO open its Submenu with module states and at most 8 recent Events.
- **AWAKE-12.9** (guard) WHERE the app runs the dev profile THE SYSTEM SHALL CONTINUE TO show the `DEV` chip on the Core row.
- **AWAKE-12.10** (guard) WHEN a Module fails and restarts THE SYSTEM SHALL CONTINUE TO restart it with backoff, keeping `self`, its State and its queued requests.
- **AWAKE-12.11** (guard) WHILE two Profiles run THE SYSTEM SHALL CONTINUE TO keep their databases, sockets and logs separate.
- **AWAKE-12.12** (guard) WHEN an existing test calls `FakePower::refuse` THE SYSTEM SHALL CONTINUE TO refuse every assertion kind.

## 13. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** **AWAKE-13.1** WHEN `cargo xtask perf` runs with Awake registered and no Lease THE SYSTEM SHALL stay inside the GOAL-11 budget in `docs/standards/performance.md`: idle 0% CPU, ≤ 1 wakeup/s, ≤ 30 MB footprint, cold start ≤ 200 ms, first menu open ≤ 80 ms, later opens ≤ 50 ms and Core Submenu open ≤ 16 ms.
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

## 14. Choose how long to keep the Mac awake

**Story:** As the user, I want to open `Keep awake` and pick `Indefinitely` or a length, so that the Mac stays awake for something that is not an agent, like music at night.

Files this story and story 15 touch: `modules/awake/src/` (guards below), `crates/ffi/src/lib.rs` and `app.rs` (AWAKE-12.3), `apps/cli/src/main.rs` (AWAKE-14.9), `apps/macos/NavoriPanel/`, `apps/macos/Navori/Features/Panel/`, `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/Navori/Core/CoreStore.swift` (AWAKE-12.7 to 12.9, AWAKE-11.12), docs (no behavior to guard).

- **AWAKE-14.1** WHEN the user opens `Keep awake` in the Awake Submenu THE SYSTEM SHALL show a third-level Submenu listing `Indefinitely`, `5 minutes`, `10 minutes`, `15 minutes`, `30 minutes`, `1 hour`, `2 hours` and `5 hours`, in that order.
- **AWAKE-14.2** WHEN the user picks a length THE SYSTEM SHALL make a duration Lease of that length the only Keep awake lease.
- **AWAKE-14.3** WHEN the user picks `Indefinitely` THE SYSTEM SHALL make a display Lease the only Keep awake lease, so that the display stays on as well as the Mac.
- **AWAKE-14.4** WHILE a Keep awake lease exists THE SYSTEM SHALL show a check on its choice: `Indefinitely` for a display or until-off Lease, and the matching length for a duration Lease.
- **AWAKE-14.5** THE SYSTEM SHALL show on the `Keep awake` row `until <time>` while a duration Lease exists, `Indefinitely` while a display or until-off Lease exists and no duration Lease does, and no value otherwise.
- **AWAKE-14.6** WHEN the user picks a checked choice THE SYSTEM SHALL end every Keep awake lease and no other Lease.
- **AWAKE-14.7** WHEN the Awake Submenu or the `Keep awake` Submenu is open THE SYSTEM SHALL move ↑ and ↓ through its commands, open `Keep awake` with →, and give Holding rows no keyboard stop.
- **AWAKE-14.8** THE SYSTEM SHALL NOT show `Release all` in the menu.
- **AWAKE-14.11** WHEN the `Keep awake` Submenu opens while the Awake Submenu sits left of level 0 THE SYSTEM SHALL place it left of the Awake Submenu, if it fits there on screen, so that it does not cover level 0.
- **AWAKE-14.9** (guard) WHEN the user runs `navori awake release-all` or `navori awake hold` THE SYSTEM SHALL CONTINUE TO end every Lease, or make the requested Manual Lease without changing a display Lease, as before.
- **AWAKE-14.10** (guard) WHEN one Keep awake choice replaces another THE SYSTEM SHALL CONTINUE TO keep the System assertion, with no moment without it.

## 15. Turn agent tracking off and on

**Story:** As the user, I want to press `Tracking agents` and see it read `On` or `Off`, so that agents keep the Mac awake only when I want, and the Mac sleeps by itself once they are done.

- **AWAKE-15.1** WHEN the user presses `Tracking agents` THE SYSTEM SHALL switch it between on and off.
- **AWAKE-15.2** THE SYSTEM SHALL show `Keep awake` then `Tracking agents` on top of the Awake Submenu, with `Tracking agents` reading a green dot and `On`, or `Off` with no dot.
- **AWAKE-15.3** WHEN Navori has no stored Tracking agents setting THE SYSTEM SHALL treat it as on.
- **AWAKE-15.4** WHEN Tracking agents changes THE SYSTEM SHALL store it, so that it reads the same after the app reopens.
- **AWAKE-15.5** IF Tracking agents cannot be stored THEN THE SYSTEM SHALL answer the Action `Failed` and leave the setting unchanged.
- **AWAKE-15.6** WHILE Tracking agents is off THE SYSTEM SHALL NOT let a Session Lease ask for an assertion.
- **AWAKE-15.7** WHILE Tracking agents is off THE SYSTEM SHALL leave Session Leases out of Awake's published Leases, so that the Holding list, the Awake row, the Icon, `navori doctor` and `navori awake status` do not count them.
- **AWAKE-15.8** WHILE Tracking agents is off THE SYSTEM SHALL keep applying the session-hold, session-stop and session-end Actions, process exits, grace, the Cap and the Session TTL to Session Leases.
- **AWAKE-15.9** WHEN Tracking agents turns on THE SYSTEM SHALL at once count every Session Lease it has followed, and hold the System assertion for them.
- **AWAKE-15.10** WHILE Tracking agents is off THE SYSTEM SHALL NOT emit a lease Event for a Session Lease.
- **AWAKE-15.11** WHEN Tracking agents changes THE SYSTEM SHALL emit one Event with a one-sentence summary stating the new setting.
- **AWAKE-15.12** THE SYSTEM SHALL carry Tracking agents on Awake's state and on the `CoreSnapshot` Awake record.
- **AWAKE-15.13** WHEN the user runs `navori awake status` THE SYSTEM SHALL state whether Tracking agents is on or off.
- **AWAKE-15.14** (guard) WHEN Tracking agents changes THE SYSTEM SHALL CONTINUE TO keep every Manual Lease and Command Lease, and the assertions they need.
- **AWAKE-15.15** (guard) WHILE Tracking agents is on THE SYSTEM SHALL CONTINUE TO hold for sessions as in stories 4 and 5, including a new Lease on the next `UserPromptSubmit` after Release all (HOOKS-1.10).
- **AWAKE-15.16** (guard) WHEN Release all runs while Tracking agents is off THE SYSTEM SHALL CONTINUE TO end every Lease, including followed Session Leases, and leave Tracking agents off.

## 16. Keep a session with running work held and labelled working

**Story:** As the user, I want a session whose main turn ended but whose subagents or background work still run to read `working` and keep the Mac awake, so that the menu tells the truth about what is running.

Files this story touches: `modules/awake/src/` (guards below), `apps/cli/src/hook.rs` and `modules/integrations/src/domain/hookfile.rs` (HOOKS-1.11 to HOOKS-1.13, HOOKS-4.17), docs (no behavior to guard). The Shell does not change: it reads `stopped`.

- **AWAKE-16.1** WHEN the subagent-start Action arrives for a session THE SYSTEM SHALL record that subagent as running and move the session's deadline to a new Session TTL from now.
- **AWAKE-16.2** WHEN the subagent-start Action arrives for a `session_id` with no Lease THE SYSTEM SHALL make a Session Lease with its Stop mark set, resolving the Harness as AWAKE-4.1 does, so that grace can end it if no `Stop` ever comes.
- **AWAKE-16.3** WHILE a session with a Stop mark has a running subagent, or its latest session-stop or subagent-stop Action said background work is in flight, THE SYSTEM SHALL keep its Lease and read no process for it.
- **AWAKE-16.4** WHEN that stops being true for a session with a Stop mark THE SYSTEM SHALL start a new grace period from that moment, after which AWAKE-5.4 to AWAKE-5.6 apply.
- **AWAKE-16.5** THE SYSTEM SHALL report a session as stopped only while it has a Stop mark, AWAKE-16.3 does not hold, and the latest reading since that Stop mark found no New descendant. An unreadable process list is not a New descendant for this purpose.
- **AWAKE-16.6** THE SYSTEM SHALL count a `caffeinate` child like any other New descendant, because Claude Code was seen to run one only while busy (2026-11-04: idle sessions had none).
- **AWAKE-16.7** THE SYSTEM SHALL keep at most 64 running subagents per session, ignoring further starts, and SHALL NOT store them or the in-flight report, so that after an app reopen the reading decides (THR-6). A session held only by work with no child process therefore ends at grace after a reopen.
- **AWAKE-16.8** IF `agent_id` is not 1 to 128 bytes of `[A-Za-z0-9._:-]` THEN THE SYSTEM SHALL answer `Invalid` and change nothing.
- **AWAKE-16.9** (guard) WHEN a Session Lease has a Stop mark THE SYSTEM SHALL CONTINUE TO end it at the Cap, whatever work was reported, so that a lost `SubagentStop` or a report that never clears cannot hold the Mac past the Cap.
- **AWAKE-16.10** (guard) WHEN a session has no reported work and no New descendant at the end of grace THE SYSTEM SHALL CONTINUE TO end its Lease.
- **AWAKE-16.11** (guard) WHEN a session-stop Action has no `background` field, as from any Harness that sends no background list, THE SYSTEM SHALL CONTINUE TO treat it as AWAKE-5.1 and AWAKE-5.2 do.

## 17. Name each session row

**Story:** As the user, I want each session row to show the session's name instead of its branch, so that two sessions on `dev` in one project can be told apart.

Files this story touches: `modules/awake/src/` and `crates/ffi/src/` (AWAKE-12.3), `apps/cli/src/hook.rs` (HOOKS-1.15 to HOOKS-1.17), `apps/macos/NavoriPanel/`, `apps/macos/Navori/Core/CoreStore.swift` (AWAKE-12.7), docs (no behavior to guard).

- **AWAKE-17.1** WHEN a session-hold or session-stop Action carries a `title` THE SYSTEM SHALL keep it as that session's title, and an Action with no title or a blank one SHALL leave the current title.
- **AWAKE-17.2** THE SYSTEM SHALL remove control characters from a title, reduce whitespace to single spaces, and keep at most 120 characters.
- **AWAKE-17.3** THE SYSTEM SHALL NOT store a title or put it in an Event or an assertion name, so that after an app reopen the row reads the branch until the next hook (ARCH-6, THR-3).
- **AWAKE-17.4** WHILE a session has a title THE SYSTEM SHALL show the title as the name on its Holding row, and sort by that name within its project.
- **AWAKE-17.5** WHILE a session has a title THE SYSTEM SHALL read the title, the Harness, the project and branch, and the pid in the row's tooltip and VoiceOver sentence.
- **AWAKE-17.6** (guard) WHILE a session has no title THE SYSTEM SHALL CONTINUE TO show its branch as the name.
- **AWAKE-17.7** (guard) WHEN a session has a title THE SYSTEM SHALL CONTINUE TO group its row by the project in its label, as AWAKE-11.13 does. *(Amended 2026-11-04 by PSTOP: the Core joins a session to a project by the label's text before the first `/`, or by the whole label, PSTOP-4.2; a live session without a label makes every project's state unknown, PSTOP-4.5.)*

## 18. Label a Codex session by its own directory

**Story:** As the user, I want a Codex row under the project I opened Codex in, so that sessions served by one shared daemon are not all shown under the daemon's project.

Files this story touches: `modules/awake/src/` and `apps/cli/src/hook.rs` (HOOKS-1.19).

- **AWAKE-18.1** WHEN a session-hold Action for a Codex session carries an absolute `cwd` THE SYSTEM SHALL build the label from that directory. The Lease SHALL keep watching the resolved process.
- **AWAKE-18.2** IF `cwd` is absent or not absolute THEN THE SYSTEM SHALL build the label from the process's working directory.
- **AWAKE-18.3** (guard) WHEN a session-hold Action for Claude Code or Grok carries a `cwd` THE SYSTEM SHALL CONTINUE TO build the label from the process's working directory.
- **AWAKE-18.4** THE SYSTEM SHALL NOT store the `cwd`, so a restored Codex Lease is labelled from the process until its next hold.

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
- Heavy work only in a process born before the First hold, spawning no child, is missed by the busy rule. A subagent is no longer missed this way (story 16), for a Harness whose subagent hooks are installed.
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
- A real `Stop` trace through this flow, and `caffeinate` at `Stop`. *(Seen 2026-11-04 on Claude Code 2.1.318: a busy session runs `caffeinate -i -t 300`, restarted about every 4 minutes; idle sessions had none; AWAKE-16.6. What it does right after `Stop` is still unmeasured.)*
- Background work after `Stop`.
- `watch` across an app reopen, and `kill -9` on the CLI.
- Whether the real session ids of Claude Code, Codex and Grok fit `[A-Za-z0-9._:-]`, 1 to 128 bytes.
- The cost of `Snapshot::take` against the menu budget.

## Comes back if

These conditions reopen a decision. They are not requirements.

- A Harness the user runs has no hooks: reopen the problem statement, and whether the 1-hour button may be the only holder while the app can reopen unattended.
- ROAD-9 runs and the Mac must sleep while an agent keeps running: Pause. *(Reopened and answered 2026-11-04 by story 15.)*
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
