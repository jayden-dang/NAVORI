# Design: Harness hooks

Feature code: HOOKS
Status: Draft
Date: 2026-11-03
Requirements: ./requirements.md

Source classes as in requirements.md: **Lock**, **Weigh**, **agent-decided**, **Unverified**. Every value this design chooses carries one. The operator has not read this file.

## Context

Awake already has the Lease engine and three Actions for a Harness session — `hold_session {session_id, pid, ppid}`, `stop_session`, `end_session` (`modules/awake/src/action.rs:17-27`). What is missing is everything on the Harness side: nothing calls those Actions when a Harness works, and nothing writes the hooks that would. `navori hook` does not exist, `integrations/` does not exist, and Settings has three tabs (`SettingsView.swift:5-29`).

Three constraints shape the approach. The CLI may link only `navori-config`, `navori-ipc` and `navori-protocol` (`xtask/src/main.rs:678-681`), so it cannot carry the file-merge logic and has no tree walk to do. The Swift Shell holds no logic and reaches Rust only through `CoreStore` (ARCH-1, ARCH-2), so the merge cannot live in Swift. And a hook runs on the critical path of every prompt of a Harness, with a hard timeout the Harness enforces, so the hook command must be small, bounded and always exit 0.

The alternative these rule out is the obvious one: a Swift `HookInstaller` beside `CLIInstaller` that edits JSON in the Shell, with a CLI `hook` that links the Core. It would break ARCH-1, ARCH-2 and the CLI's check-deps rule at once. The design instead puts the merge in a new Module that exposes typed Actions (ARCH-4), keeps `navori hook` a thin mapper from event to existing Awake Action, and lets the Shell call the Module through `dispatch`, as it already does for Awake.

Retrieval (`load-subgraph`, schema 1.1, recipe `fsubr-1.2`, derived again at this tree after the HOOKS card was added; OWNS coverage 6/11, because CORE, SHELL, CLI and BUILD have no Files blocks): `blast_radius` → AWAKE, KCONTRACT and ICON on `apps/cli/src/main.rs`, `apps/cli/tests/cli.rs`, `crates/ffi/src/lib.rs`, `crates/ffi/src/app.rs`, `crates/ffi/tests/core.rs`; AWAKE and MENU on `SettingsView.swift`; PERF, AWAKE, KCONTRACT, ICON and MENU on `CoreStore.swift`; AWAKE, KCONTRACT, PROCS on `Cargo.toml` and `docs/codebase/modules.md`. Observations: none. Every neighbour is finished work, so none collides. CLI has no Files block, so this list does not prove nothing else owns `apps/cli/`. Advisory. Scan digests: `.skills/_pending-harness-hooks/auto/scan.md`, `design-scan.md`.

## Decisions

