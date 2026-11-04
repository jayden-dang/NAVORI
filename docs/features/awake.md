# Awake

Keep this Mac awake from the terminal for a time, until turned off, or for a command's life. The session Lease Actions are driven by `navori hook`; installing it is on the Integrations tab (see `integrations.md`).

Navori holds at most one `PreventUserIdleSystemSleep` assertion while any Lease exists, and one `PreventUserIdleDisplaySleep` when a Lease asks for the display. Process-owned (session and command) Leases are stored with pid and process start time. Manual duration, until-off and display Leases are not stored; they end when the app closes. On reopen, if the process is still the same, the assertion returns.

Awake drives the menubar icon through its `tray()` contribution: the Lease glyph while held, the problem glyph when macOS refuses.

Awake has no Settings tab of its own and no `[awake]` section; hooks are installed from the Integrations tab.

## Commands

```
navori awake hold [--for <1h|30m|90s>]
navori awake release-all
navori awake status [--json]
navori awake run -- <cmd> [args...]
```

No `--for` flag means until turned off. `awake run` still runs the command when the Core is off; one stderr line; the exit code is the command's, even 3. `Display on` has no CLI verb: `navori dispatch awake '{"display":{"on":true}}'`. Neither has Tracking agents: `navori dispatch awake '{"tracking":{"on":false}}'`. `navori awake status` states it.

`navori doctor` includes an `awake` row, after `modules`. `navori actions` lists the catalog: `status` (read) and `hold_for`, `hold_until_off`, `display`, `hold_command`, `hold_session`, `stop_session`, `end_session`, `release_all`, `keep_for`, `keep_indefinitely`, `keep_off`, `tracking`, `subagent_start`, `subagent_stop` (write).

## Menu

The Awake row sits above Core. It shows a green dot while a Lease has the system assertion and nothing otherwise, with no text. VoiceOver reads the state in words. A problem is stated in the Submenu and by the tray icon.

Open it for two commands. `Keep awake` opens a third level: `Indefinitely`, which keeps the display on too, then `5 minutes`, `10 minutes`, `15 minutes`, `30 minutes`, `1 hour`, `2 hours` and `5 hours`, which let the display sleep. The current choice shows a check and pressing it turns Keep awake off; the row shows `until <time>` or `Indefinitely`. `Tracking agents` reads a green dot and `On`, or `Off`, and pressing it switches. It is on by default and keeps its setting across restarts. While it is off, sessions keep nothing awake and leave the Holding list, but Navori still follows them, so turning it on holds at once for a session mid-turn. Manual and Command Leases do not change. `Release all` is not in the menu; it stays in the CLI. Status text appears only when something is wrong: a refused assertion, a failed command, or a recorded problem. Holding lists each current Lease under `Held by you`, `Commands`, or its project, with the Harness mark for a session and `working` or `stopped`. A session reads `stopped` from `Stop` onward, including the grace period before Navori checks for child processes, and reads `working` while a subagent runs, while Claude Code's latest stop event lists background work, or once that check finds new work. Claude Code's own `caffeinate` counts as work: it runs one only while busy. The Harness and pid are in the tooltip and VoiceOver. The menu does not offer a per-Lease release, and opening it does not read the process table or the system's assertions.

Start events: `Keeping this Mac awake: <source>.` End events: reached its end time, exited, stopped, still busy 2 hours after it stopped, Release all, or `Stopped keeping this Mac awake: <source>.` for a replaced manual hold, display off, and a session end. Each change of Tracking agents emits `awake.tracking_changed`, and a session emits no lease event while it is off. `<source>` is a harness name, a duration (`1 hour` or another true length), `until off`, `display`, or the executable name — never a project or branch.

## What it does not do

- Install hooks itself; the Integrations Module does, and `navori hook` feeds the session Actions.
- Scan for harness processes in the background.
- Pause, a sleep-now command, or staying awake with the lid closed.
- Read which other apps hold the Mac awake.
- Timing options.

**DoD item 5, a stated gap:** the context fragment and the eval case are owned by MILE-3. `context()` waits for MILE-3 (KCONTRACT Out of Scope) and no AI runtime or eval runner exists. Item 5 is therefore only partly met when ROAD-8 closes. Actions with schema, description and Risk, and Events with a one-sentence summary, ship.

## Still to measure

Results live in `.skills/AWAKE/measurements.md`, which is git-ignored. These Must-measure items stay open:

- The Display assertion alone stopping idle sleep — owed by the user; needs a real sleep.
- A Lease sleeping through its deadline — owed by the user; needs a real sleep.
- Timers during dark wake — owed by the user; needs a real sleep.
- A real `Stop` trace through this flow, and `caffeinate` at `Stop` — still owed: it needs a live Harness with the hooks installed.
- Background work after `Stop` — still owed for the same reason.
