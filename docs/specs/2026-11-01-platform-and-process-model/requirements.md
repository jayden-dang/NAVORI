# Requirements: Platform layer and process model

Feature code: PROCS
Status: Implemented
Date: 2026-11-01

<!--
Rules:
- Every acceptance criterion gets a hierarchical ID: PROCS-<story>.<criterion>.
- Criteria use EARS phrasing (WHEN / WHILE / IF…THEN / WHERE / ubiquitous).
- Guard requirements protect existing behavior: WHEN <condition> THE SYSTEM SHALL CONTINUE TO <behavior>.
- IDs are immutable once Status is Approved. Retire by strikethrough — never renumber.
-->

Roadmap item: ROAD-6 (MILE-2) · Tracker: #2 · Decisions: frame-change close package (2026-11-01) — scope B, hook ↔ CLI A, discovery A, power-assertion ownership A, platform approach A.

Terms used below:
- **The system** — the two new crates: `navori-macos` (the platform layer, the only crate with hand-written `unsafe`) and `navori-procs` (the process model, built only on safe traits from `navori-macos`). No Module, CLI command or UI uses them yet; ROAD-8 and ROAD-9 are the first consumers.
- **The caller** — Rust code in the Core that uses the system (in practice ROAD-8's Awake Module, ROAD-9's hook handling and the tests of this feature).
- **The user's processes** — processes whose real UID equals the UID the Core runs as.
- **Recognized Harness** — Claude Code, Codex or Grok Build (the Harnesses in `CONTEXT.md`).
- **Recognized terminal host** — Terminal, iTerm2, Ghostty, WezTerm, kitty, Alacritty, Warp, Visual Studio Code, Cursor or Zed.

## 1. Snapshot the user's processes

**Story:** As the developer building Awake, I want one call that returns every process I own with its parent, start time and executable, so that I can see the process tree of my Mac from Rust without touching `unsafe`.

- **PROCS-1.1** WHEN the caller takes a snapshot THE SYSTEM SHALL return every one of the user's processes that is running at that moment, each with its pid, parent pid and start time, and with its executable path when it can be read (otherwise that field is unavailable per PROCS-1.7).
- **PROCS-1.2** THE SYSTEM SHALL exclude from the snapshot every process whose real UID differs from the user's UID.
- **PROCS-1.3** THE SYSTEM SHALL give each process in a snapshot an identity that differs between two processes that held the same pid at different times.
- **PROCS-1.4** WHEN the caller asks a snapshot for the ancestors or the descendants of a process in it THE SYSTEM SHALL return them, ordered nearest first.
- **PROCS-1.5** WHEN the caller asks for the argv or the working directory of a process in a snapshot THE SYSTEM SHALL read and return it for that process only.
- **PROCS-1.6** IF a process exits while a snapshot is being taken THEN THE SYSTEM SHALL leave that process out of the snapshot instead of failing the snapshot.
- **PROCS-1.7** IF the executable path, argv or working directory of a process cannot be read THEN THE SYSTEM SHALL return that field as unavailable with the reason (exited, permission denied, malformed) instead of failing the snapshot.
- **PROCS-1.8** THE SYSTEM SHALL return the argv of a process without the environment variables that follow it in the OS argument buffer.

## 2. List the running Agent sessions

**Story:** As the developer building Awake, I want to turn a snapshot into the list of running Agent sessions with Harness, project, worktree, branch, pid and terminal host, so that the Awake submenu can show rows like `Claude · NAVORI/main` / `working · pid 23719`.

