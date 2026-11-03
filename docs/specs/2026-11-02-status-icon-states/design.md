# Design: Status icon states

Feature code: ICON
Status: Approved
Date: 2026-11-03
Requirements: ./requirements.md

## Context

The status item draws one of four SF Symbols from `CoreStore.health` (`apps/macos/Navori/Core/CoreStore.swift:217`), a Swift getter that orders failed, restarting and config and writes their sentences. `navori status` sees none of it. MENU's Core row reads the same getter through `coreStatus`, and AWAKE left it alone (`docs/specs/2026-11-02-awake-leases/design.md:221`). The Module trait has no way to contribute a signal; `docs/ARCHITECTURE.md:107` leaves `tray()` as a MILE-3 obligation.

The binding constraint is one result with no idle cost: the icon, the snapshot and the CLI must read one composition (ICON-1.3), and it must cost nothing while nothing changes (GOAL-11, ARCH-16). Module State is already behind a `watch` channel and every `update_state` bumps the revision that wakes the snapshot pump (`crates/kernel/src/module.rs:138`). So composition belongs in the Kernel, computed when the status is read, with no task or timer of its own. That rules out a dedicated tray task that listens and caches, and it rules out composing in `snapshot()`, which the CLI never calls.

A second constraint is what cannot come from Rust. The Core does not exist in Core-down and has not spoken in Core-starting, so the Shell keeps those two. It also keeps a small lifecycle reading for the Core row: ICON-10.6 needs `Needs attention` on a config notice and ICON-10.7 needs `Running` while Awake cannot hold, so that row cannot be derived from the glyph.

Fresh retrieval (`fsubr-1.2`, OWNS coverage 5/10; CORE, SHELL, CLI and BUILD have no `tasks.md`, and ICON has none yet, so this is not exhaustive): AWAKE and KCONTRACT on `crates/ffi/src/app.rs` and `lib.rs`; AWAKE on `modules/awake/src/lib.rs`; KCONTRACT on `crates/kernel/src/module.rs` and `runtime.rs`; MENU, AWAKE and PERF on `CoreStore.swift` and `StatusBarController.swift`; MENU on `CoreRowState.swift` and `Features/Preview/`. Observations: none. Digest: `.skills/ICON/scan-design.md`.

Spine relied on: ARCH-1, ARCH-2, ARCH-3, ARCH-10, ARCH-16.

## Decisions

Locks from the interview are in `.skills/ICON/knowns.md` and the look is locked in `./ui-brief.md`; neither is repeated. These are new, each **proposed for approval**, with the Open Question it closes:

