# Tasks: Menubar visual foundation

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: MENU
Status: Approved
Date: 2026-11-02
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

**Goal:** Rebuild the menubar menu on glass so that it shows only real Core state, runs a tested hand-built Cascade through a real Core Submenu, and gives later features a kit to build on.

**Architecture:** Pure logic (`CascadeState`, `CascadePlacement`, `SafeDiagonal`, `CoreRowState`) lives in the `NavoriPanel` target and is tested by `swift test`. `StatusPanel` stays level 0 and owns a `CascadeController`, which runs effects on reusable `CascadePanel`s, and SwiftUI rows only report pointer, clicks and frames. A `#if DEBUG` Preview reuses the same path with sample content.

**Tech Stack:** Swift 6 language mode, tools 6.2, macOS 26 SDK; AppKit (`NSPanel`, `NSGlassEffectView`), SwiftUI, Observation, Swift Testing; Rust in `xtask` only. No new third-party dependency.

## Global Constraints

Source of truth (referenced, not copied — read before every task):

| Source | Content hash (`git hash-object`, first 12) |
|---|---|
| `docs/standards/INDEX.md` | `54e6a749595a` |
| `docs/standards/testing.md` | `b1a9b42fc1d3` |
| `docs/standards/errors-logging.md` | `c34f3f15f450` |
| `docs/standards/performance.md` | `b72898e354c6` |
| `docs/architecture/INDEX.md` (ARCH-N) | `2e318bd23ff5` |

Task 1 and Task 17 edit `docs/standards/INDEX.md` and `docs/standards/performance.md` on purpose; a changed hash after those tasks is expected.

Compact delta every task inherits:

- **ARCH-1** — Swift renders and calls Apple frameworks. No business rule enters Swift.
- **ARCH-2** — no new `import NavoriCore` outside `CoreStore`. Task 8 removes it from the panel files.
- **ARCH-15** — no WebView or JS runtime.
- **ARCH-16** — no timer runs while the Cascade is closed. The hover timer is one `DispatchWorkItem`.
- **ARCH-18** — opening the menu fetches nothing.
- **KCONTRACT** — do not rename `CoreStore.health`, `setPanelVisible`, `snapshot` or `moduleCount`; do not edit Rust Core code.
- **Testing** — test-first in `NavoriPanel` and `xtask/src/perf`. The Swift view layer is exempt (`docs/standards/testing.md`) and verified through `cargo xtask run`. Test names are sentences in domain language. No requirement IDs in source or test names.
- **Commands** — pure Swift: `swift test --arch arm64 --package-path apps/macos --filter <name>`, after `cargo xtask build-core` has generated `apps/macos/Generated`. This builds the whole package, so the app target must compile too. A new test file declares `@Suite struct <FileName>`, because `--filter` matches the suite, not the file. Perf logic: `cargo test -p xtask perf`. Before each commit: `cargo xtask ci`. Every commit builds.
- **Commits** — Conventional Commits, English, one logical change. Commit only when the user asked for the build. Never commit `apps/macos/Generated/`, `build/` or `target/`.
- **Team** — Solo band: one contributor; no assignee or reviewer theater.
- **Worktree** — build in a worktree of its own, created only after this spec, ADR 0006, the brief's amendments, the tint image, the roadmap and the catalog row are committed, because a fresh worktree has nothing that is uncommitted. KCONTRACT merges first; Task 19 merges `main` in.

Consulted: `docs/codebase/map.md` and `docs/codebase/modules.md` for placement (feature UI in `Features/<Feature>/`, chrome in `StatusBar/`). Nothing in them conflicts with the map below.

Retrieval (`load-subgraph`, schema 1.1, recipe `fsubr-1.2`, run once on the File Structure below; OWNS coverage 3 of 8 registered, 0.38): `blast_radius` → PERF on `xtask/src/main.rs`, `xtask/src/perf/`, `StatusPanel.swift`, `StatusBarController.swift`, `AppDelegate.swift`, `CoreStore.swift`, `PerfProbe.swift`, `docs/standards/performance.md`; PROCS on `xtask/src/main.rs`; KCONTRACT on `PanelView.swift`, `AppDelegate.swift`, `CoreStore.swift`, `docs/standards/performance.md`, `docs/architecture/runtime.md`. `cluster(MENU)` → focus only, because MENU had no OWNS before this file. Observations: none. PERF and PROCS are finished work; KCONTRACT is running and merges first, which Task 19 handles. CORE, SHELL, CLI and BUILD have no Files blocks, so this list does not prove nothing else owns these paths.

## File Structure

