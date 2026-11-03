# Requirements: Status icon states

Feature code: ICON
Status: In-progress
Date: 2026-11-02

<!--
Rules:
- Every acceptance criterion gets a hierarchical ID: ICON-<story>.<criterion>.
- Criteria use EARS phrasing (WHEN / WHILE / IF…THEN / WHERE / ubiquitous).
- Guard requirements protect existing behavior: WHEN <condition> THE SYSTEM SHALL CONTINUE TO <behavior>.
- IDs are immutable once Status is Approved. Retire by strikethrough — never renumber.
-->

Roadmap item: ROAD-35 (MILE-2) · Tracker: #8 · Decisions: frame-change close package and its supplement (2026-11-02), confirmed by the user.

Delivery order: this feature starts after ROAD-8 (AWAKE) merges. Design is written against that code.

The look is locked in `./ui-brief.md`. It replaces the sparkle logo and the SF Symbols on the status item.

It changes one thing in MENU's spec: MENU-G4 ("the same four symbols") is replaced by stories 2 and 7, because a config notice no longer draws the triangle and new glyphs exist. It also changes what the status item says: today's health text is one of failed, restarting or config, and the summary carries the glyph's sentence and the config notice together. It moves `tray()` ahead of `context()`, which `docs/ARCHITECTURE.md:107` places together in MILE-3; `context()` stays in MILE-3.

Terms used below:
- **The system** — the Kernel, the Awake Module, `navori-ffi`, the `navori` CLI and the Shell code that draws the status item.
- **Icon** — the image of Navori's status item in the menu bar.
- **Icon state** — the one value the Kernel composes: a glyph, an attention that may be on or off, and a summary sentence.
- **Glyph** — one of four values in the icon state: problem, Lease, AI running, normal.
- **Problem glyph** — the glyph for a failed or restarting Module, or for a problem a Module reports through `tray()`.
- **Attention** — a second mark on the Icon. On means Needs you holds a Decision.
- **`tray()`** — what a Module implements to add a signal to the icon state. It reports nothing by default.
- **Core-down** and **Core-starting** — two states the Shell draws itself, because no icon state exists then: the Core failed to start, or no snapshot has arrived yet.
- **Loop** — a repeating animation of the Icon.
- **Beat** — one short animation of the Icon, played once.
- **Sample** — an icon state chosen in the dev-only Preview, not read from the Core.

## 1. Read the icon state from the CLI

**Story:** As the developer, I want to run `navori status --json` against a running Core and read the icon state, so that I can check what the Icon should show without running the Shell.

- **ICON-1.1** WHEN the status is read THE SYSTEM SHALL compose the icon state once, in Kernel code that `Inner::status()` calls with the config notice's `title` and `message` as an input.
- **ICON-1.2** WHEN `navori status --json` runs THE SYSTEM SHALL include a `tray` field holding the glyph, the attention and the summary.
- **ICON-1.3** WHEN a snapshot is built THE SYSTEM SHALL copy `tray` from the status it read, with no second composition of the icon state in `snapshot()` or in Swift.
- **ICON-1.4** (guard) WHEN `navori status` runs without `--json` THE SYSTEM SHALL CONTINUE TO print its current output, with no line added for the icon state.

## 2. See the most important glyph win

**Story:** As the user, I want a failed Module to show the problem glyph even while the Mac is held awake, so that a glance tells me something is wrong.

- **ICON-2.1** THE SYSTEM SHALL choose the glyph in this order: problem, Lease, AI running, normal.
- **ICON-2.2** WHILE any Module is failed or restarting THE SYSTEM SHALL choose the problem glyph.
- **ICON-2.3** WHILE a config notice exists and nothing else asks for the problem glyph THE SYSTEM SHALL NOT choose the problem glyph.
- **ICON-2.4** WHILE no running Module reports a glyph and no Module is failed or restarting THE SYSTEM SHALL choose the normal glyph.
- **ICON-2.5** WHILE a Module is not running THE SYSTEM SHALL ignore what its `tray()` reports, because its last published State outlives it.
- **ICON-2.6** WHILE a Module is starting or stopped and no Module is failed or restarting THE SYSTEM SHALL NOT choose the problem glyph for that Module.

## 3. Add a signal from a Module