1. **`tray` reads State alone.** `fn tray(state: &Self::State) -> TrayContribution`, an associated function with a default body. AwakeState already carries `leases` and `system_held`, so the reopen condition on a Lease that State cannot express does not fire. Rejected: `ctx.set_tray(...)`, a second publication that could disagree with State. Closes the `tray()` signature question.
2. **A glyph always brings its sentence.** `TrayContribution { glyph: Option<(TrayGlyph, String)>, attention: Option<String> }`. Attention is on when the clause is present. Rejected: three independent optionals, where a glyph could arrive with nothing to say. `TrayState.attention` stays a `bool`, so no separate `reason` field crosses the boundary.
3. **The composer lives in `crates/kernel/src/tray.rs`.** `Kernel::tray(notice)` takes the notice's title and message as plain strings, because `ConfigNotice` is an `navori-ffi` type and the Kernel cannot name it. This confirms the shape left open: `Inner::status()` calls it once.
4. **`tray()` runs on a clone of State, inside `catch_unwind` with `AssertUnwindSafe`, and a panic switches it off.** The composer holds no channel lock while Module code runs. After a panic that Module's closure reports nothing for the rest of the process and logs one warning, because the panic hook (`crates/ffi/src/session.rs:46`) writes to the session marker on every panic, caught or not, and a repeating panic would flood it. So a bad `tray()` cannot break the status path.
5. **The Core row reads facts, not the glyph.** A pure initializer in `NavoriPanel` takes "Core failed", "has snapshot", module state names and "has config notice". `CoreStore.Health` and its sentences are deleted. This closes the Core row question. The order failed-or-restarting-then-config stays visible in Swift for this one row, a small ARCH-1 cost that today's code already carries.
6. **The Shell's value is `StatusIconState` in `NavoriPanel`.** It has six glyphs, the four from the Kernel plus Core-down and Core-starting, so that `render` has one input. `CoreStore` is the only mapper. Motion decisions live beside it as a pure reducer, so `swift test` covers them.
7. **The status button becomes layer-backed only when motion first plays.** The spike ran a layer-backed button, but nothing has measured what that does to idle. Static icons keep today's path, so the existing perf gate still measures what it measures.
8. **Words, proposed for approval:** normal `Navori`; Lease `Keeping this Mac awake`; Awake's problem `Can't hold this Mac awake. macOS refused the request. Navori keeps trying.`, the menu's wording; the two Sample-only sentences `Navori AI is running` and `A decision is waiting`, which ROAD-14 and ROAD-13 may change. Awake's sentence is built from the crate's existing `SYSTEM_PROBLEM` constant. Each piece but the last loses a trailing `.`, then pieces join with `. `. Closes the wording question.
9. **Names, proposed for approval:** `TrayState`, `TrayGlyph` with JSON values `normal`, `lease`, `ai_running`, `problem`, `TrayContribution`, `StatusIconState`, `StatusIconGlyph`, `StatusIconMotion`. Closes the names question.
10. **Dev app icon, proposed for approval:** the N in light tones on today's dev slate-blue body, so that the dev install stays obviously different (ICON-12.2). Closes the dev icon question.

No ADR: each of these is reversible with a local edit.

## Architecture

### Icon state and composer

Satisfies: ICON-1.1, ICON-2.1, ICON-2.2, ICON-2.3, ICON-2.4, ICON-2.5, ICON-2.6, ICON-3.1, ICON-3.2, ICON-3.3, ICON-3.4, ICON-5.2, ICON-5.3, ICON-5.4, ICON-6.1, ICON-6.2, ICON-6.3, ICON-6.4, ICON-6.7, ICON-10.1, ICON-10.2
Reuse: rung 2 — extends `Module` and the registration closure beside `StateFn` (type at `crates/kernel/src/runtime.rs:59`, built at `:175`); `Kernel::modules()` for lifecycle. New (rung 7): the composing function, because no existing code ranks Module signals.
Respects: ARCH-1, ARCH-3, ARCH-10, ARCH-16
Surface:
- `impl Module for` in `crates/kernel/src/system.rs:94`, `modules/awake/src/lib.rs:22`, `crates/kernel/tests/scheduler.rs:132,198`, `crates/kernel/tests/kernel.rs:75,472,500,543,619` — `frozen`: built around; the default body means none of them changes.
- `Registered` in `crates/kernel/src/runtime.rs:73` — `replace`: gains a `tray` closure. It has no external readers.
- `docs/ARCHITECTURE.md:107`, which puts `tray()` in MILE-3 — `replace`: `tray()` moves out of that sentence and `context()` stays.
- The `Module` trait code block at `docs/ARCHITECTURE.md:74-84` — `replace`: gains `tray`.
Interface:
```rust
// crates/protocol
pub enum TrayGlyph { Normal, Lease, AiRunning, Problem }   // serde snake_case
pub struct TrayState { pub glyph: TrayGlyph, pub attention: bool, pub summary: String }
// crates/kernel, re-exported with the two above
pub struct TrayContribution { pub glyph: Option<(TrayGlyph, String)>, pub attention: Option<String> }
trait Module { fn tray(_state: &Self::State) -> TrayContribution { TrayContribution::default() } }
pub fn compose_tray(entries: &[(ModuleInfo, TrayContribution)], notice: Option<(&str, &str)>) -> TrayState
impl Kernel { pub fn tray(&self, notice: Option<(&str, &str)>) -> TrayState }
```
Depth: If the composer vanished, callers would still know only "ask for the state, passing the notice" (`compose_tray` is public so tests can reach every lifecycle state with literal values); the ranking, the Running filter, the sentence order and the joining are hidden.
Locality: new `crates/kernel/src/tray.rs`; extend `module.rs`, `runtime.rs`, `lib.rs`, `crates/protocol/src/lib.rs`. leave: supervisor and scheduler.

Registration builds `tray: Box<dyn Fn() -> TrayContribution + Send + Sync>` from a clone of `state_rx`. It clones the State out of the borrow, then calls `M::tray` as Decision 4 says. The composer:

```text
for each registered Module, in registration order:
  keep its contribution only if its lifecycle is Running          // ICON-2.5, ICON-2.6
