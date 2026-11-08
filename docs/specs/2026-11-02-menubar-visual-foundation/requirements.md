# Requirements: Menubar visual foundation

Feature code: MENU
Status: Implemented
Date: 2026-11-02

<!--
Rules:
- Every acceptance criterion gets a hierarchical ID: MENU-<story>.<criterion>.
- Criteria use EARS phrasing (WHEN / WHILE / IF…THEN / WHERE / ubiquitous).
- Guard requirements protect existing behavior: WHEN <condition> THE SYSTEM SHALL CONTINUE TO <behavior>.
- IDs are immutable once Status is Approved. Retire by strikethrough — never renumber.
-->

Roadmap item: ROAD-34 (MILE-2) · Tracker: #6 · Decisions: frame-change close package (2026-11-02), confirmed by the user · UI: `docs/design/2026-11-01-menubar-ui-brief.md`, Amendments 10–12.

Terms used below:
- **The system** — the Shell in `apps/macos`, plus `xtask` for stories 8 to 10.
- **Level 0** — the first panel, shown under the status item.
- **Submenu** — a panel opened from a row of another panel.
- **Cascade** — level 0 and every open Submenu, at most 3 levels.
- **Core row** — the level 0 row that names the state of the Core and opens a Submenu.
- **Hover delay** — the short wait before a row under a resting pointer opens its Submenu. `design.md` names the value.
- **Safe diagonal** — the area between the pointer and the open Submenu, inside which moving across other rows does not switch Submenu.
- **Preview** — a surface that exists only in the dev build and shows sample data.
- **Release code** — Swift compiled in the release configuration, which `cargo xtask install` and `cargo xtask perf` both use.

## 1. Open the new glass menu

**Story:** As the user, I want to click the status item and see the new glass menu holding only what Navori really knows, so that the menu is pleasant to read and never shows anything invented.

- **MENU-1.1** WHEN level 0 opens THE SYSTEM SHALL show it 360 pt wide on an `NSGlassEffectView` with style `.clear` and a tint.
- **MENU-1.2** THE SYSTEM SHALL lay out level 0 in this order: any banners, the Awake row, the Core row, a separator, `Settings…`, `About Navori`, `Quit Navori`. (amended by AWAKE-11.1) (amended by NEEDS-11.1) *(Amended 2026-11-05, operator decision: the Core row is present only as MENU-3.11 says; where it is absent the order is otherwise unchanged and the separator stays before `Settings…`.)*
- **MENU-1.3** THE SYSTEM SHALL NOT show a header row, the app name caption or the version on level 0.
- **MENU-1.4** WHERE the code is Release code THE SYSTEM SHALL NOT show the Ask field, Needs you, the Ports or Pull Requests rows, or `You're all caught up`. (amended by AWAKE-11.1) (amended by NEEDS-3.9)
- **MENU-1.5** THE SYSTEM SHALL draw rows with SF Pro 14 pt titles and 13 pt right-aligned values, in `labelColor` and `secondaryLabelColor`, with no `tertiaryLabelColor` on glass.
- **MENU-1.6** WHEN the pointer or the keyboard selects a row THE SYSTEM SHALL fill it with the accent background at 8 pt corners and draw its text and icon in white.
- **MENU-1.7** WHEN the user activates `Settings…` THE SYSTEM SHALL close the Cascade and open the Settings window.
- **MENU-1.8** WHEN the user activates `About Navori` THE SYSTEM SHALL close the Cascade and open the Settings window on its About tab.
- **MENU-1.9** WHEN the user activates `Quit Navori` THE SYSTEM SHALL terminate the app.
- **MENU-1.10** WHILE level 0 is the key window THE SYSTEM SHALL answer `⌘,` as `Settings…` and `⌘Q` as `Quit Navori`.

## 2. See what is wrong

**Story:** As the user, I want a problem with the Core to appear at the top of the menu in plain words, so that I never mistake a broken Navori for a healthy one.

- **MENU-2.1** (guard) WHEN the Core did not start THE SYSTEM SHALL CONTINUE TO show a banner holding the real startup error, in the text `CoreStartupBanner` formats.
- **MENU-2.2** (guard) WHEN the last run ended uncleanly THE SYSTEM SHALL CONTINUE TO show a banner saying so with the reported detail.
- **MENU-2.3** (guard) WHEN the Core reports a config problem THE SYSTEM SHALL CONTINUE TO show a banner with the Core's report as its message.
- **MENU-2.4** WHILE any banner shows THE SYSTEM SHALL keep its text readable on glass per MENU-11.4, wrapping at most 4 lines.
- **MENU-2.5** IF the Core is not running THEN THE SYSTEM SHALL offer no restart button on the banner or the Core row.

