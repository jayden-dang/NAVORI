# Integrations

Put Navori's hooks into Claude Code and Grok Build from Settings, and turn their lifecycle events into session Leases through `navori hook`. Codex is listed but not installable yet.

A Harness runs `navori hook` on a lifecycle event with a JSON payload on stdin. The command reads the event name and session id, and asks the running Core for one Awake session Action. For a hold it sends its own process id and its parent process id along with the session id. Prompt text and every other payload field stay in the command.

| Event | Action |
|---|---|
| `UserPromptSubmit` | `hold_session` |
| `Stop` (only when it carries no `reason`, or `reason` is `end_turn`) | `stop_session`, with `background` when its `background_tasks` list is not empty |
| `StopCancelled`, `StopFailure` | `stop_session` |
| `SubagentStart` | `subagent_start` with the agent id and pids |
| `SubagentStop` | `subagent_stop` with the agent id, and `background` as above |
| `SessionEnd` | `end_session` |

Any other event sends nothing. The same event arriving twice for one session (Grok also runs Claude's hooks) leaves one Lease.

`navori hook` always exits 0 and prints nothing, so a Harness never stalls or fails on it. With no Core running it starts no Lease, writes no file and does not open the app. It waits up to 0.7 seconds for stdin to close on every event, so a payload that arrives more slowly sends nothing. It then gives up after 1.5 seconds (0.7 seconds for `SessionEnd`). It sends to the Core of the profile its own command line names (release, or dev with `--profile dev`) and ignores `NAVORI_PROFILE`. If the hook never fires, the Lease still ends when the Harness process exits or its TTL runs out.

Start cost, measured with the release binary on an Apple M5 Pro, macOS Darwin 27.0.0, 50 runs after 5 warm-ups: median 2.4 ms and p95 2.9 ms against a Core answering at once; median 2.3 ms and p95 2.8 ms with no Core. A second run gave medians of 2.3 ms and 2.6 ms.

## Settings

Settings has a fourth tab, `Integrations`, before About. It lists Claude Code, Codex and Grok Build, always in that order, each with one state: Not found, Not installed, Installed, Needs repair or Unreadable. The state is read when the tab opens, on a `navori dispatch` of `status`, and in `navori doctor`; nothing polls the files.

- Not installed shows `Install`. Needs repair shows `Repair` and `Remove`. Installed shows `Remove`.
- Not found disables `Install` and says why beside it (there is no `~/.claude`, `~/.codex` or `~/.grok` directory).
- Unreadable offers nothing: the file cannot be parsed, or it is valid JSON whose shape under `hooks` is not the expected one, the row says why, and you fix the file. Install, Repair and Remove all refuse an Unreadable file.
- The wording of each reason comes from the Core, not from Swift.

## Codex

The Codex row reports its state as found and offers nothing, with the reason `The Codex timeout unit is not verified yet.` The reason shows when the `~/.codex` directory exists and its Hook file is absent or parses; otherwise the row keeps its Not found or Unreadable reason. Install, Repair and Remove reply with an error for Codex. Codex hooks do not work from Navori yet.

## Commands

```
navori dispatch integrations '"status"'
navori dispatch integrations '{"install":{"harness":"claude"}}'
navori dispatch integrations '{"repair":{"harness":"grok"}}'
navori dispatch integrations '{"remove":{"harness":"claude"}}'
```

`harness` is `claude`, `codex` or `grok`. `navori actions` lists `status` (read) and `install`, `repair`, `remove` (write).

`navori doctor` has an `integrations` row, after `awake`. It names each Harness with its state (`claude: installed, codex: not installed, grok: not found`). It fails, and `doctor` exits 1, when a present Harness needs repair or is unreadable; a Harness that is not found or not installed is not a failure. With the app not running the CLI prints only the `app` row, so no hook state shows.

Events: `integrations.hook_installed`, `integrations.hook_repaired`, `integrations.hook_removed`, each with a one-sentence summary naming the Harness, and `integrations.hook_failed` at error severity.

## Files touched

Only these paths, under the home directory the Core was started with, and their Backups:

- Claude Code: `~/.claude/settings.json`
- Codex: `~/.codex/hooks.json` (never written yet)
- Grok Build: `~/.grok/hooks/navori.json`

Install adds a `UserPromptSubmit`, `Stop`, `StopFailure`, `SubagentStart`, `SubagentStop` and `SessionEnd` entry for Claude Code, and `UserPromptSubmit`, `Stop`, `StopCancelled`, `StopFailure` and `SessionEnd` for Grok. Each entry is one command handler, with the quoted absolute path of the app's `Contents/Helpers/navori` followed by ` hook` (` --profile dev hook` for the dev app), and a timeout of 2 seconds (1 for `SessionEnd`). It never uses `~/.local/bin/navori`. Install never creates `~/.claude`, `~/.codex` or `~/.grok`; for Grok it creates `~/.grok/hooks` when `~/.grok` exists.

- Every other key, hook and array order stays. Whitespace may be rewritten.
- Before changing an existing file, Navori copies it to `<name>.navori-bak-<seconds>` beside it. A file already correct, or with nothing to remove, is not touched and gets no Backup. Backups are never pruned.
- The write goes to a temporary file in the same directory, then renames over the original. A file that cannot be parsed is left alone with an error naming it.
- A symbolic link is kept and the file it points to is written.
- Repair replaces entries that are wrong (for example after the app moved), adds missing ones and removes extras, for the running profile only.
- Remove deletes the running profile's entries and any array or object that leaves empty. For Grok, a `navori.json` holding only Navori's entries is deleted after a Backup. When that path is a symbolic link, Remove does not delete it: it writes `{}` through the link and keeps the link, so a dotfile manager does not bring the hooks back. This link behaviour is agent-decided and open to the operator's call on the pull request.
- The dev and release apps keep separate entries in one file. Each reads and changes only its own, and the other's stay byte-identical.

## Not verified

None of this has run against a live Harness or a signed build. Open:

- Everything this page says about what a Harness sends or does comes from its documentation and has not been observed. That includes whether real Claude Code and Codex `Stop` payloads carry no `reason`, as documented, and whether the events carry `hook_event_name` and `session_id` as documented.
- The Codex `timeout` unit, and whether Codex runs `command` through a shell. This is why Codex is not installable.
- Whether you see the standard error of a Claude Code or Codex hook that exits 0 (the command prints none).
- Whether Grok's daemon route leaves a live Harness process above the hook, so a Grok Session Lease starts at all.
- Whether a signed build writing into `~/.claude`, `~/.codex` and `~/.grok` triggers a macOS permission prompt.
- Claude Code's `Stop` does not run on an interrupt, and no other event fires there either, so a Lease can stay open until the next `Stop`, `SessionEnd`, the Harness exiting or the 8 hour TTL. An API error fires `StopFailure`, which Install now adds, but that has not been observed with a live Harness.
- Codex skips a new or changed hook until you review it in `/hooks`, and every Repair changes its hash. A Codex hook reported as Installed may not run yet. Grok and Claude Code trust is a documentation read only.
- Keyboard and VoiceOver walks of the Integrations tab.

## What it does not do

- Change the Lease model, the Awake Actions or their timing.
- Keep the Mac awake with the lid closed, pause, or sleep now.
- Detect a Harness that has no hook by watching its process.
- Keep numbers beyond what a double holds: an integer beyond 64 bits or a number beyond double precision in a Hook file is rewritten at double precision on any change. The Backup holds the original.
- Repair a file it cannot parse, prune Backups, or add a setting for the timeouts.
- Install hooks for other Harnesses.

**DoD item 5, a stated gap:** the context fragment and the eval case are owned by MILE-3. Actions with schema, description and Risk, and Events with a one-sentence summary, ship.