lifecycle sentence = first Failed `<id> stopped: <last_error or "unknown error">`,
                     else first Restarting `<id> is restarting`, else none
problem sentences  = lifecycle sentence, then each contributed Problem sentence
glyph, sentence    = Problem and problem sentences if any exist
                     else first contributed Lease, else first AiRunning, else Normal and "Navori"
attention          = any contribution has a clause; clause = the first
summary            = [sentence, clause, "<title>. <message>"], present pieces only,
                     each but the last without a trailing ".", joined with ". "
```

A config notice is only ever a summary piece, which is what keeps it out of the glyph (ICON-2.3) and after the failed or restarting sentence (ICON-6.7). The doc comment on `tray` states the two contract rules (ICON-3.4, ICON-5.4).

### Status and snapshot path

Satisfies: ICON-1.2, ICON-1.3, ICON-1.4, ICON-6.6, ICON-10.3, ICON-10.4, ICON-11.5
Reuse: rung 2 — `Inner::status()` (`crates/ffi/src/app.rs:37`) and `snapshot()` (`crates/ffi/src/lib.rs:385`), which already moves fields out of `status`.
Respects: ARCH-1, ARCH-2
Security: TB-2
Surface:
- `CoreStatus` construction at `crates/ffi/src/app.rs:38` — `replace`: fills `tray`.
- `CoreStatus` decoders at `apps/cli/src/main.rs:178`, `crates/ffi/tests/core.rs:153` — `replace`: the field is required. A newer CLI cannot decode an older running app; Compat obligation is None and both ship in one bundle.
- `print_status` at `apps/cli/src/main.rs:442` — `frozen`: built around; it does not read the field, so its output does not change.
- `core.status` over IPC, read by `apps/cli/tests/cli.rs:82` — `frozen`: built around; the existing keys stay and one key is added.
- `CoreSnapshot` at `crates/ffi/src/lib.rs:109,385` and its readers in `CoreStore.swift` and `crates/ffi/tests/` — `replace`: gains `tray: TrayView`; other fields stay.
- `surface()` in `crates/ffi/tests/config_privacy.rs:43` — `replace`: also concatenates `tray.summary`.
Interface: `CoreStatus.tray: TrayState`; `CoreSnapshot.tray: TrayView { glyph: TrayGlyphView, attention, summary }`, a `uniffi::Record` and `uniffi::Enum` mapped by a destructuring `From` with no `..`, as `AwakeView` does.
Depth: n/a — extends `Inner::status` and `snapshot`.
Locality: `crates/protocol/src/lib.rs`, `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs`. leave: the snapshot pump, whose coalescing does not change, and `apps/cli/src/main.rs`, which needs no edit.

`Inner::status()` passes `self.config_notice` to `self.kernel.tray(...)`. The notice text holds key names and fixed words, never values (`crates/ffi/src/app.rs:168`), so `tray` carries no raw value; the extended privacy test proves it.

### Awake's signal

Satisfies: ICON-4.1, ICON-4.2, ICON-4.3, ICON-4.4, ICON-4.5, ICON-10.5
Reuse: rung 2 — `AwakeState` (`modules/awake/src/state.rs:4`) and `SYSTEM_PROBLEM` (`modules/awake/src/actor.rs:21`). It publishes only after a reconcile (`modules/awake/src/actor.rs:514,550`), so `system_held` is not transiently false on a hold that succeeds.
Respects: ARCH-3
Surface:
- `awake_view` and `awake_check` in `crates/ffi/src/app.rs` — `frozen`: built around; they read State as before.
Interface: `impl Module for Awake { fn tray(state: &AwakeState) -> TrayContribution }`.
Depth: n/a — extends `Module`.
Locality: `modules/awake/src/lib.rs`, `extend`; `actor.rs`, the constant becomes `pub(crate)`. leave: the actor's logic.

No Lease reports nothing. A Lease with `system_held` true reports the Lease glyph. A Lease with `system_held` false reports the problem glyph. `problem` is not read, so other problem sources cannot turn it on. A failed or stopped Awake keeps its last State, and the composer's Running filter ignores it (ICON-2.5).

### Shell value and Core row

Satisfies: ICON-7.1, ICON-7.2, ICON-10.6, ICON-10.7, ICON-10.8, ICON-10.9
Reuse: rung 2 — `CoreStatus` and `CoreHealth` in `apps/macos/NavoriPanel/CoreRowState.swift:5,9`, and `configProblem` for the banner, which does not change.
Respects: ARCH-1, ARCH-2
Surface:
- `CoreStore.Health`, `health` (`CoreStore.swift:15,217`) — `replace`: deleted.
- `coreStatus` (`CoreStore.swift:109`), read by `PanelView.swift:52` — `replace`: built from the new initializer; its value does not change for any state today.
- `CoreRowStateTests.swift:35-38` — `replace`: `CoreStatus(health:)` stays, and tests are added for the initializer.
- `StatusBarController.swift:64,70,80` — `replace`: see the next section.
Interface:
```swift
// NavoriPanel, no AppKit
public enum StatusIconGlyph { case normal, lease, aiRunning, problem, coreDown, starting }
public struct StatusIconState: Equatable { glyph, attention, description }
extension StatusIconState { public init(coreFailed: Bool, core: StatusIconState?) }
extension CoreHealth { public init(coreFailed: Bool, hasSnapshot: Bool, moduleStates: [String], hasConfigNotice: Bool) }
// CoreStore
var statusIcon: StatusIconState   // the only place that reads `snapshot.tray`
```
Depth: n/a — extends `NavoriPanel` and `CoreStore`.
Locality: new `apps/macos/NavoriPanel/StatusIcon.swift`; extend `CoreRowState.swift`, `CoreStore.swift`. leave: `PanelView.swift`.

The `StatusIconState` initializer returns Core-down when the Core failed, Core-starting with no Core state, and otherwise the Core's state, so that this precedence has a test. `statusIcon` calls it with a mapping of `snapshot.tray`. No glyph is chosen there. The initializer counts only `failed` and `restarting` as degraded, as `health` does today, so a `starting` or `stopped` Module leaves the row `Running`.

### Drawing and motion

Satisfies: ICON-5.1, ICON-5.5, ICON-6.5, ICON-7.3, ICON-7.4, ICON-7.7, ICON-7.8, ICON-7.9, ICON-8.1, ICON-8.2, ICON-8.3, ICON-8.4, ICON-8.5, ICON-8.6, ICON-8.7, ICON-10.10, ICON-10.12, ICON-10.13, ICON-10.15, ICON-10.16, ICON-10.17, ICON-11.1, ICON-11.2, ICON-11.3, ICON-11.4, ICON-11.6
Reuse: rung 2 — `StatusBarController.render` and its observation (`:62-90`), and the Reduce Motion observer in `GlassSurfaceView.swift:36-46`; rung 4 — `NSImage` template images and Core Animation. New (rung 7): the glyph drawing, because SF Symbols have no folded N.
Respects: ARCH-16
Surface:
- `render(health:)`, `observeHealth`, `item.button?.image` at `StatusBarController.swift:62-80` — `replace`: read `store.statusIcon`.
- The `statusItemDrawn` block at `:81-90` and `PerfProbe.markStatusItemDrawn` — `frozen`: built around; it stays guarded by `didScheduleStatusMark`.
- MENU-G4 in MENU's `requirements.md`, and the guard table row for `StatusBarController.swift` — `replace`: struck at delivery as replaced by ICON.
Interface:
```swift
// NavoriPanel: pure and tested
public struct StatusIconMotion {
    public struct Plan: Equatable { public let loop: Bool; public let beat: Bool }
    public mutating func next(_ state: StatusIconState, reduceMotion: Bool, allowBeat: Bool) -> Plan
}
// Navori target
enum StatusIconImage { static func image(_ state: StatusIconState) -> NSImage }
```
Depth: If `StatusIconImage` vanished, callers would still know only "image for this state"; the paths, the cut-outs and the template flag are hidden. If `StatusIconMotion` vanished, callers would need the rules on the previous state and the first snapshot.
Locality: new `apps/macos/Navori/StatusBar/StatusIconImage.swift`; extend `apps/macos/NavoriPanel/StatusIcon.swift`, `StatusBarController.swift`. leave: `StatusPanel`, `PerfProbe`, click handling and the panel code.

`render` returns early when the state and `plan.loop` are unchanged, so the snapshots that fire observation neither reassign the image nor restart the Loop; `beat` is not part of that comparison. A drawn image is cached per glyph and attention pair, and each state change sets a copy of it carrying the description, so a shared image is never mutated and VoiceOver gets the new sentence the way it does today. `appearsDisabled` is true only for Core-starting. A Beat fires only when attention goes off to on and the previous state was not Core-down or Core-starting, which covers the first snapshot (ICON-8.4). Loop and Beat are `CABasicAnimation`s on `button.layer` under the keys `icon.loop` and `icon.beat`, set up on first use (Decision 7). The Loop is added only if the layer has no `icon.loop`. A Beat during the Loop is added after it, so it overrides the opacity for its 0.5 s and the Loop carries on. Once motion has played, the button stays layer-backed until the app quits; the hand walk checks `appearsDisabled` and the menu-open highlight on it, because the spike did not. No `Timer`, `DispatchSourceTimer` or `CADisplayLink` is involved (ICON-11.4), and the delivery check searches for them. A repeating symbol effect is excluded: it cost 0.73–0.82% in the spike. The controller observes `accessibilityDisplayOptionsDidChangeNotification` on `NSWorkspace.shared.notificationCenter`, as `GlassSurfaceView` does, and re-renders.

The Loop's cost is measured at delivery by hand per ICON-11.2 and ICON-11.3, with the numbers recorded in `requirements.md`. The spike's Core Animation numbers were unsteady (`.skills/ICON/spike.md`), so this design does not assume a result. A number above 0.0 reopens the Loop and does not by itself block delivery.

### Samples

Satisfies: ICON-9.1, ICON-9.2, ICON-9.3, ICON-9.4, ICON-9.5, ICON-9.6, ICON-10.11, ICON-10.14
Reuse: rung 2 — `PreviewMenu.content` and `PreviewRootView`, the tint row's pattern, and `openPreview` (`StatusBarController.swift:243`). The existing right-click `Open Menu Preview` item stays the entry.
Respects: ARCH-2
Surface:
- `PreviewMenu.content(setTint:)`, called from `StatusBarController.swift:247` — `replace`: gains the Sample closures.
- MENU's manual binary check (MENU `tasks.md:480`) — `frozen`: built around; it still searches for `NAVORI-PREVIEW-SAMPLE`, and ICON adds a second marker to the same manual step.
Interface: under `#if DEBUG`: `StatusBarController.setIconSample(_ sample: StatusIconState?)` and `playIconBeat()`; `PreviewMenu.content(setTint:setIconSample:playBeat:)`.
Depth: n/a — extends the Preview.
Locality: new `apps/macos/Navori/Features/Preview/PreviewIconSamples.swift`, in `#if DEBUG`; extend `PreviewMenu.swift`, `StatusBarController.swift`. leave: `CoreStore`.