| Path | New / changed | Responsibility |
|---|---|---|
| `apps/macos/Package.swift` | changed | tools 6.2, macOS 26 |
| `.cargo/config.toml` | changed | Rust deployment target 26.0 |
| `xtask/src/main.rs` | changed | `MIN_MACOS`, `swift test` step in `ci` |
| `README.md`, `docs/architecture/runtime.md`, `docs/standards/INDEX.md`, `docs/ops/deployment.md`, `docs/product/vision.md` | changed | state macOS 26 |
| `docs/ARCHITECTURE.md` | changed | decision row pointing at ADR 0006 |
| `apps/macos/NavoriPanel/CascadePlacement.swift` | new | Submenu frame, edge flip, height limit |
| `apps/macos/NavoriPanel/SafeDiagonal.swift` | new | point in the triangle toward the open Submenu |
| `apps/macos/NavoriPanel/CascadeState.swift` | new | `CascadeNode`, `CascadeKey`, `CascadeEvent`, `CascadeEffect`, `CascadeState` |
| `apps/macos/NavoriPanel/CoreRowState.swift` | new | `StatusDot`, `CoreStatus`, `CoreRowState`, `ModuleDot` |
| `apps/macos/NavoriPanel/StatusPanelClickPolicy.swift` | changed | takes every open panel's frame |
| `apps/macos/Tests/NavoriPanelTests/CascadePlacementTests.swift` | new | placement behaviour |
| `apps/macos/Tests/NavoriPanelTests/SafeDiagonalTests.swift` | new | diagonal behaviour |
| `apps/macos/Tests/NavoriPanelTests/CascadeStateTests.swift` | new | pointer and key behaviour |
| `apps/macos/Tests/NavoriPanelTests/CoreRowStateTests.swift` | new | state names and dots |
| `apps/macos/Tests/NavoriPanelTests/StatusPanelClickPolicyTests.swift` | changed | new argument, click inside a Submenu |
| `apps/macos/Navori/StatusBar/GlassSurfaceView.swift` | new | glass with tint, opaque under Reduce Transparency |
| `apps/macos/Navori/StatusBar/CascadeContent.swift` | new | what a Cascade shows |
| `apps/macos/Navori/StatusBar/CascadeController.swift` | new | runs `CascadeState`, owns panels and the hover timer |
| `apps/macos/Navori/StatusBar/CascadePanel.swift` | new | one reusable Submenu panel per depth |
| `apps/macos/Navori/StatusBar/StatusPanel.swift` | changed | glass, parameters, close order, key routing |
| `apps/macos/Navori/StatusBar/StatusBarController.swift` | changed | builds the content, `openCoreSubmenuFresh`, `#if DEBUG` item |
| `apps/macos/Navori/Features/Panel/PanelView.swift` | changed | level 0 layout, banners; `Banner`, `SectionTitle` stay here |
| `apps/macos/Navori/Features/Panel/MenuRowView.swift` | new | `MenuRow`, `MenuRowView` |
| `apps/macos/Navori/Features/Panel/CoreSubmenuView.swift` | new | module and Event rows |
| `apps/macos/Navori/Core/CoreStore.swift` | changed | `ModuleRow`, `EventRow`, `ConfigProblem`, `coreStatus`, published rows |
| `apps/macos/Navori/Features/Settings/SettingsView.swift` | changed | `SettingsTab`, `TabView` selection |
| `apps/macos/Navori/Features/Settings/SettingsWindowController.swift` | changed | `show(tab:)` |
| `apps/macos/Navori/App/AppDelegate.swift` | changed | passes `show(tab:)` |
| `apps/macos/Navori/Features/Preview/PreviewMenu.swift` | new | `#if DEBUG` `CascadeContent` and tint slider |
| `apps/macos/Navori/Features/Preview/PreviewSampleRows.swift` | new | `#if DEBUG` sample rows and capsule buttons |
| `apps/macos/Navori/Platform/PerfProbe.swift` | changed | `SIGUSR2`, `markSubmenuOpened` |
| `xtask/src/perf/judge.rs`, `xtask/src/perf/baseline.rs`, `xtask/src/perf/logstream.rs`, `xtask/src/perf/mod.rs` | changed | submenu gate, `Mark::SubmenuOpened`, `open_submenu` |
| `docs/standards/performance.md` | changed | gate, method, floor |
| `docs/design/2026-11-01-menubar-ui-brief.md` | changed | Amendments 10–12, written in design; final tint in Task 20 |

---

### Task 1: Require macOS 26

**Files:**
- Modify: `apps/macos/Package.swift`, `.cargo/config.toml`, `xtask/src/main.rs`, `README.md`, `docs/architecture/runtime.md`, `docs/standards/INDEX.md`, `docs/ops/deployment.md`, `docs/product/vision.md`, `docs/ARCHITECTURE.md`

**Reuse:** rung 2 — `xtask ci`'s `sh(...)` step; existing pins

**Interfaces:**
- Consumes: none
- Produces: `// swift-tools-version: 6.2`, `platforms: [.macOS(.v26)]`, `MIN_MACOS = "26.0"`, `MACOSX_DEPLOYMENT_TARGET = "26.0"`

**Depends-on:** none

- [x] **Step 1: Edit** the pins and the docs. Keep `.swiftLanguageMode(.v5)` on `NavoriCore`. Add row `D9` to the decisions table in `docs/ARCHITECTURE.md`, linking ADR 0006. Edit `docs/product/vision.md:43` through `define-project` update mode: minimum macOS only.
- [x] **Step 2: Run** `cargo xtask ci` — expect: pass, which proves UniFFI's target builds under tools 6.2.
- [x] **Step 3: Verify** `cargo xtask run`, then `plutil -extract LSMinimumSystemVersion raw "build/dev/Navori Dev.app/Contents/Info.plist"` — expect: `26.0`. `grep -rn "macOS 15\|15\.0" README.md docs/product docs/architecture docs/standards/INDEX.md docs/ops .cargo apps/macos/Package.swift` — expect: no minimum-version hit.
- [x] **Step 4: Commit** `git commit -m "build: require macOS 26 for the glass menu"`

_Requirements: MENU-10.1, MENU-10.2, MENU-10.3_

---

### Task 2: Run the Swift tests in the CI gate

**Files:**
- Modify: `xtask/src/main.rs`

**Reuse:** rung 2 — `xtask ci`'s `sh(...)` step

**Interfaces:**
- Consumes: `fn ci`
- Produces: a `swift test --arch arm64 --package-path apps/macos` step, directly after `swift build`

**Depends-on:** Task 1

- [x] **Step 1: Add** the step to `fn ci`, following the `swift build` call.
- [x] **Step 2: Run** `cargo xtask ci` — expect: pass, and its output shows the 3 existing Swift tests running after the Rust steps in today's order.
- [x] **Step 3: Prove the failure path** — temporarily flip one `#expect` in `StatusPanelClickPolicyTests.swift`, run `cargo xtask ci` — expect: non-zero exit. Restore it.
- [x] **Step 4: Commit** `git commit -m "build(ci): run the Swift tests in the gate"`

_Requirements: MENU-9.2, MENU-9.3, MENU-G9_

---

### Task 3: Place a Submenu and know the safe diagonal

**Files:**
- Create: `apps/macos/NavoriPanel/CascadePlacement.swift`, `apps/macos/NavoriPanel/SafeDiagonal.swift`
- Modify: `apps/macos/NavoriPanel/StatusPanelClickPolicy.swift`, `apps/macos/Navori/StatusBar/StatusPanel.swift` (call site only, passing `[]` until Task 13)
- Test: `apps/macos/Tests/NavoriPanelTests/CascadePlacementTests.swift`, `apps/macos/Tests/NavoriPanelTests/SafeDiagonalTests.swift`, `apps/macos/Tests/NavoriPanelTests/StatusPanelClickPolicyTests.swift`

**Reuse:** rung 7 — new code in the existing `NavoriPanel` target (rung 2 for the target and the pure-policy pattern of `StatusPanelClickPolicy`); AppKit's `NSMenu` is what ADR 0006 declined.