## 3. Read the state of the Core

**Story:** As the user, I want to open the Core row and read each module's state and the recent activity, so that I can tell what Navori is doing without opening a terminal.

- **MENU-3.1** THE SYSTEM SHALL draw the Core row on one line: icon, `Core`, the state in words, and a chevron when it has a Submenu. *(Amended 2026-11-05, operator decision: this is how the row is drawn when it is shown; whether it is shown is MENU-3.11.)*
- **MENU-3.2** THE SYSTEM SHALL name the state as `Starting`, `Running`, `Needs attention` or `Not running`, following the order the Core row's initializer in `NavoriPanel` applies (`CoreStore.health` was deleted by ICON, `docs/specs/2026-11-02-status-icon-states/design.md` decision 5), with a green dot for `Running`, an orange dot for `Needs attention`, a red dot for `Not running` and no dot for `Starting`.
- **MENU-3.3** WHEN the Core row's Submenu opens THE SYSTEM SHALL list every module in the snapshot with its state, its restart count when above 0, and its detail when it has one.
- **MENU-3.4** WHEN the Core row's Submenu opens THE SYSTEM SHALL list at most 8 recent Events in the snapshot's order, each with its summary, a severity icon and its relative age in today's narrow numeric form.
- **MENU-3.5** IF the snapshot holds no Events THEN THE SYSTEM SHALL show `Nothing yet.` in place of the Event list.
- **MENU-3.6** WHILE the Submenu is open WHEN a new snapshot arrives THE SYSTEM SHALL update its rows without closing it.
- **MENU-3.7** IF the Core has no snapshot THEN THE SYSTEM SHALL show the Core row with no chevron and no Submenu. *(Amended 2026-11-05, operator decision: where the row is shown. In production a Core with no snapshot that has not failed is `Starting` and the row is hidden by MENU-3.11.)*
- **MENU-3.8** WHERE the app runs the dev profile THE SYSTEM SHALL show a `DEV` chip on the Core row. *(Amended 2026-11-05, operator decision: the dev profile shows the Core row in every state, see MENU-3.12; the chip is unchanged.)*
- **MENU-3.9** WHERE the app runs the release profile THE SYSTEM SHALL NOT show the `DEV` chip.
- **MENU-3.10** (guard) WHEN a module row is drawn THE SYSTEM SHALL CONTINUE TO color its dot green for `running`, yellow for `starting` or `restarting`, red for `failed` and gray otherwise.
- **MENU-3.11** WHERE the app runs the release profile THE SYSTEM SHALL show the Core row, and with it the Core Submenu, only while the Core is `Needs attention` or `Not running`, and SHALL NOT show it while the Core is `Running` or `Starting`. WHEN the Core's state changes while the Cascade is open THE SYSTEM SHALL add or remove the row without closing the Cascade; WHEN the row disappears (the Core recovers) THE SYSTEM SHALL close an open Core Submenu and move the selection off the row, and SHALL keep the selection on any surviving row. *(Added 2026-11-05, operator decision: the row was a placeholder until features had data (brief item 12); the Running row now exists, and a healthy Core row is noise in normal use. Cost, recorded on purpose: the Core Submenu's Events list is the only in-app place that shows the counts-only `ports.stopped` Events, so in production those Events are now visible in the app only while the Core needs attention; `navori events` and Settings → Doctor still show them. `cargo xtask perf` runs the dev profile, where the row stays, so `submenu_open_ms` (MENU-8.1) still measures a surface that production hides while the Core is healthy.)*
- **MENU-3.12** (guard) WHERE the app runs the dev profile THE SYSTEM SHALL CONTINUE TO show the Core row in every state, with the `DEV` chip. *(Added 2026-11-05, operator decision.)*
- **MENU-3.13** (guard) WHEN the Core row is shown THE SYSTEM SHALL CONTINUE TO list the same module states and the same 8 most recent Events in its Submenu (MENU-3.3, MENU-3.4), keep its chevron rules (MENU-3.7) and dot colors (MENU-3.2, MENU-3.10), and WHEN the Core row is hidden THE SYSTEM SHALL CONTINUE TO draw the banners and the status icon from the same Core state as before. *(Added 2026-11-05, operator decision.)*

## 4. Move through submenus with the pointer

