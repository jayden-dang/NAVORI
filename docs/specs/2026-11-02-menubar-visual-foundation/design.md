# Design: Menubar visual foundation

Feature code: MENU
Status: Approved
Date: 2026-11-02
Requirements: ./requirements.md

## Context

The menu today is one borderless `StatusPanel` (`NSPanel`) on an `NSVisualEffectView`, hosting one SwiftUI `PanelView` with a header, banners, a module list, an activity list and a footer (`apps/macos/Navori/StatusBar/StatusPanel.swift`, `Features/Panel/PanelView.swift`). It has no submenu and handles only Esc on the keyboard. `CoreStore` is the Shell's one state holder; the views also import the generated `NavoriCore` types directly, which ARCH-2 forbids.

The binding constraint is GOAL-11 together with truthfulness. The spike put `NSMenu` over budget (131 MB peak with a text field) and `cargo xtask perf` must stay green, so the Cascade is hand-built on panels (ADR 0006). Nothing may be invented in Release code, and only Core state exists today, so level 0 is small and the Cascade's first real user is the Core row. That rules out two alternatives: `NSMenu` with custom item views, and a level 0 that shows the brief's full layout with placeholder rows.

A hand-built Cascade fails quietly: an edge flip or a key transition can break with no crash, and the view layer has no unit tests (`docs/standards/testing.md:24`). So placement, the diagonal and the state transitions live as pure code in the existing `NavoriPanel` target, following `StatusPanelClickPolicy`, and everything AppKit stays thin around them.

KCONTRACT merges first and edits the config banner, `CoreStore.health` and `AppDelegate`. Nothing here renames what it edits, and moving the config banner behind one `CoreStore` property keeps the rebase to one place.

Fresh retrieval (`fsubr-1.2`, seeds: MENU, the requirement terms and the paths below; `owns_coverage` 3 of 8 registered, so this is partial): KCONTRACT `term` (`panel`, `banner`, `settings`), PERF `term` (`panel`, `submenu`, `banner`), PROCS `term` (`settings`, `submenu`), no `observations`. CORE, SHELL, CLI and BUILD have no Files blocks, so this list does not prove nothing else owns these paths; the reader searches under each `Surface:` below do that work.

## Decisions