**Interfaces:**
- Consumes: none
- Produces: `public enum CascadePlacement { public static let gap: CGFloat = 4; public static func frame(parent: CGRect, row: CGRect, size: CGSize, visible: CGRect) -> CGRect }`; `public enum SafeDiagonal { public static func contains(_ point: CGPoint, from anchor: CGPoint, toward submenu: CGRect) -> Bool }`; `StatusPanelClickPolicy.shouldDismissPanel(clickScreenPoint: CGPoint, statusButtonScreenFrame: CGRect, cascadeFrames: [CGRect]) -> Bool`. All coordinates are AppKit screen coordinates, origin bottom-left.

**Depends-on:** Task 2

- [x] **Step 1: Write failing tests**, with literal expected frames: with `visible` (0, 0, 1800, 900), parent (1000, 400, 360, 300), row with `maxY` 650 and `size` (360, 200): `a_submenu_opens_to_the_right_of_its_parent_aligned_to_its_row` expects (1364, 450, 360, 200). With `visible` width 1500: `a_submenu_that_would_cross_the_right_edge_opens_on_the_left` expects x 636. `a_submenu_near_the_bottom_shifts_up_to_stay_on_screen` and `a_submenu_taller_than_the_screen_is_limited_to_the_visible_height`. For the diagonal, `a_point_toward_the_submenu_is_inside`, `a_point_moving_away_is_outside`, and `the_triangle_points_left_for_a_flipped_submenu`. For the policy, `a_click_inside_an_open_panel_does_not_dismiss` and `a_click_outside_every_panel_and_the_button_dismisses`; update the two existing tests with `cascadeFrames: []`.
- [x] **Step 2: Run** `swift test --arch arm64 --package-path apps/macos --filter NavoriPanelTests` — expect: does not compile, the new names do not exist.
- [x] **Step 3: Implement** the Produces list. `NavoriPanel` imports only CoreGraphics and Foundation.
- [x] **Step 4: Run** the same command — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(panel): place submenus and find the safe diagonal"`

_Requirements: MENU-4.5, MENU-4.6, MENU-9.1, MENU-G3_

---

### Task 4: Decide the Cascade from pointer events

**Files:**
- Create: `apps/macos/NavoriPanel/CascadeState.swift`
- Test: `apps/macos/Tests/NavoriPanelTests/CascadeStateTests.swift`

**Reuse:** rung 7 — new code in the existing `NavoriPanel` target (rung 2 for the target and the pure-policy pattern of `StatusPanelClickPolicy`); AppKit's `NSMenu` is what ADR 0006 declined.

**Interfaces:**
- Consumes: `SafeDiagonal.contains`
- Produces:
```swift
public struct CascadeNode: Equatable, Sendable { public let id: String; public let isActionable: Bool; public let children: [CascadeNode]
    public init(id: String, isActionable: Bool = false, children: [CascadeNode] = []) }
public enum CascadeKey: Sendable { case up, down, left, right, `return`, escape }
public enum CascadeEvent: Sendable { case pointerOn(row: String, at: CGPoint, submenu: CGRect?); case pointerLeft(row: String); case click(row: String); case hoverTimerFired; case key(CascadeKey); case replaceTree([CascadeNode]) }
public enum CascadeEffect: Equatable, Sendable { case open(row: String); case close(depth: Int); case armHoverTimer; case cancelHoverTimer; case activate(row: String); case closeAll }
public struct CascadeState: Sendable { public init(root: [CascadeNode], maxDepth: Int = 3); public private(set) var openPath: [String]; public private(set) var selection: String?; public mutating func handle(_ event: CascadeEvent) -> [CascadeEffect] }
```
`close(depth:)` closes the panel at that depth and deeper; level 0 is depth 0, its Submenu depth 1.

**Depends-on:** Task 3

- [x] **Step 1: Write failing tests** over a tree with `core` (children `core.a`, with children `core.a.x`, with child `core.a.x.y`), `settings`, `quit`: `resting_on_a_row_with_a_submenu_opens_it_when_the_timer_fires`, `a_click_on_a_row_with_a_submenu_opens_it_at_once`, `a_click_on_an_action_row_activates_it`, `moving_inside_the_diagonal_keeps_the_submenu_and_the_selection`, `resting_on_another_row_inside_the_diagonal_switches_when_the_timer_fires`, `leaving_the_diagonal_selects_the_row_and_closes_the_submenu_it_does_not_own`, `every_new_pointer_position_rearms_the_timer`, `the_same_pointer_position_twice_changes_nothing`, `leaving_a_row_clears_its_selection_unless_it_owns_an_open_submenu`, `a_row_at_the_maximum_depth_never_opens`, `a_row_with_no_action_is_never_selected`, `replacing_the_tree_can_add_children`, `replacing_the_tree_closes_a_submenu_whose_children_are_gone`.
- [x] **Step 2: Run** `swift test --arch arm64 --package-path apps/macos --filter CascadeStateTests` — expect: does not compile.
- [x] **Step 3: Implement** the rules listed under "Cascade logic" in `design.md`.
- [x] **Step 4: Run** the same command — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(panel): decide the cascade from pointer events"`

_Requirements: MENU-4.1, MENU-4.2, MENU-4.3, MENU-4.4, MENU-4.8, MENU-9.1_

---

### Task 5: Decide the Cascade from keys

**Files:**
- Modify: `apps/macos/NavoriPanel/CascadeState.swift`
- Test: `apps/macos/Tests/NavoriPanelTests/CascadeStateTests.swift`

**Reuse:** rung 7 — new code in the existing `NavoriPanel` target (rung 2 for the target and the pure-policy pattern of `StatusPanelClickPolicy`); AppKit's `NSMenu` is what ADR 0006 declined.

**Interfaces:**
- Consumes: `CascadeState`, `CascadeKey`
- Produces: `handle(.key(_))` behaviour; no new public name

**Depends-on:** Task 4

