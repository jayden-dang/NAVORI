# Status icon

The menubar icon shows what Navori is doing now: one of four marks from the Core, two the Shell draws when the Core cannot speak, and an attention dot on top. `navori status --json` reports the same Core result.

**Pending, not done:** the Loop's CPU cost (ICON-11.2, ICON-11.3) and the by-hand walk of every Sample (ICON-11.6) have not been run. The numbers are placeholders marked `Not yet measured` in `docs/specs/2026-11-02-status-icon-states/requirements.md`. Until they are taken, the Loop is delivered but not proven free.

## What the Kernel composes

Each Module may implement `Module::tray(&State) -> TrayContribution` (default: nothing). It is a pure, cheap function of the published State, with no I/O. A contribution is an optional glyph with its sentence, and an optional attention clause. The Kernel reads it when the status is read: no task, timer or cache of its own. It ignores a Module that is not running, and a Module whose `tray` panics is ignored from then on with one warning.

`Kernel::tray(notice)` ranks the entries into one `TrayState { glyph, attention, summary }`, knowing no Module by name:

1. `problem` wins: a failed or restarting Module (`<id> stopped: <error>`, `<id> is restarting`), or a running Module that asks for it.
2. Else `lease`, then `ai_running`, then `normal` (summary `Navori`).
3. `attention` is on when any running Module gives an attention clause. It adds a clause to the summary.
4. The config notice adds `<title>. <message>` to the summary and never changes the glyph.

Awake is the one Module that contributes today: `lease` ("Keeping this Mac awake") while a Lease exists and macOS holds the assertion, `problem` when a Lease exists and the system will not hold it. It never asks for attention. Nothing contributes `ai_running` yet.

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

## Still to measure

- The Loop's CPU for Navori and for WindowServer, per `/usr/bin/top -l 6 -s 12 -stats cpu -pid <pid>`: owed by the user. A result above 0.0 reopens the Loop.
- The hand walk of every Sample in dark and light bars, with Reduce Motion on and off and with VoiceOver, including the menu-open highlight and `appearsDisabled`, and a Beat during the Loop: owed by the user.

`cargo xtask perf` stays inside budget with no metric, phase or signal added.
