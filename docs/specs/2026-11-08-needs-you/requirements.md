# Requirements: Needs you

Feature code: NEEDS
Status: Draft
Date: 2026-11-08

<!--
Rules:
- Feature code: 2-12 chars, A-Z0-9, starts with a letter, unique repo-wide.
  Registered in docs/specs/catalog/app.md (Roadmap item ROAD-13, issue #35).
- Every acceptance criterion gets a hierarchical ID: NEEDS-<story>.<criterion>.
- Criteria use EARS phrasing (WHEN / WHILE / IF…THEN / WHERE / ubiquitous).
- Guard requirements protect existing behavior: WHEN <condition> THE SYSTEM SHALL CONTINUE TO <behavior>.
- IDs are immutable once Status is Approved. Retire by strikethrough, never renumber.

Sources: the frame close package (.skills/_pending-needs-you/auto/close-package.md in the main checkout),
the locked UI brief docs/design/2026-11-08-needs-you-ui-brief.md ("brief D"), and the glossary CONTEXT.md.
A criterion marked *(agent-decided — …)* states a value an agent chose in an unattended run, not the
operator; the operator confirms or changes it when reading this file.
-->

## 1. Raise a Decision

**Story:** As a Module author, I want to raise a Decision with a title, a reason and its choices by dispatching `needs_you.raise`, so that the user sees in one place what needs their decision.

- **NEEDS-1.1** THE SYSTEM SHALL list `needs_you.raise` in the Action catalog with Risk `Write`, taking a `key`, a `title`, a `reason`, an optional `expires_at_ms` (Unix time in milliseconds), and a `primary` and a `secondary` choice, each a `label` with an optional Envelope. *(agent-decided — two choices with an optional Envelope, close-package C1)*
- **NEEDS-1.2** WHEN `needs_you.raise` names a `key` that no open Decision holds THE SYSTEM SHALL add an open Decision.
- **NEEDS-1.3** WHEN `needs_you.raise` names the `key` of an open Decision THE SYSTEM SHALL replace that Decision's title, reason, expiry and choices and SHALL NOT add a second Decision.
- **NEEDS-1.4** WHEN a Decision is raised THE SYSTEM SHALL record each choice's Risk as the catalog Risk of the Action its Envelope names, and no Risk for a choice with no Envelope; a raise carries no Risk field. *(agent-decided — Risk from the catalog, close-package C1)*
- **NEEDS-1.5** IF a choice's Envelope names a Module or an Action that is not in the Action catalog THEN THE SYSTEM SHALL refuse the raise and add nothing.
- **NEEDS-1.6** IF a choice's Envelope names a `needs_you` Action THEN THE SYSTEM SHALL refuse the raise and add nothing. *(agent-decided — keeps an answer from answering or raising another Decision)*
- **NEEDS-1.7** IF the `key`, the `title` or a choice's `label` is empty or only whitespace THEN THE SYSTEM SHALL refuse the raise and add nothing.
- **NEEDS-1.8** WHEN a Decision is raised THE SYSTEM SHALL pass its title, reason and choice labels through the Sensitive Data Shield before storing them.
- **NEEDS-1.9** WHEN Navori or the `needs_you` Module restarts THE SYSTEM SHALL keep every open Decision with its title, reason, expiry, choices and raise time.
- **NEEDS-1.10** WHEN the user runs `navori dispatch needs_you` with a raise THE SYSTEM SHALL raise the Decision exactly as a raise from a Module does.
- **NEEDS-1.11** IF `needs_you.raise` names the `key` of an open Decision whose answer is running THEN THE SYSTEM SHALL refuse the raise and change nothing. *(agent-decided)*

## 2. Dispatch an Action from inside a Module

**Story:** As a Module author, I want my Module to dispatch an Action to another Module from inside `run`, so that it can raise a Decision without depending on the Needs you crate.

- **NEEDS-2.1** THE SYSTEM SHALL give every Module, through its context, a dispatcher that sends an Envelope to any registered Module.
- **NEEDS-2.2** WHEN a Module dispatches an Envelope THE SYSTEM SHALL return control to that Module's run loop before the target Module handles the Envelope.
- **NEEDS-2.3** WHEN the target Module replies to a dispatched Envelope THE SYSTEM SHALL make the reply, or the dispatch error, available to the Module that dispatched it.
- **NEEDS-2.4** WHEN a Module dispatches an Envelope to itself THE SYSTEM SHALL deliver it as one of that Module's later requests without blocking its run loop.
- **NEEDS-2.5** IF a Module dispatches to a Module that is unknown, not running, or does not reply within the dispatch timeout THEN THE SYSTEM SHALL report the same `DispatchError` `Kernel::dispatch` returns for that case.
- **NEEDS-2.6** IF a Module other than `needs_you` dispatches an Action whose catalog Risk is `Destructive` THEN THE SYSTEM SHALL refuse that dispatch and run nothing — only the user's answer to a Decision runs a `Destructive` Action from inside the Core (ARCH-5).
- **NEEDS-2.7** WHEN a Module dispatches `needs_you.raise` through its dispatcher THE SYSTEM SHALL add the Decision exactly as for any other caller — proven by a test Module that raises, has its Decision answered, and observes the chosen Envelope run.

## 3. See open Decisions on level 0

**Story:** As the user, I want each open Decision as one compact row at the top of the menu, so that I see what needs my decision the moment I open Navori.

- **NEEDS-3.1** WHILE at least one Decision is open THE SYSTEM SHALL show on level 0, after any banners, a `Decide N` header carrying the open count, then one row per open Decision, ordered by earliest `expires_at_ms` first and then by oldest raise, with no limit on the number of rows (amends rule 6 of the 2026-11-01 menubar brief, at most 3 items). *(agent-decided — order and no cap, close-package F14)*
- **NEEDS-3.2** THE SYSTEM SHALL draw each Decide row on one 26 pt line: a lead icon in the accent color, the title truncated to fit, and the age right-aligned.
- **NEEDS-3.3** WHILE no Decision is open THE SYSTEM SHALL show neither the `Decide` header nor any Decide row.
- **NEEDS-3.4** WHILE at least one Decision is open THE SYSTEM SHALL turn on the status icon's attention mark, and WHEN the last open Decision closes THE SYSTEM SHALL turn it off.
- **NEEDS-3.5** THE SYSTEM SHALL give each Decide row its full reason as its tooltip and its accessibility value.
- **NEEDS-3.6** WHEN VoiceOver reads a Decide row THE SYSTEM SHALL read the kind, the title, the reason, the age and both choice labels, as in `Decision: Review PR #41 in zroute, Minh requested your review, 25 minutes ago, Open, Later`.
- **NEEDS-3.7** THE SYSTEM SHALL NOT list a Decision anywhere in the Needs you submenu.
- **NEEDS-3.8** THE SYSTEM SHALL carry each choice's Risk in the Core's Needs you view as the Action catalog lists it now, not as it was at raise. *(agent-decided — Risk read again after an upgrade, close-package C1)*
- **NEEDS-3.9** WHERE the code is Release code THE SYSTEM SHALL show the Decide block, the `Needs you` row and `✓ You're all caught up` (amends MENU-1.4 and rule 12 of the 2026-11-01 menubar brief).

## 4. Answer a Decision

**Story:** As the user, I want to answer a Decision with one click or one key, so that the action I chose runs and the Decision leaves the menu.

- **NEEDS-4.1** WHEN the pointer is over a Decide row or the row has keyboard focus THE SYSTEM SHALL replace its age with two mini buttons, the primary choice's label in the accent color and the secondary choice's label in a fill.
- **NEEDS-4.2** WHEN the user clicks a choice's button, or presses Return (primary) or ⌘⌫ (secondary) on a focused Decide row, THE SYSTEM SHALL dispatch `needs_you.answer` naming that Decision and that choice.
- **NEEDS-4.3** THE SYSTEM SHALL list `needs_you.answer` in the Action catalog with Risk `Destructive`.
- **NEEDS-4.4** WHEN `needs_you.answer` arrives for an open Decision whose chosen choice has an Envelope THE SYSTEM SHALL dispatch that Envelope from the Core, whatever its Risk.
- **NEEDS-4.5** WHEN the chosen Envelope's dispatch succeeds, or the chosen choice has no Envelope, THE SYSTEM SHALL close the Decision, move it to History with the chosen label, and remove its row from level 0. *(agent-decided — a choice with no Envelope closes the Decision, close-package C1)*
- **NEEDS-4.6** WHILE an answer is running THE SYSTEM SHALL show a spinner in the chosen button and disable the other button.
- **NEEDS-4.7** IF the chosen Envelope's dispatch fails, or its Action is no longer in the Action catalog when the answer arrives, THEN THE SYSTEM SHALL keep the Decision open, show `Retry` as its primary button, and carry the error in the row's tooltip and accessibility value.
- **NEEDS-4.8** WHEN the user activates `Retry` THE SYSTEM SHALL answer the Decision again with the choice that failed.
- **NEEDS-4.9** IF `needs_you.answer` names a Decision that is not open, or one whose answer is already running, THEN THE SYSTEM SHALL refuse it and dispatch nothing.
- **NEEDS-4.10** IF Navori stops while a Decision's answer is running THEN THE SYSTEM SHALL, on the next start, show that Decision in the failed state of NEEDS-4.7 with the error `Navori stopped before this finished — it may already have run`, and SHALL NOT dispatch its Envelope again until the user answers again. *(agent-decided — error copy)*
- **NEEDS-4.11** WHEN Navori AI calls `needs_you.answer` THE SYSTEM SHALL refuse the call and dispatch no Envelope.
- **NEEDS-4.12** IF `needs_you.answer` names a version of a Decision that a later raise with the same `key` has replaced THEN THE SYSTEM SHALL refuse it and dispatch nothing. *(agent-decided — a click on stale text never runs)*

## 5. Let a Decision expire

**Story:** As a Module author, I want a Decision raised with an expiry to lapse on its own, so that the user is not asked something that no longer matters.

- **NEEDS-5.1** WHILE an open Decision has no `expires_at_ms` THE SYSTEM SHALL keep it open until it is answered.
- **NEEDS-5.2** WHEN an open Decision's `expires_at_ms` passes while it is unanswered and no answer is running THE SYSTEM SHALL close it, remove its row from level 0, and add an Update of kind `decision_expired` titled `Expired: <the Decision's title>`. *(agent-decided — title copy)*
- **NEEDS-5.3** IF a raise carries an `expires_at_ms` that is not in the future THEN THE SYSTEM SHALL refuse the raise and add nothing. *(agent-decided)*
- **NEEDS-5.4** WHEN Navori starts or the Mac wakes after an open Decision's `expires_at_ms` has passed THE SYSTEM SHALL expire that Decision as NEEDS-5.2 says.
- **NEEDS-5.5** THE SYSTEM SHALL produce Updates in this feature only by expiring Decisions, and SHALL offer no Action that posts an Update. *(agent-decided — close-package C2)*

## 6. Read Updates

**Story:** As the user, I want Updates I have seen to leave the list on their own, so that the count on `Needs you` means what is new to me.

- **NEEDS-6.1** THE SYSTEM SHALL list Updates in the Needs you submenu after the To-dos, newest first, each as a two-line row with a `bell` lead, dimmer than a To-do, and no buttons.
- **NEEDS-6.2** WHEN the Needs you submenu closes THE SYSTEM SHALL mark as viewed exactly the Updates whose rows were drawn in the visible part of the list on the tab then shown. *(agent-decided — close-package C3)*
- **NEEDS-6.3** WHEN the Needs you submenu opens THE SYSTEM SHALL leave viewed Updates out of the list and out of every count, and keep them in History.
- **NEEDS-6.4** WHEN the user clicks an Update of kind `decision_expired` THE SYSTEM SHALL open nothing.
- **NEEDS-6.5** IF Navori stops before the Needs you submenu closes THEN THE SYSTEM SHALL mark no Update as viewed for that showing.

## 7. Clear updates

**Story:** As the user, I want to clear every Update at once from the Updates tab, so that the list is empty when I have dealt with them.

- **NEEDS-7.1** WHILE the Updates tab is shown THE SYSTEM SHALL show a `Clear updates` row, and SHALL NOT show it on the All or To-dos tab.
- **NEEDS-7.2** WHEN the user activates `Clear updates` THE SYSTEM SHALL move every Update to History.

## 8. Add a To-do

**Story:** As the user, I want to type a To-do and pick its due date in the Needs you submenu, so that my own task sits beside everything else that needs me.

- **NEEDS-8.1** THE SYSTEM SHALL show in the Needs you submenu, below the filter, an `Add a to-do…` field in a rounded fill with a date chip reading `No date` or the picked day.
- **NEEDS-8.2** WHEN the user clicks the field or tabs into it THE SYSTEM SHALL make the Needs you submenu the key window and take the user's typing into the field, with Return adding the To-do (NEEDS-8.4) instead of activating a row.
- **NEEDS-8.3** WHEN the user activates the date chip THE SYSTEM SHALL open a month calendar beside the submenu, with `Today`, `Tomorrow` and `No date` quick picks, and set the chip to the day picked.
- **NEEDS-8.4** WHEN the user presses Return in the field with a title that is not empty or only whitespace THE SYSTEM SHALL add a To-do with that title and the chip's due date, show it in the list at once, empty the field and reset the chip to `No date`.
- **NEEDS-8.5** IF the user presses Return in the field while it is empty or only whitespace THEN THE SYSTEM SHALL add nothing.
- **NEEDS-8.6** WHEN a To-do is added THE SYSTEM SHALL pass its title through the Sensitive Data Shield before storing it.
- **NEEDS-8.7** THE SYSTEM SHALL list `todo.add` in the Action catalog with Risk `Write`, taking a `title` and an optional due date.
- **NEEDS-8.8** WHEN Navori or the `todo` Module restarts THE SYSTEM SHALL keep every To-do with its title, due date and done state.

## 9. See a To-do's status in its date pill

**Story:** As the user, I want a To-do's date pill colored by how late it is, so that I see what is overdue without reading dates.

- **NEEDS-9.1** THE SYSTEM SHALL list open To-dos in the Needs you submenu by due date, overdue first and those with no date last, each as one 26 pt line: a checkbox, the title truncated to fit, and a date pill.
- **NEEDS-9.2** THE SYSTEM SHALL give each open To-do a status computed in the Core from the local calendar day — Overdue (due before today), Today, Soon (due in 1–3 days), Later (due in 4 days or more), None (no due date) — and color its pill `systemRed`, `systemOrange`, `systemYellow`, `systemBlue`, or a fill with secondary label, in that order.
- **NEEDS-9.3** THE SYSTEM SHALL write the pill's text as `Today`, `Tomorrow`, the weekday name (as `Sun`) for a day 2 to 6 days ahead, `Wkd D` (as `Fri 13`, `Thu 5`) for any other day before or after, and `No date` for none.
- **NEEDS-9.4** WHEN the local calendar day changes, including while the Mac slept or the menu was closed, THE SYSTEM SHALL show each To-do's new status the next time the menu or the submenu opens.
- **NEEDS-9.5** WHEN VoiceOver reads a date pill THE SYSTEM SHALL read its status in words with the full date, as `Overdue, Thursday 5 November`, `Due today`, `Due soon, Sunday 8 November`, `Due later, Friday 13 November` or `No due date`.

## 10. Tick a To-do

**Story:** As the user, I want to tick a To-do done, so that finishing it is one click.

- **NEEDS-10.1** WHEN the user ticks a To-do's checkbox THE SYSTEM SHALL strike its title through, remove it from the list after about 0.45 s, and move it to History.
- **NEEDS-10.2** THE SYSTEM SHALL list `todo.tick` in the Action catalog with Risk `Write`.
- **NEEDS-10.3** WHEN VoiceOver reads a To-do's checkbox THE SYSTEM SHALL read `Mark done`.

## 11. The Needs you row and submenu

**Story:** As the user, I want one `Needs you` row that opens my To-dos and Updates, so that everything except Decisions waits one hover away.

- **NEEDS-11.1** THE SYSTEM SHALL lay out level 0 in this order: any banners, the Decide block (NEEDS-3.1), the `Needs you` row, a separator, the `Running` row, the Core row as MENU-3.11 says, a separator, `Settings…`, `About Navori`, `Quit Navori` (amends MENU-1.2 and AWAKE-11.1).
- **NEEDS-11.2** THE SYSTEM SHALL draw the `Needs you` row with a `sparkle` icon, a value equal to the number of open To-dos plus unviewed Updates, and a chevron that opens the Needs you submenu.
- **NEEDS-11.3** WHILE any open To-do is Overdue THE SYSTEM SHALL show a red status dot on the `Needs you` row.
- **NEEDS-11.4** WHILE no Decision is open, no To-do is open and no Update is unviewed THE SYSTEM SHALL show `✓ You're all caught up` where the Decide block would be and show the `Needs you` row with no count.
- **NEEDS-11.5** THE SYSTEM SHALL lay out the Needs you submenu, about 350 pt wide, as: a `Needs you N` header, a segmented filter `All · To-dos n · Updates n`, the `Add a to-do…` field, a separator, the list, a separator, `Clear updates` on the Updates tab, and `History ›`.
- **NEEDS-11.6** WHEN the user picks a filter segment THE SYSTEM SHALL list only that kind, and on `All` list the open To-dos and then the unviewed Updates.
- **NEEDS-11.7** WHILE the shown tab has nothing to list THE SYSTEM SHALL show `✓ You're all caught up` in the list and keep the field shown.
- **NEEDS-11.8** THE SYSTEM SHALL expose the filter to VoiceOver as a tab group and put the field before the list in the keyboard order.
- **NEEDS-11.9** THE SYSTEM SHALL NOT turn on the status icon's attention mark for a To-do or an Update.

## 12. History

**Story:** As the user, I want a History of what I resolved in the last 30 days, so that I can check how I answered a Decision or undo a tick.

- **NEEDS-12.1** WHEN the user opens `History ›` THE SYSTEM SHALL list, newest first, the ticked To-dos with an `Undo` each, the answered Decisions with the label chosen, and the viewed or cleared Updates, resolved within the last `journal.retention_days` days.
- **NEEDS-12.2** THE SYSTEM SHALL offer no way to answer again a Decision listed in History.
- **NEEDS-12.3** THE SYSTEM SHALL delete each History item resolved more than `journal.retention_days` days ago, at start and at least once a day. *(agent-decided — cadence)*
- **NEEDS-12.4** WHILE `journal.retention_days` is invalid, so that the Journal sweep is paused, THE SYSTEM SHALL delete no History item.
- **NEEDS-12.5** WHEN the user activates `Undo` on a ticked To-do in History THE SYSTEM SHALL return it to the open list with its title and due date.
- **NEEDS-12.6** THE SYSTEM SHALL list `todo.undo` in the Action catalog with Risk `Write`.

## 13. What keeps working

**Story:** As the user, I want the menu, the icon and Keep awake to behave as they do today once Needs you ships, so that nothing I rely on breaks.

Files this feature touches, and what each must keep:

- `crates/kernel/src/module.rs`, `crates/kernel/src/runtime.rs` — Module context and dispatch (NEEDS-13.1, 13.2).
- `crates/kernel/src/tray.rs` — icon composition (NEEDS-13.3).
- `crates/kernel/src/system.rs` — Journal sweep, unchanged; History prunes its own tables, the kernel does not reach them (ARCH-3) (NEEDS-13.4).
- `ai/agent/src/dispatch.rs` — Navori AI's gate (NEEDS-13.5).
- `crates/ffi/src/lib.rs`, `crates/ffi/src/app.rs` — snapshot, `build_kernel`, IPC `dispatch` (NEEDS-13.6, 13.7).
- `crates/ffi/tests/actions_risk.rs` — Risk coverage (NEEDS-13.8).
- `modules/awake/` — Lease ends (NEEDS-13.9).
- `apps/macos/Navori/Features/Panel/PanelView.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/Navori/Core/CoreStore.swift` — level 0 and its submenus (NEEDS-13.10).
- `apps/macos/Navori/StatusBar/CascadePanel.swift` — key-window rule (NEEDS-13.11).
- `apps/macos/Navori/StatusBar/StatusPanel.swift` — the menu closes when it loses key to another window (NEEDS-13.13).
- `apps/macos/Navori/Features/Preview/` — dev-only preview (NEEDS-13.12).
- `apps/cli/src/main.rs` — no behavior to guard (no change; `navori dispatch` is reused, NEEDS-1.10).
- `crates/protocol/src/lib.rs` — no behavior to guard (Envelope and Risk reused unchanged).
- `crates/store/src/lib.rs` — no behavior to guard (`migrate` and queries are used, not changed).
- `crates/config/src/lib.rs` — `journal.retention_days` is read by the new Modules; its parse and its pause on an invalid value stay as they are (NEEDS-13.4).
- `modules/needs_you/`, `modules/todo/` — new; no behavior to guard.

- **NEEDS-13.1** (guard) WHEN the Shell, the CLI or Navori AI dispatches an Envelope THE SYSTEM SHALL CONTINUE TO deliver it to the named Module and return its reply or the `DispatchError` `Kernel::dispatch` returns today, under its 30 s dispatch timeout.
- **NEEDS-13.2** (guard) WHEN a Module crashes and restarts THE SYSTEM SHALL CONTINUE TO keep the requests still queued for it (the one being handled at the crash is lost, as today).
- **NEEDS-13.3** (guard) WHEN the Core composes the status icon THE SYSTEM SHALL CONTINUE TO follow ICON-3.1 to ICON-3.4, with attention meaning only that Needs you holds a Decision (ICON-5.4).
- **NEEDS-13.4** (guard) WHEN the Journal sweep runs THE SYSTEM SHALL CONTINUE TO prune Journal events older than `journal.retention_days` and to pause while that value is invalid.
- **NEEDS-13.5** (guard) WHEN Navori AI calls an Action whose Risk is not `Read` THE SYSTEM SHALL CONTINUE TO refuse it with reason `not_read_only`.
- **NEEDS-13.6** (guard) WHILE the menu is open THE SYSTEM SHALL CONTINUE TO publish Core snapshots at most every 33 ms, and at most every 250 ms while it is closed.
- **NEEDS-13.7** (guard) WHEN the user opens level 0 THE SYSTEM SHALL CONTINUE TO show the `Running` row and its submenu, and the Core row as MENU-3.11 says.
- **NEEDS-13.8** (guard) WHEN the test suite runs THE SYSTEM SHALL CONTINUE TO check that every Action's catalog Risk equals the Risk the Action reports, now including every `needs_you` and `todo` Action.
- **NEEDS-13.9** (guard) WHEN a timed Keep awake Lease reaches its end THE SYSTEM SHALL CONTINUE TO end it as today and SHALL raise no Decision.
- **NEEDS-13.10** (guard) WHERE the code is Release code THE SYSTEM SHALL CONTINUE TO hide the Ask field, the Ports and Pull Requests rows, and every sample row of the dev-only preview.
- **NEEDS-13.11** (guard) WHILE neither the `Add a to-do…` field nor the date calendar has focus THE SYSTEM SHALL CONTINUE TO keep every cascade panel from becoming the key window except while VoiceOver is on, and to route Return to the selected row.
- **NEEDS-13.13** (guard) WHEN a window outside Navori's menu and its submenus becomes the key window THE SYSTEM SHALL CONTINUE TO close the menu.
- **NEEDS-13.12** (guard) WHEN a Debug build opens the dev-only preview THE SYSTEM SHALL CONTINUE TO show it with sample data, and Release builds SHALL CONTINUE TO compile the preview out (`#if DEBUG`).

## 14. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** **NEEDS-14.1** WHEN the menu opens with the Decide block and the `Needs you` row THE SYSTEM SHALL draw the first frame within 80 ms on the first open and 50 ms on later opens, and the Needs you submenu within the 16 ms submenu budget (measured today on the Core Submenu) — the standing budgets in `docs/standards/performance.md`, verified by `cargo xtask perf` in the release run, not per feature.
- **Performance:** **NEEDS-14.2** WHILE the menu is closed THE SYSTEM SHALL hold the idle budgets of `docs/standards/performance.md` (0% CPU, ≤ 1 wake-up per second), waking for Decision expiry and day change only through the Scheduler and never more often than every 30 s (ARCH-16) — verified by the release perf run.
- **Security:** **NEEDS-14.3** THE SYSTEM SHALL keep Navori AI from answering a Decision (THR-2, across TB-3) and store every Decision, Update and To-do text only after the Sensitive Data Shield (THR-3, TB-6) — verified by the tests of NEEDS-4.11, NEEDS-1.8 and NEEDS-8.6. No criterion here claims the CLI or the IPC socket checks Risk; they do not (see Out of Scope).
- **Reliability:** **NEEDS-14.4** WHEN the `needs_you` or `todo` Module crashes, or Navori restarts, THE SYSTEM SHALL lose no open Decision, To-do, Update or History item, and SHALL create and upgrade their tables through versioned transactional migrations with a backup before each upgrade (ARCH-10, ARCH-12) — verified by restart and migration tests.
- **Accessibility:** **NEEDS-14.5** THE SYSTEM SHALL let every Decide row, choice button, To-do checkbox, the field, the date chip, the filter and `History ›` be reached and used with the keyboard and VoiceOver, and SHALL never convey a status by color alone — verified by a keyboard and VoiceOver walk in dogfood. No standing accessibility standard exists (`docs/standards/` has none); the rules come from the brief's A11y lines.

## Out of Scope

- Choosing which source raises a Decision on ship day — the operator's open question (Open Questions).
- Decisions raised by Navori AI, approvals and permission presets (ROAD-37).
- Proactive Updates and any Action that posts an Update (ROAD-23). *(agent-decided — close-package C2)*
- Pull requests (ROAD-20).
- The full TODO feature: lists, notes, repeats, editing or deleting a To-do (ROAD-42).
- Snooze; the secondary choice closes the Decision. *(agent-decided — close-package C1)*
- macOS notifications, banners and sounds.
- A creator field on To-dos.
- Anything Awake or a Lease raises into Needs you.
- A Risk gate on the CLI or the IPC socket: anything in the user's shell, a Harness agent included, can answer a Decision today as it can already dispatch a `Destructive` Action directly (`crates/ffi/src/app.rs:521-528`, `apps/cli/src/main.rs:58-63`); named for ROAD-37.
- MCP's refusal of `needs_you.answer`: no MCP surface exists; MILE-7's approval policy must refuse it through the same Risk gate.
- A per-feature perf run or gate; budgets are measured once before release.

## Open Questions

- Which sources raise a Decision on ship day (a PR review request, a reminder, …) — operator — 2026-11-08 — forbid-guess (cấm đoán). The spec ships the contract, a test producer and hand raising through `navori dispatch needs_you`. In roadmap order the first real source is ROAD-37 (MILE-3), then ROAD-20 (MILE-5).
