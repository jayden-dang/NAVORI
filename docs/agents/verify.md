# Verify / control — cold-start drive recipe

Written by `configure-repo` (Decision M). Read by `validate-feature`,
`validate-ui`, `validate-api`, and `write-dogfood`. A later agent that has
never seen this app follows this file instead of inventing commands.

**App:** Navori — dev profile: `build/dev/Navori Dev.app`, bundle id `dev.navori.app.debug`
**Primary surface:** desktop core, driven over its IPC socket through the bundled CLI
**Other surfaces:** menubar UI (native SwiftUI/AppKit — no stable automation handles yet)

All commands run from the repo root. Shorthand used below:

```sh
CLI="build/dev/Navori Dev.app/Contents/Helpers/navori"
```

Always pass `--profile dev`. Without it the CLI talks to the **release** install
(`/Applications/Navori.app`), which holds the user's real data.

## Launch

Start command (matches `docs/agents/project.md` **Run locally (dev)**):

```sh
cargo xtask run
```

It builds the core + Swift shell, bundles and signs `Navori Dev.app`, quits any running
dev instance, then `open`s the new one. Ready when: `✓ launched …` is printed **and** Doctor
passes (the core may need a second after launch — retry Doctor for up to ~10 s).

Teardown: quit the dev app by bundle id (graceful; emits `core.stopped`) — never kill by
process name:

```sh
osascript -e 'tell application id "dev.navori.app.debug" to quit'
```

## Doctor

One read-only check: is this instance worth driving?

```sh
"$CLI" --profile dev doctor
```

Pass: exit 0, every line ✓ (app reachable at the dev socket, config, database, IPC,
modules, logs, awake, integrations, ports). The `integrations` row fails, and doctor exits 1, when a
Harness hook needs repair or its file is unreadable. A dev build run from another worktree can
cause this, because the hook entry names that build's path; that is not a defect of this
instance. Exit 3 = app not running → Launch first. Any other failure: do not Drive;
fix Launch or record the blocker.

## Drive

Harness: shell (PTY not needed) running the CLI, which calls the real core over the Unix
socket. Stable handles are the CLI subcommands and their `--json` output — assert on JSON
fields, not on aligned text columns.

| Command | What it does |
|---|---|
| `"$CLI" --profile dev status [--json]` | version, uptime, module health, the composed icon state (`tray`) |
| `"$CLI" --profile dev events [--limit N] [--json]` | journal, newest first |
| `"$CLI" --profile dev actions [--json]` | every action modules accept, with risk level |
| `"$CLI" --profile dev dispatch <module> '<action-json>'` | send an action, prints the reply JSON |
| `"$CLI" --profile dev awake status [--json]` | leases keeping this Mac awake; also `hold`, `release-all`, `run -- <cmd>` |

CLI exit codes: 0 success · 1 command failed · 2 usage error · 3 app not running.

Isolation: dev and release are fully separate (data, socket, logs), so driving dev never
touches the release install. Only **one** dev instance can run — refuse to double-drive it.
Dev data lives in `~/Library/Application Support/Navori Dev/`, logs in
`~/Library/Logs/Navori Dev/`.

## Evidence

Keep under `.skills/verify/` (gitignored). A proof captures **the action and
the resulting state**, including a **reload / re-open / re-read** when the
feature claims persistence. A final screen alone is not enough. Mocks only at
a production boundary. Save `--json` outputs as files, e.g.
`.skills/verify/<date>-<feature>/events-after-restart.json`.

Cleanup never deletes evidence.

## Cleanup

Quit the dev app (Teardown above). Remove scratch the run created. Leave `.skills/verify/`
and the dev data directory alone.

## Features

Index:

| Feature | How a user reaches it | Observable end state |
|---|---|---|
| Health | `status`, `doctor` | every module healthy; doctor all ✓ (presentational — no store) |
| Journal survives restart | `events --json` across a quit + relaunch | the previous session's `core.started` and `core.stopped` events are still listed after relaunch, plus a new `core.started` |
| Action catalog | `actions --json` | lists `system.ping` (read) and `system.prune_journal` |
| Dispatch | `dispatch system '"ping"'` | prints `"pong"` |
| Awake | `awake status`, `hold`, `release-all`, `run -- <cmd>` | a lease is listed while held; gone after `release-all` or when the command exits |
| Integrations | `dispatch integrations ...`, `doctor` | Install writes the Harness hook entries into the Hook file under the temp home; Remove takes them out |
| Ports | `dispatch ports '"list"'`, `doctor` | the listening ports of the user's processes, grouped by project; the `ports` row passes (`not scanned yet` before the first scan) and fails with the outcome after a failed scan |
| Menubar panel | click the Navori Dev menu bar icon | manual drive only for now (see below) |

### Health

- **Sub-features:** `status`, `doctor`
- **How to get to it (user POV):** run `navori status` / `navori doctor`.
- **Driving it with the CLI:** `"$CLI" --profile dev status --json`;
  `"$CLI" --profile dev doctor --json`.
- **Gotchas:** right after Launch the socket may not be bound yet — retry briefly.
- **Persist check:** presentational, no store.

