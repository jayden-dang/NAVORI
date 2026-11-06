# Requirements: Awake with the lid closed, and the last sleep

Feature code: LID
Status: Draft
Date: 2026-11-06
Roadmap item: ROAD-41 (ticket #34)
UI brief: [2026-11-06-awake-lid-and-last-sleep-ui-brief.md](../../design/2026-11-06-awake-lid-and-last-sleep-ui-brief.md) (locked)

<!--
Rules:
- Every acceptance criterion gets a hierarchical ID: LID-<story>.<criterion>.
- EARS phrasing; guards use SHALL CONTINUE TO.
- IDs are immutable once Status is Approved. Retire by striking through — never renumber.
- Labels: a value marked (agent-decided) was chosen by the build agent under the card's Delegated list
  (docs/roadmap/roads/ROAD-41.md:23); (Weigh) was picked by the reviewing agent. Neither is the operator's own
  choice. Everything unlabelled comes from the card, the locked brief or the operator-confirmed close package.
-->

Terms: **Lease**, **Tracking agents** and **Running** as in `CONTEXT.md`.
- **The lid setting**: the macOS clamshell-sleep state that Navori turns off and gives back.
- **Lid sleep given back**: the lid setting is set back so closing the lid sleeps the Mac.
- **Navori holds the lid setting**: Navori has turned the lid setting off and has not yet given lid sleep back.
- **The adapter**: the providing power source reads `AC Power`.
- **The problem line**: the Running submenu's one shared problem-or-note slot, as it works today: the Core's own note, when it has one, takes the slot ahead of the Awake problem (`RunningState.swift:307-309`). A lid problem is published apart from the Awake problem and takes the slot only when neither the Core's note nor the Awake problem does, so it never marks projects Unknown or fails the `awake` doctor check; `navori doctor` always shows it (LID-6.2) (agent-decided: the slot rule is kept, because changing it would change the Running layout beyond the two new rows; kept apart from the Awake problem in design review).
- **A Lease held**: the Awake reconciler's `system` want is true. That want counts session Leases only while Tracking agents is on.

## 1. Keep working with the lid closed

**Story:** As the user with an agent working and the adapter in, I want to close the lid and have the Mac stay awake, so that the session carries on while the lid is shut.

- **LID-1.1** WHILE `Lid closed` is on, a Lease is held and the Mac is on the adapter THE SYSTEM SHALL keep the lid setting turned off, so closing the lid does not sleep the Mac.
- **LID-1.2** WHEN the last Lease ends while Navori holds the lid setting THE SYSTEM SHALL give lid sleep back.
- **LID-1.3** WHEN the user turns `Lid closed` off while Navori holds the lid setting THE SYSTEM SHALL give lid sleep back.
- **LID-1.4** WHILE Tracking agents is off THE SYSTEM SHALL NOT hold the lid setting for Agent session Leases alone.
- **LID-1.5** WHEN Navori is about to turn the lid setting off THE SYSTEM SHALL first store the `lid_held` marker, and turn the setting off only after the marker is stored.
- **LID-1.6** IF storing the `lid_held` marker fails THEN THE SYSTEM SHALL leave the lid setting as it is and show the problem line in the Running submenu (agent-decided failure handling).
- **LID-1.7** WHEN Navori gives lid sleep back THE SYSTEM SHALL delete the `lid_held` marker only after macOS accepts the call.
- **LID-1.8** IF macOS refuses the call that gives lid sleep back THEN THE SYSTEM SHALL keep the `lid_held` marker and show the problem line (agent-decided failure handling).
- **LID-1.9** IF macOS refuses the call that turns the lid setting off THEN THE SYSTEM SHALL show the problem line, set `Lid closed` to off, store that value and delete the `lid_held` marker, because the setting never changed (marker delete added in design, agent-decided).
- **LID-1.10** WHEN Navori quits normally while holding the lid setting THE SYSTEM SHALL give lid sleep back and then delete the `lid_held` marker (agent-decided).
- **LID-1.11** WHEN the Awake module is dropped while holding the lid setting, after repeated crashes or on an aborted shutdown, THE SYSTEM SHALL give lid sleep back, with the `lid_held` marker left in place (agent-decided).
- **LID-1.12** WHEN the condition behind a lid problem ends THE SYSTEM SHALL clear that problem: a later lid call macOS accepts, a re-armed power-source watch, a stored marker or value that saves, or the user setting `Lid closed` (agent-decided in design review).

## 2. Adapter only

**Story:** As the user on battery, I want `Lid closed` to show `Needs adapter` and let the lid sleep the Mac, so that a closed Mac in a bag never stays awake on battery.

- **LID-2.1** WHEN the providing power source changes from the adapter to battery or UPS while Navori holds the lid setting THE SYSTEM SHALL give lid sleep back.
- **LID-2.2** WHEN the providing power source changes to the adapter while `Lid closed` is on and a Lease is held THE SYSTEM SHALL turn the lid setting off again.
- **LID-2.3** THE SYSTEM SHALL count only `AC Power` as the adapter, and treat `Battery Power` and `UPS Power` as not the adapter (agent-decided).
- **LID-2.4** WHILE `Lid closed` is on and the Mac is not on the adapter THE SYSTEM SHALL show the `Lid closed` row's value as `On`, with a grey `Needs adapter` beside it that never replaces `On`.
- **LID-2.5** WHILE `Lid closed` is on THE SYSTEM SHALL learn the power source from one read when its watch is armed (when `Lid closed` turns on, or at start with it on) and from macOS power-source change notifications after that, with no timer and no polling (agent-decided: the `kIOPSNotifyPowerSource` notification; ARCH-16; arming only while on was changed in design).
- **LID-2.6** WHILE `Lid closed` is on, IF the power source cannot be read, or its change notification cannot be registered or stops delivering, THEN THE SYSTEM SHALL treat the Mac as not on the adapter, give lid sleep back if Navori holds the lid setting, and record a lid problem, retrying the watch as the exits watcher does today (`modules/awake/src/actor.rs:110,588,599`) (agent-decided).
- **LID-2.7** WHILE `Lid closed` is off THE SYSTEM SHALL neither read nor watch the power source (agent-decided in design).

## 3. The `Lid closed` switch

**Story:** As the user, I want to turn `Lid closed` on or off from the Running submenu or from `navori awake lid on|off`, so that I choose when the Mac may stay awake with the lid closed.

- **LID-3.1** THE SYSTEM SHALL offer an Awake Action named `lid` with input `{on: bool}` and Risk `Write`, served to the menu, the CLI, IPC and Navori AI like every Action (ARCH-4) (agent-decided name).
- **LID-3.2** WHEN the `lid` Action runs THE SYSTEM SHALL store the new value before applying it.
- **LID-3.3** IF storing the `Lid closed` value fails THEN THE SYSTEM SHALL change nothing and return the Action as failed.
- **LID-3.4** WHEN the `Lid closed` value changes THE SYSTEM SHALL write a Journal event `awake.lid_changed` with payload `{on}` (agent-decided name and payload).
- **LID-3.5** WHERE no `Lid closed` value is stored, or the stored value is not a boolean, THE SYSTEM SHALL treat `Lid closed` as off (Weigh, fork 1).
- **LID-3.6** WHEN Navori starts THE SYSTEM SHALL apply the stored `Lid closed` value.
- **LID-3.7** THE SYSTEM SHALL lay out the Running submenu as follows:
  - the `Last sleep` row, then a separator, both only when a sleep is recorded (LID-5.5)
  - `Keep awake`
  - `Tracking agents`
  - `Lid closed`
  - the problem line, only when something is wrong (shared slot, as today)
  - a separator, then the project rows
  - a separator and the command Lease rows, when there are any
  - a separator and `Stop all ports…`

  This amends PSTOP-4.11 and AWAKE-11.7.
- **LID-3.8** THE SYSTEM SHALL draw the `Lid closed` row with the icon `laptopcomputer` and the value `On` or `Off`, with a green dot while on, the same as `Tracking agents` (the brief's kit fallback, agent-decided).
- **LID-3.9** WHEN the user picks the `Lid closed` row, by click or by keyboard as with `Tracking agents`, THE SYSTEM SHALL run the `lid` Action with the opposite of the current value.
- **LID-3.10** WHEN the user runs `navori awake lid on` or `navori awake lid off` THE SYSTEM SHALL run the `lid` Action with that value (agent-decided verb).
- **LID-3.11** WHEN the user runs `navori awake status` THE SYSTEM SHALL print a `Lid closed:` line reading `on`, `off` or `on (needs adapter)`, and its `--json` form SHALL carry the same state (agent-decided wording).
- **LID-3.12** WHEN the `lid` Action asks for the value `Lid closed` already has THE SYSTEM SHALL change nothing, store nothing and write no event, as `tracking` does today (`actor.rs:258`) (agent-decided).
- **LID-3.13** IF the stored `Lid closed` value, the `lid_held` marker or the last sleep cannot be read THEN THE SYSTEM SHALL still start the Awake module and record the read failure as a lid problem. An unreadable `Lid closed` value reads as off, an unreadable last sleep shows no `Last sleep` row, and only an unreadable `lid_held` marker skips the start-up restore (agent-decided; narrowed in design review).

## 4. Lid sleep given back after a crash

**Story:** As the user whose Navori was force-killed while holding the lid, I want the next launch to give lid sleep back, so that closing the lid sleeps the Mac again.

- **LID-4.1** WHEN the Awake module first starts in a process and the `lid_held` marker is stored THE SYSTEM SHALL give lid sleep back and then delete the marker, before Navori holds the lid setting again in that process.
- **LID-4.2** WHEN the Awake module first starts in a process and no `lid_held` marker is stored THE SYSTEM SHALL NOT call the lid setting at start.
- **LID-4.3** WHEN the Awake module restarts under supervision in the same process THE SYSTEM SHALL NOT run the start-up restore again (agent-decided).
- **LID-4.4** IF the start-up restore call is refused THEN THE SYSTEM SHALL keep the `lid_held` marker, show the problem line, retry the restore, and hold the lid setting in that process only after a restore succeeds (no-hold-until-restored added in design review, agent-decided).

## 5. The last sleep

**Story:** As the user, I want the Running submenu's first row to say when the Mac last slept, so that I can tell at a glance whether it slept while I was away.

- **LID-5.1** WHEN macOS posts the will-sleep notification THE SYSTEM SHALL record the sleep time, stamped synchronously in the call the Shell makes from the notification. The Shell observes it wherever it observes the wake notification today.
- **LID-5.2** WHEN a sleep is recorded THE SYSTEM SHALL write a Journal event `awake.slept` with payload `{at_ms, reason}`. Here `reason` is the macOS `Last Sleep Reason` string, or null when it cannot be read (agent-decided name and payload).
- **LID-5.9** THE SYSTEM SHALL carry a sleep from the Shell to Awake as an Awake Action named `slept` with input `{at_ms}` and Risk `Write`, served like every Action (ARCH-4). A forged call changes only the `Last sleep` row and adds one `awake.slept` event (agent-decided in design: the Core reaches a Module only through dispatch, `crates/ffi/src/lib.rs:445-465`).
- **LID-5.3** WHEN a sleep is recorded THE SYSTEM SHALL store its time, so the last sleep survives a relaunch and the Journal's retention sweep (agent-decided).
- **LID-5.4** WHILE a sleep is recorded THE SYSTEM SHALL show `Last sleep` with the icon `moon` and the time as its value, as the first row of the Running submenu, followed by a separator.
- **LID-5.5** WHILE no sleep has been recorded THE SYSTEM SHALL show neither the `Last sleep` row nor its separator.
- **LID-5.6** THE SYSTEM SHALL write the `Last sleep` value in the user's local time zone as one of the forms below. The brief gives the examples; the 24-hour clock and the 2-to-6-day window are agent-decided.
  - `HH:mm` for today
  - `Yesterday HH:mm` for the calendar day before
  - `EEE HH:mm`, such as `Wed 11:43`, for 2 to 6 calendar days before
  - `d MMM`, such as `30 Oct`, for older dates in the same year (agent-decided)
  - `d MMM yyyy` for an earlier year (agent-decided)
- **LID-5.7** THE SYSTEM SHALL draw the `Last sleep` row with no hover highlight and no action when clicked.
- **LID-5.8** THE SYSTEM SHALL count a sleep for every will-sleep notification macOS posts to the Shell, and record nothing for a dark wake or a maintenance sleep that posts none (agent-decided; that no notification is posted is unverified on hardware, see Open Questions).

## 6. Doctor reports the lid

The whole doctor check, LID-6.1 to LID-6.3, is agent-decided.

**Story:** As the user, I want `navori doctor` to show whether Navori holds the lid, so that I can check the feature without closing the lid.

- **LID-6.1** WHEN `navori doctor` runs THE SYSTEM SHALL add a `lid` check, after `ports` as the last row (agent-decided placement), that reports four things:
  - whether `Lid closed` is on
  - whether Navori holds the lid setting
  - whether the Mac is on the adapter, or `not watched` while `Lid closed` is off (LID-2.7)
  - whether the `lid_held` marker is stored
- **LID-6.2** IF a lid problem is recorded THEN THE SYSTEM SHALL fail the `lid` check with that problem's text, even when the problem line in the menu shows the Core's note instead.
- **LID-6.3** THE SYSTEM SHALL name the `lid` check in the Settings sentence that lists the checks, after `ports`.

## 7. What keeps working

**Story:** As the user upgrading Navori, I want Leases, Tracking agents, the CLI and the Running menu to behave as before, so that the lid feature costs me nothing I already rely on.

Files this change touches. Each has its guard, or `no behavior to guard`:

| File | Guard |
|---|---|
| `modules/awake/src/domain/table.rs`, `domain/reconciler.rs`, `actor.rs` | LID-7.1, 7.3, 7.4 |
| `modules/awake/src/state.rs`, `action.rs`, `event.rs`, `infra/stored.rs`, `infra/platform.rs`, `lib.rs` | LID-7.2, 7.3, 7.11 |
| `crates/macos/src/power.rs`, `fake/power.rs`, root `Cargo.toml:56-57` (`objc2-io-kit` / `objc2-core-foundation` features) | LID-7.1, 7.11 |
| `crates/macos/src/` new lid-setting and power-source files and their fakes | no behavior to guard (new) |
| `crates/store/src/lib.rs` (a `kv_delete`) | LID-7.6 |
| `crates/ffi/src/lib.rs`, `app.rs` | LID-7.5, 7.7, 7.8 |
| `apps/cli/src/main.rs` | LID-7.9, 7.12 |
| `apps/macos/Navori/Platform/WakeObserver.swift`, `App/AppDelegate.swift`, `Core/CoreStore.swift` | LID-7.5, 7.10 |
| `apps/macos/Navori/Features/Settings/SettingsView.swift` (doctor sentence) | LID-7.7 |
| `apps/macos/NavoriPanel/AwakeRowState.swift` (exhaustive switch) | LID-7.16 |
| `apps/macos/Navori/Features/Panel/RunningSubmenuView.swift`, `StatusBar/StatusBarController.swift`, `apps/macos/NavoriPanel/RunningState.swift`, `AwakeSubmenuState.swift`, `KeepAwakeChoice.swift` | LID-7.13, 7.14, 7.15, 7.16 |
| `docs/specs/2026-11-02-awake-leases/requirements.md`, `2026-11-04-ports-panel-and-stop/requirements.md` (amendment notes) | no behavior to guard (docs) |

- **LID-7.1** (guard) WHEN a Lease is held THE SYSTEM SHALL CONTINUE TO hold the System power assertion, whatever the `Lid closed` value, the power source or the lid.
- **LID-7.2** (guard) WHEN Tracking agents is set THE SYSTEM SHALL CONTINUE TO store `awake/tracking` before applying it, to default it to on, and to write `awake.tracking_changed` with payload `{on}`.
- **LID-7.3** (guard) WHEN a Lease starts or ends THE SYSTEM SHALL CONTINUE TO write `awake.lease_started` or `awake.lease_ended` with payload `{key, kind}` under today's rules, including writing none for session Leases while Tracking agents is off.
- **LID-7.4** (guard) WHEN the Awake module first starts in a process THE SYSTEM SHALL CONTINUE TO restore the stored Leases, and on a supervised restart it SHALL CONTINUE TO keep its in-memory Leases without re-reading them.
- **LID-7.5** (guard) WHEN the Mac wakes THE SYSTEM SHALL CONTINUE TO send the Scheduler's wake pulse through `system_did_wake`.
- **LID-7.6** (guard) WHEN the Store opens THE SYSTEM SHALL CONTINUE TO run the same migrations, with no new schema version, and `kv_set`/`kv_get` SHALL CONTINUE TO behave as today.
- **LID-7.7** (guard) WHEN `navori doctor` runs THE SYSTEM SHALL CONTINUE TO report `config`, `database`, `ipc`, `modules`, `awake`, `integrations`, `logs` and `ports`, in that order and unchanged (AWAKE-12.1), with `lid` only appended after them.
- **LID-7.8** (guard) WHEN a snapshot is pushed THE SYSTEM SHALL CONTINUE TO carry every field `CoreSnapshot` and `AwakeView` have today (AWAKE-12.3).
- **LID-7.9** (guard) WHEN `cargo xtask check-deps` runs THE SYSTEM SHALL CONTINUE TO allow the CLI only `navori-config`, `navori-ipc` and `navori-protocol` (AWAKE-12.6).
- **LID-7.10** (guard) WHEN Swift reads Awake THE SYSTEM SHALL CONTINUE TO import the generated bindings only in `CoreStore` and `NavoriBanner` (AWAKE-12.7).
- **LID-7.11** (guard) WHEN an existing test calls `FakePower::refuse` THE SYSTEM SHALL CONTINUE TO refuse every assertion kind (AWAKE-12.12).
- **LID-7.12** (guard) WHEN the user runs `navori awake hold`, `release-all`, `status` or `run` THE SYSTEM SHALL CONTINUE TO behave as today, with `status` still printing its `Tracking agents:` line.
- **LID-7.13** (guard) WHEN the Running submenu is shown THE SYSTEM SHALL CONTINUE TO behave as follows (PSTOP-4.11 to 4.13):
  - `Keep awake` opens its own submenu
  - clicking `Tracking agents` toggles it
  - project rows show a dot and a name
  - command Lease rows sit after their own separator
  - `Stop all ports…` stays the last row and keeps its inline confirmation (PSTOP-7.2)
- **LID-7.14** (guard) WHEN the app opens the Running submenu THE SYSTEM SHALL CONTINUE TO read neither the process table nor the system's assertions (AWAKE-11.10). It SHALL also read neither the lid setting nor the power source then; both reach the menu only through the Core snapshot.
- **LID-7.15** (guard) THE SYSTEM SHALL CONTINUE TO avoid saying in the menu that the Mac will sleep, or that another app holds it awake (AWAKE-11.8, AWAKE-11.9).
- **LID-7.16** (guard) WHILE Navori holds the lid setting THE SYSTEM SHALL CONTINUE TO set the `Running` row's dot from session Leases and ports only (PSTOP-4.9), and the status icon from the Leases and the System assertion only.
- **LID-7.17** (guard) WHEN the Core has its own note for the Running submenu THE SYSTEM SHALL CONTINUE TO show it in the shared problem-or-note slot ahead of the Awake problem.

## 8. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** **LID-8.1** WHILE the menu is closed THE SYSTEM SHALL add no timer and no polling for the lid setting or the power source (ARCH-16). Verified by a module test with fakes that count power-source reads: none while `Lid closed` is off, exactly one when the watch is armed, then one per change notification.
  - **LID-8.2** WHEN the Running submenu opens THE SYSTEM SHALL make no IOKit, power-source or notify call. Verified by a test that builds the menu state from a snapshot with fakes that count calls.
  - The GOAL-11 budget (AWAKE-13.1) stays binding. Per `docs/standards/INDEX.md:27`, it is measured at the final milestone, not gated on this feature.
- **Security:** **LID-8.3** THE SYSTEM SHALL add no socket, root step or helper process for the lid. The `lid` and `slept` Actions SHALL carry Risk `Write`, the same as `tracking`, and reach Navori only through the paths that already serve `tracking`: the in-process `NavoriCore.dispatch` for the menu and Navori AI, and the user-private IPC socket (mode 0600, peer-uid check) for the CLI (threat model TB-2, TB-3). Verified by the Action-doc test and `cargo xtask check-deps`.
- **Reliability:** **LID-8.4** THE SYSTEM SHALL never reach a state where Navori holds the lid setting and no `lid_held` marker is stored.
  - Verified by fake-ordered tests that stop the module after each step: marker write, set, clear, marker delete.
  - Also verified by a drop test that records the clear when the module is dropped.
  - Also verified by a test where the power-source watch dies while Navori holds the lid setting; the fake records the clear (LID-2.6).
- **Accessibility:** **LID-8.5** THE SYSTEM SHALL let VoiceOver read the `Last sleep` row as static text and the `Lid closed` row as a switch with its state:
  - `Last sleep` reads like `Last sleep, today at 09:20`.
  - `Lid closed` reads `Lid closed, stay awake, on`.
  - On battery, `Lid closed` reads `Lid closed, stay awake, on, needs adapter` (agent-decided).

  Verified by Swift tests on the accessibility labels. The text comes from the locked brief; `docs/standards/` has no accessibility standard.

## Out of Scope

- Holding the lid on battery, at any battery level (operator: adapter only).
- Sleep information inside project menus, or anywhere but the one `Last sleep` row.
- A sleep log, history view, duration or reason shown in the menu.
- A watchdog, helper process, root step or privileged helper.
- Reading `pmset -g log`, or any system log, when the menu opens.
- Thermal release logic (agent-decided; macOS keeps its own thermal-emergency and low-battery sleep).
- Telling apart, or protecting, another app's use of the same lid setting. Navori restores only when its own marker says it held, and a release or restore can clear another app's hold (residual limit of the operator's marker rule).
- The brief's battery proposal (to 20%) and its "unconditional restore at launch". Both were superseded by the card.

## Open Questions

Owned unknowns. Each can be settled only by the operator's real lid run; tests here use a fake, by charter.
- That NSWorkspace's will-sleep notification is silent for a dark-wake or maintenance sleep (LID-5.8) — operator (Power Nap lid run) — before merge — forbid-guess.
- That giving lid sleep back while the lid is closed sleeps the Mac within seconds. The 5 s test bound is agent-decided, from one control run of 4 s (LID-1.2, LID-2.1) — operator (lid run) — before merge — forbid-guess.
- That a reboot gives lid sleep back. Inferred from xnu, where the init code sets the mask to 0 — operator — before merge — forbid-guess.
- Whether powerd uses the same lid bit and could clash with Navori — operator — before merge — forbid-guess.
