# Tasks: Status icon states

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: ICON
Status: Approved
Date: 2026-11-03
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md
UI brief: ./ui-brief.md

**Goal:** Make the menu bar icon and the app icon say what Navori is doing at a glance, from one state the Kernel composes, in the locked Fold look.

**Architecture:** The Kernel composes one `TrayState` from Module lifecycle, a new default `Module::tray` and the config notice; `Inner::status()` calls it once, `CoreStatus` carries it over IPC and `snapshot()` copies it to Swift. `CoreStore` maps it to a plain `StatusIconState` in `NavoriPanel`, adding Core-down and Core-starting. The status item draws that value as a hand-drawn template image, with opacity motion from Core Animation and Samples only in debug builds.

**Tech Stack:** Rust 2024, tokio, serde, UniFFI, Swift (AppKit, Core Animation, Swift Testing). No new third-party dependency.

## Global Constraints

Source of truth (referenced, not copied — read before every task):

| Source | Content hash (`git hash-object`, first 12) |
|---|---|
| `docs/standards/INDEX.md` | `f83e09733ee4` |
| `docs/standards/testing.md` | `b1a9b42fc1d3` |
| `docs/standards/errors-logging.md` | `f84b3ceeea11` |
| `docs/standards/performance.md` | `ebf1670ff150` |
| `docs/architecture/INDEX.md` (ARCH-N) | `2e318bd23ff5` |

Compact delta every task inherits:

- **Locks** — `.skills/ICON/knowns.md`, `ui-brief.md` and `design.md` Decisions 1–10 are settled. The `## Not locked` and `## Must measure` lists in `requirements.md` are not facts.
- **ARCH-1** — no glyph is chosen in Swift. The Core row's lifecycle reading (Decision 5) is the one stated exception.
- **ARCH-2** — no new Swift file imports `NavoriCore`. Only `CoreStore.swift` and `NavoriBanner` import it, and that stays so. `StatusIconState` crosses into drawing and the Preview, never `TrayView`.
- **ARCH-3** — the composer knows no Module by name. `cargo xtask check-deps` is part of `cargo xtask ci`.
- **ARCH-16** — no `Timer`, `DispatchSourceTimer`, `CADisplayLink` or tokio interval for the icon.
- **`NavoriPanel`** — pure: no AppKit, no SwiftUI, no `NavoriCore`.
- **Testing** — test-first. Rust tests go in each crate's `tests/` through the public API, with real `Store::open_in_memory`. Test names are sentences in domain language, with no requirement IDs. The Swift view layer is verified by hand (`docs/standards/testing.md`, Exempt).
- **Lints** — no `unwrap`/`expect` outside tests. `missing_debug_implementations` is on and CI denies warnings: every new public type has `Debug`, by hand when it holds `dyn` fields.
- **Commands** — task gate: `cargo nextest run -p <crate> --test <file>`, or `cargo xtask build-core && swift test --arch arm64 --package-path apps/macos --filter <Suite>`. `swift test` builds the whole package, including the `Navori` target, and `apps/macos/Generated/` goes stale when the UniFFI surface changes, so `build-core` comes first. Before each commit: `cargo xtask ci`. In this shell `top` may be an alias, so use `/usr/bin/top`.
- **Commits** — Conventional Commits, English, one logical change. Commit only when the user asked for the build.
- **Team** — Solo band (`docs/agents/project.md`): one contributor; no assignee or reviewer theater.

## File Structure

Consulted: `docs/codebase/map.md`, for module and test placement; `docs/standards/` for Global Constraints. Nothing in them conflicts with the map below.

Retrieval (`load-subgraph`, schema 1.1, recipe `fsubr-1.2`, run once on this map; OWNS coverage 5/10): `blast_radius` → KCONTRACT on `crates/kernel/src/module.rs`, `runtime.rs`; AWAKE and KCONTRACT on `crates/ffi/src/app.rs`, `lib.rs`, `apps/cli/tests/cli.rs`; AWAKE on `modules/awake/src/lib.rs`; PERF, AWAKE, KCONTRACT and MENU on `CoreStore.swift`; PERF, AWAKE and MENU on `StatusBarController.swift`; MENU on `CoreRowState.swift`, `PreviewMenu.swift`; KCONTRACT, MENU, PROCS and AWAKE on `docs/ARCHITECTURE.md`. `cluster(ICON)` → focus only, because ICON had no OWNS before this file. Observations: none. CORE, SHELL, CLI and BUILD have no Files blocks, so this list does not prove nothing else owns these paths.