**Story:** As the developer building a later Module, I want to add a signal by writing only that Module's `tray()`, so that the icon's composition never needs editing for a new source.

- **ICON-3.1** THE SYSTEM SHALL give every Module a `tray()` that reports nothing unless the Module overrides it.
- **ICON-3.2** WHEN a running Module's `tray()` reports a glyph with its sentence, or attention with its clause, THE SYSTEM SHALL use it in the icon state with no edit to the composing code — verified by a test Module in `crates/kernel/tests`.
- **ICON-3.3** THE SYSTEM SHALL take failed, restarting and the config notice as the composer's own inputs, not as results of `tray()`.
- **ICON-3.4** THE SYSTEM SHALL state in the doc comment of `tray()` that a Module reports the problem glyph only when its main function is failing.

## 4. See the Icon follow a real Lease

**Story:** As the user, I want to keep the Mac awake for an hour, see the Icon change, press Release all and see it change back, so that the Icon tells me whether the Mac is being held.

- **ICON-4.1** WHILE Awake has any Lease, including a `display` Lease, and its state says `system_held` is true THE SYSTEM SHALL report the Lease glyph from `Awake::tray()`.
- **ICON-4.2** WHILE Awake has no Lease THE SYSTEM SHALL report no glyph from `Awake::tray()`.
- **ICON-4.3** IF Awake has any Lease and its state says `system_held` is false THEN THE SYSTEM SHALL report the problem glyph from `Awake::tray()`, with a sentence saying the Mac cannot be held awake.
- **ICON-4.4** WHILE `system_held` is false THE SYSTEM SHALL NOT show the Lease glyph or a sentence saying the Mac is held.
- **ICON-4.5** WHILE `system_held` is true, whatever else Awake's `problem` records (`display`, `release`, `store`, `platform`, `exits`), THE SYSTEM SHALL NOT report the problem glyph from `Awake::tray()`.

## 5. See one mark while a Decision waits

**Story:** As the developer, I want to pick a Sample with the problem glyph and attention on and see both on the Icon, so that the mark is proven before Needs you exists.

- **ICON-5.1** THE SYSTEM SHALL give attention two states, on and off, drawn with one shape.
- **ICON-5.2** WHILE a running Module's `tray()` reports attention THE SYSTEM SHALL set attention on in the icon state.
- **ICON-5.3** WHILE the problem glyph and attention both apply THE SYSTEM SHALL carry both in the icon state, so that neither hides the other.
- **ICON-5.4** THE SYSTEM SHALL state in the doc comment of `tray()` that attention means Needs you holds a Decision, and that To-dos, Updates, pull request counts, failing checks and port counts do not turn it on.
- **ICON-5.5** THE SYSTEM SHALL draw attention with no second level and no meaning carried by color.

## 6. Hear what the Icon says

**Story:** As a VoiceOver user, I want to focus the status item with a Lease held and a bad config value and hear both, so that I get what the Icon says and what the banner says.

- **ICON-6.1** WHEN the icon state is composed THE SYSTEM SHALL build the summary as the glyph's sentence, then a Decision clause while attention is on, then the config notice's `title` and `message` while a notice exists.
- **ICON-6.2** WHEN a failed or restarting Module and a problem reported by `tray()` both exist THE SYSTEM SHALL put the failed or restarting sentence first.
- **ICON-6.3** THE SYSTEM SHALL use the first failed Module's sentence, else the first restarting Module's sentence, in today's wording: `<id> stopped: <detail>`, with `unknown error` when there is no detail, and `<id> is restarting`.
- **ICON-6.4** WHEN a config notice is in the summary THE SYSTEM SHALL use its `title` and `message` unchanged, joined as today's `<title>. <message>`, with no new config sentence and no count in place of the key list.
- **ICON-6.5** WHILE a snapshot exists THE SYSTEM SHALL use the summary as the Icon's accessibility description.
- **ICON-6.6** (guard) WHEN one or more config values are bad THE SYSTEM SHALL CONTINUE TO name `config.toml` and each bad key in the status-item health text, which is now the summary, and to say whether the whole file is on defaults (KCONTRACT-2.3, KCONTRACT-2.4, KCONTRACT-2.5).
- **ICON-6.7** (guard) WHEN several problems exist at once THE SYSTEM SHALL CONTINUE TO put a failed or restarting Module ahead of a config problem in the health text, which is now the summary (KCONTRACT-G12).