1. **One thin command, one new Module.** `navori hook` maps an event to one of the three existing Awake Actions and sends it; `modules/integrations` (`navori-mod-integrations`) owns install, repair, remove and status. [Lock for the single command, ARCHITECTURE.md:300; Module and Actions: Weigh, picks-r1 F1]
2. **The hook never sends the pid walk.** It sends its own pid and its parent's; the Awake Module resolves the Harness (`resolve_session`). [Lock, ARCHITECTURE.md:311]
3. **The hook is fail-silent and bounded.** Whole run is inside one deadline from process start; stdin is read on a plain thread; the process ends with `std::process::exit(0)`; a panic is caught and also exits 0. [agent-decided; deadlines from requirements HOOKS-2.3]
4. **Status is computed on demand by one pure function.** `inspect(env)` reads the three files and returns the same record for Settings (`status` Action) and for `navori doctor` (`Inner::doctor` calls it directly). No Module state is read by doctor, so a file edited outside Navori is seen at once. [agent-decided; HOOKS-3.12, 7.5]
5. **Hook entry identity is per profile and by command shape**, parsed from the shell string: one single-quoted program path ending `Contents/Helpers/navori`, then `hook` (release) or `--profile dev hook` (dev). [agent-decided; HOOKS-5.2, 8.1]
6. **Install and Repair are one algorithm** (`converge`): bring the running profile's entries to exactly the desired set, in place where one exists, appended where not. Install on an already-correct file and Repair on an Installed one change nothing. [agent-decided; HOOKS-4.10, 5.3, 5.5]
7. **JSON is edited as a parsed tree that keeps key order.** The workspace `serde_json` gets its `preserve_order` feature (`indexmap` is already in `Cargo.lock`). This changes `Map` ordering for every `serde_json::Value` in the workspace; check task C4 proves the suite still passes before the feature is kept. [agent-decided; HOOKS-4.8]
8. **Writes are temp-file + rename in the file's own directory**, preceded by a copy named `<file>.navori-bak-<unix seconds>`. A symlinked Hook file is resolved first and the link is kept. `tempfile` (already a workspace dependency, dev-only elsewhere) becomes a regular dependency of the new crate. [agent-decided; HOOKS-4.7, 4.12, 4.13]
9. **The Core learns the Helper path and home from `CoreConfig`.** `CoreConfig` gains `helper_path`; `build_kernel` gains an `IntegrationsEnv { home, dev, helper }` argument. [Weigh, picks-r1 F4]
10. **The Settings tab decodes the Module's reply in a small view model; it holds no rules.** The Module returns, per Harness, the state, the reason text and the list of Actions that are allowed; Swift maps state to a label and an icon. [Weigh, picks-r1 F7; ARCH-1]
11. **No ADR.** Nothing here is hard to reverse: the hook-file shape is the Harnesses' own, and the Module is additive. The one workspace-wide change (decision 7) is gated by a check task, not an ADR.
12. **No new third-party dependency.** `tempfile`, `serde_json` (plus its `preserve_order` feature), `schemars` and `serde` are already installed, so no adoption question goes to the operator.

### Check tasks (the requirements' Open Questions, as work)