| Path | Kind | Responsibility |
|---|---|---|
| `crates/protocol/src/lib.rs` | changed | `TrayGlyph`, `TrayState`, `CoreStatus.tray` |
| `crates/kernel/src/tray.rs` | new | composes the icon state |
| `crates/kernel/src/module.rs` | changed | `TrayContribution`, `Module::tray` |
| `crates/kernel/src/runtime.rs` | changed | the per-Module tray closure, `Kernel::tray` |
| `crates/kernel/src/lib.rs` | changed | re-exports |
| `crates/kernel/tests/tray.rs` | new | composer behavior |
| `crates/ffi/src/app.rs` | changed | `Inner::status` fills `tray` |
| `crates/ffi/src/lib.rs` | changed | `TrayView`, `TrayGlyphView`, `CoreSnapshot.tray` |
| `crates/ffi/tests/core.rs` | changed | status and snapshot agree; health text guards |
| `crates/ffi/tests/config_privacy.rs` | changed | `tray` carries no raw value |
| `apps/cli/tests/cli.rs` | changed | `--json` has `tray`; plain output unchanged |
| `modules/awake/src/lib.rs` | changed | `Awake::tray` |
| `modules/awake/src/actor.rs` | changed | `SYSTEM_PROBLEM` becomes `pub(crate)` |
| `modules/awake/tests/tray.rs` | new | Awake's signal |
| `apps/macos/NavoriPanel/StatusIcon.swift` | new | `StatusIconState`, `StatusIconGlyph`, `StatusIconMotion` |
| `apps/macos/NavoriPanel/CoreRowState.swift` | changed | `CoreHealth` initializer from facts |
| `apps/macos/Tests/NavoriPanelTests/StatusIconTests.swift` | new | motion rules |
| `apps/macos/Tests/NavoriPanelTests/CoreRowStateTests.swift` | changed | Core row guards |
| `apps/macos/Navori/Core/CoreStore.swift` | changed | `statusIcon`; `coreStatus` from facts; `Health` deleted |
| `apps/macos/Navori/StatusBar/StatusIconImage.swift` | new | draws glyphs and the mark |
| `apps/macos/Navori/StatusBar/StatusBarController.swift` | changed | renders `StatusIconState`, plays motion, holds a Sample |
| `apps/macos/Navori/Features/Preview/PreviewIconSamples.swift` | new | Sample list and control, debug only |
| `apps/macos/Navori/Features/Preview/PreviewMenu.swift` | changed | shows the Sample control |
| `xtask/assets/make-icon.swift` | changed | Fold app icon |
| `xtask/assets/AppIcon.icns`, `xtask/assets/AppIconDev.icns` | changed | regenerated icons |
| `docs/ARCHITECTURE.md` | changed | `Module` code block and the `tray()` sentence |
| `docs/specs/2026-11-02-menubar-visual-foundation/requirements.md` | changed | strike MENU-G4 |
| `docs/specs/2026-11-02-status-icon-states/requirements.md` | changed | recorded Loop numbers |
| `docs/features/status-icon.md` | new | feature note, as `docs/features/awake.md` |

---

### Task 1: Compose the icon state from Module lifecycle and the config notice

**Files:**
- Create: `crates/kernel/src/tray.rs`, `crates/kernel/tests/tray.rs`
- Modify: `crates/protocol/src/lib.rs`, `crates/kernel/src/module.rs`, `crates/kernel/src/runtime.rs`, `crates/kernel/src/lib.rs`

**Reuse:** rung 2 — extends `Module` and the registration closure beside `StateFn` (type at `crates/kernel/src/runtime.rs:59`, built at `:175`); `Kernel::modules()` for lifecycle. New (rung 7): the composing function, because no existing code ranks Module signals.