## 7. See the Icon start and fail

**Story:** As the user, I want to launch Navori and see the Icon go from starting to its real state, so that the Icon is right before and after the Core exists.

- **ICON-7.1** WHILE a snapshot exists and the Core started THE SYSTEM SHALL draw the Icon only from the icon state, with no choice of glyph made in Swift.
- **ICON-7.2** THE SYSTEM SHALL draw from a plain Swift value that `CoreStore` maps from the icon state, so that the drawing code and the Preview need no generated types (ARCH-2).
- **ICON-7.3** THE SYSTEM SHALL draw each glyph and attention as a monochrome template image.
- **ICON-7.4** THE SYSTEM SHALL give the AI running glyph a static image different from the normal glyph's.
- ~~**ICON-7.5**~~ (guard) replaced by ICON-7.7: the locked set replaces `xmark.octagon`.
- ~~**ICON-7.6**~~ (guard) replaced by ICON-7.8: the locked set replaces `circle.dotted`.
- **ICON-7.7** IF the Core fails to start THE SYSTEM SHALL draw the Core not running glyph from `./ui-brief.md`, with the description `Core not running` — verified by hand with a data directory the Core cannot open.
- **ICON-7.8** WHILE no snapshot has arrived and the Core has not failed THE SYSTEM SHALL draw the Starting glyph from `./ui-brief.md`, dimmed (MENU-G11), with the description `Starting`.
- **ICON-7.9** THE SYSTEM SHALL draw every glyph and the attention mark from the shapes in `./ui-brief.md`.

## 8. See the Icon move only when it means something

**Story:** As the user, I want to watch the Loop and the Beat on a Sample, then turn on Reduce Motion and see both stop, so that motion adds to the Icon and is never the only signal.

- **ICON-8.1** WHILE the glyph shown is AI running THE SYSTEM SHALL play the Loop.
- **ICON-8.2** WHILE the glyph shown is problem, Lease or normal, or the Icon shows Core-down or Core-starting THE SYSTEM SHALL NOT play the Loop.
- **ICON-8.3** WHEN attention goes from off to on after the Icon has drawn a snapshot THE SYSTEM SHALL play one Beat.
- **ICON-8.4** WHEN the first snapshot after launch has attention on THE SYSTEM SHALL NOT play a Beat.
- **ICON-8.5** WHILE attention stays on THE SYSTEM SHALL NOT play another Beat, however many Decisions arrive.
- **ICON-8.6** WHILE Reduce Motion is on THE SYSTEM SHALL play no Loop and no Beat.
- **ICON-8.7** WHEN the Reduce Motion setting changes while the app runs THE SYSTEM SHALL apply it with no restart of the app — verified by hand, turning the setting on and off in System Settings during a looping Sample.

## 9. Judge every state on the real status item

**Story:** As the developer, I want to pick each Sample from the dev build and see it on the real status item, so that I can judge what is readable before its source exists.

- **ICON-9.1** WHERE the code is compiled in the debug configuration THE SYSTEM SHALL offer a Sample for each glyph with attention on and with attention off, for Core-down and for Core-starting, the last one dimmed.
- **ICON-9.2** WHERE the code is compiled in the debug configuration THE SYSTEM SHALL offer a way to play the Loop and the Beat on a Sample.
- **ICON-9.3** WHEN a Sample is chosen THE SYSTEM SHALL draw it through the same drawing code as a real snapshot.
- **ICON-9.4** WHILE a Sample is on THE SYSTEM SHALL keep it on the Icon, whatever closes the Preview panel, until the user turns it off.
- **ICON-9.5** WHEN the user turns the Sample off THE SYSTEM SHALL return the Icon to the real snapshot with no Beat.
- **ICON-9.6** THE SYSTEM SHALL compile Samples out of Release code, verified by searching the release binary for a string that exists only in the Preview.

## 10. Keep what exists working

**Story:** As the user, I want the menu, the CLI and Awake to behave as before after the Icon changes, so that a new Icon breaks nothing I rely on.