- **PROCS-2.1** WHEN the caller asks a snapshot for Agent sessions THE SYSTEM SHALL return one Agent session per running process of a Recognized Harness, including a Harness that runs under a generic runtime such as `node` or `bun`.
- **PROCS-2.2** THE SYSTEM SHALL NOT return a process as a separate Agent session when one of its ancestors is a process of the same Recognized Harness.
- **PROCS-2.3** WHEN an Agent session's working directory is inside a git repository's main checkout THE SYSTEM SHALL report the project as the name of that checkout's root directory, no worktree, and the branch checked out there.
- **PROCS-2.4** WHEN an Agent session's working directory is inside a linked git worktree THE SYSTEM SHALL report the project as the name of the main checkout's root directory, the worktree as the name of the linked worktree's root directory, and the branch checked out in that worktree.
- **PROCS-2.5** WHEN an Agent session's working directory is inside a git submodule THE SYSTEM SHALL report the submodule as the project and SHALL NOT report it as a worktree.
- **PROCS-2.6** IF the HEAD of the resolved repository is detached THEN THE SYSTEM SHALL report the branch as detached together with the abbreviated commit id.
- **PROCS-2.7** IF an Agent session's working directory is not inside a git repository THEN THE SYSTEM SHALL report the project as the name of the working directory, with no worktree and no branch.
- **PROCS-2.8** IF an Agent session's working directory cannot be read THEN THE SYSTEM SHALL still return the session, with the project, worktree and branch unavailable.
- **PROCS-2.9** WHEN an ancestor of an Agent session's process is a Recognized terminal host THE SYSTEM SHALL report the nearest such ancestor as the session's terminal host.
- **PROCS-2.10** IF no ancestor of an Agent session's process is a Recognized terminal host (for example under tmux or after re-parenting to launchd) THEN THE SYSTEM SHALL report the terminal host as unknown.

## 3. Find the Harness behind a hook caller

**Story:** As the developer building `navori hook`, I want the Core to tell me which Harness process a calling process belongs to, so that the Lease can watch the real Harness instead of the short-lived hook process.

- **PROCS-3.1** WHEN the caller asks which Harness a running process belongs to THE SYSTEM SHALL return the nearest ancestor of that process (or the process itself) that is a process of a Recognized Harness, with its Harness and its identity.
- **PROCS-3.2** IF no ancestor of the process, nor the process itself, belongs to a Recognized Harness THEN THE SYSTEM SHALL answer "not a Harness" without an error.
- **PROCS-3.3** IF the process the caller names is not running THEN THE SYSTEM SHALL answer "not running", distinct from "not a Harness".

## 4. Learn the moment a process exits

**Story:** As the developer building Awake, I want one exit probe that watches a child it starts, kills it and prints the exit event it receives without polling, so that I can see a Lease will end when its Harness dies.

- **PROCS-4.1** WHEN a watched process exits THE SYSTEM SHALL deliver one exit event carrying that process's identity.
- **PROCS-4.2** IF a process has already exited when the caller starts watching it THEN THE SYSTEM SHALL deliver its exit event at once instead of an error.
- **PROCS-4.3** IF the pid of a watched identity now belongs to a different process THEN THE SYSTEM SHALL treat the watched process as exited.
- **PROCS-4.4** WHEN the caller stops watching a process THE SYSTEM SHALL deliver no further event for it.
- **PROCS-4.5** WHEN the caller drops the watcher THE SYSTEM SHALL release every OS resource the watcher held (descriptors and threads).
- **PROCS-4.6** WHEN the caller creates a new watcher after dropping a previous one THE SYSTEM SHALL watch the processes registered on the new watcher with the same behavior as PROCS-4.1 – PROCS-4.4.

## 5. See whether a Harness still has work running

**Story:** As the developer building Awake, I want to see the live descendants of a Harness process and the CPU time each has used, so that Awake can keep a Lease while background work runs after the Harness says `Stop`.

- **PROCS-5.1** WHEN the caller asks for the activity of a running process THE SYSTEM SHALL return that process's live descendants, each with its identity and its cumulative CPU time.
- **PROCS-5.2** IF the process the caller names is not running THEN THE SYSTEM SHALL answer "not running".

## 6. Hold and release a power assertion

**Story:** As the developer building Awake, I want one assertion lifecycle probe that holds an assertion, checks `pmset -g assertions` lists it, releases it and checks it is gone, so that I can see Navori keeps the Mac awake exactly while an assertion is held.