**Interfaces:**
- Produces: in `navori_protocol`, `enum TrayGlyph { Normal, Lease, AiRunning, Problem }` with `#[serde(rename_all = "snake_case")]`, and `struct TrayState { glyph: TrayGlyph, attention: bool, summary: String }`, both `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`. In `navori_kernel`, re-exported with those two: `#[derive(Debug, Clone, Default, PartialEq, Eq)] struct TrayContribution { pub glyph: Option<(TrayGlyph, String)>, pub attention: Option<String> }`; `pub fn compose_tray(entries: &[(ModuleInfo, TrayContribution)], notice: Option<(&str, &str)>) -> TrayState`; `Kernel::tray(&self, notice: Option<(&str, &str)>) -> TrayState`, which passes default contributions until Task 2.

**Depends-on:** none

- [ ] **Step 1: Write failing tests** in `crates/kernel/tests/tray.rs`. Table-driven through `compose_tray` with literal `ModuleInfo` values: `a_healthy_core_is_normal_and_says_navori`; `a_failed_module_shows_the_problem_glyph_and_its_sentence`; `a_restarting_module_shows_the_problem_glyph_and_its_sentence`; `the_first_failed_module_wins_over_a_restarting_one`; `a_failed_module_with_no_error_text_says_unknown_error`; `a_starting_or_stopped_module_stays_normal`; `a_config_notice_alone_stays_normal_and_joins_the_summary` with title `config.toml has bad values`; `a_failed_module_comes_before_the_config_notice`; `a_notice_keeps_its_title_and_message_unchanged`. Through a real Kernel, with a Module copied from the `Counter` pattern in `crates/kernel/tests/kernel.rs`: `a_module_that_gives_up_shows_the_problem_glyph`; `a_module_with_no_tray_builds_and_runs_unchanged`.
- [ ] **Step 2: Run** `cargo nextest run -p navori-kernel --test tray` — expect: does not compile, `Kernel::tray` is missing.
- [ ] **Step 3: Implement** the types, `compose_tray` in `tray.rs` from the pseudocode in `design.md`, and `Kernel::tray`. The Running filter lives in `compose_tray`. Summary pieces: each but the last loses a trailing `.`, then they join with `. `.
- [ ] **Step 4: Verify** `cargo nextest run -p navori-kernel` — expect: pass, including the existing restart test in `crates/kernel/tests/kernel.rs`, which guards restart with State and queued requests. `cargo xtask ci` — expect: pass.
- [ ] **Step 5: Commit** `feat(kernel): compose the icon state from module lifecycle`.

_Requirements: ICON-2.2, ICON-2.3, ICON-2.4, ICON-2.6, ICON-3.3, ICON-6.3, ICON-6.4, ICON-6.7, ICON-10.1, ICON-10.2_

---

### Task 2: Let a Module add a signal with `tray()`

**Files:**
- Modify: `crates/kernel/src/module.rs`, `crates/kernel/src/runtime.rs`, `crates/kernel/src/tray.rs`, `crates/kernel/src/lib.rs`
- Test: `crates/kernel/tests/tray.rs`

**Reuse:** rung 2 — extends `Module` and the registration closure beside `StateFn` (type at `crates/kernel/src/runtime.rs:59`, built at `:175`); `Kernel::modules()` for lifecycle. New (rung 7): the composing function, because no existing code ranks Module signals.

**Interfaces:**
- Consumes: `TrayGlyph`, `TrayState`, `TrayContribution`, `compose_tray`, `Kernel::tray` (Task 1)
- Produces: `fn tray(_state: &Self::State) -> TrayContribution` on `Module`, defaulting to `TrayContribution::default()`.

**Depends-on:** Task 1

- [ ] **Step 1: Write failing tests.** Through `compose_tray`: `problem_beats_lease_beats_ai_running_beats_normal`; `a_stopped_or_failed_module_is_ignored_even_with_a_lease_contribution`; `a_failed_module_comes_before_a_problem_a_module_reports`; `attention_turns_on_and_keeps_the_glyph`; `a_problem_and_attention_both_stay_in_the_state`; `the_summary_is_glyph_sentence_then_decision_then_config`; `a_sentence_ending_in_a_period_joins_without_a_double_period`. Through a real Kernel with test Modules: `a_module_adds_a_glyph_with_no_edit_to_the_composer`; `a_panicking_tray_reports_nothing_and_is_called_only_once`, counting calls with a `static AtomicUsize` because `tray` has no `self`.
- [ ] **Step 2: Run** `cargo nextest run -p navori-kernel --test tray` — expect: does not compile, `Module::tray` is missing.
- [ ] **Step 3: Implement** `Module::tray` with its doc comment stating both contract rules: report the problem glyph only when the Module's main function is failing; attention means Needs you holds a Decision, and To-dos, Updates, pull request counts, failing checks and port counts do not turn it on. At registration, build a `Box<dyn Fn() -> TrayContribution + Send + Sync>` from a clone of `state_rx`: clone the State out of the borrow, call `M::tray` inside `catch_unwind(AssertUnwindSafe(...))`, and after a panic set an `AtomicBool`, log one `tracing::warn!` and return the default from then on. `Kernel::tray` passes contributions from `ModuleState::Running` Modules only, in registration order.
- [ ] **Step 4: Verify** the same command — expect: pass. `cargo xtask ci` — expect: pass.
- [ ] **Step 5: Commit** `feat(kernel): let a module add a signal to the icon state`.