- [x] **Step 1: Write failing tests**: `down_and_up_wrap_through_rows_with_an_action`, `down_skips_rows_with_no_action`, `right_opens_the_submenu_and_moves_in`, `left_closes_the_submenu_and_returns_to_its_row`, `return_does_what_a_click_on_the_selection_does`, `escape_closes_the_deepest_panel`, `escape_at_level_zero_closes_everything`, `a_real_pointer_move_after_keys_takes_the_one_selection`, `a_row_moving_under_a_still_pointer_does_not_take_a_keyboard_selection`.
- [x] **Step 2: Run** `swift test --arch arm64 --package-path apps/macos --filter CascadeStateTests` — expect: the new tests fail on their assertions.
- [x] **Step 3: Implement** the key rules.
- [x] **Step 4: Run** the same command — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(panel): decide the cascade from keys"`

_Requirements: MENU-5.1, MENU-5.2, MENU-5.3, MENU-5.4, MENU-5.5, MENU-5.6, MENU-9.1_

---

### Task 6: Put level 0 on glass

**Files:**
- Create: `apps/macos/Navori/StatusBar/GlassSurfaceView.swift`
- Modify: `apps/macos/Navori/StatusBar/StatusPanel.swift`, `apps/macos/Navori/Features/Panel/PanelView.swift` (width only)

**Reuse:** rung 4 — AppKit `NSGlassEffectView` (macOS 26), `NSColor(name:dynamicProvider:)`, `NSWorkspace.accessibilityDisplayOptionsDidChangeNotification`; rung 7 — one small wrapper.

**Interfaces:**
- Consumes: none
- Produces: `final class GlassSurfaceView: NSView { init(content: NSView); var tint: NSColor }`; `enum GlassTint { static let standard: NSColor }` (black at 65% in dark appearance, white at 55% in light); `StatusPanel.width == 360`

**Depends-on:** Task 1, Task 3

- [x] **Step 1: Implement** `GlassSurfaceView`: `NSGlassEffectView` with style `.clear`, corner radius 12 and `tint`, no availability branch and no `NSVisualEffectView` fallback. While Reduce Transparency is on, it shows an opaque `windowBackgroundColor` layer, and it swaps on the notification. Remove `NSVisualEffectView` from `StatusPanel`.
- [x] **Step 2: Run** `cargo xtask ci` — expect: pass. `grep -rn "NSVisualEffectView\|#available(macOS" apps/macos/Navori` — expect: no hit.
- [x] **Step 3: Check by hand** with `cargo xtask run`: glass over a bright window and over a dark one; Reduce Transparency on and off while open; open under the status item, near the right screen edge, on a second Space and over a full-screen app, with the frontmost app staying active. For the off-screen fallback, launch the binary with `NAVORI_PERF=1 NAVORI_PERF_HOME=<temp dir>`, send `kill -USR1 <pid>`, and see the panel at the top of the main screen.
- [x] **Step 4: Run** `cargo xtask perf` — expect: `menu_open_after_launch_ms` ≤ 80 and `menu_reopen_ms` ≤ 50. A failure stops the plan here and goes to the user, because every later task sits on this surface.
- [x] **Step 5: Commit** `git commit -m "feat(shell): put the menu on clear glass"`

_Requirements: MENU-1.1, MENU-6.3, MENU-10.4, MENU-G1, MENU-G2, MENU-G14_

---

### Task 7: Open Settings on a chosen tab

**Files:**
- Modify: `apps/macos/Navori/Features/Settings/SettingsView.swift`, `apps/macos/Navori/Features/Settings/SettingsWindowController.swift`, `apps/macos/Navori/App/AppDelegate.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/Navori/StatusBar/StatusPanel.swift`, `apps/macos/Navori/Features/Panel/PanelView.swift`

**Reuse:** rung 2 — extends `SettingsWindowController` and the existing `TabView`.

**Interfaces:**
- Consumes: none
- Produces: `enum SettingsTab { case general, diagnostics, about }`; `SettingsWindowController.show(tab: SettingsTab = .general)`; `openSettings: (SettingsTab) -> Void` on `StatusBarController`, `StatusPanel` and `PanelView`

**Depends-on:** Task 6

- [x] **Step 1: Implement**: `TabView(selection:)` bound to a small `@Observable` that the controller holds; `AppDelegate.applicationShouldHandleReopen` still calls `show()`.
- [x] **Step 2: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 3: Check by hand**: `show(tab: .about)` from a temporary call opens on About; an already-open window switches tab; General, Diagnostics and About keep their content; launching the app again opens Settings.
- [x] **Step 4: Commit** `git commit -m "feat(settings): open the window on a chosen tab"`

_Requirements: MENU-1.8, MENU-G8_

---

### Task 8: Name the state of the Core

**Files:**
- Create: `apps/macos/NavoriPanel/CoreRowState.swift`
- Modify: `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/Navori/Features/Panel/PanelView.swift`
- Test: `apps/macos/Tests/NavoriPanelTests/CoreRowStateTests.swift`

**Reuse:** rung 2 — `Banner`, `SectionTitle`, the module and activity row content in `PanelView.swift`, and SwiftUI's `.keyboardShortcut` as the footer uses it today; rung 7 — `MenuRowView`, because no existing row has a value, chevron and selection.

**Interfaces:**
- Consumes: `CoreStore.Health`
- Produces (`NavoriPanel`): `public enum StatusDot: Sendable { case green, yellow, orange, red, gray }`; `public enum CoreStatus: Sendable { case starting, running, needsAttention, notRunning }`; `public struct CoreRowState: Equatable, Sendable { public let label: String; public let dot: StatusDot?; public let hasSubmenu: Bool; public init(status: CoreStatus) }`; `public enum ModuleDot { public static func color(state: String) -> StatusDot }`
- Produces (`CoreStore`): `struct ModuleRow: Identifiable, Equatable { let id: String; let state: String; let detail: String?; let restarts: UInt32 }`; `struct EventRow: Identifiable, Equatable { let id: String; let tsMs: Int64; let severity: String; let summary: String }`; `struct ConfigProblem: Equatable { let title: String; let message: String }`; `var moduleRows: [ModuleRow]`, `var eventRows: [EventRow]`, `var uncleanExit: String?`, `var configProblem: ConfigProblem?`, `var hasSnapshot: Bool`, `var coreStatus: CoreStatus`

**Depends-on:** Task 2, Task 7