Files this feature touches, with what they guard:

| File | Guard |
|---|---|
| `crates/kernel/src/module.rs`, `crates/kernel/src/runtime.rs` | ICON-10.1, ICON-10.2 |
| `crates/protocol/src/lib.rs` | ICON-10.3 |
| `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs` | ICON-10.3, ICON-10.4 |
| `crates/ffi/tests/core.rs`, `crates/ffi/tests/config_privacy.rs` | ICON-6.6, ICON-10.3, ICON-10.4, ICON-11.5 |
| `modules/awake/` (gains `tray()`) | ICON-10.5 |
| `apps/cli/src/main.rs` (`print_status` decodes the grown `CoreStatus`), `apps/cli/tests/cli.rs` | ICON-1.4, ICON-10.3 |
| `apps/macos/Navori/Core/CoreStore.swift` | ICON-6.6, ICON-6.7, ICON-10.6, ICON-10.7, ICON-10.8, ICON-10.9 |
| `apps/macos/Navori/StatusBar/StatusBarController.swift` | ICON-7.7, ICON-7.8, ICON-10.10, ICON-10.11, ICON-10.12, ICON-10.13, ICON-10.15, ICON-10.16, ICON-10.17 |
| `apps/macos/Navori/Features/Panel/PanelView.swift` (reads `coreStatus`) | ICON-10.6, ICON-10.7 |
| `apps/macos/Navori/Features/Preview/` | ICON-10.14 |
| `apps/macos/NavoriPanel/` (new plain value), `apps/macos/Tests/NavoriPanelTests/CoreRowStateTests.swift` | ICON-10.6, ICON-10.7 |
| `xtask/assets/make-icon.swift`, `xtask/assets/AppIcon.icns`, `xtask/assets/AppIconDev.icns` | ICON-12.2, ICON-12.3 |
| `docs/ARCHITECTURE.md`, MENU's `requirements.md` | no behavior to guard |
| MENU's `design.md` and `tasks.md`, which cite MENU-G4 | no behavior to guard: a closed record of what was true at delivery, left as it is |

- **ICON-10.1** (guard) WHEN a Module has no `tray()` of its own THE SYSTEM SHALL CONTINUE TO build and run with no edit, including the `system` Module and every test Module.
- **ICON-10.2** (guard) WHEN a Module fails and restarts THE SYSTEM SHALL CONTINUE TO restart it with backoff, keeping `self`, its State and its queued requests.
- **ICON-10.3** (guard) WHEN the status or a snapshot is read THE SYSTEM SHALL CONTINUE TO carry every field `CoreStatus` and `CoreSnapshot` have today.
- **ICON-10.4** (guard) WHEN snapshots are pushed THE SYSTEM SHALL CONTINUE TO coalesce them at 33 ms while the panel is visible and 250 ms while it is not.
- **ICON-10.5** (guard) WHEN Awake reports its state THE SYSTEM SHALL CONTINUE TO report the System assertion as not held while it is not held (AWAKE-8.6), and no assertion as held while the Module is failed (AWAKE-8.9).
- **ICON-10.6** (guard) WHILE a config notice exists and no Module is failed or restarting THE SYSTEM SHALL CONTINUE TO show `Needs attention` on the Core row (MENU-3.2).
- **ICON-10.7** (guard) WHILE Awake has a Lease with no System assertion and every Module is running THE SYSTEM SHALL CONTINUE TO show `Running` on the Core row.
- **ICON-10.8** (guard) WHEN the Core reports a config problem THE SYSTEM SHALL CONTINUE TO show a banner with the Core's report as its message (MENU-2.3).
- **ICON-10.9** (guard) WHEN Swift reads the icon state THE SYSTEM SHALL CONTINUE TO import the generated bindings only in `CoreStore` and `NavoriBanner`, the two files that import them today.
- **ICON-10.10** (guard) WHEN the user clicks the status button with the panel open THE SYSTEM SHALL CONTINUE TO have the toggle handle that click, not the outside-click monitor (MENU-G3).
- **ICON-10.11** (guard) WHEN the user right-clicks or control-clicks the status item THE SYSTEM SHALL CONTINUE TO show the native `NSMenu` with `Open <app name>`, `Settings…`, `Open Logs Folder` and `Quit <app name>`, with any Preview item present only under `#if DEBUG` (MENU-G7, MENU-7.1).
- **ICON-10.12** (guard) WHILE the app runs THE SYSTEM SHALL CONTINUE TO carry the app name as the status button's tooltip (MENU-G11).
- **ICON-10.13** (guard) WHEN the status item first draws in perf mode THE SYSTEM SHALL CONTINUE TO log `statusItemDrawn` (MENU-G12), once, however many times the image changes, as the code does today.
- **ICON-10.14** (guard) WHILE the Preview is open THE SYSTEM SHALL CONTINUE TO dispatch no Action from a sample row (MENU-7.5).
- **ICON-10.15** (guard) WHEN `SIGUSR1` or `SIGUSR2` arrives in perf mode THE SYSTEM SHALL CONTINUE TO open the real menu or the real Core Submenu.
- **ICON-10.16** (guard) WHEN the user chooses `Open Logs Folder` THE SYSTEM SHALL CONTINUE TO open the logs directory from the snapshot.
- **ICON-10.17** (guard) WHEN the first snapshot arrives with the panel open THE SYSTEM SHALL CONTINUE TO replace the Cascade's tree once.