The Sample control is a row in the Preview listing each state in ICON-9.1, plus `Play beat` and `Turn off`. The chosen Sample is held on the controller, not on the panel, so it outlives the panel (ICON-9.4), and `render` draws `sample ?? store.statusIcon`. Choosing or clearing a Sample calls `StatusIconMotion.next` with `allowBeat: false` (ICON-9.5). The new file carries a marker longer than 15 bytes, `NAVORI-ICON-SAMPLE`, so that `strings` can find it.

### Logo

Satisfies: ICON-12.1, ICON-12.2, ICON-12.3
Reuse: rung 2 — `xtask/assets/make-icon.swift` and `cargo xtask icon` (`xtask/src/main.rs:366`).
Surface:
- `AppIcon.icns`, `AppIconDev.icns`, copied into bundles at `xtask/src/main.rs:347` — `replace`: regenerated and checked in.
Interface: `cargo xtask icon`, unchanged.
Depth: n/a — extends `make-icon.swift`.
Locality: `xtask/assets/make-icon.swift`. leave: `xtask/src/main.rs`.

`sparkle` and its two calls go. The planes from `./ui-brief.md` are drawn in the 824 px body with the y axis flipped. The release body gradient and hairline change to suit a light icon; the dev body keeps today's colors, with light planes (Decision 10).

