# Status icon

The menubar icon shows what Navori is doing now: one of four marks from the Core, two the Shell draws when the Core cannot speak, and an attention dot on top. `navori status --json` reports the same Core result.

**Verified by hand on 2026-11-03** on a debug build: every Sample on the real status item, the Loop, the Beat, Reduce Motion (launched with it on, and toggled live in System Settings), a bad `config.toml`, an unreadable database, and the app icon. Numbers and the one open question are under "Measured" below.

## What the Kernel composes

Each Module may implement `Module::tray(&State) -> TrayContribution` (default: nothing). It is a pure, cheap function of the published State, with no I/O. A contribution is an optional glyph with its sentence, and an optional attention clause. The Kernel reads it when the status is read: no task, timer or cache of its own. It ignores a Module that is not running, and a Module whose `tray` panics is ignored from then on with one warning.

`Kernel::tray(notice)` ranks the entries into one `TrayState { glyph, attention, summary }`, knowing no Module by name:

1. `problem` wins: a failed or restarting Module (`<id> stopped: <error>`, `<id> is restarting`), or a running Module that asks for it.
2. Else `lease`, then `ai_running`, then `normal` (summary `Navori`).
3. `attention` is on when any running Module gives an attention clause. It adds a clause to the summary.
4. The config notice adds `<title>. <message>` to the summary and never changes the glyph.

Awake and Ports contribute today. Awake gives `lease` ("Keeping this Mac awake") while a Lease exists and macOS holds the assertion, `problem` when a Lease exists and the system will not hold it. It never asks for attention. Ports gives `problem` ("Can't scan ports. <outcome>") while its last scan failed, and nothing else. Nothing contributes `ai_running` yet.

## State and glyphs

`TrayState` and `TrayGlyph` (`normal`, `lease`, `ai_running`, `problem`) live in `navori_protocol`; `CoreStatus.tray` carries them. Through UniFFI they are `TrayView` and `TrayGlyphView` on `CoreSnapshot.tray`. The raw text of a bad config value does not enter `tray`.

## Settings

None: there is no Settings tab and no config key for the icon.

## CLI

`navori status --json` includes `"tray": {"glyph": "normal", "attention": false, "summary": "Navori"}` beside the existing fields. Plain `navori status` prints no icon line.

## Shell

`CoreStore.statusIcon` is the only reader of `snapshot.tray`. It maps the four Core glyphs to `StatusIconGlyph` and adds the two Shell readings: `coreDown` ("Core not running") when the Core failed to start, and `starting` ("Starting") while no snapshot has arrived. Neither has an attention mark. The Core row keeps its own lifecycle reading (`CoreHealth`), so it still says `Needs attention` on a config notice.

`StatusIconImage` draws 18 pt template images from the folded-N planes, so the system tints them for dark and light bars: `normal` (three planes, the middle one solid), `lease` (rounded square with the planes cut out), `ai_running` (diagonal in three pieces), `problem` (triangle with a cut-out `!`), `coreDown` (a struck line), `starting` (outline only, with `appearsDisabled`). Attention clears a ring and draws a dot at the top right. Images are cached per glyph and attention; the summary becomes the accessibility description.

## Motion

Motion plays in Core Animation, with no `Timer`, `DispatchSourceTimer` or `CADisplayLink` (ARCH-16), and only on opacity.

- **Loop:** while the glyph is `ai_running`, opacity fades 1.0 to 0.35 over 1 second each way (autoreverse, a 2 second cycle), forever.
- **Beat:** one dip to 0.2 over 0.25 seconds each way (0.5 seconds in all) when attention newly turns on after the icon has drawn a real snapshot (not from Starting or Core-down).
- **Reduce Motion** on: neither plays. Every glyph and the attention dot are carried in shape, so the icon reads with motion off and without color. Changing the setting re-renders the icon.

## Samples (debug builds)