**Story:** As the user, I want to move the pointer from a row into its Submenu the way I do in a macOS menu, so that the Cascade never closes under me on the way there.

- **MENU-4.1** WHEN the pointer rests on a row that has a Submenu for the Hover delay THE SYSTEM SHALL open that Submenu.
- **MENU-4.2** WHEN the user clicks a row that has a Submenu THE SYSTEM SHALL open it at once.
- **MENU-4.3** WHILE the pointer moves inside the Safe diagonal THE SYSTEM SHALL keep the open Submenu and not select the rows it crosses.
- **MENU-4.4** WHEN the pointer stops on another row of the parent panel or leaves the Safe diagonal THE SYSTEM SHALL select that row and close the Submenu it left.
- **MENU-4.5** WHEN a Submenu opens THE SYSTEM SHALL place it beside its parent row on the right.
- **MENU-4.6** IF the Submenu would cross the right edge of the screen's visible frame THEN THE SYSTEM SHALL place it on the left of its parent.
- **MENU-4.7** IF a panel is taller than the screen's visible frame allows THEN THE SYSTEM SHALL keep it inside the visible frame and scroll its content.
- **MENU-4.8** THE SYSTEM SHALL allow at most 3 levels in a Cascade.
- **MENU-4.9** WHEN the user clicks inside any panel of the Cascade THE SYSTEM SHALL keep the Cascade open unless the click activated a row that closes it.
- **MENU-4.10** WHEN the user clicks outside the Cascade and outside the status button THE SYSTEM SHALL close the whole Cascade.
- **MENU-4.11** WHEN the user clicks the status button while the Cascade is open THE SYSTEM SHALL close the whole Cascade and not reopen it.
- **MENU-4.12** WHEN key status moves between panels of the Cascade THE SYSTEM SHALL keep the Cascade open.
- **MENU-4.13** WHEN key status leaves the Cascade for anything else THE SYSTEM SHALL close the whole Cascade.
- **MENU-4.14** WHEN the Cascade opens THE SYSTEM SHALL call `CoreStore.setPanelVisible(true)` exactly once, and opening or closing a Submenu inside it SHALL NOT call it.
- **MENU-4.15** WHEN the whole Cascade closes THE SYSTEM SHALL call `CoreStore.setPanelVisible(false)` exactly once, and closing a Cascade that is not open SHALL NOT call it.
- **MENU-4.16** WHEN the pointer is on a row of the deepest open panel THE SYSTEM SHALL select that row at once, whatever direction the pointer is moving. *(Added 2026-11-04: the Safe diagonal also held rows of the Submenu it leads to, so the highlight lagged behind a pointer moving away from the parent.)*
- **MENU-4.17** WHEN the pointer turns away from the open Submenu THE SYSTEM SHALL select the parent row under it within 16 pt of travel, without waiting for the pointer to stop. *(Added 2026-11-04: the Safe diagonal measured from a stale point, so a wide triangle covered rows the pointer was not crossing on its way to the Submenu.)*
- **MENU-4.18** (guard) WHEN the pointer travels to the open Submenu along an uneven path, to any of its rows THE SYSTEM SHALL CONTINUE TO keep the Submenu open and the parent row selected.
- **MENU-4.19** WHEN a Submenu opens THE SYSTEM SHALL place it beside its parent row where that row is now, wherever the Cascade last opened. *(Added 2026-11-04: row positions were kept in screen coordinates and went stale when the panel opened on another display.)*
- **MENU-4.20** (guard) WHEN the Cascade opens on any display THE SYSTEM SHALL CONTINUE TO anchor level 0 under the status button of that display and to keep Submenus inside that display's visible frame, as in MENU-4.5 to MENU-4.7.

## 5. Drive the menu from the keyboard

**Story:** As the user, I want to open the Core Submenu and activate `Settings…` with the arrow keys and Return, so that I never need the pointer.

- **MENU-5.1** WHEN the user presses ↓ or ↑ THE SYSTEM SHALL move the selection to the next or previous row with an action in the deepest open panel, wrapping at the ends.
- **MENU-5.2** WHEN the user presses → on a row that has a Submenu THE SYSTEM SHALL open it and move the keyboard into it.
- **MENU-5.3** WHEN the user presses ← in a Submenu THE SYSTEM SHALL close it and return the selection to its parent row.
- **MENU-5.4** WHEN the user presses Return on a selected row THE SYSTEM SHALL do what clicking that row does.
- **MENU-5.5** WHEN the user presses Esc THE SYSTEM SHALL close the deepest open panel.
- **MENU-5.6** WHEN the pointer moves onto a row after keyboard use THE SYSTEM SHALL move the one selection to that row.