### Documentation (infrastructure — no Satisfies)

Reuse: rung 2 — existing docs.
Interface: none.
Depth: n/a — documentation.
Locality: `docs/ARCHITECTURE.md:74-84,107`; MENU's `requirements.md` (strike MENU-G4); `docs/features/` per this repo's delivery pattern; ICON's `requirements.md` for the numbers.

## UI design

Grounding: lifted from the locked `./ui-brief.md`; tokens are macOS system semantics. The surface is native and not browser-rendered, and the section is filled because MENU and AWAKE set that precedent. Geometry and hex values live only in the brief.

### Status item icon

Layout: an 18 × 18 pt template image centered in the 38 × 33 pt status item cell.
Components: `rung 2 — NSStatusItem` button image; `new (rung 7) — hand-drawn glyph set`, because SF Symbols have no folded N; `new (rung 7) — attention mark` in the same image.
States: normal two-tone N · keeping awake, a solid tile with the N cut out · AI running, a broken fold, with the Loop · problem, a solid triangle with `!` cut out · Core not running, a folded × · starting, an outline N, dimmed · any of the first four with the attention mark, with a Beat when it appears.
Type & color: no type; template image, tinted by the system. Partial alpha only for the fold tone.
A11y: each state differs in shape; VoiceOver reads the summary, or `Core not running` or `Starting`. Motion stops under Reduce Motion.