_Requirements: ICON-2.1, ICON-2.5, ICON-3.1, ICON-3.2, ICON-3.4, ICON-5.2, ICON-5.3, ICON-5.4, ICON-6.1, ICON-6.2_

---

### Task 3: Carry the icon state to the CLI and the snapshot

**Files:**
- Modify: `crates/protocol/src/lib.rs`, `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs`
- Test: `crates/ffi/tests/core.rs`, `crates/ffi/tests/config_privacy.rs`, `apps/cli/tests/cli.rs`

**Reuse:** rung 2 — `Inner::status()` (`crates/ffi/src/app.rs:37`) and `snapshot()` (`crates/ffi/src/lib.rs:385`), which already moves fields out of `status`.

**Interfaces:**
- Consumes: `Kernel::tray`, `TrayState`, `TrayGlyph` (Task 1)
- Produces: `CoreStatus.tray: TrayState`; `CoreSnapshot.tray: TrayView`; `#[derive(Debug, Clone, PartialEq, uniffi::Record)] TrayView { glyph: TrayGlyphView, attention: bool, summary: String }`; `#[derive(Debug, Clone, PartialEq, uniffi::Enum)] TrayGlyphView { Normal, Lease, AiRunning, Problem }`, mapped by a destructuring `From` with no `..`.

**Depends-on:** Task 1

- [ ] **Step 1: Write failing tests.** In `apps/cli/tests/cli.rs`: `status_json_carries_the_icon_state`, asserting `glyph` is `normal`, `attention` is false and `summary` is `Navori`, and that `pid`, `version`, `profile`, `data_dir`, `socket_path` and `modules` stay; `status_plain_output_has_no_icon_line`. In `crates/ffi/tests/core.rs`: `the_snapshot_and_the_status_carry_the_same_icon_state`; `a_bad_value_is_named_in_the_icon_summary` with `log_level`; `a_whole_file_fallback_says_so_in_the_icon_summary`. In `crates/ffi/tests/config_privacy.rs`: extend `surface()` with `snapshot.tray.summary`.
- [ ] **Step 2: Run** `cargo nextest run -p navori-ffi --test core` — expect: does not compile, `tray` is missing.
- [ ] **Step 3: Implement.** `Inner::status()` passes `config_notice`'s `title` and `message` to `self.kernel.tray`. `snapshot()` moves `tray` from the status it read, with no second composition. `apps/cli/src/main.rs` is not edited.
- [ ] **Step 4: Verify** `cargo nextest run -p navori-ffi --test core`, `cargo nextest run -p navori-ffi --test config_privacy`, `cargo nextest run -p navori-cli --test cli` — expect: pass, including `snapshot_pushes_coalesce_at_33ms` and `a_secret_in_a_bad_value_reaches_no_report` unchanged. `cargo xtask ci` — expect: pass.
- [ ] **Step 5: Commit** `feat(ffi): carry the icon state to the cli and the snapshot`.

_Requirements: ICON-1.1, ICON-1.2, ICON-1.3, ICON-1.4, ICON-6.6, ICON-10.3, ICON-10.4, ICON-11.5_

---

### Task 4: Show a real Lease in the icon state

**Files:**
- Create: `modules/awake/tests/tray.rs`
- Modify: `modules/awake/src/lib.rs`, `modules/awake/src/actor.rs`

**Reuse:** rung 2 — `AwakeState` (`modules/awake/src/state.rs:4`) and `SYSTEM_PROBLEM` (`modules/awake/src/actor.rs:21`).