## 6. Use the menu with VoiceOver and the system display settings

**Story:** As a user relying on VoiceOver, Reduce Transparency or Reduce Motion, I want the menu to honor them, so that I can read and move through it the same way.

- **MENU-6.1** THE SYSTEM SHALL have VoiceOver read each row as one element holding its title, its value and, when it has one, the fact that it opens a Submenu.
- **MENU-6.2** THE SYSTEM SHALL convey every state in words as well as by a dot.
- **MENU-6.3** WHEN Reduce Transparency is on THE SYSTEM SHALL draw every panel of the Cascade on an opaque surface.
- **MENU-6.4** WHEN Reduce Motion is on THE SYSTEM SHALL open and close panels of the Cascade with no animation.

## 7. Judge the design in a dev preview

**Story:** As the developer, I want to open a preview in the dev build with sample data and try tints live, so that I can pick the tint on a full menu before any feature ships.

- **MENU-7.1** WHERE the code is compiled in the debug configuration THE SYSTEM SHALL offer an item in the right-click menu that opens the Preview.
- **MENU-7.2** WHEN the Preview opens THE SYSTEM SHALL show the brief's Ask field, Needs you and the Awake, Ports and Pull Requests rows filled with sample data.
- **MENU-7.3** THE SYSTEM SHALL let the Preview's rows open a real Cascade 3 levels deep.
- **MENU-7.4** WHEN the user changes the tint control in the Preview THE SYSTEM SHALL apply that tint to the open panels and show its value.
- **MENU-7.5** WHILE the Preview is open THE SYSTEM SHALL NOT dispatch any Action from a sample row.
- **MENU-7.6** THE SYSTEM SHALL compile the Preview and its sample data out of Release code, verified by searching the release binary for a marker string from the sample data.

## 8. Measure opening a Submenu

**Story:** As the developer, I want `cargo xtask perf` to fail when the Core Submenu opens too slowly, so that the Cascade stays fast before features build on it.

- **MENU-8.1** WHEN `cargo xtask perf` runs THE SYSTEM SHALL measure `submenu_open_ms` from the trigger to the first presented frame of the Core row's Submenu, opened with no pointer through `SIGUSR2`.
- **MENU-8.2** IF `submenu_open_ms` exceeds 16 ms THEN THE SYSTEM SHALL fail the run.
- **MENU-8.3** THE SYSTEM SHALL NOT report `submenu_open_ms` as skipped in `cargo xtask perf`.
- **MENU-8.4** WHILE the app is not in perf mode THE SYSTEM SHALL install no `SIGUSR2` handler, the same as `SIGUSR1` today.

## 9. Keep the Cascade logic under test

**Story:** As the developer, I want a broken edge flip or key transition to turn `cargo xtask ci` red, so that a later feature cannot break the Cascade unnoticed.

- **MENU-9.1** THE SYSTEM SHALL hold the Cascade's placement geometry and its state transitions in the `NavoriPanel` target, which imports no AppKit, and test them there.
- **MENU-9.2** WHEN `cargo xtask ci` runs THE SYSTEM SHALL run `swift test` on the macOS package.
- **MENU-9.3** IF a Swift test fails THEN THE SYSTEM SHALL fail `cargo xtask ci`.

## 10. Require macOS 26

**Story:** As the user, I want the app and its docs to state one minimum macOS, 26, so that there is no fallback look that never runs.

- **MENU-10.1** THE SYSTEM SHALL build the Swift package and the Rust static library with a macOS 26 deployment target.
- **MENU-10.2** WHEN an app bundle is built THE SYSTEM SHALL write `LSMinimumSystemVersion` as `26.0`.
- **MENU-10.3** THE SYSTEM SHALL have `docs/product/vision.md`, `docs/architecture/runtime.md`, `docs/standards/INDEX.md`, `docs/ops/deployment.md` and `README.md` state macOS 26 as the minimum. The vision edit goes through `define-project`.
- **MENU-10.4** THE SYSTEM SHALL call the glass API with no availability branch and no `NSVisualEffectView` fallback for older systems.