- **PROCS-6.1** WHEN the caller holds a system assertion with a reason THE SYSTEM SHALL create one `PreventUserIdleSystemSleep` assertion named with that reason.
- **PROCS-6.2** WHEN the caller holds a display assertion with a reason THE SYSTEM SHALL create one `PreventUserIdleDisplaySleep` assertion named with that reason.
- **PROCS-6.3** WHEN the caller releases a held assertion THE SYSTEM SHALL release it at the OS, so `pmset -g assertions` no longer lists it.
- **PROCS-6.4** IF the caller stops referencing a held assertion without releasing it THEN THE SYSTEM SHALL release it at the OS.
- **PROCS-6.5** IF the OS refuses to create an assertion THEN THE SYSTEM SHALL return an error carrying the OS return code and hold nothing.

## 7. Keep `unsafe` inside the platform crate

**Story:** As the developer, I want one guard probe — prepared compile-fail fixtures run through the lints CI uses — that shows `unsafe` outside `navori-macos` and an `unsafe` block without `// SAFETY:` are both rejected, so that ARCH-9 holds by machine and not by memory.

- **PROCS-7.1** THE SYSTEM SHALL build `navori-procs` under the workspace lint `unsafe_code = "forbid"`.
- **PROCS-7.2** IF an `unsafe` block in `navori-macos` has no `// SAFETY:` comment THEN `cargo xtask ci` SHALL fail — enforced by `clippy::undocumented_unsafe_blocks = "warn"` in both `[workspace.lints.clippy]` and `navori-macos`'s own lint table, which CI's `-D warnings` turns into a failure.
- **PROCS-7.3** THE SYSTEM SHALL expose from `navori-macos` only items callable without `unsafe`.
- **PROCS-7.4** (guard) WHEN `cargo xtask ci` runs THE SYSTEM SHALL CONTINUE TO forbid `unsafe` code in every workspace crate that inherits the workspace lints.
- **PROCS-7.5** (guard) WHEN `cargo xtask check-deps` runs THE SYSTEM SHALL CONTINUE TO reject any non-dev dependency of `navori-cli` other than `navori-config`, `navori-ipc` and `navori-protocol`, so the CLI never links `navori-macos` or `navori-procs`.
- **PROCS-7.6** (guard) WHEN `cargo xtask check-deps` runs THE SYSTEM SHALL CONTINUE TO reject every non-dev dependency on `navori-ffi`.
- **PROCS-7.7** (guard) WHEN `cargo xtask ci` runs THE SYSTEM SHALL CONTINUE TO pass every existing test of the workspace.
- **PROCS-7.8** IF a workspace crate other than `navori-ffi` and `navori-macos` does not inherit the workspace lints THEN `cargo xtask check-deps` SHALL fail and name the crate.

## 8. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** (GOAL-11, `docs/standards/performance.md`, ARCH-16; no `docs/product/metrics.md` or `docs/ops/reliability.md` exists)
  - **PROCS-8.1** WHILE no watcher exists THE SYSTEM SHALL run no thread and no timer of its own — verified by a test that counts the process's threads before and after taking a snapshot, resolving sessions and holding an assertion.
  - **PROCS-8.2** WHILE a watcher exists and no watched process exits THE SYSTEM SHALL cause no wakeup — verified by a test that registers a watch on a sleeping child and measures, over 10 s, zero events, no idle package wakeups and a process CPU-time increase below 2 ms (no CPU time = below measurement noise).
  - **PROCS-8.3** WHEN a watched process exits THE SYSTEM SHALL deliver its exit event from an OS exit notification, never from polling — verified by a real-OS test that kills a child and receives the event within 10 ms on this Mac (spike: 0.2 ms median) while the watcher registers no timer.
  - **PROCS-8.4** WHEN the caller takes a snapshot and resolves its Agent sessions on this Mac (about 1,250 processes, about 890 of them the user's) THE SYSTEM SHALL finish within 10 ms, median of 5 runs — verified by a timed real-OS test in the normal test suite (a debug build, the pessimistic case; spike: about 4 ms), with the release figure recorded once in task evidence.
- **Security:** (`docs/security/threat-model.md`, Approved)
  - **PROCS-8.5** THE SYSTEM SHALL NOT write the argv, the environment or the working directory of any process to logs, Events, the Journal or panic text (logs: THR-3 and `docs/standards/errors-logging.md`; Events, Journal and panic text: ARCH-6) — verified by a test that captures `tracing` output and error `Display` text while resolving processes whose argv holds a fake token.
  - **PROCS-8.6** THE SYSTEM SHALL NOT read the argv or the working directory of a process whose real UID differs from the user's UID (extends THR-5's current-UID-only rule from acting on processes to reading them) — verified by a test with a fake platform layer that records which pids had argv or working directory read.