- [x] **Step 1: Write failing tests**: `each_status_has_its_label_dot_and_submenu` (a table: `Starting` no dot and no Submenu, `Running` green with one, `Needs attention` orange with one, `Not running` red and no Submenu), `module_dots_follow_the_module_state` (a table: `running` green, `starting` and `restarting` yellow, `failed` red, `stopped` gray).
- [x] **Step 2: Run** `swift test --arch arm64 --package-path apps/macos --filter CoreRowStateTests` — expect: does not compile.
- [x] **Step 3: Implement** the Produces lists. `configProblem` holds today's title and `snapshot.configError` as its message. Move `PanelView` to these properties, retype its private `ModulesSection` and `ActivitySection` to `ModuleRow` and `EventRow`, and remove its `import NavoriCore`.
- [x] **Step 4: Run** the same command, then `cargo xtask ci` — expect: pass, and the panel looks as it did.
- [x] **Step 5: Commit** `git commit -m "feat(shell): name the core state in plain rows"`

_Requirements: MENU-3.2, MENU-3.7, MENU-3.10_

---

### Task 9: Let `StatusPanel` take what it shows

**Files:**
- Create: `apps/macos/Navori/StatusBar/CascadeContent.swift`
- Modify: `apps/macos/Navori/StatusBar/StatusPanel.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`

**Reuse:** rung 2 — extends `StatusPanel` and `StatusBarController`; rung 7 — `CascadeController`, because no existing piece owns child panels.

**Interfaces:**
- Consumes: `CascadeNode`, `PanelView`, `SettingsTab`
- Produces:
```swift
struct CascadeContent { let tree: () -> [CascadeNode]; let root: AnyView; let submenu: (String) -> AnyView; let activate: (String) -> Void }
StatusPanel(content: CascadeContent, onVisibility: @escaping (Bool) -> Void)
```

**Depends-on:** Task 4, Task 8

- [x] **Step 1: Implement**: `StatusBarController` builds `CascadeContent` around today's `PanelView` and binds `onVisibility` to `store.setPanelVisible`. `StatusPanel` no longer holds `store`. Behavior does not change.
- [x] **Step 2: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 3: Check by hand**: the panel opens, closes and shows the same content as before.
- [x] **Step 4: Commit** `git commit -m "refactor(shell): let the status panel take its content"`

_Requirements: MENU-G6_

---

### Task 10: Show the new level 0

**Files:**
- Create: `apps/macos/Navori/Features/Panel/MenuRowView.swift`, `apps/macos/Navori/StatusBar/CascadeController.swift`
- Modify: `apps/macos/Navori/Features/Panel/PanelView.swift`, `apps/macos/Navori/StatusBar/StatusPanel.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`

**Reuse:** rung 2 — `Banner`, `SectionTitle`, the module and activity row content in `PanelView.swift`, and SwiftUI's `.keyboardShortcut` as the footer uses it today; rung 7 — `MenuRowView`, because no existing row has a value, chevron and selection.

**Interfaces:**
- Consumes: `CascadeState`, `CoreRowState`, `CoreStore` rows, `SettingsTab`, `GlassSurfaceView`
- Produces:
```swift
struct MenuRow { let id: String; let icon: String?; let title: String; let value: String?; let dot: StatusDot?; let opensSubmenu: Bool; let shortcut: KeyEquivalent? }
struct MenuRowView: View { init(row: MenuRow, cascade: CascadeController) }
@MainActor @Observable final class CascadeController { var selection: String? { get }; func pointer(on row: String, at screenPoint: CGPoint); func pointerLeft(_ row: String); func click(_ row: String); func key(_ key: CascadeKey); func setRowFrame(_ row: String, _ frameInPanel: CGRect); func reset() }
StatusPanel.cascade: CascadeController { get }
```
Row ids: `core`, `settings`, `about`, `quit`.

**Depends-on:** Task 4, Task 8, Task 9

- [x] **Step 1: Implement** `PanelView` in the order banners, Core row, `Divider`, `Settings…`, `About Navori`, `Quit Navori`, with no header, version, Core section or Activity section. `MenuRowView` draws 14 pt titles, 13 pt values, `labelColor` and `secondaryLabelColor`, and the accent fill at 8 pt corners with white text while selected. The `Button` action is the row's only click report. `CascadeController` runs `CascadeState` and the hover timer; it handles selection and `activate`, and logs and ignores `open` until Task 12.
- [x] **Step 2: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 3: Check by hand** in `cargo xtask run`: layout and type against the brief; a Core row with its state; the `DEV` chip in the dev build. A first click on `Settings…` closes the menu and opens Settings; `About Navori` opens the About tab; `Quit Navori` quits. `.onContinuousHover` selects rows with another app frontmost, or apply the design's fallback. Status icon, dimming, tooltip and the right-click menu are unchanged.
- [x] **Step 4: Commit** `git commit -m "feat(shell): show the new level 0 menu"`

_Requirements: MENU-1.2, MENU-1.3, MENU-1.5, MENU-1.6, MENU-1.7, MENU-1.9, MENU-3.8, MENU-G4, MENU-G7, MENU-G11_

---

### Task 11: See what is wrong

**Files:**
- Modify: `apps/macos/Navori/Features/Panel/PanelView.swift`, `apps/macos/Navori/StatusBar/StatusPanel.swift`

**Reuse:** rung 2 — `Banner`, `SectionTitle`, the module and activity row content in `PanelView.swift`, and SwiftUI's `.keyboardShortcut` as the footer uses it today; rung 7 — `MenuRowView`, because no existing row has a value, chevron and selection.

**Interfaces:**
- Consumes: `store.startupError`, `store.uncleanExit`, `store.configProblem`
- Produces: `Banner` message in `labelColor`

**Depends-on:** Task 10

- [x] **Step 1: Implement** the banners above the Core row, startup first; change the `Banner` message from `.secondary` to `labelColor`, keeping `lineLimit(4)`; add no restart control. Level 0 re-fits when a banner appears.
- [x] **Step 2: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 3: Check by hand**: an invalid `config.toml` shows the config banner with its report; `kill -9` of a dev build then a relaunch shows the unclean-exit banner; a garbage file at `<temp home>/Library/Application Support/Navori Dev/navori.db`, with the binary launched directly under `NAVORI_PERF=1 NAVORI_PERF_HOME=<temp home>`, shows the startup banner with the `startup failed:` text. Each is readable on a bright background and offers no restart button.
- [x] **Step 4: Commit** `git commit -m "feat(shell): keep core problems readable on glass"`