## 11. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** **MENU-11.1** WHEN `cargo xtask perf` runs on the finished branch THE SYSTEM SHALL meet every GOAL-11 budget in `docs/standards/performance.md`, including first menu open ≤ 80 ms, later opens ≤ 50 ms, submenu open ≤ 16 ms, footprint ≤ 60 MB with the menu open and ≤ 30 MB idle, idle CPU 0% and ≤ 1 wake-up per second — verified by that run.
- **Performance:** **MENU-11.2** WHILE the Cascade is closed THE SYSTEM SHALL run no timer of its own (ARCH-16) — verified by inspection of the design and the idle metrics of `cargo xtask perf`.
- **Security:** None — no new trust boundary, Action or stored data. Rows show snapshot fields the Shell already shows, and `docs/security/threat-model.md` needs no new entry.
- **Reliability:** None — no standing target; the Shell keeps no state of its own. `docs/ops/reliability.md` is absent. Failure visibility is story 2.
- **Accessibility:** **MENU-11.3** THE SYSTEM SHALL meet stories 5 and 6 — verified by a manual pass with VoiceOver, Reduce Transparency and Reduce Motion on. `docs/standards/accessibility.md` is absent, so the targets come from the brief.
- **Accessibility:** **MENU-11.4** WHEN the chosen tint is on a white background in dark appearance, and on a black background in light appearance, THE SYSTEM SHALL give primary text at least 4.5:1 and secondary text at least 3:1 — verified by measuring screenshots from the Preview.

## Guarding what exists

Every file this change touches, with the existing behavior at risk.

| File | Existing behavior | Guarded by |
|---|---|---|
| `apps/macos/Navori/StatusBar/StatusPanel.swift` | Anchors under the button, clamps to the screen, fallback when the button is off screen; outside click, second click, Esc and loss of key close it; first-frame callback; non-activating, above other windows, on every Space; reports visibility. Today `close()` reports `false` even when already closed, which MENU-4.15 changes on purpose | MENU-G1, MENU-G2, MENU-G3, MENU-G5, MENU-G6, MENU-G14, MENU-4.14, MENU-4.15 |
| `apps/macos/Navori/StatusBar/StatusBarController.swift` | Left click toggles; right click shows the native menu; `openMenuFresh` closes with no animation and starts the clock after closing; dims the icon while starting; tooltip; `statusItemDrawn` mark | MENU-G3, ~~MENU-G4~~ (replaced by ICON), MENU-G5, MENU-G7, MENU-G11, MENU-G12 |
| `apps/macos/Navori/Features/Panel/PanelView.swift` | Banners, module and activity lists, footer. Its header is removed on purpose (MENU-1.3); KCONTRACT changes its config banner first | MENU-2.1, MENU-2.2, MENU-2.3, MENU-3.3, MENU-3.4, MENU-3.5 |
| `apps/macos/Navori/Features/Settings/SettingsView.swift`, `SettingsWindowController.swift` | General, Diagnostics and About tabs. Opening on a chosen tab is new (MENU-1.8) | MENU-G8 |
| `apps/macos/Navori/Core/CoreStore.swift` | ~~`health` order~~ (deleted by ICON; the Core row reads facts through a `NavoriPanel` initializer), `setPanelVisible`, `moduleCount`; KCONTRACT edits these members, so their names stay | MENU-3.2, MENU-G5, MENU-G6 |
| `apps/macos/Navori/Platform/PerfProbe.swift` | `SIGUSR1` opens the menu in perf mode; `SIGTERM` handling; marks only in perf mode | MENU-G5, MENU-G12, MENU-G13, MENU-8.4 |
| `apps/macos/NavoriPanel/StatusPanelClickPolicy.swift` and its test | A click on the status button does not dismiss; a click elsewhere does | MENU-G3 |
| `apps/macos/Package.swift` | Targets and dependencies stay; tools version and platform change on purpose (MENU-10.1) | MENU-G9 |
| `xtask/src/main.rs` | `ci` steps and their order, with `swift test` added; bundle Info.plist other than the minimum version | MENU-G9 |
| `xtask/src/perf/judge.rs`, `baseline.rs`, `logstream.rs`, `mod.rs` | Every other metric's judgement; `menuOpened` parsing; `submenu_open_ms` stops being skipped on purpose (MENU-8.3), so the tests that assert the skip text change with it | MENU-G5, MENU-G10 |
| `.cargo/config.toml` | Build settings other than the deployment target | MENU-G9 |
| `docs/standards/performance.md`, `docs/standards/INDEX.md`, `docs/ops/deployment.md`, `README.md`, `docs/product/vision.md`, `docs/architecture/runtime.md`, `docs/design/2026-11-01-menubar-ui-brief.md` | Wording only | no behavior to guard |
| New Cascade, row and Preview Swift files, new `NavoriPanel` files | None yet | no behavior to guard |

