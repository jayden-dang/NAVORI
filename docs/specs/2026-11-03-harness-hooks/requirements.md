# Requirements: Harness hooks

Feature code: HOOKS
Status: In-progress
Date: 2026-11-03
Approved by the operator ("approve", 2026-11-03, on the review page at c2da2f5, no card selected).

<!--
Rules:
- Every acceptance criterion gets a hierarchical ID: HOOKS-<story>.<criterion>.
- Criteria use EARS phrasing (WHEN / WHILE / IF…THEN / WHERE / ubiquitous).
- Guard requirements protect existing behavior: WHEN <condition> THE SYSTEM SHALL CONTINUE TO <behavior>.
- IDs are immutable once Status is Approved. Retire by strikethrough — never renumber.
-->

Roadmap item: ROAD-9 (MILE-2) · Tracker: #9 · Decisions: frame-change close package `close-package.md`, approved by `harness-hooks-interpret` in round 2 of an unattended run (charter `.skills/_pending-harness-hooks/auto/charter.md`). The operator approved this file on 2026-11-03 (line 6) and amended it the same day (HOOKS-4.16).

Source classes. Each criterion that fixes a choice carries one of these beside it:
- **Lock** — the charter's decided lines, cited by `file:line`.
- **Weigh** — a pick by `harness-hooks-interpret` or a research note; evidence, not the operator's choice.
- **agent-decided** — a value neither a Lock nor a source settles; the option that leaves every documented contract as it is. Not the operator's choice.
- **Unverified** — a fact not yet seen working; a check task sits in the design.

Terms used below:
- **The system** — the `navori hook` command, the Integrations Module, `navori-ffi` and the Shell code that shows Integrations.
- **Harness**, **Lease**, **Agent session**, **Integration**, **Action**, **Event** — as in `CONTEXT.md`.
- **Hook command** — `navori hook`, the one command all three Harnesses run.
- **Hook entry** — the group Navori writes under one event name in a Harness's hook file: one `command` handler with a `timeout`.
- **Hook file** — `~/.claude/settings.json` (Claude Code), `~/.codex/hooks.json` (Codex), `~/.grok/hooks/navori.json` (Grok Build). [Lock, docs/ARCHITECTURE.md:304-306]
- **Running profile** — the profile (`release` or `dev`) of the app whose Core handles the Action.
- **Helper** — the `navori` CLI inside the app bundle, `Contents/Helpers/navori`.
- **Present Harness** — a Harness whose config directory (`~/.claude`, `~/.codex`, `~/.grok`) exists.
- **Backup** — a copy of a Hook file taken before a write, beside it, named `<file>.navori-bak-<timestamp>`. [agent-decided]
- **Awake Actions** — `hold_session`, `stop_session`, `end_session` of the Awake Module (`modules/awake/src/action.rs:18-24`).

## 1. A Harness turn keeps the Mac awake

**Story:** As the user, I want a prompt I send to Claude Code, Codex or Grok to keep the Mac awake until the turn ends, so that I can see a Session Lease appear and go away without touching anything.