**Interfaces:**
- Consumes: `TrayContribution`, `TrayGlyph` through `navori_kernel` (Task 2); `Rig` from `modules/awake/tests/common/`
- Produces: `impl Module for Awake { fn tray(state: &AwakeState) -> TrayContribution }`. Sentences: `Keeping this Mac awake`; `Can't hold this Mac awake.` followed by `SYSTEM_PROBLEM`.

**Depends-on:** Task 2

- [ ] **Step 1: Write failing tests** in `modules/awake/tests/tray.rs`: `no_lease_reports_nothing`; `a_held_lease_reports_the_lease_glyph`; `a_display_lease_counts_as_a_lease`; `a_refused_lease_reports_the_problem_glyph_and_says_it_cannot_hold`; `a_refused_lease_never_says_the_mac_is_held`; `other_problems_do_not_report_the_problem_glyph` (table-driven over `display`, `release`, `store`, `platform`, `exits` texts, with `system_held` true); `a_refusal_through_the_kernel_reports_no_assertion_and_the_problem_glyph`, using the `refusal.rs` setup and `Kernel::tray`.
- [ ] **Step 2: Run** `cargo nextest run -p navori-mod-awake --test tray` — expect: failures, the default reports nothing.
- [ ] **Step 3: Implement** `Awake::tray` reading only `leases` and `system_held`.
- [ ] **Step 4: Verify** the same command — expect: pass. `cargo xtask ci` — expect: pass.
- [ ] **Step 5: Commit** `feat(awake): show a held or refused lease in the icon state`.

_Requirements: ICON-4.1, ICON-4.2, ICON-4.3, ICON-4.4, ICON-4.5, ICON-10.5_

---

### Task 5: Give the Shell one value, and keep the Core row correct

**Files:**
- Create: `apps/macos/NavoriPanel/StatusIcon.swift`
- Modify: `apps/macos/NavoriPanel/CoreRowState.swift`, `apps/macos/Navori/Core/CoreStore.swift`
- Test: `apps/macos/Tests/NavoriPanelTests/CoreRowStateTests.swift`, `apps/macos/Tests/NavoriPanelTests/StatusIconTests.swift`

**Reuse:** rung 2 — `CoreStatus` and `CoreHealth` in `apps/macos/NavoriPanel/CoreRowState.swift:5,9`, and `configProblem` for the banner, which does not change.

**Interfaces:**
- Consumes: `snapshot.tray` (Task 3), generated `TrayGlyphView` cases `.normal`, `.lease`, `.aiRunning`, `.problem`
- Produces: `public enum StatusIconGlyph: Equatable, Sendable { case normal, lease, aiRunning, problem, coreDown, starting }`; `public struct StatusIconState: Equatable, Sendable { public let glyph: StatusIconGlyph; public let attention: Bool; public let description: String; public init(glyph:attention:description:) }`; `extension StatusIconState { public init(coreFailed: Bool, core: StatusIconState?) }`; `extension CoreHealth { public init(coreFailed: Bool, hasSnapshot: Bool, moduleStates: [String], hasConfigNotice: Bool) }`; `CoreStore.statusIcon: StatusIconState`.

**Depends-on:** Task 3

- [ ] **Step 1: Write failing tests** in `CoreRowStateTests.swift`: `a_config_notice_alone_needs_attention`; `a_refused_lease_with_every_module_running_stays_running`; `a_failed_or_restarting_module_needs_attention`; `a_starting_or_stopped_module_stays_running`; `a_failed_core_is_not_running`; `no_snapshot_is_starting`. In `StatusIconTests.swift`: `a_failed_core_is_core_down_whatever_the_core_said`; `no_core_state_yet_is_starting`; `a_core_state_is_used_unchanged`.
- [ ] **Step 2: Run** `cargo xtask build-core && swift test --arch arm64 --package-path apps/macos --filter 'CoreRowStateTests|StatusIconTests'` — expect: does not compile, the initializers are missing.
- [ ] **Step 3: Implement.** The initializer counts only `failed` and `restarting` as degraded. `coreStatus` reads it. The new `StatusIconState` initializer returns `coreDown` with `Core not running` when the Core failed, `starting` with `Starting` when `core` is nil, and otherwise `core`. `statusIcon` calls it with a mapping of `snapshot?.tray`, which is the only reader of `tray`. `health` stays until Task 6.
- [ ] **Step 4: Verify** the same command — expect: pass. `cargo xtask ci` — expect: pass. `grep -rln 'import NavoriCore' apps/macos/Navori apps/macos/NavoriPanel apps/macos/NavoriBanner` — expect: only `CoreStore.swift` and `CoreStartupBanner.swift`.
- [ ] **Step 5: Check by hand** with `cargo xtask run`: a bad value in `config.toml` shows the banner and `Needs attention` on the Core row, as it does today.
- [ ] **Step 6: Commit** `refactor(shell): give the status item one plain value`.

