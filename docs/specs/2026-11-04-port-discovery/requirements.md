# Requirements: Port discovery

Feature code: PORTS
Status: Implemented
Approved by the operator ("approve", 2026-11-04 16:34 +07, typed in the conductor session after reading the review sheet for 827b756; relayed in .skills/_pending-ports/auto/operator-reply.md).
Date: 2026-11-04

<!--
Roadmap item: ROAD-17 port-discovery (MILE-4). Claim: GitHub issue #19 (open, unassigned, label status:shaping). This unattended run did not assign it; its charter names #19 as the only ticket and allows no other tracker change.
Written in an unattended run under .skills/_pending-ports/auto/charter.md (`.skills/` is gitignored: those run files are not in the repository). The operator has not reviewed this file. A value marked `agent-decided` was picked by an agent session, not by the operator. A criterion with no such mark follows a document of record, cited where it matters.
"THE SYSTEM" is the running Core plus the `navori` CLI, unless a criterion names a part. No criterion here is Shell or Swift UI.
"Listener" is one process holding at least one TCP socket in LISTEN state. "Entry" is one row of `ports.list`.
"Counted repo" is a git repository whose root passes PORTS-2.4.
"Fake" means the existing `fake` feature of `navori-macos` and a fake container source. No default test touches Docker. The real process table is read only by real-OS tests, and by crates/ffi tests that start a real core, where the Module scans it read-only.
Grouping reading of L2 and L8 (`agent-decided`): the levels are Project, Worktree, Process, Ports; `harness` is a field on each entry, not a level.