- **Reliability:** (no `docs/ops/reliability.md`; domain judgment)
  - **PROCS-8.7** THE SYSTEM SHALL NOT panic on any argument buffer the OS returns — verified by a property-based test that parses random and truncated buffers.
- **Accessibility:** None — the feature has no UI; it is two Rust crates with no user-facing surface.

## Files touched and guards

| File | Existing behavior | Guard |
|---|---|---|
| `Cargo.toml` (workspace members, dependencies, lints) | Workspace lint forbids `unsafe` in every crate that inherits it; `navori-ffi` opts out | PROCS-7.4 |
| `Cargo.lock` | — | no behavior to guard |
| `crates/macos/**` (new) | — | no behavior to guard |
| `crates/procs/**` (new) | — | no behavior to guard |
| `xtask/src/main.rs` (check-deps, CI lint step) | CLI thin-client rule (non-dev dependencies only); nothing has a non-dev dependency on `navori-ffi` (`navori-cli` dev-depends on it); CI runs fmt, clippy, tests, check-deps, Swift build | PROCS-7.5, PROCS-7.6, PROCS-7.7 |
| `docs/architecture/system.md`, `docs/architecture/INDEX.md` (ARCH-9 enforcement row to name `navori-macos` beside `navori-ffi`), `docs/codebase/map.md`, `docs/codebase/modules.md`, `docs/codebase/dependencies.md`, `docs/ARCHITECTURE.md` (process-model and Awake seeds) | documentation only | no behavior to guard |

## Out of Scope

- Socket, port and listening-socket discovery, and port grouping — ROAD-17 (MILE-4).
- Any API shaped for the Context Engine — MILE-3.
- Reading processes of another UID beyond excluding them; acting on any process (signals, kill) — ROAD-18.
- Any timer, periodic rescan or discovery schedule — consumers refresh through the Scheduler (ROAD-7).
- Lease, Reconciler, TTL, grace and the system/display policy — ROAD-8.
- `navori hook`, its IPC payload, timeout and behavior when the Core is not running — ROAD-9.
- Idle-wakeup and footprint gating of the whole app — ROAD-5 and ROAD-28.
- Harnesses other than Claude Code, Codex and Grok Build, and configuring Harnesses or terminal hosts from the UI.
- Wiring the crates into `navori-ffi` or any Module — the first consumer (ROAD-8) does it.
- EndpointSecurity — not used under the current posture (restricted entitlement); revisit if signing or entitlements change.

## Open Questions

None. The four questions raised at approval were settled by the design spike and research (2026-11-01): exit latency and snapshot cost are now the measured bounds in PROCS-8.3 and PROCS-8.4 (`.skills/PROCS/spike.md` Q1, Q4); argv and working directory of `node`-hosted and native Harness processes are readable from a Hardened-Runtime-signed binary (spike Q2); Claude Code, Codex and Grok Build run `UserPromptSubmit`, `Stop` and `SessionEnd` command hooks synchronously by default (`.skills/research/2026-11-01-harness-hook-blocking.md`).