_Requirements: ICON-7.2, ICON-10.6, ICON-10.7, ICON-10.8, ICON-10.9_

---

### Task 6: Draw the locked glyphs on the status item

**Files:**
- Create: `apps/macos/Navori/StatusBar/StatusIconImage.swift`
- Modify: `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/Navori/Core/CoreStore.swift`

**Reuse:** rung 2 — `StatusBarController.render` and its observation (`:62-90`), and the Reduce Motion observer in `GlassSurfaceView.swift:36-46`; rung 4 — `NSImage` template images and Core Animation. New (rung 7): the glyph drawing, because SF Symbols have no folded N.

**Interfaces:**
- Consumes: `StatusIconState`, `CoreStore.statusIcon` (Task 5)
- Produces: `enum StatusIconImage { static func image(_ state: StatusIconState) -> NSImage }`, returning a copy of a per-glyph-and-attention cached template image with `state.description` as its `accessibilityDescription`; `StatusBarController.render(_ state: StatusIconState)`.

**Depends-on:** Task 5

- [ ] **Step 1: Implement** `StatusIconImage` from the coordinates in `ui-brief.md`, with a flipped 18 pt `NSImage`, `isTemplate = true` and the mark's clear-then-fill. Replace `observeHealth` and `render(health:)`; delete `CoreStore.Health` and `health`. `appearsDisabled` is true only for `.starting`. Keep the `statusItemDrawn` block and `didScheduleStatusMark` as they are.
- [ ] **Step 2: Verify** `cargo xtask ci` — expect: pass. `grep -rn 'store.health\|CoreStore.Health' apps/macos` — expect: nothing.
- [ ] **Step 3: Check by hand** with `cargo xtask run` in a dark menu bar. The icon shows the normal N. `"build/dev/Navori Dev.app/Contents/Helpers/navori" --profile dev awake hold --for 1h` shows the tile, and `awake release-all` returns the N. Left click toggles the panel and a second click closes it; right click and control-click show the native menu with `Open`, `Settings…`, `Open Logs Folder` and `Quit`; `Open Logs Folder` opens the folder; the tooltip is the app name; the Core submenu appears when the first snapshot arrives with the panel open. Put a garbage file at `<temp home>/Library/Application Support/Navori Dev/navori.db` and launch the binary directly under `NAVORI_PERF=1 NAVORI_PERF_HOME=<temp home>`: the icon shows the folded ×. During launch it shows the dim outline N. VoiceOver reads the summary, including a bad config value.
- [ ] **Step 4: Run** `cargo xtask perf` — expect: pass, with `statusItemDrawn` logged once per launch and `SIGUSR1` and `SIGUSR2` still opening the real menu and Core Submenu.
- [ ] **Step 5: Commit** `feat(shell): draw the fold glyphs on the status item`.

_Requirements: ICON-5.1, ICON-5.5, ICON-6.5, ICON-7.1, ICON-7.3, ICON-7.4, ICON-7.7, ICON-7.8, ICON-7.9, ICON-10.10, ICON-10.12, ICON-10.13, ICON-10.15, ICON-10.16, ICON-10.17_

---

### Task 7: Move the icon only when it means something

**Files:**
- Modify: `apps/macos/NavoriPanel/StatusIcon.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`
- Test: `apps/macos/Tests/NavoriPanelTests/StatusIconTests.swift`

**Reuse:** rung 2 — `StatusBarController.render` and its observation (`:62-90`), and the Reduce Motion observer in `GlassSurfaceView.swift:36-46`; rung 4 — `NSImage` template images and Core Animation. New (rung 7): the glyph drawing, because SF Symbols have no folded N.