| ID | Question | Method | Blocks |
|---|---|---|---|
| C1 | Claude Code and Codex stdin keys for event and session id; does `Stop` carry `reason` (HOOKS-1.3, 1.6) | `research` (Context7 and the Harnesses' own docs on disk) plus **payloads recorded by the operator** from a live run. A live Harness run is operator dogfood, not a step of this work session: it would touch the Harness's real session data outside the worktree (charter hard stop) | The Grok keys (verified in `10-hooks.md:673-677`) may be coded now. The Claude and Codex acceptance tests and the `reason` rule wait for C1 |
| C2 | Codex `hooks.json` timeout key and unit for 0.166 (HOOKS-4.5), and whether Codex runs `command` through a shell (so a single-quoted path with a space works) | `research` / Context7 for the pinned version, then an operator-recorded run. The key `timeout` is already seen in the real `~/.codex/hooks.json` (read only); the unit is not | Codex Install (HOOKS-4.1 for Codex) |
| C3 | CLI start cost against the 0.7 s deadline, and how soon a payload reaches stdin (HOOKS-2.3, 10.2) | Time a release-built `navori hook` against the in-process test Core over a temp home (`running_core`, `apps/cli/tests/cli.rs`) — never the real dev app or its data directory. Payload arrival time needs a live Harness, so it is recorded in C5 | The deadline numbers; a payload that reaches stdin after 0.7 s starts no Lease and nothing backstops a missed acquire, so C5 must report it |
| C4 | Does enabling `preserve_order` break the suite or change visible output (decision 7) | Add the feature (it edits `Cargo.lock`: `serde_json` gains `indexmap`), run `cargo xtask ci`, and diff `navori actions`, `navori dispatch awake status` and `navori status --json` output before and after. Order-visible but unasserted today: CLI `to_string_pretty` (`main.rs:415`), dispatch replies to Swift (`lib.rs:381`), the catalog schema (`kernel/src/runtime.rs:155`) | Decision 7 stays or falls back to a path that edits the text by byte range; also grep for JSON text the store writes or hashes (`Store::kv_set` takes a `serde_json::Value`, `crates/store/src`) and confirm old rows still read. If any stored text, comparison or hash depends on sorted order, decision 7 falls back, because a persisted-format change is a charter hard stop |
| C5 | Signed build: macOS permission prompt writing `~/.claude`, `~/.codex`, `~/.grok`; Grok daemon-route ancestor; Claude/Codex stderr on exit 0 (HOOKS-2.2) | Operator dogfood after this stage | Nothing in code; claims in docs stay Unverified |

## Architecture

### `navori hook` command

Satisfies: HOOKS-1.1, HOOKS-1.2, HOOKS-1.3, HOOKS-1.4, HOOKS-1.5, HOOKS-1.6, HOOKS-1.7, HOOKS-1.8, HOOKS-1.9, HOOKS-1.10, HOOKS-2.1, HOOKS-2.2, HOOKS-2.3, HOOKS-2.4, HOOKS-2.5, HOOKS-2.6, HOOKS-8.4, HOOKS-9.1, HOOKS-10.2, HOOKS-10.3
Reuse: rung 2 — `Client::connect`/`call`, `socket_path`, `Failure`, the clap `Command` enum (`apps/cli/src/main.rs:32, 140, 248`); rung 2 — the fail-open `ask_for_command_lease` pattern (`main.rs:294-318`); rung 3 — `std::os::unix::process::parent_id`, `std::process::id`, `std::thread`, `std::sync::mpsc::recv_timeout`. New code (rung 7) only for the event map.
Respects: ARCH-1 (no logic beyond a map), ARCH-3, ARCH-11 (the existing user-private socket), ARCH-14 (`--profile` global flag picks the socket)
Security: THR-6
Interface: `navori hook` — no arguments, payload on stdin, no output, exit 0. Internally `hook::plan(stdin: &[u8]) -> Option<Plan>` with `Plan { action: serde_json::Value, deadline: Duration }`, pure.
Depth: If `plan` vanished, a caller would have to know the per-Harness event table, the Grok `reason` filter, which events carry a pid, and the per-event deadlines — that is the whole feature, and it fits in one pure function of bytes; the I/O around it is a dozen lines.
Locality: extend — `apps/cli/src/main.rs` gains one `Command::Hook` variant and an early branch in `main()` beside `Awake Run`; new `apps/cli/src/hook.rs`. Neighbour `awake run` leaves.

Flow, with `start = Instant::now()` taken first and **before `Cli::parse`**:

0. `main()` looks at the raw arguments first. It skips an optional `--profile <value>` or `--profile=<value>`, and if the next token is `hook` it takes the hook path and never calls clap — any other command, including `awake run -- echo hook`, goes to clap exactly as today (HOOKS-9.1): the profile is `dev` only when the arguments carry `--profile dev`, and is `release` in every other case. The `NAVORI_PROFILE` variable is **ignored** on this path, so a release hook cannot reach the dev Core from a shell that exports `dev`, and a bad value cannot make clap print and exit 2 (which would block a prompt). The runtime build, the panic hook and `catch_unwind` also sit inside this path, so no failure there returns exit 1. [agent-decided; HOOKS-2.1, 8.4]
1. A std thread reads stdin, capped at 16 MiB (`take`), and sends the bytes on a channel; the main thread waits with `recv_timeout(0.7 s)`, the shortest deadline, because the event is not known until the bytes are parsed. A Harness that never closes stdin therefore cannot hold the hook. Past the cap the JSON is cut and fails to parse, so nothing is dispatched. [agent-decided]
2. `plan` parses only `hook_event_name`, `session_id`, `reason` (serde ignores everything else, so prompt text is never kept or sent — HOOKS-1.8). Event map: `UserPromptSubmit` → `{"hold_session":{"session_id","pid":<own>,"ppid":<parent>}}`, both pids converted with `i32::try_from` and nothing dispatched if either fails; `Stop` with no `reason` or `reason == "end_turn"`, `StopCancelled`, `StopFailure` → `{"stop_session":{"session_id"}}`; `SessionEnd` → `{"end_session":{"session_id"}}`; anything else → `None`. Deadline: 0.7 s for `SessionEnd`, 1.5 s otherwise. [Lock for the table; deadlines and the `reason` rule agent-decided]
3. A current-thread tokio runtime (as `main()` builds today) runs `timeout(start + deadline − now, connect + call("core.dispatch", {"module":"awake","action":…}))`. Any error, including `IpcError::NotRunning` and an Invalid reply for a non-Harness pid, is dropped.
4. The whole body is inside `catch_unwind`; a panic hook that prints nothing is installed first. `std::process::exit(0)` ends the process, so the stdin thread cannot delay exit.

Duplicate delivery (HOOKS-1.9), a fresh prompt after Release all (1.10) and the TTL/exit backstop (2.5) are properties of the shipped Awake Module; the hook seam tests them against a real Core rather than adding code. The `check-deps` rule already forbids the CLI from linking the Module or `navori-macos` (2.6); the design adds a dependency on none.

### Integrations Module — shell and Actions

Satisfies: HOOKS-3.11, HOOKS-3.12, HOOKS-4.14, HOOKS-10.1
Reuse: rung 2 — the Awake Module as template: `Module` trait impl in `lib.rs`, `ModuleAction` + `catalog()` in `action.rs`, event constants and one-sentence helpers in `event.rs` (`modules/awake/src/{lib,action,event}.rs`), `ModuleCtx::emit`, `Request::respond`, `NoConfig`. New code (rung 7) for the Actions' bodies.
Respects: ARCH-3 (depends on `navori-kernel` only, no other Module), ARCH-4, ARCH-10, ARCH-13 (a bad Hook file becomes the Unreadable state, never a stop), ARCH-16 (no timer)
Security: THR-9, TB-7
Interface: Module id `integrations`; `IntegrationsAction::{Status, Install{harness}, Repair{harness}, Remove{harness}}`, `harness` ∈ `claude | codex | grok`; every Action replies with `IntegrationsState { harnesses: [HarnessStatus] }` in the fixed order claude, codex, grok, where `HarnessStatus { harness, path, state, reason, offers }`, `state` ∈ `not_found | not_installed | installed | needs_repair | unreadable`, `offers` ⊂ `install | repair | remove`. Risks: `status` read, the other three write. Public to the crate's callers: `Integrations::new(IntegrationsEnv)`, `inspect(&IntegrationsEnv) -> IntegrationsState` (sync, pure of the clock).
Depth: If the Module vanished, a caller would have to know three file locations and shapes, how to recognise Navori's entry per profile, how to merge without losing a foreign hook, how to back up and write atomically, and what each outcome means — and would get the answer from a four-Action interface and one status record.
Locality: new crate `modules/integrations/` (`lib.rs`, `action.rs`, `event.rs`, `state.rs`, `domain/`, `infra/` per `docs/codebase/map.md:45`); workspace `Cargo.toml` gains the member and the `navori-mod-integrations` path dependency. `modules/awake/` leaves.

The Module is stateless between calls: `initial_state` is empty, so nothing is read at start (HOOKS-3.12); each Action scans, acts, rescans and replies with the result. The Module publishes no state (the published state stays the empty initial one) because nothing would refresh it; `kernel.state("integrations")` is not authoritative and nothing reads it. File work runs in `tokio::task::spawn_blocking` so the actor's loop never blocks on disk. Events (`integrations.hook_installed`, `.hook_repaired`, `.hook_removed` at `Info`; `.hook_failed` at `Error`) carry a fixed sentence naming the Harness, never the file's contents (ARCH-6 passes it again at the bus). With no timer and no read while idle, the idle budget is unchanged (HOOKS-10.1), checked by the existing `cargo xtask perf`. [agent-decided names and wording]

### Hook-file model (domain)

Satisfies: HOOKS-3.3, HOOKS-3.4, HOOKS-3.5, HOOKS-3.6, HOOKS-3.7, HOOKS-3.8, HOOKS-4.1, HOOKS-4.2, HOOKS-4.3, HOOKS-4.4, HOOKS-4.5, HOOKS-4.8, HOOKS-4.10, HOOKS-5.1, HOOKS-5.2, HOOKS-5.3, HOOKS-5.5, HOOKS-6.1, HOOKS-6.2, HOOKS-6.3, HOOKS-6.5, HOOKS-8.1, HOOKS-8.2, HOOKS-8.3, HOOKS-9.6
Reuse: rung 5 — `serde_json::Value` with the `preserve_order` feature (decision 7); rung 3 — std only for the shell-string parse. New code (rung 7).
Respects: ARCH-14
Security: THR-9, TB-7
Interface: pure functions over `serde_json::Value`, no I/O: `desired(harness, dev, helper) -> Vec<(Event, Handler)>`; `classify(&Value, harness, dev, helper) -> Classification { state, ours }`; `converge(&mut Value, …) -> Changed`; `strip(&mut Value, …) -> Changed`.
Depth: If it vanished, a caller would need to know the three events-per-Harness tables, the matcher-group shape, the command grammar that marks an entry as Navori's for one profile, and the rules for leaving everything else alone — and would re-derive them at each call site; the four functions keep all of that behind a tree-in, tree-out contract that a table of fixtures tests.
Locality: new — `modules/integrations/src/domain/`.

Shape (verified read-only against the real `~/.codex/hooks.json` and `~/.grok/hooks/orca-status.json`; Claude Code's file is the documented same shape): `{"hooks": {"<Event>": [ {"hooks": [ {"type":"command","command":"…","timeout":N} ]} ]}}`. Navori's group is `{"hooks":[handler]}` with no `matcher`.

- **Events.** Claude and Codex: `UserPromptSubmit`, `Stop`, `SessionEnd`. Grok: those plus `StopCancelled`, `StopFailure`. [Lock, ARCHITECTURE.md:304-306]
- **Command.** `'<helper>' hook` (release) or `'<helper>' --profile dev hook` (dev), the path in POSIX single quotes with `'` written `'\''`, so spaces in `Navori Dev.app` and any metacharacter are inert. [Weigh, picks-r1 F4; the quoting style is agent-decided]
- **Timeout.** 2 for every event but `SessionEnd`, 1 for `SessionEnd`, as integers, for Claude and Grok; Codex follows C2. [agent-decided]
- **Identity.** A handler is the running profile's if its `command` parses to one quoted program whose last three components are `Contents/Helpers/navori`, then exactly `hook` (release) or `--profile dev hook` (dev). A handler of the other profile, or any foreign command, is never touched. Only the single-quoted form is recognised: an older or hand-edited entry with double quotes or none counts as foreign, so Install may then add a second entry beside it; this is documented, not guarded. [agent-decided]
- **Classification.** Root not an object, or `hooks` / an event not the expected type → `unreadable` with the reason. Else collect the running profile's handlers across all events. None → `not_installed`. Exactly the desired set — each required event once with a handler deep-equal to the desired one, alone in a group with no key but `hooks`, nothing in any other event, no `async` — `installed`; anything else `needs_repair`. A directory that is absent is `not_found` (decided by `infra`).
- **`converge`.** Navori's handler must live alone in a group whose only key is `hooks`. For each required event: keep the first of ours that already does, replacing its value with the desired handler; remove every other handler of ours from wherever it is (a group that also holds a foreign handler or a `matcher` keeps those, minus ours); if none was kept, append a new canonical group. Remove ours from every other event. A group left with no handlers because of this is dropped, and so is an event array emptied by it. Foreign handlers, groups, keys and array order are untouched; removal from an object uses `shift_remove`, never `swap_remove` (which `preserve_order` makes the default for `Map::remove` and which would move the last key into the hole). Parsing is the only reformatting: whitespace is rewritten (Out of Scope in requirements).
- **`strip` (Remove).** Remove all of ours; prune groups, event arrays and a `hooks` object that this removal emptied. For Grok, if the document is then `{}`, the file is to be deleted; for Claude and Codex the file is always kept.

### File I/O (infra)

Satisfies: HOOKS-4.7, HOOKS-4.9, HOOKS-4.11, HOOKS-4.12, HOOKS-4.13, HOOKS-4.15, HOOKS-5.4, HOOKS-6.4, HOOKS-10.4, HOOKS-10.5, HOOKS-10.6
Reuse: rung 5 — `tempfile::NamedTempFile::new_in(dir)` + `persist` for the atomic write; rung 3 — `std::fs` for read, copy, metadata, `canonicalize`.
Respects: ARCH-13, ARCH-14
Security: THR-9, TB-7
Interface: `read(env, harness) -> Read::{NoDir, NoFile, Doc(Value, Target), Unreadable(reason)}`; `apply(env, harness, Plan) -> Result<Outcome, ApplyError>` where `Plan` is write, delete or nothing.
Depth: If it vanished, a caller would need to know to resolve a symlink first, copy before writing, write in the same directory, carry the file's mode over, and never create `~/.claude`, `~/.codex` or `~/.grok` — and would need to do so in the same order every time; this keeps that order behind two calls.
Locality: new — `modules/integrations/src/infra/`.

Paths come only from `env.home` + the three fixed relative paths (HOOKS-4.15); the module has no other path input. Order of an `apply`: resolve the target (follow a symlink, keep the link), read and parse (a failure returns before any write or Backup — 4.11), compute the new tree, return `Nothing` if unchanged (no Backup — 4.10, 5.5, 6.5), copy to `<name>.navori-bak-<unix seconds>` beside the resolved file (`create_new`; a collision adds `-1`, `-2`), write a temp file in the same directory, set its permissions explicitly to the original's mode (a `NamedTempFile` starts at 0600; a brand-new file stays 0600), `sync_all`, `persist`. A dangling symlink is `Unreadable`. Ownership, extended attributes and hard links are not kept (a hard-linked file is de-linked); this is a documented limit. [agent-decided] A missing Hook file is created, and for Grok `~/.grok/hooks` is created only when `~/.grok` exists; `~/.claude`, `~/.codex` and `~/.grok` themselves are never created (4.9). If any step after the Backup fails, the original is untouched because the rename never happened (10.6). Every test builds an `IntegrationsEnv` over a `tempfile::TempDir` home (10.4, 10.5); `CoreConfig.home_dir` is already injectable (`crates/ffi/src/lib.rs:36-44`).

### Core wiring (`navori-ffi`) and `navori doctor`

Satisfies: HOOKS-4.6, HOOKS-7.1, HOOKS-7.2, HOOKS-7.3, HOOKS-7.4, HOOKS-7.5, HOOKS-9.2, HOOKS-9.5
Reuse: rung 2 — `build_kernel` registration (`crates/ffi/src/app.rs:241-254`), the doctor row pattern `awake_check` (`app.rs:123, 330`) and `check(name, ok, detail)` (`app.rs:141`). New code (rung 7) for the row's text.
Respects: ARCH-2, ARCH-4, ARCH-13
Surface:
| Reader | Disposition |
|---|---|
| `apps/macos/Navori/Core/CoreStore.swift:26` builds `CoreConfig(...)` | replace — passes `helper_path: AppEnvironment.cliURL.path` |
| `crates/ffi/tests/core.rs:35`, `crates/ffi/tests/config_privacy.rs:25`, `apps/cli/tests/cli.rs:52` build `CoreConfig {…}` | replace — add the field in this change |
| generated Swift bindings for `CoreConfig` | replace — regenerated by the build, never hand-edited |
| `crates/ffi/src/lib.rs:257` is the only caller of `build_kernel`, and `lib.rs:276` builds `Inner` | replace — builds the `IntegrationsEnv` once from `CoreConfig` and passes it to both |
| `Inner::doctor` is called synchronously from the async IPC handler (`app.rs:364`) | leave — three small file reads on a runtime worker, accepted |
| `navori doctor` (`apps/cli/src/main.rs:364-405`) prints whatever rows the Core returns | leave — row-generic; a new row needs no edit |
| Swift `DiagnosticsTab` list and its hint text naming the checks (`SettingsView.swift:106-133`) | replace — hint text names the new row |
| `docs/features/awake.md` ("navori doctor includes an awake row, after modules"), `docs/features/status-icon.md` | compat — still true; new `docs/features/integrations.md` names the new row; no removal needed |
Interface: `CoreConfig.helper_path: String`; `build_kernel(…, integrations: IntegrationsEnv)`; `Inner` holds the same `IntegrationsEnv` (it has `paths`, `profile` and `kernel` today but not the home or the Helper path); `Inner::doctor()` pushes one `integrations` row between `awake` and `logs`.
Depth: n/a — extends `Inner::doctor` and `build_kernel`.
Locality: extend — `crates/ffi/src/lib.rs`, `crates/ffi/src/app.rs`; the row calls `navori_mod_integrations::inspect` directly, not the Module, so doctor stays synchronous and never reads a stale snapshot. [agent-decided; Weigh for the row itself, picks-r1 F3]

The row is ok when no present Harness is `needs_repair` or `unreadable`; its detail names all three Harnesses and their states (`claude: installed, codex: not installed, grok: not found`), so a missing hook is reported without failing (HOOKS-7.2, 7.3). With the app stopped the CLI prints only the `app` row and exits 3, so there is no hook state (7.4) — no code needed. Settings and doctor agree (7.5) because both call `inspect`. Existing rows and exit codes are untouched (9.2).

### Settings → Integrations tab (Shell)

Satisfies: HOOKS-3.1, HOOKS-3.2, HOOKS-3.9, HOOKS-3.10, HOOKS-9.4, HOOKS-10.7
Reuse: rung 2 — `SettingsTab` and the `TabView` in `SettingsView.swift:5-29`, `Form{Section}.formStyle(.grouped)` as in `GeneralTab`, `CoreStore.dispatch(module:actionJSON:)`, the `performAwake` call-and-log pattern in `CoreStore.swift`. New code (rung 7): the view model and the tab.
Respects: ARCH-1, ARCH-2 (no `NavoriCore` import outside `CoreStore`)
Surface:
| Reader | Disposition |
|---|---|
| `SettingsTab` enum and the `TabView` | replace — one added case, tab item and tag |
| `SettingsWindowController.show(tab:)` and its 560×420 window (`SettingsView.swift:29`) | leave — three rows with a caption and buttons fit; verified in the manual walk, else both sizes change together |
| `DiagnosticsTab` hint text | replace — see Core wiring |
Interface: `IntegrationsModel` (`@MainActor @Observable`): `rows: [IntegrationRow]`, `refresh()`, `perform(_ action, harness)`; `IntegrationRow` is the decoded `HarnessStatus`. The tab lists Claude Code, Codex, Grok Build in that order, each with a label and SF Symbol per state (text always, never colour alone), the `reason` caption, and Install / Repair / Remove buttons; Install shows for `not_found` and `not_installed` and is disabled unless `offers` contains it; Repair shows for `needs_repair`; Remove shows when offered; `unreadable` offers nothing and shows the reason. Each button's accessibility label names its Harness. The tab refreshes on appear and after every Action.
Depth: If `IntegrationsModel` vanished, the tab would need to know the JSON shape of the Module's reply and how to build the four Action strings; it keeps that to `refresh`, `perform` and one decoded row type. The tab itself extends `SettingsView`.
Locality: extend — `apps/macos/Navori/Features/Settings/SettingsView.swift`, new `IntegrationsTab.swift` in the same folder; `CoreStore.swift` gains only the `helper_path` argument.

State wording is Swift's mapping of the Module's `state` string; the *reason* sentences come from the Module (HOOKS-3.9), so a Harness that is not found is explained in Rust, not Swift. A dispatch failure shows the error text in the tab and logs it, as `performAwake` does for Awake. There is no browser-rendered surface here, so the template's `## UI design` section is deleted: the Settings window is native SwiftUI.

### Workspace and docs

Satisfies: none — infrastructure (labelled): workspace member, `serde_json` feature, `tempfile` regular dependency, docs. Each line below supports a section above.
Reuse: rung 5 — already-installed dependencies; rung 2 — `docs/features/awake.md` as the template for `docs/features/integrations.md`.
Surface:
| Reader | Disposition |
|---|---|
| every crate using `serde_json::Map`/`Value` object order (workspace `Cargo.toml:49`) | replace — decision 7 changes ordering from sorted to insertion order; check task C4 runs the full suite, and fixtures that assumed sorted output are updated in the same change; if C4 fails beyond fixtures, decision 7 falls back |
| `docs/codebase/modules.md` (module table near line 34, `navori-ffi` row), `docs/codebase/dependencies.md` (rows near lines 22, 25, 27), `docs/codebase/map.md:29` | replace — add the new Module and the `helper_path` note |
| `docs/architecture/integrations.md` "Planned" row for Harness hooks | replace — moves to Built with the real contract |
| `docs/features/awake.md` ("hook install is not", "no Settings tab") | replace — wording updated to point at `integrations.md` |
| `docs/ARCHITECTURE.md:312` and `docs/architecture/integrations.md:40` say `(harness, session_id)` while the Awake key is `session_id` alone (`modules/awake/src/domain/table.rs:25-30`) | frozen — Awake's key is out of scope (requirements Out of Scope); the docs are left and the mismatch is listed in the report for a later realignment |
Interface: none.
Depth: n/a — infrastructure.
Locality: extend — `Cargo.toml`, `modules/integrations/Cargo.toml`, `docs/`.

## Seams for testing

| Seam | Kind | Covers |
|---|---|---|
| `hook::plan` (pure bytes → `Option<Plan>`) | unit | HOOKS-1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8, 2.3 |
| `navori hook` binary against a real in-process Core (`running_core`, `apps/cli/tests/cli.rs`, existing), including `awake run -- echo hook` still running its command and returning its exit code, and runs with `NAVORI_PROFILE=bogus` (must exit 0, no output) and `NAVORI_PROFILE=dev` against a release Core (must reach the release Core) | integration | HOOKS-1.9, 1.10, 2.1, 2.2, 2.4, 2.5, 2.6, 8.4, 9.1, 10.2, 10.3 |
| `domain::{classify, converge, strip}` over fixture trees (new, pure) | unit | HOOKS-3.3, 3.4, 3.5, 3.6, 3.7, 3.8, 4.1, 4.2, 4.3, 4.4, 4.5, 4.8, 4.10, 5.1, 5.2, 5.3, 5.5, 6.1, 6.2, 6.3, 6.5, 8.1, 8.2, 8.3, 9.6 |
| `Integrations` Actions through the Kernel over a temp home (new; the one new public seam, same shape as the Awake tests in `modules/awake/tests/`) | integration | HOOKS-3.11, 3.12, 4.7, 4.9, 4.11, 4.12, 4.13, 4.14, 4.15, 5.4, 6.4, 10.4, 10.5, 10.6 |
| `Inner::doctor` through the Core (`crates/ffi/tests/core.rs`, existing) | integration | HOOKS-4.6, 7.1, 7.2, 7.3, 7.4, 7.5, 9.2, 9.5 |
| `cargo xtask perf` (existing) | e2e | HOOKS-10.1 |
| Settings → Integrations on the built app: sight, keyboard and VoiceOver walk (no Swift test target covers the `Navori` app target) | manual | HOOKS-3.1, 3.2, 3.9, 3.10, 9.4, 10.7 |

## Coverage check

Every ID of HOOKS-1.1 to 10.7 appears in exactly one `Satisfies:` line above, except the two deliberately unmapped:
- **HOOKS-9.3** and **HOOKS-9.7** guard the Awake Actions, Lease model and timing constants. `modules/awake/` is not edited, so no section satisfies them; they are discharged by the existing Awake test suite staying green and by a diff check that `modules/awake/` is untouched.

HOOKS-7.4 is satisfied by *no code*, as noted in Core wiring.