_Requirements: MENU-2.1, MENU-2.2, MENU-2.3, MENU-2.4, MENU-2.5_

---

### Task 12: Read the Core in a real Submenu

**Files:**
- Create: `apps/macos/Navori/StatusBar/CascadePanel.swift`, `apps/macos/Navori/Features/Panel/CoreSubmenuView.swift`
- Modify: `apps/macos/Navori/StatusBar/CascadeController.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/Navori/Features/Panel/PanelView.swift`

**Reuse:** rung 2 — extends `StatusPanel` and `StatusBarController`; rung 7 — `CascadeController`, because no existing piece owns child panels.

**Interfaces:**
- Consumes: `CascadePlacement.frame`, `SafeDiagonal`, `CascadeEffect`, `GlassSurfaceView`, `CoreStore.moduleRows`, `CoreStore.eventRows`, `ModuleDot`
- Produces: `final class CascadePanel: NSPanel` (non-activating, `.popUpMenu`, `canBecomeKey == false`, `isReleasedWhenClosed = false`, `animationBehavior = .none`); `struct CoreSubmenuView: View { init(store: CoreStore) }`; `CascadeController.open(_ row: String, onFirstFrame: (@MainActor () -> Void)? = nil)`; `CascadeController.closeSubmenus()`, with no animation

**Depends-on:** Task 3, Task 5, Task 11

- [x] **Step 1: Implement** `CascadePanel`, one per depth, ordered out rather than closed. `CascadeController` runs `open` and `close(depth:)`, sends `replaceTree` when the first snapshot arrives, and re-places an open Submenu when level 0 re-fits. `CoreSubmenuView` lists modules with state, restarts and detail, then at most 8 Events with severity icon and narrow relative age, or `Nothing yet.`. Content scrolls and is limited to the height `CascadePlacement` allows.
- [x] **Step 2: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 3: Check by hand**: hover opens the Submenu after the delay and a click opens it at once; with another app frontmost, a first click and hover both reach the Submenu; moving diagonally across other rows keeps it, and resting on another row switches. It flips left at the right screen edge; a short-screen window scrolls it; a new Event appears with it open. With no snapshot, the row has no chevron. Turn VoiceOver on and confirm it reaches the Submenu, which is never key; if it cannot, apply the design's fallback now.
- [x] **Step 4: Commit** `git commit -m "feat(shell): read modules and activity in the core submenu"`

_Requirements: MENU-3.1, MENU-3.3, MENU-3.4, MENU-3.5, MENU-3.6, MENU-3.7, MENU-4.1, MENU-4.2, MENU-4.3, MENU-4.4, MENU-4.5, MENU-4.6, MENU-4.7_

---

### Task 13: Close the whole Cascade once

**Files:**
- Modify: `apps/macos/Navori/StatusBar/StatusPanel.swift`, `apps/macos/Navori/StatusBar/CascadeController.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`

**Reuse:** rung 2 — extends `StatusPanel` and `StatusBarController`; rung 7 — `CascadeController`, because no existing piece owns child panels.

**Interfaces:**
- Consumes: `StatusPanelClickPolicy.shouldDismissPanel(clickScreenPoint:statusButtonScreenFrame:cascadeFrames:)`, `CascadeController.reset()`
- Produces: `private var isOpen` in `StatusPanel`; the close order in `design.md`; `CascadeController.openFrames: [CGRect]`

**Depends-on:** Task 12

- [x] **Step 1: Implement** the close order: return if `!isOpen`; `isOpen = false`; `cascade.reset()`; `onVisibility(false)`; `super.close()`. `show` sets `isOpen = true` and calls `onVisibility(true)` once. `toggle` reads `isOpen`, not `isVisible`. The timer closure does nothing when `isOpen` is false. `closeAll` calls `close()`. The outside-click policy gets `cascade.openFrames`.
- [x] **Step 2: Temporarily log** each `onVisibility` call in the dev build. Run `cargo xtask ci` — expect: pass.
- [x] **Step 3: Check by hand** that each path logs exactly one `true` then one `false`, with a Submenu open each time: click outside; click the status button; click inside the Submenu (nothing closes); switch app; right-click the status item; `Settings…`; Esc from level 0. A click on the status button does not reopen it. Closing with the hover timer armed opens nothing afterward. Remove the log.
- [x] **Step 4: Commit** `git commit -m "fix(shell): close the whole cascade once on every path"`

_Requirements: MENU-4.9, MENU-4.10, MENU-4.11, MENU-4.12, MENU-4.13, MENU-4.14, MENU-4.15, MENU-G6_

---

### Task 14: Drive the menu from the keyboard

**Files:**
- Modify: `apps/macos/Navori/StatusBar/StatusPanel.swift`, `apps/macos/Navori/Features/Panel/PanelView.swift`

**Reuse:** rung 2 — extends `StatusPanel` and `StatusBarController`; rung 7 — `CascadeController`, because no existing piece owns child panels.

**Interfaces:**
- Consumes: `CascadeController.key(_:)`, `CascadeKey`
- Produces: `StatusPanel.sendEvent(_:)` override

**Depends-on:** Task 5, Task 13

- [x] **Step 1: Implement**: keyDown for key codes 126, 125, 123, 124, 36, 76 and 53 becomes `cascade.key(...)`; ⌘-chords fall through. While the first responder is an `NSText`, every key but Esc passes through. `cancelOperation` sends `key(.escape)`. `Settings…` and `Quit Navori` carry `⌘,` and `⌘Q`.
- [x] **Step 2: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 3: Check by hand** with the pointer parked outside the menu: ↓ reaches every row and wraps; → opens the Core Submenu; ← returns to the Core row; Return on `Settings…` opens Settings; Esc closes the Submenu, then level 0; `⌘,` and `⌘Q` work. If `.keyboardShortcut` does not answer, apply the design's fallback. Moving the pointer then moves the one selection.
- [x] **Step 4: Commit** `git commit -m "feat(shell): drive the menu from the keyboard"`

_Requirements: MENU-1.10, MENU-5.1, MENU-5.2, MENU-5.3, MENU-5.4, MENU-5.5, MENU-5.6_

---

### Task 15: Honor VoiceOver and Reduce Motion