**Interfaces:**
- Consumes: `StatusIconState`, `render` (Tasks 5, 6)
- Produces: `public struct StatusIconMotion { public struct Plan: Equatable, Sendable { public let loop: Bool; public let beat: Bool }; public init(); public mutating func next(_ state: StatusIconState, reduceMotion: Bool, allowBeat: Bool) -> Plan }`.

**Depends-on:** Task 6

- [ ] **Step 1: Write failing tests** in `StatusIconTests.swift`: `the_loop_plays_only_on_the_ai_glyph` (table-driven over the other five glyphs); `attention_turning_on_plays_one_beat`; `the_first_snapshot_with_attention_on_plays_no_beat`; `no_beat_when_the_previous_state_was_starting_or_core_down`; `attention_staying_on_plays_no_more_beats`; `attention_turning_on_again_after_off_plays_a_beat`; `reduce_motion_plays_no_loop_and_no_beat`; `a_beat_during_the_ai_loop_keeps_the_loop_in_the_plan`; `allow_beat_false_plays_no_beat`.
- [ ] **Step 2: Run** `cargo xtask build-core && swift test --arch arm64 --package-path apps/macos --filter StatusIconTests` — expect: does not compile, `StatusIconMotion` is missing.
- [ ] **Step 3: Implement** the reducer. In `render`, return early when state and `plan.loop` are unchanged. Set `wantsLayer` only when motion first plays. Add `CABasicAnimation`s for `opacity` on `button.layer` under `icon.loop` (1 to 0.35, 1 s, autoreverses, repeating; only if the layer has no `icon.loop`) and `icon.beat` (1 to 0.2 and back, 0.5 s in total), and remove `icon.loop` when the plan stops it. Observe `accessibilityDisplayOptionsDidChangeNotification` on `NSWorkspace.shared.notificationCenter` as `GlassSurfaceView.swift` does, and re-render.
- [ ] **Step 4: Verify** the same command — expect: pass. `cargo xtask ci` — expect: pass. `grep -n 'Timer\|DispatchSourceTimer\|CADisplayLink' apps/macos/Navori/StatusBar/StatusIconImage.swift apps/macos/Navori/StatusBar/StatusBarController.swift apps/macos/NavoriPanel/StatusIcon.swift` — expect: nothing.
- [ ] **Step 5: Commit** `feat(shell): play the loop and the beat on the status item`.

_Requirements: ICON-8.1, ICON-8.2, ICON-8.3, ICON-8.4, ICON-8.5, ICON-8.6, ICON-8.7, ICON-11.4_

---

### Task 8: Judge every state with Samples

**Files:**
- Create: `apps/macos/Navori/Features/Preview/PreviewIconSamples.swift`
- Modify: `apps/macos/Navori/Features/Preview/PreviewMenu.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`

**Reuse:** rung 2 — `PreviewMenu.content` and `PreviewRootView`, the tint row's pattern, and `openPreview` (`StatusBarController.swift:243`). The existing right-click `Open Menu Preview` item stays the entry.

**Interfaces:**
- Consumes: `StatusIconState`, `StatusIconMotion`, `render` (Tasks 5–7)
- Produces: under `#if DEBUG`: `StatusBarController.setIconSample(_ sample: StatusIconState?)`, `StatusBarController.playIconBeat()`; `PreviewMenu.content(setTint:setIconSample:playBeat:)`; `PreviewIconSamples.marker` = `NAVORI-ICON-SAMPLE`.

**Depends-on:** Task 7

- [ ] **Step 1: Implement** a row in the Preview offering each glyph with attention off and on, Core-down, Core-starting, `Play beat` and `Turn off`, with descriptions from Decision 8. The controller holds the Sample and `render` draws `sample ?? store.statusIcon`. Choosing or clearing calls `next` with `allowBeat: false`. Everything is in `#if DEBUG`. The Sample row dispatches no Action.
- [ ] **Step 2: Verify** `cargo xtask ci` — expect: pass.
- [ ] **Step 3: Check by hand** with `cargo xtask run`: every Sample draws on the real status item; the AI Sample loops; `Play beat` plays; a Sample stays after a click closes the Preview; `Turn off` returns to the real state with no Beat; the right-click menu keeps its four items, with `Open Menu Preview` only here.
- [ ] **Step 4: Check the binary:** `swift build --arch arm64 --package-path apps/macos -c release`, then `strings "$(swift build --arch arm64 --package-path apps/macos -c release --show-bin-path)/Navori" | grep -c NAVORI-ICON-SAMPLE` — expect: 0. `swift build --arch arm64 --package-path apps/macos`, then the same `strings` line with `-c release` dropped — expect: at least 1.
- [ ] **Step 5: Commit** `feat(shell): add icon samples to the dev preview`.