## 11. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** **ICON-11.1** WHEN `cargo xtask perf` runs THE SYSTEM SHALL stay inside the GOAL-11 budget in `docs/standards/performance.md`, with no signal, phase or metric added by this feature — verified by `cargo xtask perf`.
- **Performance:** **ICON-11.2** WHEN this feature is delivered THE SYSTEM SHALL have Navori's CPU during the Loop recorded in this file, measured by hand as the median of 5 windows of 12 seconds on a debug Sample with the panel closed, against the static Icon, using the rule that 0.05% rounds to 0.1 and fails. Delivery needs the number recorded, not a passing number. A result above 0.0 after rounding reopens the Loop (see Comes back if).
  - **Loop cost: not yet measured.** Owed by the user; it needs a live GUI. On a debug build with the AI Sample on and the panel closed, and again with the static normal Sample, run `/usr/bin/top -l 6 -s 12 -stats cpu -pid <Navori Dev pid>`, drop the first sample (cumulative), and take the median of the other 5 windows. Do the same for WindowServer (pid from `pgrep -x WindowServer`). Record both below with the machine and its load. If Navori rounds above 0.0, the Loop reopens.

    | Case | Navori CPU % | WindowServer CPU % | Machine and load |
    |---|---|---|---|
    | AI Sample, panel closed | Not yet measured | Not yet measured | Not yet measured |
    | Normal Sample (static) | Not yet measured | Not yet measured | Not yet measured |
- **Performance:** **ICON-11.3** WHEN that measurement is taken THE SYSTEM SHALL have WindowServer's CPU recorded beside it, with no budget, and no wake-up count offered as evidence, because the counter read 0 for a 10 fps timer in the spike — verified by reading the recorded numbers.
- **Performance:** **ICON-11.4** WHILE the menu is closed THE SYSTEM SHALL fire no timer more often than every 30 seconds for the Icon (ARCH-16) — verified by searching the Icon's drawing code for `Timer`, `DispatchSourceTimer` and `CADisplayLink`. This is not recorded as met until the Loop's drawing method is chosen.
- **Security:** **ICON-11.5** THE SYSTEM SHALL NOT put the raw text of a bad config value, or a TOML source snippet, in `tray`, which crosses the local IPC socket (TB-2) — verified by a test in `crates/ffi/tests/config_privacy.rs` reading `tray` with a bad value in `config.toml`.
- **Reliability:** None — no standing target; `docs/ops/reliability.md` does not exist. The failure paths are in stories 2, 4 and 7.
- **Accessibility:** **ICON-11.6** THE SYSTEM SHALL carry every glyph and attention in shape, so that each stays readable with motion off and without color — verified by a manual walk of every Sample with Reduce Motion on. No `docs/standards/accessibility.md` exists; VoiceOver and Reduce Motion are in stories 6 and 8.

## 12. See the new logo

**Story:** As the user, I want to install Navori and see the folded N as its app icon in Finder and the Dock, so that the app looks like what the menu bar shows.