### Journal survives restart

- **Sub-features:** event bus → redaction → SQLite journal
- **How to get to it (user POV):** `navori events`; events also feed the panel.
- **Driving it with the CLI:**
  1. `"$CLI" --profile dev events --limit 5 --json > .skills/verify/<run>/before.json` —
     note the newest `core.started` id.
  2. Quit (Teardown), then `cargo xtask run` and wait for Doctor.
  3. `"$CLI" --profile dev events --limit 5 --json > .skills/verify/<run>/after.json`.
- **Gotchas:** `events` prints newest first. The sweep prunes at the configured retention
  (default 30 days) unless it is paused, and a paused sweep deletes nothing until the
  next start on a fixed file — do not assert on anything older than that retention.
- **Persist check:** after relaunch, the `core.started` id from step 1 is still present,
  with a `core.stopped` after it and a new `core.started` on top.

### Action catalog

- **Sub-features:** none
- **How to get to it (user POV):** `navori actions`.
- **Driving it with the CLI:** `"$CLI" --profile dev actions --json`.
- **Gotchas:** new modules add rows; assert on presence, not on the full list.
- **Persist check:** presentational, no store.

### Dispatch

- **Sub-features:** typed action routing through the kernel
- **How to get to it (user POV):** `navori dispatch system '"ping"'`.
- **Driving it with the CLI:** `"$CLI" --profile dev dispatch system '"ping"'` → `"pong"`.
  Malformed JSON → exit 2.
- **Gotchas:** the action argument is JSON — a bare name needs inner quotes (`'"ping"'`);
  an action with fields is `'{"name": {...}}'`. Do not dispatch `prune_journal` against
  data you need as evidence.
- **Persist check:** `ping` changes no state.

### Awake

- **Sub-features:** duration, until-off, command, session, and display leases
- **How to get to it (user POV):** `navori awake hold`, `release-all`, `status`, `run -- <cmd>`.
- **Driving it with the CLI:**
  1. `"$CLI" --profile dev awake hold --for 1h` then `"$CLI" --profile dev awake status --json`.
  2. `"$CLI" --profile dev awake release-all`.
  3. `"$CLI" --profile dev awake run -- sleep 5` — the command's exit code, even 3; with the Core off the command still runs.
- **Gotchas:** no flag on `hold` means until turned off. `Display on` has no CLI verb; dispatch `{"display":{"on":true}}`. Session hold is `dispatch`, not a CLI verb. Tracking agents is `dispatch` too: `{"tracking":{"on":false}}`. While it is off, `status` lists no session, though Navori still follows them.
- **Persist check:** a command or session lease is still listed after quit and relaunch if the process is still the same; a duration or until-off lease is not. Tracking agents reads the same after relaunch.

### Ports

- **Sub-features:** `list` and `open` of the ports the user's own processes listen on; the `ports` doctor row; the problem glyph on the status icon after a failed scan
- **How to get to it (user POV):** no screen yet; the CLI and the `ports.list` reply only.
- **Driving it with the CLI:** `"$CLI" --profile dev dispatch ports '"list"'`, then `'{"open":{"port":3000}}'`; `"$CLI" --profile dev doctor` for the row.
- **Gotchas:** the first scan runs 5 s after launch, so an early `list` answers with no entries and no scan time. A failed scan keeps the last good list and sets `problem`. A Docker read failure shows in `docker`, never in `problem`.

### Integrations

- **Sub-features:** status, install, repair and remove of Navori's hooks in the Claude Code, Codex and Grok Build Hook files; the `integrations` doctor row
- **How to get to it (user POV):** Settings, Integrations tab.
- **Driving it with the CLI:** `"$CLI" --profile dev dispatch integrations '"status"'`, then
  `'{"install":{"harness":"claude"}}'`, `'{"repair":{"harness":"claude"}}'` and
  `'{"remove":{"harness":"claude"}}'`; `"$CLI" --profile dev doctor` for the row.
- **Gotchas:** a dev build's Core uses the real home, so Install writes real Harness files
  (`~/.claude/settings.json`, `~/.grok/hooks/navori.json`). Agents drive this only over a temp
  home: launch with `NAVORI_PERF=1 NAVORI_PERF_HOME=<tmp> HOME=<tmp>` and create `<tmp>/.claude`
  first, since Install never creates a Harness directory. Codex is listed but its actions reply
  with an error. Never run a live Harness against it.
- **Persist check:** the Hook file under the temp home holds the entries after Install and
  after relaunch; Remove takes them out and leaves a `.navori-bak-<seconds>` Backup beside it.

### Menubar panel

- **Sub-features:** see `docs/design/2026-11-01-menubar-ui-brief.md`
- **How to get to it (user POV):** click the Navori Dev icon in the menu bar.
- **Driving it:** no stable automation handles yet (no accessibility identifiers wired);
  ask the user to drive it, or use screen-level computer use with screenshots as evidence.
- **Gotchas:** dev and release icons can both be in the menu bar — make sure it is the dev one.
- **Persist check:** close and reopen the panel; for state, re-read via the CLI.