_Requirements: ICON-9.1, ICON-9.2, ICON-9.3, ICON-9.4, ICON-9.5, ICON-9.6, ICON-10.11, ICON-10.14_

---

### Task 9: Draw the new logo

**Files:**
- Modify: `xtask/assets/make-icon.swift`, `xtask/assets/AppIcon.icns`, `xtask/assets/AppIconDev.icns`

**Reuse:** rung 2 — `xtask/assets/make-icon.swift` and `cargo xtask icon` (`xtask/src/main.rs:366`).

**Interfaces:**
- Consumes: the logo artwork in `ui-brief.md`
- Produces: regenerated `AppIcon.icns` and `AppIconDev.icns`; `cargo xtask icon` unchanged.

**Depends-on:** none

- [ ] **Step 1: Implement.** Remove `sparkle` and its two calls. Draw the three planes from `ui-brief.md` in the 824 px body: a brief unit is 8.24 px from the body's origin at (100, 100), with the y axis flipped. Release: the brief's gradient and colors, and rework the three effects written for a dark body: the white top highlight, the white hairline and the white glow behind the old sparkle. Dev: today's slate-blue body and effects, with the N in light tones.
- [ ] **Step 2: Regenerate and check by hand.** `cargo xtask icon`, then `sips -s format png -Z 512 xtask/assets/AppIcon.icns --out /tmp/navori-icon-release.png` and the same for `AppIconDev.icns`. View both, and the small sizes in Finder; release and dev are clearly different.
- [ ] **Step 3: Verify** `cargo xtask ci` — expect: pass. `cargo xtask run` — expect: the dev app shows the dev icon in Finder and the Dock.
- [ ] **Step 4: Commit** `feat(build): draw the fold logo as the app icon`.

_Requirements: ICON-12.1, ICON-12.2, ICON-12.3_

---

### Task 10: Measure, walk and record

**Files:**
- Modify: `docs/ARCHITECTURE.md`, `docs/specs/2026-11-02-menubar-visual-foundation/requirements.md`, `docs/specs/2026-11-02-status-icon-states/requirements.md`
- Create: `docs/features/status-icon.md`

**Reuse:** rung 2 — `docs/features/awake.md` as the note's pattern; `cargo xtask perf`.

**Interfaces:**
- Consumes: everything above
- Produces: Loop numbers in `requirements.md`; MENU-G4 struck as replaced by ICON.

**Depends-on:** Tasks 1–9

- [ ] **Step 1: Run** `cargo xtask perf` — expect: pass, with no metric, phase or signal added.
- [ ] **Step 2: Measure the Loop by hand.** On a debug build with the AI Sample on and the panel closed, and again with the static normal Sample: `/usr/bin/top -l 6 -s 12 -stats cpu -pid <Navori Dev pid>`, dropping the first sample, which is cumulative, and the same for WindowServer. Take each median of the 5 windows. Record Navori's numbers and WindowServer's in `requirements.md` under ICON-11.2, with the machine and its load. No wake-up count is offered as evidence. If Navori rounds above 0.0, stop and tell the user: this reopens the Loop.
- [ ] **Step 3: Walk every Sample by hand** in dark and light menu bars, with Reduce Motion on and off and with VoiceOver: each state reads in shape with motion off; the menu-open highlight and `appearsDisabled` look right once the button has a layer; a Beat during the Loop plays and the Loop carries on. Record anything that fails, and stop if a reopen condition in `requirements.md` is met.
- [ ] **Step 4: Update docs:** `tray()` in the `Module` block and the sentence in `docs/ARCHITECTURE.md`, with `context()` left in MILE-3; strike MENU-G4 in MENU's `requirements.md`; write `docs/features/status-icon.md`.
- [ ] **Step 5: Verify** `cargo xtask ci` — expect: pass.
- [ ] **Step 6: Commit** `docs(ICON): record the loop cost and the delivered icon`.

_Requirements: ICON-11.1, ICON-11.2, ICON-11.3, ICON-11.6_