- **ICON-12.1** THE SYSTEM SHALL draw the release app icon from the logo artwork in `./ui-brief.md`, at every size in the iconset.
- **ICON-12.2** (guard) WHEN the dev bundle is built THE SYSTEM SHALL CONTINUE TO give it an app icon visibly different from the release icon, so that the two installs can be told apart.
- **ICON-12.3** (guard) WHEN `cargo xtask icon` runs THE SYSTEM SHALL CONTINUE TO regenerate both `.icns` files from `make-icon.swift`.

## Out of Scope

- Building the sources of a Decision or of AI running. ROAD-13 and ROAD-14 write their own `tray()`. This feature wires only Awake's.
- Wiring any source of the Loop.
- Color on the Icon, a second attention level, or kinds of attention.
- To-dos, Updates, pull request counts, failing checks and port counts on the Icon.
- A third field for motion in the icon state.
- A demo Module, a cargo feature, or a fake Action.
- A new signal, phase or metric in `cargo xtask perf`, and any fake icon state under `NAVORI_PERF=1`.
- A line for the icon state in the plain output of `navori status`.
- `context()`. It stays in MILE-3.
- The `cup.and.saucer` icon on the Awake row in the menu, which AWAKE shipped. It stays.

## Not locked

Recorded so that nobody reads these as requirements.

- `Inner::status()` calling the composer and passing the notice in: design confirms the exact call.
- How `tray()` sits beside the state closure at `crates/kernel/src/runtime.rs:175`.
- A Loop or Beat must not restart when the Shell reassigns the image, which happens on every snapshot today (`StatusBarController.render`, re-run by its observation).
- A panic inside `tray()` must not take down the status path. How is for design.
- The status block's text in AWAKE's design as the sentence for ICON-4.3.
- If the config sentence is shortened later, cut only the fix-it advice, and re-check KCONTRACT-2.4 and KCONTRACT-2.5.
- A plain-output line for `summary`, later, if it is read by eye often.

## Must measure

These are not known facts. A spike runs at the start of design, on a real status item.

- Whether the attention mark can be read in the status item's cell, and which shape.
- Whether the Beat can be seen inside the status button. An old spike saw no frame change from a pulse symbol effect.
- Whether the mark keeps the menu bar's tint in light and dark menu bars.
- How a long summary sounds in VoiceOver when many keys are bad and the Journal sweep is paused.
- The Loop's real cost. Core Animation's 0.00% in `.skills/ICON/knowns.md` comes from an old spike and is an estimate, not a gate number.

## Comes back if

These conditions reopen a decision. They are not requirements.

- ROAD-14 wires a Loop source: it needs a measurement on Release code first, and that measurement cannot be a fake icon state under `NAVORI_PERF=1`. How it measures is ROAD-14's to choose.
- The hand measurement of the Loop rounds above 0.0: review the Loop itself.
- The spike or dogfood shows a second mark cannot be read in the status item's cell, or the Icon should stay one image with both questions answered only in the menu: the shape of the icon state.
- The spike shows the Beat cannot be seen in the status button: the Beat.
- Dogfood shows one Decision mark is too little, and two shapes can be read in the cell: the attention levels.
- After ROAD-14, the user looks at the Loop more than at the awake glyph: move AI running above Lease.
- macOS refusal never occurs in dogfood: the problem branch in `Awake::tray()`.
- Another Module wants the problem glyph for a failure outside its main function: ICON-3.4.

## Open Questions

- The dev app icon's look: `design-solution` proposes, the user approves — Jayden Đặng — 2026-11-03 — forbid-guess.
- The wording of the Lease, AI running, normal and Decision sentences, and of any Preview control: `design-solution` proposes, the user approves — Jayden Đặng — 2026-11-02 — forbid-guess.
- The type names, and the glyph names in the JSON: `design-solution` proposes, the user approves — Jayden Đặng — 2026-11-02 — forbid-guess.
- The `tray()` signature, including whether a Module's signal can be expressed from its `State` alone: `design-solution` proposes after reading the merged code — Jayden Đặng — 2026-11-02 — forbid-guess.
- How the Core row reads the result while keeping ICON-10.6 and ICON-10.7: `design-solution` proposes — Jayden Đặng — 2026-11-02 — forbid-guess.