Guards named above:

- **MENU-G1** (guard) WHEN level 0 opens from a click THE SYSTEM SHALL CONTINUE TO anchor it under the status button, clamped 8 pt inside the screen's visible frame.
- **MENU-G2** (guard) IF the status button has no real on-screen frame THEN THE SYSTEM SHALL CONTINUE TO place level 0 at the top of the main screen's visible frame.
- **MENU-G3** (guard) WHEN the user clicks the status button with the panel open THE SYSTEM SHALL CONTINUE TO have the toggle handle that click, not the outside-click monitor.
- ~~**MENU-G4** (guard) WHEN `store.health` changes THE SYSTEM SHALL CONTINUE TO update the status icon with the same four symbols.~~ Replaced by ICON (`docs/specs/2026-11-02-status-icon-states/`): the icon now follows the Core's composed `tray`, drawn as the fold glyphs, not four SF Symbols.
- **MENU-G5** (guard) WHEN `cargo xtask perf` sends `SIGUSR1` THE SYSTEM SHALL CONTINUE TO close any open menu with no animation, start the clock after that close, and log `menuOpened ms=.. first=.. modules=..` in today's format at the first presented frame, with `modules` from `CoreStore.moduleCount`.
- **MENU-G6** (guard) WHEN the Shell reports menu visibility THE SYSTEM SHALL CONTINUE TO go through `CoreStore.setPanelVisible`.
- **MENU-G7** (guard) WHEN the user right-clicks the status item THE SYSTEM SHALL CONTINUE TO show the native `NSMenu` with `Open`, `Settings…`, `Open Logs Folder` and `Quit`.
- **MENU-G8** (guard) WHEN the Settings window opens THE SYSTEM SHALL CONTINUE TO offer the General, Diagnostics and About tabs with their current content.
- **MENU-G9** (guard) WHEN `cargo xtask ci` runs THE SYSTEM SHALL CONTINUE TO run fmt, clippy, `cargo test --workspace`, `check-deps`, `lint-probe`, the Core build and `swift build`, in that order.
- **MENU-G10** (guard) WHEN `cargo xtask perf` judges a metric other than `submenu_open_ms` THE SYSTEM SHALL CONTINUE TO use today's budgets and floors.
- **MENU-G11** (guard) WHILE the Core is starting THE SYSTEM SHALL CONTINUE TO dim the status icon, and the status button SHALL CONTINUE TO carry the app name as its tooltip.
- **MENU-G12** (guard) WHEN the status item first draws in perf mode THE SYSTEM SHALL CONTINUE TO log `statusItemDrawn`.
- **MENU-G13** (guard) WHEN perf mode receives `SIGTERM` THE SYSTEM SHALL CONTINUE TO handle it as it does today.
- **MENU-G14** (guard) WHEN level 0 opens THE SYSTEM SHALL CONTINUE TO open without activating the app, above other windows, on every Space and over full-screen apps.

## Out of Scope

- The Ask field, Needs you and the Awake, Ports and Pull Requests rows in Release code. Their features ship them: ROAD-15, ROAD-13, ROAD-8, ROAD-18, ROAD-20.
- Any change to `CoreSnapshot` or Rust Core code. The `configNotice` field arrives from KCONTRACT. The deployment target in `.cargo/config.toml` is a build setting, not a Core change.
- `NSMenu` for level 0. The right-click menu stays `NSMenu` and keeps its existing `performClick`.
- A restart button on the banner or the Core row. No Action exists for it.
- Status icon states and animation: ROAD-35.
- Type-select, press-and-drag selection, and menu scroll arrows.
- Full Event history in Settings. `navori events` already serves it.
- A separate About window.
- Swift tests of AppKit views. `docs/standards/testing.md` exempts the view layer.
- Fallback support for macOS below 26.

## Comes back if

These conditions reopen a decision. They are not requirements.

- ~~The Awake row ships: the Core row stays.~~ Superseded 2026-11-05 by the operator's decision (MENU-3.11): in production the Core row shows only while the Core needs attention or is down; the dev profile keeps it.
- The Cascade and its gate turn out larger than the visual work in design: split the Cascade into its own ROAD.
- The real Cascade exceeds 16 ms after optimizing: change the budget through `define-project`, or reconsider.
- The Safe diagonal dominates the Cascade's complexity for a narrow menu: drop it.
- A Mac below macOS 26 needs Navori: the minimum version.