Guard walk (Step 3 of specify-behavior). Files the change touches:
- crates/macos/src/process.rs, lib.rs, fake/processes.rs: new reads beside existing ones. Guards PORTS-9.2, PORTS-9.3.
- crates/macos/src/procargs.rs: argv read stays environment-free. Guard PORTS-9.2.
- crates/procs/src/location.rs: new function beside `resolve_location`. Guard PORTS-9.1.
- crates/procs/src/snapshot.rs, lib.rs, harness.rs, lookup.rs: may gain public items. Guard PORTS-9.3.
- crates/ffi/src/app.rs, crates/ffi/Cargo.toml: Module registered, doctor row added. Guards PORTS-9.4, PORTS-9.5.
- Cargo.toml: workspace member and dependency entry. Guard PORTS-9.5.
- modules/ports/**: new, no behavior to guard.
- crates/kernel/**: no change expected, no behavior to guard. If `Module::tray` is used, guard PORTS-9.6 applies.
- apps/macos/**, apps/cli/**: no change. Guard PORTS-9.7 for the menu; generic dispatch reaches the Module.
- xtask/src/perf/mod.rs, docs/standards/performance.md: PORTS-10.2 adds a metric, and an enabled metric with no measurement fails (performance.md:33). Guard PORTS-9.8.
- docs/architecture/integrations.md, docs/security/threat-model.md: text only, no behavior to guard.
-->

## 1. List my listening ports

**Story:** As the user, I want to run `navori dispatch ports '"list"'`, so that I can read every TCP port my own processes are listening on without running `lsof`.

- **PORTS-1.1** WHEN a scan runs THE SYSTEM SHALL find listeners by listing the current UID's processes, then their socket file descriptors, then their TCP sockets in LISTEN state, without running `lsof` (L6) or any other subprocess (`agent-decided`: the wider rule).
- **PORTS-1.2** THE SYSTEM SHALL read file descriptors and sockets only of processes the scan's listing showed under the current user's real UID, and SHALL drop any result from a process whose start time or real UID changed during the scan (`agent-decided`: the drop, since a pid can be reused between listing and reading).
- **PORTS-1.3** WHEN `ports.list` is dispatched THE SYSTEM SHALL reply with the last published scan and SHALL NOT start a scan for that call (`agent-decided`).
- **PORTS-1.4** THE SYSTEM SHALL return one entry per (pid, port), holding every address that process has bound for that port, so a v4 and a v6 socket on one port are one entry (`agent-decided`). PORTS-4.5 keys Docker ports differently.
- **PORTS-1.5** THE SYSTEM SHALL put these fields on each entry: port, bound addresses, pid, process start time, process name, exe path when read, project, worktree, branch, `missing_path` (the removed working directory or `working_dir` of PORTS-2.5 and PORTS-4.6, absent otherwise), harness, kind, and for a Docker port the container id and name (`agent-decided`: the field set, including what ROAD-18 needs to act on an entry).
  (Amended 2026-11-04 by PSTOP: entries gain `root` and a container's `image`; both additive.)
- **PORTS-1.6** THE SYSTEM SHALL take the process name from the exe path's last part, and IF the exe cannot be read THEN from the short name the OS reports for that pid (`agent-decided`).
- **PORTS-1.7** THE SYSTEM SHALL return entries grouped by project, then worktree, then process, in an order that is the same for the same listeners: no-project group last, then names compared case-insensitively, then pid, then port (L8; `agent-decided`: the order).
- **PORTS-1.8** THE SYSTEM SHALL state in the reply when the scan it carries was taken (`agent-decided`).
- **PORTS-1.9** IF a process exits, or its pid no longer has the start time seen at listing, while its sockets are being read THEN THE SYSTEM SHALL leave out that process's sockets from this scan (`agent-decided`, following crates/procs/src/snapshot.rs:48-85).
- **PORTS-1.10** IF a same-UID process's sockets cannot be read for any other reason THEN THE SYSTEM SHALL state in the reply how many such processes the scan skipped (`agent-decided`).
- **PORTS-1.11** THE SYSTEM SHALL expose `list` on Module `ports` as an Action with Risk `read`, a JSON schema and a description written for people and LLMs.

## 2. See which project a port belongs to

**Story:** As the user, I want each port listed under the project and worktree it came from, so that I can tell at a glance which checkout is holding a port.

- **PORTS-2.1** WHEN a listener's working directory lies in a counted repo THE SYSTEM SHALL set the entry's project, worktree and branch from that repo, using the same project text `resolve_location` gives, so the Shell can join Ports and Awake by project (L13; `agent-decided`: the same-text rule).
- **PORTS-2.2** WHEN a listener's working directory has no counted repo or cannot be read THE SYSTEM SHALL try the same walk from the directory of its exe path (`agent-decided`).
- **PORTS-2.3** IF neither walk finds a counted repo THEN THE SYSTEM SHALL give the entry no project, no worktree and no branch, and group it in the one no-project group, which the Running menu names `Other` (`agent-decided`).
- **PORTS-2.4** THE SYSTEM SHALL count a repo root only when it lies strictly under the user's home directory and not inside a dot-directory directly below home, so `/opt/homebrew` and `~/.oh-my-zsh` place nothing (`agent-decided`). A fixture proves both.
- **PORTS-2.5** IF a listener's working directory no longer exists and its nearest existing ancestor lies in a counted repo THEN THE SYSTEM SHALL take only the project from that repo, set worktree and branch to none, keep the removed path in `missing_path` and infer no name from it (`agent-decided`).
- **PORTS-2.6** IF a listener's exe path cannot be read THEN THE SYSTEM SHALL make no join from the exe and SHALL NOT fall back to `argv[0]` (`agent-decided`).
- **PORTS-2.7** WHEN a process's working directory has been removed THE SYSTEM SHALL read the removed path through the Rust reader, or PORTS-2.5 is reopened — verified by an ignored real-OS test, since this was only read through Python on the same libproc call (`agent-decided`).

## 3. See which Harness started a port

**Story:** As the user, I want a port to name the Harness that started it even after that Harness has exited, so that I can tell an orphaned server from one I started myself.

- **PORTS-3.1** THE SYSTEM SHALL set `harness` to the Harness that started the entry's process: first the nearest recognized live process in its ancestor chain, including itself, then the marker in PORTS-3.2 (GOAL-3; `agent-decided`: this meaning and order).
- **PORTS-3.2** WHEN no live ancestor is recognized THE SYSTEM SHALL read the listener's own exec-time environment and match these exact names: `CLAUDE_CODE_CHILD_SESSION` gives Claude Code, `CODEX_THREAD_ID` gives Codex, `GROK_AGENT` gives Grok Build (`agent-decided`).
- **PORTS-3.3** IF a listener carries more than one of those names THEN THE SYSTEM SHALL give it no Harness, since the names cannot say which is innermost (`agent-decided`). A fixture proves this.
- **PORTS-3.4** THE SYSTEM SHALL match those names only in the environment part of the OS argument buffer, so an argument string such as `GROK_AGENT=1` matches nothing (`agent-decided`). A fixture proves this.
- **PORTS-3.5** IF the environment cannot be read THEN THE SYSTEM SHALL give the entry no Harness for that scan and add no error to it (`agent-decided`).
- **PORTS-3.6** THE SYSTEM SHALL remember a completed environment read, match or no match, once per process identity (pid and start time), keep only the resulting Harness in memory, drop it when the listener goes and never persist it, while a read error is not remembered and the next scan may read again (`agent-decided`).
- **PORTS-3.7** THE SYSTEM SHALL read a process's environment only when its real UID is the current user's and only through the read that re-checks its identity around the read, so a reused pid is never read (`agent-decided`, extending PROCS-8.6, which names argv and the working directory).
- **PORTS-3.8** THE SYSTEM SHALL give a Docker port no Harness: `com.docker.backend` carries no marker. This is a known limit against GOAL-3 (`agent-decided`).
- **PORTS-3.9** WHEN a real listener is started by Claude Code, by Codex and by Grok Build, with that Harness then gone or detached, THE SYSTEM SHALL show that Harness on its entry, and a failure reopens that marker — a manual check before ship, since `CODEX_THREAD_ID` has never been seen on a listener here (`agent-decided`).

## 4. See which container holds a Docker port

**Story:** As the user, I want a Docker port listed under its container's project, so that the databases of one checkout are not all lumped together under one Docker process.

- **PORTS-4.1** WHILE a listener whose exe name is `com.docker.backend` exists THE SYSTEM SHALL read the running containers from the Docker Engine API over its Unix socket on every scan (L12 leaves the read to this design; `agent-decided`: this path and frequency).
- **PORTS-4.2** WHILE no such listener exists THE SYSTEM SHALL NOT contact the Docker socket (`agent-decided`).
- **PORTS-4.3** THE SYSTEM SHALL send only the request that lists running containers, and SHALL send no write to Docker (`agent-decided`). *(Amended 2026-11-04 by PSTOP: this holds for the scan. A `kill` request may send the one `stop` call of PSTOP-3.3, PSTOP-3.9.)*
- **PORTS-4.4** THE SYSTEM SHALL keep from each container only its id, its name, its published host addresses and ports, and the value of its `com.docker.compose.project.working_dir` label, and SHALL drop every other field (`agent-decided`).
- **PORTS-4.5** THE SYSTEM SHALL attribute a Docker port by matching a published (host address, port) to the backend's bound address, returning one entry per (pid, port, container), so two containers publishing one port on different addresses are two entries (`agent-decided`). Fixtures cover that case and a publish on `0.0.0.0` or `::`.
- **PORTS-4.6** WHEN a container has a `working_dir` THE SYSTEM SHALL set the entry's project, worktree and branch from it under PORTS-2.1, PORTS-2.4 and PORTS-2.5 (`agent-decided`): a live linked worktree gives that worktree, and a removed one gives the project, no worktree and no branch.
- **PORTS-4.7** IF a Docker port matches no container, or its container has no usable `working_dir`, THEN THE SYSTEM SHALL keep kind `docker` and give it no project (`agent-decided`).
- **PORTS-4.8** IF the socket is absent or refused, or the read passes its time limit or size limit, or the response cannot be parsed, THEN THE SYSTEM SHALL keep every backend port with kind `docker` and no project, and state in the reply that Docker could not be read (`agent-decided`).
- **PORTS-4.9** THE SYSTEM SHALL bound the read by a time limit and a response size limit, and publish each scan once, with its Docker result, so a hung socket costs one limit while `ports.list` answers from the last published scan under PORTS-7.6 (`agent-decided`; design sets the values).

## 5. See what kind of thing holds a port

**Story:** As the user, I want each port to say whether it is a dev server, database, Docker or macOS's own, so that I can tell what I would be freeing.

- **PORTS-5.1** THE SYSTEM SHALL give each entry one kind, the first that applies of: `docker` when the exe name is `com.docker.backend`; `database` when the exe name is `postgres`, `mysqld`, `mariadbd`, `redis-server`, `valkey-server` or `mongod`; `system` when the entry has no project and the exe path is under a path in PORTS-5.2; `dev_server` when the entry has a project; else `other` (L7 names dev server, database, docker; `agent-decided`: `system`, `other`, the rule and the names). A fixture covers each rule.
- **PORTS-5.2** THE SYSTEM SHALL treat as system paths `/System/`, `/usr/libexec/` and `/usr/sbin/`, and SHALL NOT treat `/System/Volumes/Data/`, `/usr/bin/` or `/usr/local/` as system paths (`agent-decided`; the `/System/Volumes/Data/` exclusion is a precaution).
- **PORTS-5.3** IF an exe path cannot be read THEN THE SYSTEM SHALL match no exe rule for that entry, so it reads `dev_server` with a project and `other` without one, and stays visible (`agent-decided`).
- **PORTS-5.4** THE SYSTEM SHALL list `system` entries in `ports.list` like any other, and leave hiding them to ROAD-18 (L2; the running-menu brief :54).

## 6. Get a URL for a port

**Story:** As the user, I want to dispatch `ports.open` for a port and get its URL back, so that a tool or Navori AI can open the right address.

- **PORTS-6.1** WHEN `ports.open` is dispatched for a port in the last published scan THE SYSTEM SHALL reply with `http://localhost:<port>` for a loopback or wildcard address, or `http://<address>:<port>` for any other (`agent-decided`: the `http` scheme and the address rule).
- **PORTS-6.2** WHEN `ports.open` is dispatched THE SYSTEM SHALL launch nothing and SHALL NOT connect to the port (Risk `read` observes only; `agent-decided`: no probe; ROAD-18 decides when to show `Open`).
- **PORTS-6.3** IF the port is not in the last published scan THEN THE SYSTEM SHALL reply with an error naming the port (`agent-decided`).
- **PORTS-6.4** IF the port has more than one address that gives different URLs and the call names no address THEN THE SYSTEM SHALL reply with an error listing those addresses, and the call MAY name one (`agent-decided`).
- **PORTS-6.5** THE SYSTEM SHALL expose `open` on Module `ports` as an Action with Risk `read`, a JSON schema and a description written for people and LLMs.

## 7. See a new server without asking

**Story:** As the user, I want a server I just started to appear in `ports.list` on its own, so that the list is current when I read it.

- **PORTS-7.1** WHILE the panel is visible THE SYSTEM SHALL scan every 3 seconds.
- **PORTS-7.2** WHILE the panel is not visible THE SYSTEM SHALL scan every 30 seconds.
- **PORTS-7.3** THE SYSTEM SHALL time scans only through the Scheduler's wait handle, with no timer of its own (ARCH-16).
- **PORTS-7.4** WHEN the panel opens THE SYSTEM SHALL run the next scan no later than 3 seconds after, and SHALL NOT scan at the moment of opening (L9; `agent-decided`: the reading of the moment of opening).
- **PORTS-7.5** WHEN the Module starts THE SYSTEM SHALL run its first scan after start-up has finished, within 30 seconds, and not on the start path (`agent-decided`).
- **PORTS-7.6** WHILE a scan is running THE SYSTEM SHALL keep answering `ports.list` and `ports.open` from the last published scan (`agent-decided`).
- **PORTS-7.7** IF a scan comes due while one is running THEN THE SYSTEM SHALL skip it (`agent-decided`).

## 8. Know when the list cannot be trusted

**Story:** As the user, I want `ports.list` and `navori doctor` to say when ports could not be scanned, so that an empty list never reads as nothing running.

- **PORTS-8.1** WHILE no scan has finished THE SYSTEM SHALL reply to `ports.list` with no entries and a statement that nothing has been scanned yet (`agent-decided`).
- **PORTS-8.2** IF a scan fails THEN THE SYSTEM SHALL state the failure in the reply, naming the outcome only, and keep the entries and scan time of the last good scan (`agent-decided`).
- **PORTS-8.3** WHILE the last scan failed THE SYSTEM SHALL fail the `ports` row of `navori doctor` (docs/architecture/integrations.md:43 says the error is shown there; `agent-decided`: shown as a failing row, as `awake_check` does).
- **PORTS-8.4** WHILE the last scan succeeded or none has finished THE SYSTEM SHALL pass the `ports` row of `navori doctor`, including when Docker could not be read (`agent-decided`).
- **PORTS-8.5** IF the Ports Module's state cannot be read THEN THE SYSTEM SHALL fail the `ports` row of `navori doctor` (`agent-decided`, as `awake_check` does).
- **PORTS-8.6** WHILE the last scan failed THE SYSTEM SHALL ask the status icon for the problem glyph (crates/kernel/src/module.rs:70; `agent-decided`), and a Docker read failure SHALL NOT ask for it.

## 9. Keep everything else as it is

**Story:** As the author, I want `cargo xtask ci` to pass with the Ports Module in, so that adding Ports changes nothing that already works.

- **PORTS-9.1** (guard) WHEN Awake labels a Lease THE SYSTEM SHALL CONTINUE TO use `resolve_location` as it is today, including its fallback to the working directory's name when no repo is found (crates/procs/src/location.rs:38-42).
- **PORTS-9.2** (guard) WHEN argv is read THE SYSTEM SHALL CONTINUE TO return it without the environment (PROCS-1.8).
- **PORTS-9.3** (guard) WHEN a snapshot with sessions is taken THE SYSTEM SHALL CONTINUE TO meet PROCS-8.4 and PROCS-8.8.
- **PORTS-9.4** (guard) WHEN `navori doctor` runs THE SYSTEM SHALL CONTINUE TO report every row it reports today, in today's order, with `ports` added after `logs` (`agent-decided`: the position).
- **PORTS-9.5** (guard) WHEN `cargo xtask check-deps` runs THE SYSTEM SHALL CONTINUE TO reject a Module depending on another Module and any crate other than `navori-ffi` depending on a Module, with the new crate named `navori-mod-ports` so those rules see it (xtask/src/main.rs:697-727), and `unsafe` SHALL stay confined to `navori-macos` and `navori-ffi` by the workspace lints.
- **PORTS-9.6** (guard) WHEN port counts change THE SYSTEM SHALL CONTINUE TO leave the status icon's attention mark off (crates/kernel/src/module.rs:71-72).
- **PORTS-9.7** (guard) WHEN the release menu renders THE SYSTEM SHALL CONTINUE TO show no Ports row until ROAD-18 ships it (MENU requirements).
- **PORTS-9.8** (guard) WHEN `cargo xtask perf` runs THE SYSTEM SHALL CONTINUE TO measure every existing metric the same way, and to fail an enabled metric with no measurement.

## 10. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

Consult record: `docs/product/metrics.md` absent, no `docs/ops/reliability.md` (only `docs/ops/deployment.md`), so targets come from GOAL-11, `docs/standards/performance.md` and locks. `docs/security/threat-model.md` is Approved and used for THR-1, THR-3 and THR-5.

- **Performance:** **PORTS-10.1** WHILE the Ports Module runs at the 30 s cadence THE SYSTEM SHALL keep every metric the gate budgets in budget, including idle CPU at 0.0 after rounding, idle wake-ups, footprint, cold start and `.app` size (GOAL-11) — verified by `cargo xtask perf` on the same machine before and after the Module with `com.docker.backend` listening, both results reported (`agent-decided`: the method). No `perf-baseline.json` exists yet, so a red run before the Module is reported, not blamed on it.
- **Performance:** **PORTS-10.2** WHEN a scan runs on this Mac THE SYSTEM SHALL finish it, excluding the Docker read, within 50 ms as the median of 5 — verified by an ignored real-OS test run through the perf gate, not the normal suite, because PROCS-8.4 flaked under load (`agent-decided`: turning "usually < 50 ms" of docs/ARCHITECTURE.md:321 into this limit, and the method).
- **Security:** **PORTS-10.3** THE SYSTEM SHALL let no environment value and no unmatched environment name leave the reader by a return value, an error, a log line, an Event, the Journal, a panic or published state, and SHALL wipe the read buffer (THR-1, THR-3, PROCS-8.5; `agent-decided`: the wipe, as commit 75ffedd does for argument scratch) — verified by a leak test in the style of crates/procs/tests/no_leaks.rs with a fake buffer holding a token.
- **Security:** **PORTS-10.4** THE SYSTEM SHALL write no working directory, argv or container working directory of any process to logs, Events, the Journal or panic text (PROCS-8.5; `agent-decided`: extending it to a container's `working_dir`) — verified by the same leak test.
- **Security:** **PORTS-10.5** THE SYSTEM SHALL never read a process the listing showed under another UID, and SHALL return or count nothing from a pid that another user took over during the scan (THR-5, L11; `agent-decided`: the drop) — verified with `FakeProcesses::reads()` and `replace_on_read`, as PROCS-8.6 does.
- **Reliability:** **PORTS-10.6** WHEN the Module restarts after a crash THE SYSTEM SHALL publish the not-scanned state of PORTS-8.1 and scan again within 30 seconds, with nothing to recover from disk (`agent-decided`: the state and the 30 s) — verified by a test that fails the Module once.
- **Accessibility:** None — ROAD-17 ships no UI; ROAD-18 owns the menu's accessibility.

## Out of Scope

- The ports panel, the Running menu, the `CoreSnapshot` field and all Swift: ROAD-18.
- Stop, kill and `ports.kill`, the `protected` list, and whether `protected` also covers a project row's `Stop N`: ROAD-18 and ROAD-37. *(Amended 2026-11-04 by PSTOP: the `kill` Action, its fixed protected set and the `Stop` controls are built by the PSTOP feature; the approval step stays with ROAD-37.)*
- Re-reading a container before stopping it: ROAD-18. *(Amended 2026-11-04 by PSTOP: PSTOP-3.1 re-reads the running containers before a stop.)*
- A project's checkout path for the child menu's header (running-menu brief :54): no field carries it here; ROAD-18 adds it. *(Amended 2026-11-04 by PSTOP: entries carry `root`, the root of the oldest listener, and the Running view carries the project's checkout path, PSTOP-4.6.)*
- Any write to Docker, or any Engine API call other than listing running containers. *(Amended 2026-11-04 by PSTOP: PSTOP-3.9 allows one more request, `stop` of a container, and no other; the scan itself still sends only the list, PORTS-4.3.)*
- Connecting to a port to learn what it speaks.
- Other UIDs and EndpointSecurity.
- Persisting anything: no Owner, table or migration. No per-port Event.
- A `navori ports` subcommand: generic `navori dispatch` reaches the Actions.
- Known limits, accepted: `kimi-webbridge` reads `dev_server` under `klynt` because its working directory is there. A tool built in a repo joins that repo when its working directory has no repo. A project outside home lands in `Other`. A Lease in a repo PORTS-2.4 rejects shows a project while Ports says none, so the Shell's text join misses it. A Docker backend whose binary was replaced while running loses `docker` until it restarts.

## Open Questions

The coverage gate was off, so no Owned unknowns exist. These inputs stay unverified, each with its check before ship. Each is owned by whoever builds this feature, is due before ROAD-17 ships, and must be measured, not guessed.

- `CODEX_THREAD_ID` on a real listener: in Codex's docs and binary, on no process here — check PORTS-3.9.
- `GROK_AGENT` as a contract: seen live on 74 processes, no docs read — check PORTS-3.9.
- A removed working directory through the Rust reader: read only through Python on the same libproc call — check PORTS-2.7.
- The Docker read's cost in the Rust client: measured only in Python — check PORTS-10.1.
- A publish on `0.0.0.0` or `::`, and one port on two host addresses: never seen here — fixtures in PORTS-4.5.
- `com.docker.compose.project.working_dir` as a documented contract: exported constant in docker/compose `pkg/api/labels.go`, seen live, no docs page — PORTS-4.7 covers its absence.
- The database names in PORTS-5.1: recalled, none runs natively here — a fixture per rule, and the operator's review.