### Logo

Layout: app icon artwork on Apple's 824 px body in a 1024 px canvas.
Components: `rung 2 — xtask/assets/make-icon.swift`; `new (rung 7)` — the artwork.
States: release; dev, with a distinct body.
Type & color: colors as in the brief.
A11y: not applicable beyond the app name.

## Seams for testing

New seams: `Kernel::tray` and `compose_tray`, one module. `StatusIconMotion` and the two initializers sit behind the existing `NavoriPanel` test target.

| Seam | Kind | Covers |
|---|---|---|
| `compose_tray` with literal values, and `Kernel::tray` with test Modules, in new `crates/kernel/tests/tray.rs` | integration | ICON-1.1, 2.1–2.6, 3.1–3.3, 5.2, 5.3, 6.1–6.4, 6.7, 10.1, 10.2 |
| `Awake::tray` on `AwakeState` values, in `modules/awake/tests/` | unit | ICON-4.1–4.5 |
| Awake refused by `FakePower`, through a Kernel | integration | ICON-4.3, 10.5 |
| `NavoriCore` in `crates/ffi/tests/core.rs` | integration | ICON-1.3, 6.6, 10.3, 10.4 |
| `surface()` in `crates/ffi/tests/config_privacy.rs` | integration | ICON-11.5 |
| The real `navori` binary in `apps/cli/tests/cli.rs` | e2e | ICON-1.2, 1.4, 10.3 |
| `StatusIconMotion` and the `StatusIconState` and `CoreHealth` initializers, in `NavoriPanelTests` | unit | ICON-7.7, 7.8, 8.1–8.6, 10.6, 10.7 |
| `cargo xtask ci`, `cargo xtask perf` | gate | ICON-10.9, 11.1 |
| Read of the doc comment; search of the drawing code | review | ICON-3.4, 5.4, 7.1, 7.2, 11.4 |
| Hand walk of every Sample, in dark and light menu bars, with VoiceOver and Reduce Motion | manual | ICON-5.1, 5.5, 6.5, 7.3, 7.4, 7.7–7.9, 8.7, 9.1–9.5, 10.8, 10.10–10.17, 11.6 |
| `strings` on the release and debug binaries | manual | ICON-9.6 |
| Hand measurement of the Loop | manual | ICON-11.2, 11.3 |
| `cargo xtask icon`, then Finder and the Dock in both profiles | manual | ICON-12.1–12.3 |

## Coverage check

Every live ID in `requirements.md` appears in exactly one `Satisfies:` line above. ICON-7.5 and ICON-7.6 are struck. Deliberately unmapped: none.