**Files:**
- Modify: `apps/macos/Navori/Features/Panel/MenuRowView.swift`, `apps/macos/Navori/Features/Panel/CoreSubmenuView.swift`, `apps/macos/Navori/Features/Panel/PanelView.swift`, `apps/macos/Navori/StatusBar/StatusPanel.swift`

**Reuse:** rung 2 — `Banner`, `SectionTitle`, the module and activity row content in `PanelView.swift`, and SwiftUI's `.keyboardShortcut` as the footer uses it today; rung 7 — `MenuRowView`, because no existing row has a value, chevron and selection.

**Interfaces:**
- Consumes: `MenuRow`, `ModuleRow`, `EventRow`
- Produces: accessibility modifiers; no new name

**Depends-on:** Task 14

- [x] **Step 1: Implement**: each row and each module and Event row is `.accessibilityElement(children: .ignore)` with a label and value; a row that opens a Submenu has the hint `Opens a submenu`; a row that acts has the button trait. Every state dot sits beside its words. `StatusPanel` reads `accessibilityDisplayShouldReduceMotion` at show and uses `.none` when it is on.
- [x] **Step 2: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 3: Manual pass**: with VoiceOver on, walk level 0 and the Core Submenu and hear each row as one sentence, including that the Core row opens a Submenu. With Reduce Motion on, the panel appears with no animation. With Reduce Transparency on, every panel of the Cascade is opaque. Record the result in the task report.
- [x] **Step 4: Commit** `git commit -m "feat(shell): read the menu well with VoiceOver and Reduce Motion"`

_Requirements: MENU-6.1, MENU-6.2, MENU-6.4, MENU-11.3_

---

### Task 16: Judge the design in a dev preview

**Files:**
- Create: `apps/macos/Navori/Features/Preview/PreviewMenu.swift`, `apps/macos/Navori/Features/Preview/PreviewSampleRows.swift`
- Modify: `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/Navori/StatusBar/StatusPanel.swift`

**Reuse:** rung 2 — `StatusPanel`, `CascadeController`, `MenuRowView`, `GlassSurfaceView`; rung 7 — sample content, which exists nowhere else.

**Interfaces:**
- Consumes: `CascadeContent`, `StatusPanel(content:onVisibility:)`, `GlassSurfaceView.tint`
- Produces: `#if DEBUG enum PreviewMenu { static func content(setTint: @escaping (NSColor) -> Void) -> CascadeContent }`; `StatusPanel.setTint(_ tint: NSColor)`; a `#if DEBUG` item `Open Menu Preview` in `StatusBarController.showMenu`; the marker string `NAVORI-PREVIEW-SAMPLE`

**Depends-on:** Task 15

- [x] **Step 1: Implement** the Preview from the brief's Surfaces 1 and 2 in files wrapped whole in `#if DEBUG`: Ask field, Needs you with capsule buttons with no action, Awake, Ports and Pull Requests rows, and a tint slider row showing its value. Awake opens `Agent sessions`, which opens a level 3 row. `onVisibility` does nothing. `activate` does nothing.
- [x] **Step 2: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 3: Check by hand** in `cargo xtask run`: the right-click item opens the Preview; 3 levels open; the slider retints every open panel; typing works in the Ask field; `Navori Dev` logs no dispatched Action.
- [x] **Step 4: Verify the compile-out**: `swift build -c release --arch arm64 --package-path apps/macos`, then `BIN=$(swift build -c release --arch arm64 --package-path apps/macos --show-bin-path)`, `test -f "$BIN/Navori"` — expect: exit 0, so the search cannot pass on a missing file. `strings "$BIN/Navori" | grep -c NAVORI-PREVIEW-SAMPLE` — expect: `0`. The same with `-c debug` — expect: at least `1`. The marker is over 15 bytes so Swift stores it as a real string in the binary.
- [x] **Step 5: Commit** `git commit -m "feat(shell): add a dev-only preview of the full menu"`

_Requirements: MENU-7.1, MENU-7.2, MENU-7.3, MENU-7.4, MENU-7.5, MENU-7.6_

---

### Task 17: Measure opening a Submenu

**Files:**
- Modify: `xtask/src/perf/judge.rs`, `xtask/src/perf/baseline.rs`, `xtask/src/perf/logstream.rs`, `xtask/src/perf/mod.rs`, `apps/macos/Navori/Platform/PerfProbe.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`, `docs/standards/performance.md`
- Test: `xtask/src/perf/judge.rs`, `xtask/src/perf/baseline.rs`, `xtask/src/perf/logstream.rs`, `xtask/src/perf/mod.rs` (each `#[cfg(test)]`)

**Reuse:** rung 2 — `Kind::Gated`, the shape of `open_menu`, `Mark`, `installSignalHandlers`, the shape of `openMenuFresh`, the frame link.

**Interfaces:**
- Consumes: `CascadeController.open(_:onFirstFrame:)`, `PerfProbe.captureTrigger`
- Produces: `Mark::SubmenuOpened { ms: f64 }`; log line `submenuOpened ms=<one decimal>`; `fn is_submenu_open(mark: &Mark) -> bool`; `fn open_submenu(launched: &Launched, log: &mut LogStream) -> Result<f64>`; `PhaseAProcess.submenu_open_ms: f64`, that process's median of 5; `StatusBarController.openCoreSubmenuFresh()`; `PerfProbe.markSubmenuOpened(since: UInt64)`; metric `submenu_open_ms` as `Kind::Gated { budget: 16.0 }`, floor 5.0; `Kind::Skipped` deleted

**Depends-on:** Task 12, Task 13, Task 16