- **HOOKS-1.1** WHEN the Hook command reads a payload whose event is `UserPromptSubmit` THE SYSTEM SHALL dispatch the Awake Action `hold_session` with the payload's session id, the command's own pid and its parent's pid. [Lock, docs/ARCHITECTURE.md:304-306, 311]
- **HOOKS-1.2** WHEN the Hook command reads a payload whose event is `Stop` and which carries no `reason`, or a `reason` equal to `end_turn`, THE SYSTEM SHALL dispatch `stop_session` with the session id. [Lock, ARCHITECTURE.md:304-306; keying the Grok filter on the payload's `reason` and not on the Harness is agent-decided]
- **HOOKS-1.3** WHEN the Hook command reads a `Stop` payload whose `reason` is anything other than `end_turn` THE SYSTEM SHALL dispatch nothing. This drops the extra observe-only `Stop` that Grok fires at session end (`~/.grok/docs/user-guide/10-hooks.md:677, 703`). [Lock for Grok, ARCHITECTURE.md:305; applying it to any Harness is agent-decided. Documented, not yet recorded: the Claude Code and Codex `Stop` payloads carry no `reason`, so for them the rule drops nothing (research note `research-hooks-claude-codex.md`, docs summary); see Open Questions]
- **HOOKS-1.4** WHEN the Hook command reads a payload whose event is `StopCancelled` or `StopFailure` THE SYSTEM SHALL dispatch `stop_session` with the session id. [Lock, ARCHITECTURE.md:305]
- **HOOKS-1.5** WHEN the Hook command reads a payload whose event is `SessionEnd` THE SYSTEM SHALL dispatch `end_session` with the session id and SHALL send no pid. [Lock, ARCHITECTURE.md:311; AWAKE close-package.md:10]
- **HOOKS-1.6** THE SYSTEM SHALL read the event from `hook_event_name` and the session id from `session_id` — the two keys Grok verifiably sends beside its camelCase keys (`~/.grok/docs/user-guide/10-hooks.md:673-677`). [Lock for the fields, ARCHITECTURE.md:300. Claude Code: `session_id` and `hook_event_name` are documented common fields (code.claude.com/docs/en/hooks, raw). Codex: documented (raw SessionEnd example; the rest from a docs summary). No recorded payload of either yet: Task 11]
- **HOOKS-1.7** WHEN the Hook command reads a payload whose event is none of the above THE SYSTEM SHALL dispatch nothing.
- **HOOKS-1.8** THE SYSTEM SHALL send the Core nothing from a payload except the session id, the agent id of a subagent event, whether background work is in flight, the session's title, the event's mapped Action and the two pids, so that prompt text and other payload fields never leave the command. [agent-decided; ARCH-6] *(Amended 2026-11-04 by HOOKS-1.11 to HOOKS-1.17 and HOOKS-1.19.)*
- **HOOKS-1.9** WHEN the same event arrives twice for one session id, as when Grok runs Claude's hook as well as its own, THE SYSTEM SHALL dispatch each arrival and leave one Lease. [Lock, ARCHITECTURE.md:312; relies on the Module: `modules/awake/src/action.rs:75`, `modules/awake/src/domain/table.rs:248-259`]
- **HOOKS-1.11** WHEN the Hook command reads a payload whose event is `SubagentStart` THE SYSTEM SHALL dispatch `subagent_start` with the session id, the agent id, the command's own pid and its parent pid. [Operator's choice, 2026-11-04]
- **HOOKS-1.12** WHEN the Hook command reads a payload whose event is `SubagentStop` THE SYSTEM SHALL dispatch `subagent_stop` with the session id and the agent id, and SHALL send no pid. [Operator's choice, 2026-11-04]
- **HOOKS-1.13** WHEN a subagent event carries no agent id THE SYSTEM SHALL dispatch nothing, so that a Harness that reads Claude's hooks with another payload changes nothing.
- **HOOKS-1.14** WHEN a `Stop` or `SubagentStop` payload has a non-empty `background_tasks` list THE SYSTEM SHALL add `background: true` to the Action and send nothing else from that list. A list of another shape reads as empty, and SHALL NOT lose the event. [Operator's choice, 2026-11-04; seen on a real Claude Code 2.1.318 run]
- **HOOKS-1.15** WHEN a `UserPromptSubmit` or `Stop` payload is read THE SYSTEM SHALL add a `title` to the Action: its non-blank `session_title`, or else the latest `custom-title` record in the last 128 KB of its `transcript_path`, or else the latest `ai-title` record there. [Operator's choice, 2026-11-04]
- **HOOKS-1.16** THE SYSTEM SHALL take a title from a transcript only from a whole line that parses as one of those records, and SHALL send nothing else from the file.
- **HOOKS-1.17** (guard) WHEN the transcript is missing, unreadable or holds no such record THE SYSTEM SHALL CONTINUE TO dispatch the event, with no title.
- **HOOKS-1.18** WHEN the Hook command reads a payload whose event is `Interrupt` THE SYSTEM SHALL dispatch `stop_session` with the session id. [Operator, 2026-11-04; Codex 0.166 sends `session_id` and `hook_event_name` on it, `codex-rs/hooks/src/schema.rs:628-638`]
- **HOOKS-1.19** WHEN a `UserPromptSubmit` payload carries `cwd` THE SYSTEM SHALL add it to `hold_session`, and SHALL NOT send `cwd` with any other event. [Operator, 2026-11-04; Codex 0.166 ran hooks under its shared `app-server` daemon, so the process directory is the daemon's]
- **HOOKS-1.10** WHEN a Session Lease has ended by Release all and a new `UserPromptSubmit` arrives for that session THE SYSTEM SHALL make a new Lease. [Weigh, AWAKE close-package.md Weigh list]

## 2. A hook never blocks a Harness

**Story:** As the user, I want to quit Navori and keep prompting Claude Code, Codex or Grok, so that a prompt neither stalls nor fails because the hook has no Core to reach.

- **HOOKS-2.1** THE SYSTEM SHALL end the Hook command with exit code 0 in every case, including an unreadable or invalid payload, a missing event or session id, an unknown event, an unusable environment variable or argument, no socket, a Core that is not running, a Core error, an Action reply of invalid or not-a-Harness, and its own deadline. [agent-decided; research note `harness-hook-blocking` says exit 2 blocks a prompt]
- **HOOKS-2.2** THE SYSTEM SHALL write nothing to standard output and nothing to standard error from the Hook command. [agent-decided, picks-r1 F5. Claude Code documents that Claude never sees the stderr of a hook that exits 0 (raw); whether the user sees it, and what Codex does, is not stated]
- **HOOKS-2.3** WHEN the Hook command has not finished its Core call within its client deadline THE SYSTEM SHALL give up and exit. The deadline is 1.5 seconds for `UserPromptSubmit`, `Stop`, `StopCancelled`, `StopFailure`, `SubagentStart` and `SubagentStop`, and 0.7 seconds for `SessionEnd`. [agent-decided; re-checked against the measured start cost in Task 4 before these numbers are kept]
- **HOOKS-2.4** WHEN the Core is not running THE SYSTEM SHALL start no Lease, write no file and not open the app. [Lock only for "ROAD-9 owns the Core-not-running case", PROCS close-package.md:50. AWAKE close-package.md:23 (lock 17) settles `awake run` (runs its command, does not open the app); applying them to the hook, and the wording "starts no Lease, writes no file", are agent-decided]
- **HOOKS-2.5** IF the Hook command is missing, broken or never fires THEN THE SYSTEM SHALL CONTINUE TO end a Session Lease when its Harness process exits or its TTL expires. (guard) [Lock, ARCHITECTURE.md:315]
- **HOOKS-2.6** THE SYSTEM SHALL make the Hook command depend only on `navori-config`, `navori-ipc` and `navori-protocol`, and SHALL NOT link `navori-macos`. [Lock, xtask/src/main.rs:678-682]

## 3. Settings shows each Harness's hook state

**Story:** As the user, I want to open Settings → Integrations and see whether each Harness has Navori's hook, so that I know the Mac will stay awake for it.

- **HOOKS-3.1** THE SYSTEM SHALL add a fourth Settings tab, `Integrations`, beside General, Diagnostics and About. [Weigh, picks-r1 F7; "ROAD-9 chooses where Integrations sits", AWAKE close-package.md:55]
- **HOOKS-3.2** THE SYSTEM SHALL list Claude Code, Codex and Grok Build in that order, one row each, always, whether or not the Harness is present. [Weigh, picks-r1 F8]
- **HOOKS-3.3** THE SYSTEM SHALL give each row one state from this set: Not found, Not installed, Installed, Needs repair, Unreadable. [agent-decided]
- **HOOKS-3.4** WHEN a Harness's config directory does not exist THE SYSTEM SHALL show Not found. [Weigh, picks-r1 F7]
- **HOOKS-3.5** WHEN the config directory exists and the Hook file holds none of Navori's Hook entries for the running profile THE SYSTEM SHALL show Not installed.
- **HOOKS-3.6** WHEN the Hook file holds a complete, correct set of the running profile's Hook entries THE SYSTEM SHALL show Installed. "Correct" is defined in HOOKS-5.1.
- **HOOKS-3.7** WHEN the Hook file holds some but not a complete correct set of the running profile's Hook entries THE SYSTEM SHALL show Needs repair.
- **HOOKS-3.8** WHEN the Hook file exists and cannot be parsed THE SYSTEM SHALL show Unreadable and SHALL NOT offer Install, Repair or Remove for that row. [agent-decided; THR-9]
- **HOOKS-3.9** WHILE a row is Not found THE SYSTEM SHALL disable its Install button and show a one-sentence reason beside it, taken from the Module's status reply and not worded in Swift. [Weigh, picks-r1 F7; ARCH-1]
- **HOOKS-3.10** THE SYSTEM SHALL offer an Install button on Not installed, a Repair button on Needs repair, and a Remove button on Installed and Needs repair. [Lock, ARCHITECTURE.md:314]
- **HOOKS-3.11** THE SYSTEM SHALL serve the state, Install, Repair and Remove as Actions of a new Integrations Module — `status` (Risk read), `install`, `repair` and `remove` (Risk write), each taking a `harness` — so that the Shell, `navori dispatch` and Navori AI reach them the same way. [Weigh, picks-r1 F1; ARCH-4]
- **HOOKS-3.12** THE SYSTEM SHALL read the Hook files only when a status is asked for — from the Integrations tab, `navori doctor` or a dispatch — and SHALL NOT poll them or hold a timer. [agent-decided; ARCH-16]
- **HOOKS-3.13** WHILE the Codex row reads Installed or Needs repair THE SYSTEM SHALL show that Codex runs these hooks only after the user trusts them in `/hooks`. [agent-decided, 2026-11-04; the operator may reword or reverse it. Codex 0.166 skips an untrusted hook with no warning in `codex exec`, seen in a temporary `CODEX_HOME`.]

## 4. Install a Harness's hook

**Story:** As the user, I want to press Install on a Harness and have Navori add its hooks without disturbing mine, so that I set it up once and never edit a config by hand.

- **HOOKS-4.1** WHEN the user installs for Claude Code or Codex THE SYSTEM SHALL add Hook entries for `UserPromptSubmit`, `Stop` and `SessionEnd`; for Codex, once the check task of HOOKS-4.5 is done. [Lock, ARCHITECTURE.md:304, 306]
- **HOOKS-4.2** WHEN the user installs for Grok Build THE SYSTEM SHALL write `~/.grok/hooks/navori.json` with Hook entries for `UserPromptSubmit`, `Stop`, `StopCancelled`, `StopFailure` and `SessionEnd`. [Lock, ARCHITECTURE.md:305]
- **HOOKS-4.3** THE SYSTEM SHALL write each Hook entry as a matcher group holding one handler of type `command`, with the command described in HOOKS-4.4 and the timeout of HOOKS-4.5, and without `async`. [Weigh: research note says `async: true` can reparent the ppid chain to launchd; the group shape is the one Claude settings, `~/.codex/hooks.json` and Grok files already use]
- **HOOKS-4.4** THE SYSTEM SHALL write the command as the quoted absolute path of the Helper followed by ` hook` for the release profile, and the quoted Helper path followed by ` --profile dev hook` for the dev profile. THE SYSTEM SHALL NOT use the `~/.local/bin/navori` link. [Weigh, picks-r1 F4; ARCH-14. Reasons: a hook runs with a minimal PATH, and the link exists only after the user installs the CLI]
- **HOOKS-4.5** THE SYSTEM SHALL write `timeout` for Claude Code and Grok as whole seconds: 2 for every event except `SessionEnd`, and 1 for `SessionEnd`. For Codex THE SYSTEM SHALL write the key and unit that the design's check task finds, and Install for Codex SHALL NOT be built before that task is done. Found 2026-11-04: key `timeout`, whole seconds, so Codex takes the same 2 and 1 (`.skills/research/2026-11-03-codex-hook-timeout.md`). [agent-decided; Grok allows fractions in seconds, `~/.grok/docs/user-guide/10-hooks.md:185`; Codex: the key `timeout` is confirmed locally (binary strings, read only); the unit is documented as seconds (raw) but the docs name no version, and shell versus direct exec is not stated: Task 12]
- **HOOKS-4.6** THE SYSTEM SHALL take the Helper path from the Shell at start, in the Core's configuration. [Weigh, picks-r1 F4; `apps/macos/Navori/App/AppEnvironment.swift:33`]
- **HOOKS-4.7** WHEN the user installs and the Hook file exists THE SYSTEM SHALL write a Backup before changing it. [Lock, ARCHITECTURE.md:314; Backup place and name agent-decided]
- **HOOKS-4.8** WHEN the user installs THE SYSTEM SHALL keep every key, value and array order of the file that is not a Hook entry of the running profile. File whitespace may be rewritten. [Lock "never overwrites other hooks", ARCHITECTURE.md:314; the whitespace allowance is agent-decided and narrows the close package's "every unrelated byte". Design risk: `serde_json` is built without `preserve_order` (Cargo.toml:49), so object key order needs that feature or another reader]
- **HOOKS-4.9** WHEN the user installs and the Hook file does not exist THE SYSTEM SHALL create it, and SHALL NOT create the Harness's config directory (`~/.claude`, `~/.codex`, `~/.grok`). For Grok it SHALL create the `~/.grok/hooks` directory when `~/.grok` exists. [agent-decided; Weigh, picks-r1 F7]
- **HOOKS-4.10** WHEN the user installs and the file already holds the running profile's complete correct set THE SYSTEM SHALL leave the file unchanged and write no Backup. [agent-decided]
- **HOOKS-4.11** IF the target Hook file cannot be parsed THEN THE SYSTEM SHALL change nothing, write no Backup and return an error naming the file and the parse failure. [agent-decided; THR-9]
- **HOOKS-4.12** THE SYSTEM SHALL write the Hook file by writing a temporary file in the same directory and renaming it, so a failure part-way leaves the original intact. [agent-decided]
- **HOOKS-4.13** IF the Hook file path is a symbolic link THEN THE SYSTEM SHALL write to the file it points to and keep the link. [agent-decided; dotfile managers link these files]
- **HOOKS-4.14** WHEN an Install, Repair or Remove ends THE SYSTEM SHALL emit an Event with a one-sentence summary naming the Harness and the outcome, and an error-severity Event when it fails. [agent-decided; AWAKE close-package.md:27 (lock 19) says ROAD-8's own Actions and Events ship with ROAD-8, and this applies the same Definition-of-Done rule to the new Module by analogy]
- **HOOKS-4.15** THE SYSTEM SHALL write to the three Hook file paths and their Backups only, resolving `~` from the Core's configured home directory. [agent-decided; TB-7]
- **HOOKS-4.17** WHEN the user installs or repairs for Claude Code THE SYSTEM SHALL also write Hook entries for `SubagentStart` and `SubagentStop` at timeout 2, and a Claude file from before this change SHALL read as needing repair until then. [Operator's choice, 2026-11-04; fields from `claude-code.d.ts`, Claude Code 2.1.x]
- **HOOKS-4.16** WHEN the user installs for Claude Code THE SYSTEM SHALL also add a Hook entry for `StopFailure`, so that an API error lets the Session Lease end. A user interrupt fires no event, so that path keeps the gap recorded in Open Questions. [Operator's choice, 2026-11-03; `StopFailure` as an event: code.claude.com/docs/en/hooks]
- **HOOKS-4.18** WHEN the user installs or repairs for Codex THE SYSTEM SHALL also write a Hook entry for `Interrupt` at timeout 2, and a Codex file from before this entry SHALL read Needs repair. [Operator, 2026-11-04; Codex clamps this timeout to 1..=3, `codex-rs/hooks/src/engine/discovery.rs:948-970`]

## 5. Repair a hook that drifted

**Story:** As the user, I want one Repair button to fix a hook that points at a moved app or lost an event, so that I do not have to find out what changed.

- **HOOKS-5.1** THE SYSTEM SHALL treat a Harness's entries as correct when, for the running profile, every required event has a Hook entry whose command and timeout equal what Install would write now and which has no `async`. [Weigh, picks-r1 F4; research note on `async`]
- **HOOKS-5.2** THE SYSTEM SHALL recognise the running profile's Hook entry as one whose command is a single-quoted program path ending `/Contents/Helpers/navori`, then ` hook` for release or ` --profile dev hook` for dev, so a moved app's stale entry is found; an entry written any other way counts as foreign. [agent-decided; replaces picks-r1 F4's profile-blind rule after review finding 1]
- **HOOKS-5.3** WHEN the user repairs THE SYSTEM SHALL replace each of the running profile's entries that is not correct, add each missing one and remove each that Install would not write, and SHALL NOT leave two entries of the running profile for one event. [agent-decided]
- **HOOKS-5.4** WHEN the user repairs THE SYSTEM SHALL follow HOOKS-4.7, 4.8, 4.11, 4.12 and 4.13 as Install does.
- **HOOKS-5.5** WHEN the user repairs a Harness that is Installed THE SYSTEM SHALL change nothing. [agent-decided]

## 6. Remove a Harness's hook

**Story:** As the user, I want to take Navori's hook out of a Harness and keep everything else in its config, so that removing Navori leaves nothing of Navori's hooks behind.

- **HOOKS-6.1** WHEN the user removes for Claude Code or Codex THE SYSTEM SHALL delete only the running profile's Hook entries, and any array or object that its removal leaves empty, and keep the file. [Lock "never overwrites other hooks", ARCHITECTURE.md:314; empty-container rule agent-decided, picks-r1 F8]
- **HOOKS-6.2** WHEN the user removes for Grok Build and `navori.json` holds only Hook entries of Navori THE SYSTEM SHALL delete the file after writing a Backup. [agent-decided, picks-r1 F8]
- **HOOKS-6.3** WHEN the user removes for Grok Build and `navori.json` holds anything that is not a Navori Hook entry THE SYSTEM SHALL delete only the running profile's entries and keep the file. [agent-decided, picks-r1 F8; THR-9]
- **HOOKS-6.4** WHEN the user removes THE SYSTEM SHALL follow HOOKS-4.7, 4.11, 4.12 and 4.13 as Install does.
- **HOOKS-6.5** WHEN the user removes and the Hook file holds none of the running profile's entries THE SYSTEM SHALL leave the file unchanged and write no Backup. [agent-decided]

## 7. `navori doctor` reports hooks

**Story:** As the user, I want `navori doctor` to tell me a hook is missing or broken, so that I can fix it before a long job runs while the Mac sleeps.

- **HOOKS-7.1** THE SYSTEM SHALL add one fixed `integrations` check to the Core's doctor reply. [Weigh, picks-r1 F3; shape of AWAKE close-package.md lock 18]
- **HOOKS-7.2** WHEN no present Harness is Needs repair or Unreadable THE SYSTEM SHALL report the check ok. A Harness that is Not found, or that is Not installed, is not a failure. [Weigh, picks-r1 F3]
- **HOOKS-7.3** WHEN a present Harness is Needs repair or Unreadable THE SYSTEM SHALL report the check failed. In both cases THE SYSTEM SHALL name every Harness with its state in the check's detail, so a missing hook is reported even when it is not a failure. [Lock "`navori doctor` … report the hook", ARCHITECTURE.md:314-315; the ok/failed split and the detail's shape are agent-decided]
- **HOOKS-7.4** WHILE the app is not running THE SYSTEM SHALL report no hook state, because the CLI prints only the `app` row then. [agent-decided, accepted in review; `apps/cli/src/main.rs:376-383, 400-401`]
- **HOOKS-7.5** THE SYSTEM SHALL show in Settings → Integrations and in `navori doctor` the same state for a Harness at one moment, because both come from the same computation. [Weigh, picks-r1 F3]

## 8. The dev and release apps keep separate hooks

**Story:** As the author, I want installing the dev app's hooks to leave the release app's hooks alone, so that I can test the hook flow without breaking my daily setup.

- **HOOKS-8.1** WHEN the dev profile installs, repairs or removes THE SYSTEM SHALL touch only entries whose command carries `--profile dev` before `hook`, and WHEN the release profile does, only entries that carry none. [agent-decided; ARCH-14, review finding 1]
- **HOOKS-8.2** WHILE a release entry and a dev entry both exist in one Hook file THE SYSTEM SHALL keep both, and each profile's Settings and doctor SHALL read only its own. [agent-decided; ARCH-14]
- **HOOKS-8.3** WHEN the dev profile installs while a correct release entry exists THE SYSTEM SHALL leave the release entry byte-identical. [agent-decided; ARCH-14]
- **HOOKS-8.4** WHEN a Hook entry runs THE SYSTEM SHALL send its events to the Core of the profile the entry names, release or dev, and to no other, and SHALL ignore the `NAVORI_PROFILE` environment variable to do so. [Lock for the separation, ARCH-14 (no shared sockets), `apps/cli/src/main.rs:24, 140`; ignoring the variable is agent-decided]

## 9. Guards on existing behavior

Files this change touches: `apps/cli/src/main.rs`, `apps/cli/tests/cli.rs`, `crates/ffi/src/lib.rs`, `crates/ffi/src/app.rs`, `crates/ffi/tests/core.rs`, `modules/integrations/` (new), `Cargo.toml`, `apps/macos/Navori/Features/Settings/SettingsView.swift`, `apps/macos/Navori/Core/CoreStore.swift`, `docs/features/`, `docs/ARCHITECTURE.md`, `docs/architecture/integrations.md`. `modules/awake/` is read by this feature and not changed: no behavior to guard beyond the lines below.

- **HOOKS-9.1** (guard) WHEN any `navori` command other than `hook` runs THE SYSTEM SHALL CONTINUE TO behave and exit as it does today, including exit codes 0, 1, 2 and 3 and `awake run` running its command when the Core is down. [`apps/cli/src/main.rs:3, 295`]
- **HOOKS-9.2** (guard) WHEN `navori doctor` runs THE SYSTEM SHALL CONTINUE TO print its existing rows, `config`, `database`, `ipc`, `modules`, `awake` and `logs`, in the same order, and exit 0, 1 or 3 by the same rule. [`crates/ffi/src/app.rs:63`]
- **HOOKS-9.3** (guard) WHEN the Awake Actions `hold_session`, `stop_session` and `end_session` run THE SYSTEM SHALL CONTINUE TO validate, hold and release exactly as shipped, with the same grace, recheck, cap and TTL. [Lock, AWAKE close-package.md:7, 16; `modules/awake/src/actor.rs:272-300`]
- **HOOKS-9.4** (guard) WHEN Settings opens THE SYSTEM SHALL CONTINUE TO show General, Diagnostics and About with their present content. [`SettingsView.swift:5-29`]
- **HOOKS-9.5** (guard) WHEN the app starts THE SYSTEM SHALL CONTINUE TO use the existing `CoreConfig` fields with their meanings; the Helper path is one added field. [`crates/ffi/src/lib.rs:36-44`]
- **HOOKS-9.6** (guard) WHEN a Harness's Hook file holds hooks that are not Navori's THE SYSTEM SHALL CONTINUE TO run them unchanged after every Install, Repair and Remove. [Lock, ARCHITECTURE.md:314]
- **HOOKS-9.7** (guard) THE SYSTEM SHALL CONTINUE TO keep the Lease model, the Awake Actions' names and fields, and the Module's timing constants as shipped. [Lock, AWAKE close-package.md:7, 16]

## 10. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** **HOOKS-10.1** WHILE the menu is closed and no Integrations Action runs THE SYSTEM SHALL add no timer, no wake-up and no file read to the app, so that the idle budget in `docs/standards/performance.md` (idle CPU 0.0%, wake-ups ≤ 1/s, footprint ≤ 30 MB) holds — verified by `cargo xtask perf`. [Standing doc, Approved; ARCH-16]
- **Performance:** **HOOKS-10.2** WHEN the Hook command runs against a running test Core THE SYSTEM SHALL finish inside its client deadline of HOOKS-2.3 — verified by a test that reports the measured round trip, which the design uses to confirm or change the deadlines. No standing latency target exists in the standards. [agent-decided]
- **Security:** **HOOKS-10.3** THE SYSTEM SHALL satisfy THR-6 by accepting a Session Lease only through the Awake Actions, which require a session id of 1–128 bytes of `[A-Za-z0-9._:-]` and a pid inside a Harness, so that a forged payload cannot hold the Mac awake longer than the Module's cap and TTL — verified by a test that sends a forged id and a non-Harness pid. [`docs/security/threat-model.md` THR-6; `modules/awake/src/actor.rs:272-300`]
- **Security:** **HOOKS-10.4** THE SYSTEM SHALL satisfy THR-9 at TB-7 by writing a Backup first, merging only the running profile's entries, refusing a file it cannot parse and writing atomically — verified by tests against a temporary home holding foreign hooks, an unparseable file, a linked file and a failing write. [`docs/security/threat-model.md` THR-9, TB-7]
- **Security:** **HOOKS-10.5** THE SYSTEM SHALL run its tests only against a temporary home directory, so that no test reads or changes the real `~/.claude`, `~/.codex` or `~/.grok`. [Lock, charter hard stop; `CoreConfig.home_dir`, `crates/ffi/src/lib.rs:36-44, 234`]
- **Reliability:** **HOOKS-10.6** IF a Hook file write fails THEN THE SYSTEM SHALL leave the original file as it was and return an error, and a Backup left behind SHALL be a complete copy — verified by a test with a read-only directory. [agent-decided]
- **Accessibility:** **HOOKS-10.7** THE SYSTEM SHALL make every Integrations row reachable and operable by keyboard, give each button a name that includes the Harness, and show state as text and not only as colour. There is no `docs/standards/accessibility.md`, so this target is domain judgement and carries no standing ID — verified by a keyboard walk and a VoiceOver walk on the built app, which are manual and Unverified until run. [agent-decided]

## Out of Scope

- Changing the Lease model, the Awake Actions or the Module's timing numbers. [Lock, AWAKE close-package.md:7, 16]
- Staying awake with the lid closed. [Lock, ARCHITECTURE.md:317]
- Pause, `Let my Mac sleep now` or a `sleep_now` Action. [Lock, AWAKE close-package.md:17]
- The Harness's MCP server entry (the charter's wording; ROAD-24 `mcp-server` is Navori's own server, docs/roadmap/INDEX.md:139). [Lock]
- A session list or `Show terminal`. [Weigh, AWAKE close-package.md Weigh list]
- Detecting a Harness with no hook by watching its process. No roadmap item owns it (`docs/roadmap/INDEX.md` names none); `docs/ARCHITECTURE.md:308` is its only home, and the AWAKE close package lists it off limits for ROAD-8. This feature does not build it. [agent-decided scope line]
- Pruning old Backups. Each Install, Repair or Remove that changes a file writes one; they stay. [agent-decided]
- Rewriting a Hook file's whitespace to match its original. [agent-decided]
- Repairing an unparseable Hook file. The row says Unreadable and the user fixes the file. [agent-decided]
- Hooks for any other Harness, and Cursor's hook file that Grok also reads.
- Ordering a late Grok `StopCancelled` that arrives after the next `UserPromptSubmit`; the Module cannot order it and this feature does not change the Module. [Unverified risk, research note]
- Changing the lease key from `session_id` to `(harness, session_id)`. `docs/ARCHITECTURE.md:312` says the second, the Module keys by the first (`modules/awake/src/domain/table.rs:25-30`); the mismatch is recorded here and in the report for a later realignment. [Weigh, picks-r1 F9]
- The Context Gateway fragment and the eval case that Definition of Done item 5 asks for; they belong to MILE-3, a stated gap against the DoD as in AWAKE. [Lock, AWAKE close-package.md:19]
- A Settings control for the grace, recheck, cap, TTL or the hook timeouts. [Lock, AWAKE close-package.md:15-16]

## Open Questions

- A Codex session's Lease watches the shared `app-server` daemon, which outlives a closed Codex. A normal close removed the Lease in a real run and the source runs `SessionEnd` at session shutdown (`codex-rs/core/src/session/handlers.rs:338`). A Codex killed with no `SessionEnd` is unobserved and may hold until the 8 hour TTL. After a Navori restart a Codex row shows the daemon's project until its next prompt, because the reported directory is not stored — owner: operator — 2026-11-04 — forbid-guess: do not claim a crashed Codex releases sooner than the TTL.
- Whether a recorded Claude Code or Codex `Stop` payload matches the documentation (no `reason` on stdin), which keeps HOOKS-1.3 from dropping a real turn end — owner: operator-recorded payload, Task 11 — 2026-11-03 — forbid-guess: do not widen or narrow the `reason` rule on the strength of documentation alone.
- Whether recorded Claude Code and Codex payloads carry `hook_event_name` and `session_id` as documented, and, for Codex 0.166, the `timeout` unit (documented as seconds, not pinned to a version) and whether `command` runs through a shell (not stated) — owner: operator-recorded run, Tasks 11 and 12 — 2026-11-03 — forbid-guess: do not write a Codex timeout unit or rely on shell quoting for Codex before Task 12 confirms them.
- Whether the user sees the standard error of a Claude Code or Codex hook that exits 0 (Claude Code: Claude never sees it; the user is not stated; Codex: not stated), and so whether HOOKS-2.2 is needed or only harmless — owner: operator dogfood, Task 15 — 2026-11-03 — forbid-guess: do not add a stderr line on the strength of a guess.
- The start cost of the `navori` CLI against the 0.7 s `SessionEnd` deadline of HOOKS-2.3 — owner: harness-hooks-work, measured in Task 4 — 2026-11-03 — forbid-guess: do not shorten the deadline before it is measured.
- Whether Grok's daemon route leaves a live Harness ancestor for a hook's pid chain, and so whether a Grok Session Lease starts there — owner: operator dogfood, Task 15 — 2026-11-03 — forbid-guess: do not claim Grok hooks keep the Mac awake until a real Grok session has been watched.
- Whether a signed build writing into `~/.claude`, `~/.codex` and `~/.grok` triggers a macOS permission prompt — owner: operator dogfood, Task 15 — 2026-11-03 — forbid-guess: do not claim Install works on a signed build until it has run on one.
- Claude Code's `Stop` does not run on a user interrupt (code.claude.com/docs/en/hooks), and no other event fires then, so a Session Lease can last until the next turn's `Stop`, `SessionEnd`, the Harness process exiting or the 8 h TTL. The milestone outcome ("goes back to normal sleep after they stop") is not fully met on that path. This gap is unrun, not measured. API errors are closed by HOOKS-4.16. Codex is unchecked: it has an `Interrupt` event, and whether `Stop` runs on an interrupt is not stated — owner: operator — 2026-11-03 — forbid-guess: do not treat an interrupt as covered until a real session has been watched.
- Codex skips a new or changed hook until the user reviews and trusts it in `/hooks` (docs, raw), and every Repair changes the hook's hash. So a Codex hook that Navori reports as Installed may not run yet. Grok lists its own global hooks and the Claude Code settings file it reads as trusted "After folder trust" (`~/.grok/docs/user-guide/10-hooks.md:68-69`, verified locally), which is a folder trust and not a per-hook review; it states no hash-based hook review. The Claude Code page read for the research note states no trust step, and the audit reports that Claude Code normally picks up direct edits to its hooks through a file watcher (stated by the audit, not seen in the pages read here). Both are documentation reads, unrun. What Settings and `navori doctor` should say about this is undecided — owner: operator — 2026-11-03 — forbid-guess: do not word a Codex row as working until the hook is trusted.