1. The menu is a Cascade of `NSPanel`s (ADR 0006).
2. Only level 0 ever becomes key. Submenu panels return `canBecomeKey == false`, so keyboard input always arrives at level 0 and no Submenu can make level 0 resign. A later feature with a text field in a Submenu (ROAD-13's `Add a to-do…`) changes this for its own panel.
3. All Cascade decisions are made by a pure `CascadeState`: events in, effects out, no clock. A one-shot timer in the Shell turns the Hover delay into an event (ARCH-16).
4. The Hover delay starts at 150 ms. It is one constant, tuned by use.
5. The default tint is black at 65% in dark appearance and white at 55% in light, as one dynamic `NSColor`. MENU-11.4 is the floor when it is tuned.
6. `StatusPanel` stays the level 0 class and takes what it shows as parameters. The Preview reuses it with sample content, so it exercises the real Cascade rather than a copy.
7. Panel views stop importing `NavoriCore`. `CoreStore` publishes plain Shell structs for modules, Events and banners. `SettingsView.swift` keeps its import; that is existing debt outside this feature.
8. `submenu_open_ms` is the median across the 5 processes of 5 opens each, the same shape as `menu_reopen_ms`, with a regression floor of 5 ms like the other menu metrics. The first open of a process counts as one of those 5.
9. The Preview and its sample data sit in `#if DEBUG`, not behind `AppEnvironment.isDev`, because `cargo xtask perf` runs Release code under the dev identity.
10. The minimum macOS rises to 26 in one change, in every place that pins it, with no availability branch.

## Architecture

Vocabulary for every section below: **module**, **interface**, **implementation**, **seam** (public surface where behavior is observable and substitutable). Prefer deep modules — an interface much simpler than what it hides.

### Cascade logic

Satisfies: MENU-4.1, MENU-4.2, MENU-4.3, MENU-4.4, MENU-4.5, MENU-4.6, MENU-4.8, MENU-5.1, MENU-5.2, MENU-5.3, MENU-5.4, MENU-5.5, MENU-5.6, MENU-9.1
Reuse: rung 7 — new code in the existing `NavoriPanel` target (rung 2 for the target and the pure-policy pattern of `StatusPanelClickPolicy`); AppKit's `NSMenu` is what ADR 0006 declined.
Respects: ARCH-1, ARCH-16
Interface:
```swift
public struct CascadeNode { id: String; isActionable: Bool; children: [CascadeNode] }
public struct CascadeState {
    public init(root: [CascadeNode], maxDepth: Int = 3)
    public var openPath: [String] { get }           // row ids whose Submenu is open
    public var selection: String? { get }           // the one selected row
    public mutating func handle(_ event: CascadeEvent) -> [CascadeEffect]
}
public enum CascadeKey { up, down, left, right, `return`, escape }
public enum CascadeEvent { pointerOn(row: String, at: CGPoint, submenu: CGRect?), pointerLeft(row: String),
                           click(row: String), hoverTimerFired, key(CascadeKey), replaceTree([CascadeNode]) }
public enum CascadeEffect { open(row: String), close(depth: Int), armHoverTimer, cancelHoverTimer,
                            activate(row: String), closeAll }
public enum CascadePlacement { static func frame(parent: CGRect, row: CGRect, size: CGSize, visible: CGRect) -> CGRect }
public enum SafeDiagonal { static func contains(_ point: CGPoint, from anchor: CGPoint, toward submenu: CGRect) -> Bool }
public enum StatusPanelClickPolicy { static func shouldDismissPanel(clickScreenPoint:, statusButtonScreenFrame:, cascadeFrames: [CGRect]) -> Bool }
```
Depth: If this module vanished, callers would have to rebuild the selection, path, wrap, depth, diagonal and placement rules themselves; with it they know only the events, the effects and two geometry functions.
Surface:
- `StatusPanelClickPolicy.shouldDismissPanel` — `StatusPanel.swift:93`, `Tests/NavoriPanelTests/StatusPanelClickPolicyTests.swift` — replace: gains `cascadeFrames`.
Locality: new files `apps/macos/NavoriPanel/CascadeState.swift`, `CascadePlacement.swift`, `SafeDiagonal.swift`; `extend` `StatusPanelClickPolicy`.

Rules `handle` applies:
- A row with children counts as actionable, and non-actionable rows are never selected.
- Every `pointerOn` at a new point records that row as `pendingRow` and re-arms the timer, so a moving pointer never lets it fire. A `pointerOn` at the same point as the last one is ignored, so rows moving under a still pointer cannot take a keyboard selection (MENU-5.6).
- `CascadeState` keeps the diagonal's anchor: the last point outside the diagonal. Outside it, `pointerOn` selects the row at once, closing any Submenu that row does not own, and the timer then opens the row's own Submenu. Inside it, selection and the open Submenu stay.
- `hoverTimerFired` selects `pendingRow`, closes a Submenu that row does not own, and opens its own. So a pointer resting on another row inside the diagonal switches after the Hover delay (MENU-4.4).
- `pointerLeft` clears the selection unless the selected row owns an open Submenu, and cancels the timer.
- `click` on a row with children opens it at once; on a row with an action it emits `activate`. `key(.return)` does the same for the selection.
- `key(.down)` and `key(.up)` wrap within the deepest open panel. `key(.right)` opens and moves in; `key(.left)` closes the deepest. `key(.escape)` closes the deepest, and at level 0 emits `closeAll`.
- A row at `maxDepth` never opens a Submenu.
- `replaceTree` takes the tree as it now is, which may add or remove children. It closes any open path whose row no longer has children and clears a selection that is gone, and closes nothing else.

`CascadePlacement.frame` places the panel right of its parent aligned to the row's top, flips left when it would cross `visible.maxX`, then shifts up and limits its height so it stays inside `visible`.

### Cascade windows

Satisfies: MENU-4.7, MENU-4.9, MENU-4.10, MENU-4.11, MENU-4.12, MENU-4.13, MENU-4.14, MENU-4.15, MENU-6.4, MENU-11.2, MENU-G1, MENU-G2, MENU-G3, MENU-G4, MENU-G6, MENU-G7, MENU-G11, MENU-G14
Reuse: rung 2 — extends `StatusPanel` and `StatusBarController`; rung 7 — `CascadeController`, because no existing piece owns child panels.
Respects: ARCH-16
Surface:
- `StatusPanel.init(store:openSettings:)` — `StatusBarController.swift:12-15` — replace: takes a `CascadeContent` value and `onVisibility: (Bool) -> Void`.
- `StatusPanel.show` — `StatusBarController.swift:88` (`openMenuFresh`), `:114` (`openPanel`), `toggle` at `StatusPanel.swift:58` — replace: same signature with `onFirstFrame`; its body builds the surface and resets the Cascade, and sets `isOpen` first.
- `StatusPanel.close` — `StatusPanel.swift:58,98,120`, `StatusBarController.swift:80,95,109`, `cancelOperation` — replace: see the close order below. `cancelOperation` routes through `key(.escape)`.
- `store.setPanelVisible` at `StatusPanel.swift:84,114` — replace: reached through `onVisibility`, which `StatusBarController` binds to `store.setPanelVisible`. KCONTRACT's `tasks.md:342` diff check runs on its own branch, which merges first, so it is unaffected.
- `CoreStore.setPanelVisible` — `StatusBarController.swift` through `onVisibility` — frozen, built around: its signature and FFI call stay, because KCONTRACT's Scheduler reads them; it now gets one `true` and one `false` per Cascade.
- `StatusPanel.width` and `PanelView.swift:27` — replace: 380 becomes 360. `PanelView.swift:23`'s 440 pt limit and `StatusPanel.maxHeight` — replace: the limit comes from `CascadePlacement`.
Interface:
```swift
struct CascadeContent { tree: () -> [CascadeNode]; root: AnyView;
                        submenu: (String) -> AnyView; activate: (String) -> Void }
StatusPanel(content: CascadeContent, onVisibility: (Bool) -> Void)
@MainActor @Observable final class CascadeController {
    var selection: String? { get }
    func pointer(on row: String, at screenPoint: CGPoint)
    func click(_ row: String)
    func pointerLeft(_ row: String)
    func key(_ key: CascadeKey)
    func setRowFrame(_ row: String, _ frameInPanel: CGRect)
    func reset()
}
```
Depth: If `CascadeController` vanished, `StatusPanel` and the rows would have to create, place, reuse and close panels and own the timer; with it they only report pointer, click, key and row frames.
Locality: new `StatusBar/CascadeContent.swift`, `StatusBar/CascadeController.swift`, `StatusBar/CascadePanel.swift`; `extend` `StatusPanel.swift`, `StatusBarController.swift`.

Flow: a row reports the pointer with `.onContinuousHover`, using `NSEvent.mouseLocation` for the point, and its frame with `.onGeometryChange` on `.frame(in: .global)`, which `CascadeController` converts to screen coordinates through the row's panel. A row's `Button` action is its only click report, so one click cannot activate twice. `CascadeController` calls `CascadeState.handle` and runs the effects: `armHoverTimer` is one `DispatchWorkItem` on the main queue, replaced each time, and it does nothing when `isOpen` is false.

Close order, on every path: `StatusPanel.close()` returns early if `isOpen` is false, then sets `isOpen = false`, calls `cascade.reset()` (cancels the timer, orders out every `CascadePanel`, rebuilds `CascadeState`), calls `onVisibility(false)`, then `super.close()`. The flag rather than `isVisible` is what makes this exact-once, because `super.close()` can call `resignKey` and re-enter (MENU-4.15). `show` sets `isOpen = true` and calls `onVisibility(true)` once (MENU-4.14). A `closeAll` effect calls `StatusPanel.close()`.

Keyboard path: `StatusPanel` overrides `sendEvent` and turns keyDown for the arrows, Return and Esc into `cascade.key(...)`, because the hosting view is first responder and would otherwise take them. ⌘-chords fall through to `performKeyEquivalent`. When the first responder is a text view, only in the Preview, keys pass through except Esc.

A `CascadePanel` is one reusable borderless, non-activating `NSPanel` per depth, at `.popUpMenu` level with the same collection behavior as level 0, `canBecomeKey == false`, `isReleasedWhenClosed = false`, and `animationBehavior = .none`. It is ordered out, not closed. A Submenu's content is scrollable and limited to the height `CascadePlacement` allows (MENU-4.7), and it re-fits when its content changes, keeping its top edge. Level 0 re-fits the same way when a banner appears, and then re-places any open Submenu through `CascadePlacement`, since its row moved.

The outside-click monitor has already been seen reporting a click on the app's own status-button window (commit `dab5a48`), so the design does not rely on it missing Submenu clicks: `StatusPanelClickPolicy` takes every open panel's frame and does not dismiss for a click inside one (MENU-4.9), and it keeps the status-button click with the toggle (MENU-G3). Decision 2 discharges MENU-4.12: key status has nowhere to move inside the Cascade, so `resignKey` means it left (MENU-4.13). Level 0 reads `accessibilityDisplayShouldReduceMotion` at show and uses `.none` instead of `.utilityWindow` when it is on (MENU-6.4). Status-icon rendering and the right-click menu do not change (MENU-G4, MENU-G7, MENU-G11).

Facts only a running app can settle, checked first in the build, each with its fallback:
- `.onContinuousHover` and a first click on a SwiftUI `Button` reach a non-key, non-activating panel. The spike confirmed `.onHover` and clicks on level 0 only. Fallback: `acceptsFirstMouse` on the hosting view and a tracking area in `CascadePanel`.
- `sendEvent` sees keys with the hosting view as first responder, and `.keyboardShortcut` answers inside the panel. Fallback: match `⌘,` and `⌘Q` in `sendEvent`.
- VoiceOver reaches a panel that is never key. Fallback: let Submenu panels become key, and treat `resignKey` as leaving only when the new key window is outside the Cascade.

### Glass surface

Satisfies: MENU-1.1, MENU-6.3, MENU-11.4
Reuse: rung 4 — AppKit `NSGlassEffectView` (macOS 26), `NSColor(name:dynamicProvider:)`, `NSWorkspace.accessibilityDisplayOptionsDidChangeNotification`; rung 7 — one small wrapper.
Respects: ARCH-15
Interface: `GlassSurfaceView(content: NSView)` and `var tint: NSColor`. `GlassTint.standard` holds the default.
Depth: If this view vanished, panels would need to know only "put my content on the surface"; the style, corner radius, tint and the Reduce Transparency switch stay inside it.
Locality: new `StatusBar/GlassSurfaceView.swift`; `extend` `StatusPanel.swift` where it builds `NSVisualEffectView` today.

It holds an `NSGlassEffectView` with style `.clear`, corner radius 12 and the tint. While Reduce Transparency is on it shows an opaque `windowBackgroundColor` layer instead, and it swaps on the notification. The first-frame `CADisplayLink` attaches to the panel's `contentView` as today, and the spike measured that way on a glass content view.

### Level 0 and rows

Satisfies: MENU-1.2, MENU-1.3, MENU-1.4, MENU-1.5, MENU-1.6, MENU-1.7, MENU-1.9, MENU-1.10, MENU-2.1, MENU-2.2, MENU-2.3, MENU-2.4, MENU-2.5, MENU-3.1, MENU-3.2, MENU-3.3, MENU-3.4, MENU-3.5, MENU-3.6, MENU-3.7, MENU-3.8, MENU-3.9, MENU-3.10, MENU-6.1, MENU-6.2, MENU-11.3
Reuse: rung 2 — `Banner`, `SectionTitle`, the module and activity row content in `PanelView.swift`, and SwiftUI's `.keyboardShortcut` as the footer uses it today; rung 7 — `MenuRowView`, because no existing row has a value, chevron and selection.
Respects: ARCH-1, ARCH-2, ARCH-13
Surface:
- `PanelView` — `StatusPanel.swift:43` — replace: built by `StatusBarController` as `CascadeContent.root`.
- `ModulesSection`, `ActivitySection` — private to `PanelView.swift` — replace: their rows move to the Core Submenu.
- `store.snapshot` read from panel views (`PanelView.swift:16,54,57`) — replace with `store.moduleRows`, `store.eventRows`, `store.uncleanExit`, `store.configProblem`, `store.hasSnapshot`.
- `CoreStore.snapshot` (`StatusBarController.swift:120`, `SettingsView.swift`), `CoreStore.health` (`StatusBarController.swift:37`), `CoreStore.moduleCount` (`StatusBarController.swift:90`) — frozen, built around: new properties are additions and these names stay, because KCONTRACT edits `health`. `configProblem` follows `configNotice` at rebase, taking its `title` and `message`.
- `Banner` — `PanelView.swift:165-186` — replace: its message moves from `.secondary` to `labelColor`, because it carries the real error and MENU-11.4 holds secondary text to only 3:1.
Interface:
```swift
struct MenuRow { id, icon, title, value, dot, opensSubmenu, shortcut }
MenuRowView(row: MenuRow, cascade: CascadeController)
// in NavoriPanel, tested
public enum CoreStatus { starting, running, needsAttention, notRunning }
public struct CoreRowState { label: String; dot: StatusDot?; hasSubmenu: Bool; init(status: CoreStatus) }
public enum ModuleDot { static func color(state: String) -> StatusDot }
```
Depth: If `MenuRowView` vanished, every caller would have to rebuild the row's look, its hover and click reporting and its accessibility text; with it a caller knows only a `MenuRow` value.
Locality: new `Features/Panel/MenuRowView.swift`, `CoreSubmenuView.swift`; new `NavoriPanel/CoreRowState.swift`, because `docs/standards/testing.md:24` holds mapping logic to tests and no test target can import the `Navori` executable; `extract` the module and activity rows from `PanelView.swift`; `extend` `CoreStore.swift` with the plain structs and a direct `coreStatus`; `leave` `CoreStartupBanner`.

`PanelView` becomes: banners, the Core row, `Divider`, then rows for `Settings…`, `About Navori` and `Quit Navori`. Activating a row closes the Cascade and calls the action. `Settings…` and `Quit Navori` keep their `.keyboardShortcut` (MENU-1.10). `CoreStore.coreStatus` maps `Health` case for case: `starting` to `.starting`, `ok` to `.running`, `degraded` to `.needsAttention`, `down` to `.notRunning`. `CoreRowState` turns that into the label, dot and chevron. `@Observable` reads redraw an open Submenu when a new snapshot arrives, with no Cascade involvement (MENU-3.6). The Core row has no children until there is a snapshot (MENU-3.7); the snapshot never returns to nil, so `StatusBarController` re-reads `tree()` and sends `replaceTree` once, when the first snapshot arrives while the panel is open. Each row uses `.accessibilityElement(children: .ignore)` with a label, value and "Opens a submenu" hint, plus the button trait when it acts (MENU-6.1). Whether VoiceOver can reach a panel that is never key is unverified; the manual pass in MENU-11.3 decides, and Decision 2 reopens if it cannot.

### Settings on a chosen tab

Satisfies: MENU-1.8, MENU-G8
Reuse: rung 2 — extends `SettingsWindowController` and the existing `TabView`.
Surface:
- `SettingsWindowController.show()` — `AppDelegate.swift:18,31` — replace: `show(tab: SettingsTab = .general)`.
- `openSettings: () -> Void` — `AppDelegate.swift:18`, `StatusBarController.swift:9,18,20,110` — replace: `(SettingsTab) -> Void`.
Interface: `enum SettingsTab { general, diagnostics, about }`; `SettingsWindowController.show(tab:)`.
Depth: n/a — extends `SettingsWindowController`.
Locality: `extend` `SettingsView.swift` with a `TabView` selection binding held in a small `@Observable`; `leave` the three tab views.

### Dev preview

Satisfies: MENU-7.1, MENU-7.2, MENU-7.3, MENU-7.4, MENU-7.5, MENU-7.6
Reuse: rung 2 — `StatusPanel`, `CascadeController`, `MenuRowView`, `GlassSurfaceView`; rung 7 — sample content, which exists nowhere else.
Interface: `#if DEBUG`: `PreviewMenu.content() -> CascadeContent`; one extra item in `StatusBarController.showMenu`.
Depth: If the Preview vanished, nothing in Release code would notice; callers know only that one menu item.
Locality: new `Features/Preview/PreviewMenu.swift`, `PreviewSampleRows.swift`, all inside `#if DEBUG`; `extend` `StatusBarController.showMenu` under `#if DEBUG`.

The Preview's level 0 holds the Ask field, Needs you with capsule buttons, Awake, Ports and Pull Requests rows and a tint slider. Awake opens `Agent sessions`, which opens a level 3 row (MENU-7.3). Its capsule buttons and rows have no action wired (MENU-7.5). Every sample title carries the marker `NAVORI-PREVIEW-SAMPLE` in one string, which is the search term for MENU-7.6. `onVisibility` does nothing for the Preview, so it does not affect the Scheduler. The slider sets `tint` on each open panel's `GlassSurfaceView` and shows the value (MENU-7.4).

### Submenu perf gate

Satisfies: MENU-8.1, MENU-8.2, MENU-8.3, MENU-8.4, MENU-11.1, MENU-G5, MENU-G10, MENU-G12, MENU-G13
Reuse: rung 2 — `Kind::Gated`, the shape of `open_menu`, `Mark`, `installSignalHandlers`, the shape of `openMenuFresh`, the frame link.
Surface:
- `submenu_open_ms` entry — `xtask/src/perf/judge.rs:94-97` — replace with `Kind::Gated { budget: 16.0 }`, floor 5.0.
- `Kind::Skipped` — `judge.rs:9,133,423,523`, `baseline.rs:58` — replace: deleted with its print arm, since nothing else skips.
- Tests naming the skip — `judge.rs:296,346,511,514,531`, `baseline.rs:190,226` — replace: they assert the gate, and that a baseline stores the metric.
- `docs/standards/performance.md:19,35` and the floors line — replace.
- `Mark` and `parse_message` — `xtask/src/perf/logstream.rs:31-50` — extend: new `Mark::SubmenuOpened { ms }`; `menuOpened` and `statusItemDrawn` parsing unchanged.
- `measure_phase_a` — `xtask/src/perf/mod.rs:105-140` and the measured map `:239` — extend: a new `open_submenu`, beside `open_menu` and leaving it unchanged, runs 5 `USR2` opens after the reopens, while the last-opened menu is still up, before `peak` is sampled at `:132`.
- `footprint_menu_open_mb` — `mod.rs:137` — frozen, built around: still sampled after the first menu open, so its gate does not include the Submenu. Submenu memory is regression-checked only through `peak_footprint_mb`. MENU-G10 keeps today's metric, and the spike's 28 MB leaves a wide margin.
Interface: `SIGUSR2` in perf mode → `StatusBarController.openCoreSubmenuFresh()` → log line `submenuOpened ms=..`; `PerfProbe.markSubmenuOpened(since:)`.
Depth: n/a — extends the perf gate.
Locality: `extend` `judge.rs`, `baseline.rs`, `logstream.rs`, `mod.rs`, `PerfProbe.swift`, `StatusBarController.swift`; `leave` `probe.rs`, `home.rs`, `launch.rs`.

`openCoreSubmenuFresh` mirrors `openMenuFresh`: it closes an open Core Submenu with no animation, captures the trigger after that, opens it through `CascadeController`, and marks at the Submenu panel's first presented frame. If level 0 is not open it faults and marks nothing, and the run fails on the missing measurement. `SIGUSR2` is ignored then sourced inside `installSignalHandlers`, which runs only in perf mode, so outside perf mode there is no handler (MENU-8.4) and the default action applies, as with `SIGUSR1` (PERF-5.3). A perf run starts the real Core in a temporary home, so the Core row has its Submenu unless startup failed, which faults the same way.

### CI and the minimum macOS

Satisfies: MENU-9.2, MENU-9.3, MENU-10.1, MENU-10.2, MENU-10.3, MENU-10.4, MENU-G9
Reuse: rung 2 — `xtask ci`'s `sh(...)` step; existing pins.
Surface:
- `fn ci` — `xtask/src/main.rs:1009-1011` — extend: a `swift test --arch arm64 --package-path apps/macos` step after `swift build`.
- `MIN_MACOS` — `xtask/src/main.rs:17,419` — replace with `"26.0"`.
- `MACOSX_DEPLOYMENT_TARGET` — `.cargo/config.toml:7` — replace with `"26.0"`.
- `platforms` and tools version — `apps/macos/Package.swift:1,8` — replace with `swift-tools-version: 6.2` and `.macOS(.v26)`. `NavoriCore` keeps `.swiftLanguageMode(.v5)`.
- Docs — `README.md:8`, `docs/product/vision.md:43` through `define-project`, `docs/architecture/runtime.md:64`, `docs/standards/INDEX.md:21`, `docs/ops/deployment.md:16` — replace.
Interface: `cargo xtask ci` gains one step; no new command.
Depth: n/a — extends `xtask ci` and existing pins.
Locality: `extend` `xtask/src/main.rs`; `leave` the bundle steps other than the minimum version.

## UI design

The section's predicate names browser surfaces and this feature's surface is AppKit. It is filled because the brief locks a real surface and the same review applies.

Grounding: the user's words in `docs/design/2026-11-01-menubar-ui-brief.md`, Amendments 10–12, lifted here; macOS system semantic colors as the token source; `docs/standards/design-tokens.md` is absent. Tint values come from `docs/design/2026-11-02-clear-glass-tints.png`.

### Level 0

Layout: 360 pt wide with 5 pt padding: banners, then the Core row, a separator, then `Settings…`, `About Navori`, `Quit Navori`. Rows are 26 pt tall with a 20 pt lead.
Components: rung 4 — `NSGlassEffectView`; rung 2 — `Banner`; new (rung 7) — `MenuRowView`, reason: no existing row has value, chevron and selection.
States: default · hover and keyboard selection (accent background, 8 pt corners, white text and icon) · Core starting (`Starting`, no dot, no chevron until a snapshot exists) · Core running (green dot, `Running`) · needs attention (orange dot, `Needs attention`) · Core down (red dot, `Not running`, no chevron, error banner) · unclean exit and config problem (warning banner) · several banners (stacked, startup first).
Type & color: SF Pro 14 pt titles, 13 pt right-aligned values; `labelColor`, `secondaryLabelColor`, `selectedContentBackgroundColor`, `systemGreen`, `systemOrange`, `systemRed`; SF Symbols in monochrome; `DEV` chip uses `systemOrange` at 25% as today.
A11y: ↑↓←→, Return, Esc; `⌘,` and `⌘Q`; each row one VoiceOver element; state in words as well as the dot; opaque under Reduce Transparency; no animation under Reduce Motion; contrast per MENU-11.4.

### Core Submenu

Layout: a panel beside the Core row, 360 pt wide: `SectionTitle("Core")`, module rows, then `SectionTitle("Activity")`, at most 8 Event rows, scrolling when taller than the screen allows.
Components: rung 2 — today's module and activity rows, `SectionTitle`; rung 4 — `ScrollView`.
States: default · many modules (scrolls) · no Events (`Nothing yet.`) · module with restarts (`running · 2 restarts`) · module with detail (detail on a second line in `secondaryLabelColor`) · live update while open.
Type & color: 13 pt names, 12 pt detail and age in `secondaryLabelColor`; module dots `systemGreen`, `systemYellow`, `systemRed`, `systemGray` as today; severity icons as today, with `.secondary` in place of `.tertiary` on age.
A11y: rows have no action, so arrows do not stop on them; ← and Esc return to the Core row; VoiceOver reads each module and Event as one element.

### Dev preview

Layout: as level 0, with the brief's Ask field, Needs you, Awake, Ports and Pull Requests rows and a tint slider row showing the value. Dev build only.
Components: rung 2 — all of the above; new (rung 7) — sample rows, capsule buttons and the slider row, compiled out of Release code.
States: default · level 2 and level 3 open · tint being changed. Buttons have no action.
Type & color: per the brief's Surfaces 1 and 2.
A11y: not a user surface, and exempt from MENU-11.3.

## Seams for testing

| Seam | Kind | Covers |
|---|---|---|
| `CascadeState.handle` in `NavoriPanel`, including a pointer resting on another row inside the diagonal and rows moving under a still pointer | unit (`swift test`) | MENU-4.1, 4.2, 4.3, 4.4, 4.8, 5.1, 5.2, 5.3, 5.4, 5.5, 5.6, 9.1 |
| `CoreRowState`, `ModuleDot` in `NavoriPanel` | unit | MENU-3.1, 3.2, 3.7, 3.10 |
| `CascadePlacement.frame` in `NavoriPanel` | unit | MENU-4.5, 4.6, 4.7, 9.1 |
| `SafeDiagonal.contains` in `NavoriPanel` | unit | MENU-4.3, 9.1 |
| `StatusPanelClickPolicy` (existing, gains `cascadeFrames`) | unit | MENU-4.9, 4.10, G3 |
| `xtask/src/perf/judge.rs`, `logstream.rs`, `baseline.rs` (existing) | unit | MENU-8.2, 8.3, G5, G10 |
| `cargo xtask perf` | e2e | MENU-8.1, 8.4, 11.1, 11.2, G5, G12, G13 |
| `cargo xtask ci` | e2e, including a run with a failing Swift test | MENU-9.2, 9.3, G9 |
| Repo checks: a release build, because `cargo xtask ci` builds debug only; `plutil`; `strings` on the release and debug binaries; `grep` | scripted | MENU-1.4, 7.6, 10.1, 10.2, 10.3, 10.4 |
| Shell, manual per `docs/standards/testing.md:24` (Swift view layer exempt), through `cargo xtask run` | manual | MENU-1.1 – 1.10, 2.1 – 2.5, 3.1 – 3.10, 4.7, 4.9 – 4.15, 6.1 – 6.4, 7.1 – 7.5, 11.3, 11.4, G1, G2, G4, G6, G7, G8, G11, G14 |

## Coverage check

Every requirement ID in `requirements.md` appears in exactly one `Satisfies:` line above. None is unmapped. The Security and Reliability attributes are `None` in `requirements.md` and carry no ID.