- [x] **Step 1: Write failing tests** in `logstream.rs`, `judge.rs`, `baseline.rs` and `mod.rs`: `a_submenu_mark_parses_its_time`, `a_submenu_mark_with_an_extra_field_is_rejected`, `a_submenu_open_over_sixteen_ms_fails_the_run`, `a_missing_submenu_measurement_fails_the_run`, `no_metric_reports_itself_as_skipped`, `a_baseline_stores_the_submenu_metric`, `the_submenu_metric_is_the_median_of_each_process_median`. Change the skip assertions at `judge.rs` 346, 511, 514, 531 and `baseline.rs` 226; `baseline.rs` 190 stays as input, and `judge.rs` 296 needs no change. `within_budget()` at `judge.rs` 251 and the `process()` helper at `mod.rs` 593 gain the new value.
- [x] **Step 2: Run** `cargo test -p xtask perf` — expect: does not compile, `Mark::SubmenuOpened` does not exist.
- [x] **Step 3: Implement** the Produces list. Deleting `Kind::Skipped` touches `judge.rs` 9, 96, 133, 423, 523 and `baseline.rs` 58. `open_submenu` sends `USR2` and awaits the mark with `is_submenu_open`, with no probe sample; phase A runs it 5 times after the reopens, before `peak` is sampled, and `insert_median` takes each process's median. `openCoreSubmenuFresh` reaches the Submenu through `panel.cascade`. `SIGUSR2` is ignored then sourced inside `installSignalHandlers`, as `SIGUSR1` is. With level 0 not open, `openCoreSubmenuFresh` faults and marks nothing.
- [x] **Step 4: Update** `docs/standards/performance.md`: the budget row, the skip list line removed, floor 5 ms added, and the method: median across processes of 5 Core Submenu opens each. The test at `judge.rs` 505-531 reads this file, so its assertions and this wording change together.
- [x] **Step 5: Run** `cargo test -p xtask perf` — expect: pass. `cargo xtask perf` — expect: `submenu_open_ms` measured at ≤ 16, no line says skipped, `menuOpened` and `statusItemDrawn` still parse, and a clean `SIGTERM` exit. A failure over budget stops and goes to the user.
- [x] **Step 6: Verify** that a dev build not in perf mode installs no handler: `kill -USR2 <pid>` ends the process, as `USR1` does.
- [x] **Step 7: Commit** `git commit -m "feat(perf): gate how fast the core submenu opens"`

_Requirements: MENU-8.1, MENU-8.2, MENU-8.3, MENU-8.4, MENU-G5, MENU-G10, MENU-G12, MENU-G13_

---

### Task 18: Prove the release menu holds only real state

**Files:**
- Modify: `apps/macos/Navori/Features/Panel/PanelView.swift`, `apps/macos/Navori/StatusBar/StatusPanel.swift`

**Reuse:** rung 2 — `Banner`, `SectionTitle`, the module and activity row content in `PanelView.swift`, and SwiftUI's `.keyboardShortcut` as the footer uses it today; rung 7 — `MenuRowView`, because no existing row has a value, chevron and selection.

**Interfaces:**
- Consumes: none
- Produces: no new name

**Depends-on:** Task 17

- [x] **Step 1: Search** for the now-unused `ModulesSection`, `ActivitySection`, the `Self.maxHeight` constant and the `440` limit; delete what has no reader.
- [x] **Step 2: Verify the source**: `grep -rnE 'Ask Navori|Needs you|caught up|Pull Requests|"Awake"|"Ports"' apps/macos/Navori` — expect: hits only in `Features/Preview/`. `strings` cannot prove this, because Swift stores strings of 15 bytes or fewer inline.
- [x] **Step 3: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 4: Check by hand** in a release bundle, `cargo xtask bundle --profile release`, launched after quitting any running Navori: level 0 shows only the Core row and its three actions, with no `DEV` chip, and the right-click menu has no Preview item.
- [x] **Step 5: Commit** `git commit -m "refactor(shell): remove the old panel sections"`

_Requirements: MENU-1.4, MENU-3.9_

---

### Task 19: Carry the config notice from KCONTRACT

**Files:**
- Modify: `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/Navori/Features/Panel/PanelView.swift`

**Reuse:** rung 2 — `Banner`, `SectionTitle`, the module and activity row content in `PanelView.swift`, and SwiftUI's `.keyboardShortcut` as the footer uses it today; rung 7 — `MenuRowView`, because no existing row has a value, chevron and selection.

**Interfaces:**
- Consumes: `snapshot.configNotice` with `title` and `message`, `CoreStore.systemDidWake()`, `WakeObserver` (all from KCONTRACT)
- Produces: `CoreStore.configProblem` reading `configNotice`

**Depends-on:** Task 18

- [x] **Step 1: Precondition** — KCONTRACT is on `main`. If it is not when every other task is done, stop and ask the user (design: KCONTRACT merges first).
- [x] **Step 2: Merge** with `git merge main`, not a rebase, because a rebase would replay each commit's conflict. In a conflict in `PanelView.swift`, keep the new layout; `configProblem` in `CoreStore.swift` takes `title` and `message` from `configNotice` with no Shell wording; keep `WakeObserver` in `AppDelegate` and the new `show(tab:)` call.
- [x] **Step 3: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 4: Check by hand**: `log_level = 42` shows a banner naming `log_level` and not saying the whole file is on defaults; an unparseable `config.toml` says the whole file is on defaults; a sleep and wake still pulses. `CoreStore.setPanelVisible` is still reached once per open and close.
- [x] **Step 5: Commit** the merge, then `git commit -m "feat(shell): show the core's config notice on the new banner"` if any change remains.

_Requirements: MENU-2.3, MENU-G6_

---

### Task 20: Pick the tint and pass the gate

**Files:**
- Modify: `apps/macos/Navori/StatusBar/GlassSurfaceView.swift`, `docs/design/2026-11-01-menubar-ui-brief.md`

**Reuse:** rung 4 — AppKit `NSGlassEffectView` (macOS 26), `NSColor(name:dynamicProvider:)`, `NSWorkspace.accessibilityDisplayOptionsDidChangeNotification`; rung 7 — one small wrapper.

**Interfaces:**
- Consumes: the Preview's tint slider
- Produces: final `GlassTint.standard`

**Depends-on:** Task 19

- [x] **Step 1: Pick with the user** in the Preview on their real wallpaper. Measure screenshots of the chosen tint: dark appearance on a white background and light appearance on a black one. Expect primary text ≥ 4.5:1 and secondary ≥ 3:1. A tint the user prefers that misses this goes back to them with the numbers; it is not silently raised.
- [x] **Step 2: Set** `GlassTint.standard` and record the value in the brief's Amendment 11.
- [x] **Step 3: Run** `cargo xtask ci`, then `cargo xtask perf` — expect: every GOAL-11 budget met. Confirm from the idle metrics that the closed menu holds 0% CPU, and by reading `CascadeController` that the only timer is the one-shot hover work item.
- [x] **Step 4: Commit** `git commit -m "feat(shell): set the menu's glass tint"`

_Requirements: MENU-11.1, MENU-11.2, MENU-11.4_