The dev Preview menu has an `Icon samples` list: Normal, Keeping awake, AI running and Problem, each with and without attention, plus Core not running and Starting (dimmed). A choice is held on the real status item until `Turn off`, and the real snapshot returns with no Beat. `Play beat` plays one Beat. Samples dispatch no Action and are compiled out of release.

## The app icon

The app icon is the folded-N logo, drawn by `xtask/assets/make-icon.swift` into the checked-in `AppIcon.icns` and `AppIconDev.icns` (`cargo xtask icon`).

## What it does not do

- Choose a glyph in Swift beyond the Core-down and Starting readings.
- Offer an `ai_running` source: no Module reports it yet.
- Turn attention on for To-dos, Updates, pull request counts, failing checks or port counts; only a Decision waiting for the user does.

## Measured

- **Loop cost.** Navori's CPU is 0.0% (median of 5 windows of 12 s) during the Loop and with the static icon. WindowServer was about 6% with the static icon and 28-37% while the first Loop ran, a result consistent with the render server redrawing the menu bar at the display rate (only CPU was measured). The 5.5 against 5.9 pair is the final build; the 8.2 and 5.5 rows are the same hint on two runs, so noise is about 3 points. The Loop now carries a 10 fps frame-rate hint (`preferredFrameRateRange` 8 to 12), and WindowServer reads 5.5% with the Loop against 5.9% static, which supports "the Loop now costs no more than the static icon". Both tables are in `docs/specs/2026-11-02-status-icon-states/requirements.md` under ICON-11.2.
- **After the 10 fps hint.** The Loop, a Beat during the Loop (the Loop carried on) and Reduce Motion toggled live (on: the icon stopped moving at constant brightness; off: the Loop resumed) were walked again by hand on 2026-11-03.
- **Glyphs.** All ten Samples draw as designed in a dark menu bar; the AI Sample dims and recovers, `Play beat` dims once, and a Beat during the Loop leaves the Loop going.
- **Reduce Motion.** With it on at launch the AI Sample stays at full opacity. Toggling it in System Settings while the Loop plays stops the Loop at once, and turning it off starts it again.
- **Core states.** A bad `config.toml` keeps the normal glyph, names `log_level` without its value in the summary, and shows the banner and `Needs attention` on the Core row. An unreadable database shows the folded cross and reads "Core not running".
- **Accessibility description.** The status item's accessibility description is the summary for each state checked (Normal, Problem with attention, AI with attention, Core not running, Starting). This is what VoiceOver reads; VoiceOver itself was not run.
- **App icon.** Finder and the Dock resolve the dev bundle to the slate-blue icon and the release file to the paper-white one. `/Applications/Navori.app` still shows the old icon until `cargo xtask install` replaces it.

## Not verified

- A real light menu bar. On this Mac the bar stays dark in the light appearance because the wallpaper is dark and cannot be changed and restored. The glyphs were re-drawn as black ink on a light bar from the dark-bar captures; every shape reads, and only Starting, which is dimmed by design, is faint.
- VoiceOver speech. VoiceOver ran, but its AppleScript interface returned no phrases here. What it reads is the status item's accessibility description, which was read directly for Normal, Problem with attention, AI with attention, Core not running and Starting.
- The real Starting state at launch: it lasts less than one screenshot interval; its shape is verified through the Sample.
- The 10 fps hint on a ProMotion display.
- The Beat's own cost with the hint was not measured.
- All measurements are from one 3440x1440 display at load about 2 to 6.

`cargo xtask perf` stays inside budget with no metric, phase or signal added. Measured 2026-11-03 on b4e093a (no `--record`): cold_start 76.4 ms of 200, menu_open_after_launch 54.2 of 80, menu_reopen 12.1 of 50, footprint_menu_open 29.2 MB of 60, idle_cpu 0 %, idle_wakeups 0 /s, idle_footprint 20.2 MB of 30, app_size 9.2 MB of 25, submenu_open 4.3 ms of 16.
