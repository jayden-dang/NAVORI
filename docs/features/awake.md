# Awake

Keep this Mac awake from the terminal for a time, until turned off, or for a command's life. A session Lease Action is there for hook work; hook install is not.

Navori holds at most one `PreventUserIdleSystemSleep` assertion while any Lease exists, and one `PreventUserIdleDisplaySleep` when a Lease asks for the display. Process-owned (session and command) Leases are stored with pid and process start time. Manual duration, until-off and display Leases are not stored; they end when the app closes. On reopen, if the process is still the same, the assertion returns.

Awake drives the menubar icon through its `tray()` contribution: the Lease glyph while held, the problem glyph when macOS refuses.

There is no Settings tab and no `[awake]` section.

## Commands

```
navori awake hold [--for <1h|30m|90s>]
navori awake release-all
navori awake status [--json]
navori awake run -- <cmd> [args...]
```

No `--for` flag means until turned off. `awake run` still runs the command when the Core is off; one stderr line; the exit code is the command's, even 3. `Display on` has no CLI verb: `navori dispatch awake '{"display":{"on":true}}'`.

`navori doctor` includes an `awake` row, after `modules`. `navori actions` lists the catalog: `status` (read) and `hold_for`, `hold_until_off`, `display`, `hold_command`, `hold_session`, `stop_session`, `end_session`, `release_all` (write).

## Menu

The Awake row sits above Core. It shows `Off`, `On` with the sources holding, `Not held`, or `Unavailable`.

Open it for `Keep awake for 1 hour` (always 3600 seconds), `Keep awake until I turn it off`, the status block, `Display on` (it reads `Display off` once that Lease exists), and `Release all`. `Release all` stays visible and does nothing while no Lease exists. Holding lists each current Lease: `1 hour` or `Until off`, `Display`, a command's executable, or a session. The menu does not offer a per-Lease release, and opening it does not read the process table or the system's assertions.

Start events: `Keeping this Mac awake: <source>.` End events: reached its end time, exited, stopped, still busy 2 hours after it stopped, Release all, or `Stopped keeping this Mac awake: <source>.` for a replaced manual hold, display off, and a session end. `<source>` is a harness name, a duration (`1 hour` or another true length), `until off`, `display`, or the executable name — never a project or branch.

## What it does not do

- Install hooks or `navori hook` — that is ROAD-9.
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
- A real `Stop` trace through this flow, and `caffeinate` at `Stop` — owed by ROAD-9, which installs the hooks.
- Background work after `Stop` — owed by ROAD-9.
