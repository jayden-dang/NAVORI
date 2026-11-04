# Requirements: Ports panel and stop

Feature code: PSTOP
Status: Draft
Date: 2026-11-04

<!--
Roadmap item: ROAD-18 ports-panel-and-stop (MILE-4). Claim: GitHub issue #25 (open, unassigned, labels enhancement and status:shaping). This unattended run did not assign it: docs/agents/issue-tracker.md configures no Assign operation and the charter allows no other tracker change.
Written in an unattended run under .skills/_pending-ports-panel/auto/charter.md (`.skills/` is gitignored: those run files are not in the repository). The operator has not reviewed this file. `(Weigh, card N)` marks a value picked by the peer session ports-panel-interpret in interview card N; `agent-decided` marks a value picked by an agent session. Neither is the operator's own choice. A criterion with neither mark follows a document of record, cited where it matters. The decisions are listed in .skills/_pending-ports-panel/auto/close-package.md (approved by the peer, not by the operator) and ledger.md.
"THE SYSTEM" is the running Core plus the `navori` CLI, unless a criterion names the Shell (the Swift app).
"Listener" is one process holding at least one TCP socket in LISTEN state (CONTEXT.md). "Entry" is one row of `ports.list` (PORTS-1.5). "Target" is what a `kill` request names: a process by (pid, start time) or a container by id.
"Working" is a project with at least one session Lease that AWAKE-16.5 does not report as stopped. "Idle" is a known project with none. "Unknown" is a project whose working state the Core cannot establish (PSTOP-4.4, 4.5). The Running menu draws idle as an orange ring and working as a filled green dot (running-menu brief, Surface 2).
"Stoppable list" is the part of the Running view that names, per project, the targets the Shell sends back: it leaves out protected targets (PSTOP-2.1, 2.3). N in `Stop N` is the number of ports in that list. Targets are deduplicated by process or container, so one target can hold several ports, and a result applies to every port of its target.
"Fake" means the existing `fake` feature of `navori-macos`, a fake signaller added by this feature and a fake container source. No default test signals a real process or touches Docker.
The ARCH-5 limit (`agent-decided` wording, Weigh card 1): ARCH-5 says a Destructive Action runs only after explicit human approval. Nothing in the kernel, FFI or CLI reads `Risk` today (rg over crates/kernel, crates/ffi, crates/ipc, apps/cli, 2026-11-04). This feature adds the first Destructive Action. Its only approval is the Shell's inline confirmation. A CLI or IPC caller of the same user is not asked (PSTOP-1.17). ARCH-5 is not met for that caller until ROAD-37.

Guard walk (Step 3 of specify-behavior). Files the change touches:
- modules/ports/src/action.rs, actor.rs, lib.rs, state.rs: new Action, stop path, new env members. Guards PSTOP-1.18, 1.19, 1.20, 2.6.
- modules/ports/src/infra/docker.rs, docker.rs: a second Engine request beside the list call. Guards PSTOP-3.7, 3.8.
- crates/macos/src/process.rs, lib.rs, fake/: a signalling seam and a group read beside the process reads. No behavior to guard beyond PSTOP-1.18 to 1.20 (PSTOP-1.16 is a rule on the new seam, not a guard).
- crates/ffi/src/lib.rs, app.rs: a new snapshot field built from two Modules' states. Guards PSTOP-4.18, 4.20.
- apps/macos/Navori/Core/CoreStore.swift: the one binding importer gains the stop call and the new field. Guard PSTOP-4.18.
- apps/macos/Navori/Features/Panel/PanelView.swift, AwakeSubmenuView.swift, MenuRowView.swift and new Running views; apps/macos/Navori/StatusBar/StatusBarController.swift; apps/macos/NavoriPanel/*: the Awake row becomes the Running row. Guards PSTOP-4.18, 4.19, 4.20.
- apps/macos/Navori/Features/Settings/SettingsView.swift: one sentence. Guard PSTOP-8.2.
- docs/architecture/integrations.md: the Docker row. No behavior to guard.
- docs/specs/2026-11-02-awake-leases/requirements.md: amended in place by this feature (PSTOP-4.19). No new behavior; the guards on Awake's Module are PSTOP-4.18.
- crates/kernel/**, crates/protocol/**, crates/procs/**, crates/store/**: no change expected, no behavior to guard.
-->

## 1. Stop exactly the listeners I name

**Story:** As the user, I want to dispatch `ports.kill` for listeners I was shown and get a result for each, so that their ports are freed and nothing I did not name is touched.

- **PSTOP-1.1** THE SYSTEM SHALL register on the Ports Module an Action `kill` with Risk `destructive`, a description written for people and LLMs, and a JSON schema (ARCH-4; Seed, docs/ARCHITECTURE.md:319-326).
- **PSTOP-1.2** THE SYSTEM SHALL accept a `kill` request made of a selector and a non-empty list of targets (Weigh, card 1: the exact targets travel with the request). The selector is `pid`, or `worktree` with a project and a worktree, or `project`; an absent project in a `project` selector means the no-project group `Other` (PORTS-2.3). A process target is (pid, start time); a container target is a container id. The selector keeps the Seed's three forms (agent-decided, over the peer's lean to drop the selector: the Seed's `ports.kill{pid|worktree|project}` is a Lock).
- **PSTOP-1.3** THE SYSTEM SHALL act only on the targets in the request and SHALL NOT add a process, container or port the request does not name, whatever the selector (Weigh, card 2 follow-up: a selector is a consistency check on the targets, never an expansion).
- **PSTOP-1.4** IF the target list is empty, a target is malformed, or a `pid` selector does not name exactly one process target THEN THE SYSTEM SHALL reject the request as invalid and signal nothing.
- **PSTOP-1.5** WHEN a process target is handled THE SYSTEM SHALL read that pid again and, IF no such process exists, answer `AlreadyGone`; IF one exists whose start time differs from the target's, whose real UID is not the current user's, or which is no longer a listener in a fresh scan, answer `Stale`; and in both cases signal nothing (Weigh, card 1; integrations.md:28 "only processes of the current UID").
- **PSTOP-1.6** WHEN the selector is `worktree` or `project` THE SYSTEM SHALL check from the target's fresh read that it lies in the named worktree or project, and otherwise SHALL answer `OutsideSelector` for that target and signal nothing (Weigh, card 2).
- **PSTOP-1.7** WHEN a process target has passed PSTOP-1.5, PSTOP-1.6 and the protected rules of story 2 THE SYSTEM SHALL send SIGTERM, wait 3 seconds, check again that the same pid still has the same start time, and only then send SIGKILL (Lock: integrations.md:28).
- **PSTOP-1.8** THE SYSTEM SHALL signal a target's process group only when the target is its group's leader, the group is neither Navori's own nor that of one of its ancestors, every other listener found in that group is in the request's target list and not protected, and every such listener could be read; in every other case it SHALL signal the pid alone (Weigh, card 4: leader-only; the group-listener rule is agent-decided, from the peer's finding that a leader's group on this Mac held two unrelated listeners).
- **PSTOP-1.9** THE SYSTEM SHALL answer each process target with one of `Stopped` (gone, the group was signalled), `StoppedAlone` (gone, the pid alone was signalled), `AlreadyGone` (no such process at the re-read of PSTOP-1.5), `StillStopping`, `Stale`, `Protected`, `OutsideSelector` or `Failed` with a fixed sentence (Weigh, cards 4 and 5; the result names are agent-decided).
- **PSTOP-1.10** IF a signal is refused by the OS, or 1 second after SIGKILL the same pid still has the same start time (`agent-decided`: the 1 second, inside the deadline of PSTOP-1.11), THEN THE SYSTEM SHALL answer `Failed` for that target with a fixed sentence that names no path and no environment value, and SHALL go on with the others.
- **PSTOP-1.11** THE SYSTEM SHALL reply to a `kill` request once, after every target has an answer or when 25 seconds have passed since the request, answering `StillStopping` for any target not finished, with one answer per target in the request's order (`agent-decided`: the 25 s deadline, under the 30 s dispatch timeout at crates/kernel/src/runtime.rs:308).
- **PSTOP-1.12** WHILE a `kill` request is being carried out THE SYSTEM SHALL keep answering `ports.list` and `ports.open` and keep its scan cadence (`agent-decided`: the stop runs as its own task, as scans do).
- **PSTOP-1.13** IF the Ports Module stops or is restarted while a `kill` request is in flight THEN THE SYSTEM SHALL send no further signal for that request (`agent-decided`).
- **PSTOP-1.14** WHEN a `kill` request has been answered THE SYSTEM SHALL start a scan at once unless one is in flight, so that the next published state shows the result (`agent-decided`).
- **PSTOP-1.15** WHEN a `kill` request has been answered THE SYSTEM SHALL emit one Event whose one-sentence summary gives the number of targets per answer and names no process, path or container (`agent-decided`).
- **PSTOP-1.16** THE SYSTEM SHALL keep every signalling and process-group call in `navori-macos` behind a trait with a fake, so that `unsafe` stays confined as PORTS-9.5 requires and no default test signals a real process (docs/standards/testing.md).
- **PSTOP-1.17** WHEN a valid `kill` request arrives from the CLI or the IPC socket THE SYSTEM SHALL carry it out without a further approval step, until ROAD-37 adds one (Weigh, card 1; ARCH-5 limit above).
- **PSTOP-1.18** (guard) WHEN `ports.list` or `ports.open` is dispatched THE SYSTEM SHALL CONTINUE TO reply as PORTS-1.3, PORTS-6.1, PORTS-6.2 and PORTS-7.6 require, from the last published scan and without starting one.
- **PSTOP-1.19** (guard) WHEN the Ports Module runs THE SYSTEM SHALL CONTINUE TO scan every 3 s while the panel is open and every 30 s while it is closed, and the port scan SHALL CONTINUE TO meet the 50 ms `port_scan_ms` budget (modules/ports/tests/timing.rs; `cargo xtask perf`).
- **PSTOP-1.20** (guard) WHEN the dev and release profiles both run THE SYSTEM SHALL CONTINUE TO share no data, log or socket (ARCH-14), and a `kill` SHALL write its Event only to the running profile's Journal.

## 2. Never stop what could hurt

**Story:** As the user, I want a group stop to leave databases, system services, Docker and Navori alone, so that pressing `Stop` is safe.

- **PSTOP-2.1** WHEN the selector is `worktree` or `project` THE SYSTEM SHALL answer `Protected` for a target whose fresh read has kind `Database` or `System`, or which is a container whose image name is a database image (`postgres`, `mysql`, `mariadb`, `redis`, `valkey`, `mongo`; the list is `agent-decided`, read from the `Image` of the Engine list reply, with no new request), and SHALL signal nothing for it (Weigh, card 2; the image rule is Weigh, interpret's finding 2 on the design).
- **PSTOP-2.2** WHEN the selector is `pid` THE SYSTEM SHALL handle a `Database` or `System` target like any other (Weigh, card 2: the user pointed at that one process).
- **PSTOP-2.3** THE SYSTEM SHALL, under every selector, answer `Protected` and signal nothing for the Docker backend process (`com.docker.backend`), for Navori's own Core process, for any ancestor of it, and for the `navori` helper process (Weigh, card 2).
- **PSTOP-2.4** THE SYSTEM SHALL decide protection from the target's fresh read and SHALL NOT take a kind, a name or a flag from the request.
- **PSTOP-2.5** THE SYSTEM SHALL fix the protected set in code and SHALL NOT add a config key, file or table for it (`agent-decided`: a configurable list, which THR-5 names, needs a persisted format; it is a later ROAD).
- **PSTOP-2.6** (guard) WHEN the Ports Module reads its config THE SYSTEM SHALL CONTINUE TO use `NoConfig` and to accept the config files of today.

## 3. Stop a container

**Story:** As the user, I want a project's containers stopped by the same press as its processes, so that a Compose project's ports are freed.

- **PSTOP-3.1** WHEN a container target is handled THE SYSTEM SHALL list the running containers again and go on only if one with that id is running and still publishes a port to the host, and otherwise SHALL answer `Stale` for that target (port-discovery/requirements.md:162; Weigh, card 5).
- **PSTOP-3.2** WHEN the selector is `worktree` or `project` THE SYSTEM SHALL check from the fresh read that the container lies in the named worktree or project, as PSTOP-1.6 does for a process.
- **PSTOP-3.3** THE SYSTEM SHALL send one Docker Engine `stop` request per container however many of its ports are listed, without a `t` value, so that the container's own stop timeout applies (Weigh, card 5).
- **PSTOP-3.4** THE SYSTEM SHALL answer a container target `Stopped` on a 204 reply, `AlreadyGone` on a 304 or 404 reply, `StillStopping` when the request deadline of PSTOP-1.11 ends first, and otherwise `Failed` with a fixed sentence that never repeats the daemon's reply text (Weigh, card 5).
- **PSTOP-3.5** THE SYSTEM SHALL carry out container stops and process stops of one request at the same time, inside the one deadline of PSTOP-1.11 (`agent-decided`).
- **PSTOP-3.6** IF Docker cannot be reached THEN THE SYSTEM SHALL answer every container target `Failed` and SHALL go on with the process targets.
- **PSTOP-3.7** (guard) WHEN the running containers are listed THE SYSTEM SHALL CONTINUE TO use the 2 second and 1 MiB limits of today, and the stop request SHALL have its own, longer time limit.
- **PSTOP-3.8** (guard) WHEN a container is stopped THE SYSTEM SHALL CONTINUE TO write no container working directory and no container environment value to a log or an Event (PORTS-10.4).
- **PSTOP-3.9** THE SYSTEM SHALL send the Docker Engine no request other than the list of running containers and `stop`.
- **PSTOP-3.10** THE SYSTEM SHALL describe in the Docker row of docs/architecture/integrations.md that Navori writes to Docker only by stopping a container, in place of "Nothing is written to Docker".

## 4. See what is running

**Story:** As the user, I want to open `Running` and read from a dot which projects have nobody working in them, so that I know where idle ports are.

- **PSTOP-4.1** THE SYSTEM SHALL publish in the Core snapshot one Running view, composed in `navori-ffi` from the Ports and Awake states, holding one project row per project and one for the no-project group, each with its state (working, idle or unknown), its session rows, its port rows and its stoppable list (Weigh, card 3: the join lives in the Core; "the Shell joins" is read as "no Module joins", `agent-decided`).
- **PSTOP-4.2** THE SYSTEM SHALL join a Lease to a project by its label's text before the first `/`, or by the whole label when it has no `/` (the rule `AwakeSubmenuState` uses, apps/macos/NavoriPanel/AwakeSubmenuState.swift:186-192; unlike it, PSTOP-4.5 does not send an unlabeled Lease to `Other`), and the Core and Swift tests SHALL share one fixture for it (Weigh, card 3).
- **PSTOP-4.3** THE SYSTEM SHALL give a project the state working when it has at least one live session Lease that AWAKE-16.5 does not report as stopped, and idle when Awake is known and it has none.
- **PSTOP-4.4** THE SYSTEM SHALL give the state unknown to every project, and the stoppable list of each SHALL be empty, WHEN Awake is not registered, its state does not deserialize, it records a problem, its Module is down, or Tracking agents is off (Weigh, card 3 for the first four; the fifth is `agent-decided`: with tracking off Awake publishes no session Lease, modules/awake/src/domain/table.rs:687). One test per case.
- **PSTOP-4.5** WHEN any live session Lease has no label THE SYSTEM SHALL give every project the state unknown (agent-decided).
- **PSTOP-4.6** THE SYSTEM SHALL carry in each project row the project's checkout path and the start time of its oldest listener, which the child menu shows as `up 2d 20h` (port-discovery/requirements.md:164; `agent-decided`: the oldest listener's start as the uptime source).
- **PSTOP-4.7** THE SYSTEM SHALL treat the Running view as a view: the Core SHALL decide every `kill` from its own fresh read (PSTOP-1.5, 2.4) and not from the view (Weigh, card 3).
- **PSTOP-4.8** THE SYSTEM SHALL replace the Awake row of level 0 with a row named `Running` that shows an icon, the name, a dot and a chevron and no text value (running-menu brief, Surface 1).
- **PSTOP-4.9** WHILE any session Lease is live or any port is listed THE SYSTEM SHALL show a green dot on the `Running` row, and otherwise no dot, with VoiceOver reading `Running, active` or `Running, nothing running`.
- **PSTOP-4.10** IF the Awake record is missing and the Ports record is missing, or both Modules are stopped, THEN THE SYSTEM SHALL show the `Running` row without a submenu, as the Awake row does today.
- **PSTOP-4.11** THE SYSTEM SHALL lay out the Running submenu as `Keep awake`, `Tracking agents`, a problem line only when something is wrong, a separator, the project rows, a separator and `Stop all ports…` (running-menu brief, Surface 2).
- **PSTOP-4.12** THE SYSTEM SHALL order project rows with no working session first, then by case-insensitive name, with `Other` last.
- **PSTOP-4.13** THE SYSTEM SHALL draw a project row as a dot and a name, with no counts, ages, port numbers or status words: a filled green dot for working, an orange ring for idle, and no dot for unknown, with PSTOP-4.21 saying why (Weigh, card 7: a ring would say nobody is working).
- **PSTOP-4.14** WHILE nothing is running THE SYSTEM SHALL show `Nothing running` in the project list and no `Stop all ports…`.
- **PSTOP-4.15** WHEN the app opens the Running submenu THE SYSTEM SHALL NOT read the process table, the system's assertions or Docker, and SHALL draw from the published state (AWAKE-11.10).
- **PSTOP-4.16** WHEN a project row appears or disappears while the panel is open THE SYSTEM SHALL keep keyboard and pointer navigation valid, moving focus to the parent row if the focused row left (`agent-decided`).
- **PSTOP-4.17** THE SYSTEM SHALL have VoiceOver read a project row as its state, for example `klynt, working` and `openklynt-main, 3 idle ports`, and for a project of unknown state `<name>, working state unknown`, and let ↑ and ↓ move through rows and → open the child menu.
- **PSTOP-4.18** (guard) WHEN the Awake Module, its Actions, its CLI use, its config or its Events are used THE SYSTEM SHALL CONTINUE TO use the names `awake` and `Awake` as today, and `Keep awake` and `Tracking agents` SHALL CONTINUE TO work as AWAKE specifies.
- **PSTOP-4.19** THE SYSTEM SHALL supersede AWAKE-11.1 (the row's name), AWAKE-11.7 (menu text about Leases only), AWAKE-11.13 (Holding groups), AWAKE-11.15, AWAKE-11.17 and AWAKE-17.7 by this story, and docs/specs/2026-11-02-awake-leases/requirements.md SHALL carry an amendment note on each, as its 2026-11-04 notes do (running-menu brief, Open questions; Amendment 5 of the 2026-11-01 brief).
- **PSTOP-4.21** WHILE a project's state is unknown THE SYSTEM SHALL show one note line in the Running submenu, in the place of the problem line and in no error colour, that says why `Stop` is off: "Tracking agents is off, so Stop is off." when Tracking agents is off; the existing Awake problem text when Awake records a problem or its Module is down (no second line); "Awake cannot be read, so Stop is off." when Awake is not registered or its state does not deserialize; "A session has no project label, so Stop is off." for an unlabeled live Lease; and no line while the state is known. The Core produces the sentence and the Shell renders it. (Weigh, card 7: the line and its conditions; the wording of each sentence and the Awake-unreadable sentence are `agent-decided`.)
- **PSTOP-4.20** (guard) WHEN session or port counts change THE SYSTEM SHALL CONTINUE TO leave the status icon as it is today: no icon rule changes, and the Running dot, the project rows and the icon are not claimed to share one rule (ICON; PORTS-9.6).

## 5. Free a project's ports in two actions

**Story:** As the user, I want to hover an idle project, press `Stop 3` and confirm, so that its ports are freed in two actions.

- **PSTOP-5.1** WHEN the pointer rests on an idle project row that has at least one stoppable port THE SYSTEM SHALL show `Stop N` on the row, where N is the number of ports in its stoppable list (PSTOP header definition), and SHALL show none on a working or unknown row.
- **PSTOP-5.2** WHEN `Stop N` is pressed THE SYSTEM SHALL replace it with an inline confirmation on the same row that names the N ports, never a number of processes, and SHALL send nothing yet.
- **PSTOP-5.3** WHEN the confirmation is accepted THE SYSTEM SHALL send one `kill` request with the `project` selector and the targets of the row's stoppable list.
- **PSTOP-5.4** WHEN the confirmation is dismissed or the menu closes THE SYSTEM SHALL send nothing.
- **PSTOP-5.5** WHILE the request is in flight THE SYSTEM SHALL show the row as stopping and SHALL ignore a second press.
- **PSTOP-5.6** WHEN a result is anything other than `Stopped`, `StoppedAlone` or `AlreadyGone` THE SYSTEM SHALL keep the row and show on that port's row in the child menu why, from the result (`Stale`, `Protected`, `OutsideSelector`, `StillStopping` or the failure sentence), and fewer stops than the confirmed N SHALL read as per-target results, not as an error (Weigh, card 6).
- **PSTOP-5.7** WHEN every target is gone THE SYSTEM SHALL show the row gone after the next published state.
- **PSTOP-5.8** THE SYSTEM SHALL let Tab reach `Stop N` and VoiceOver read it as `Stop 3 ports`, and pressing `Stop N` SHALL NOT open the child menu.

## 6. Stop one port from a project's menu

**Story:** As the user, I want to open a project's child menu and stop one port, so that I free that port without stopping the rest.

- **PSTOP-6.1** THE SYSTEM SHALL head the child menu with the project's name, its checkout path in a monospaced face, and `up 2d 20h` when the project has no working session (running-menu brief, Surface 3).
- **PSTOP-6.2** THE SYSTEM SHALL list under `Sessions` each session with its Harness mark, title, dot and `branch · state`, dimming a stopped session, and under `Ports` each port with its kind mark, process or container name and `:port` at the right, leaving out a section that has no rows.
- **PSTOP-6.3** WHEN the pointer rests on a port row THE SYSTEM SHALL show a `Stop` control in place of the port, beside the `Open` and `Copy` controls of PSTOP-9.1.
- **PSTOP-6.4** WHEN `Stop` on a port row is pressed and confirmed THE SYSTEM SHALL send one `kill` request naming that one target, with the `pid` selector for a process and the `project` selector for a container (`agent-decided`: a container has no pid, so the `pid` form of PSTOP-1.4 cannot name it, and PSTOP-3.2 still checks its project membership), and SHALL show the row as `Stop` running while it is in flight.
- **PSTOP-6.5** WHEN a result for the row is anything other than `Stopped`, `StoppedAlone` or `AlreadyGone` THE SYSTEM SHALL show the row as `Stop` failed with the reason from the result.
- **PSTOP-6.6** THE SYSTEM SHALL show `Stop N ports` at the end of the child menu of every project whose stoppable list is not empty, a working project included (running-menu brief, Surface 2: a working row has no `Stop`, "it is in the child menu"), and a press SHALL confirm and send as PSTOP-5.2 and PSTOP-5.3 do.
- **PSTOP-6.7** THE SYSTEM SHALL end the child menu of `Other` with `N system ports hidden`, where hidden means not counted by a group stop and not that the row cannot be stopped one at a time (Weigh, card 2).
- **PSTOP-6.9** THE SYSTEM SHALL give each port row a tooltip and a VoiceOver sentence that name the port's worktree (when it has one) and the Harness that started it (when known), so that the project, worktree and Harness of every port are shown as GOAL-3 asks, while the visible row stays the kind mark, the name and `:port` of the locked brief (`agent-decided`: the two Locks differ on the visible row; the choice is Open Question 6). A Docker port has no Harness (PORTS-3.8, a known limit against GOAL-3), so its sentence names none.
- **PSTOP-6.8** THE SYSTEM SHALL have VoiceOver read each row as one sentence that includes its state word, so that state is never colour alone.

## 7. Stop all idle ports (not `Other`)

**Story:** As the user, I want to press `Stop all ports…` and confirm, so that the ports of every named project nobody is working in are freed together.

- **PSTOP-7.1** WHEN at least one idle project other than `Other` has a stoppable port THE SYSTEM SHALL show `Stop all ports…` after the project list, and otherwise SHALL show no such row, including when every project is unknown (Weigh, card 6; absent and not disabled is agent-decided). `Other` is never reached by `Stop all ports…`: it has no Lease, so it is always idle once Awake is known, and its listeners are unplaced (Weigh, card 6 as revised by the peer after the design; `agent-decided`). `Other` keeps its own `Stop N` and child menu.
- **PSTOP-7.2** WHEN `Stop all ports…` is pressed THE SYSTEM SHALL always ask for confirmation, naming the number of ports and of projects it stops, never a number of processes, how many ports in working or unknown projects it leaves ("in use"), and how many ports in `Other` it leaves, and SHALL keep the count of hidden system ports in its own note (menubar brief :94 confirmation rule; Weigh, card 6).
- **PSTOP-7.3** THE SYSTEM SHALL take the counts of PSTOP-7.2 from the same stoppable lists it sends.
- **PSTOP-7.4** WHEN the confirmation is accepted THE SYSTEM SHALL send one `kill` request per idle project other than `Other`, each with the `project` selector and that project's stoppable targets, containers included, all at once (`agent-decided`: one request per project keeps PSTOP-1.6 meaningful).
- **PSTOP-7.5** WHEN the replies arrive THE SYSTEM SHALL show the results per target as PSTOP-5.6 does.

## 8. Doctor text names ports

**Story:** As the user, I want the empty Doctor view in Settings to list `ports` among the checks, so that it matches what `navori doctor` runs.

- **PSTOP-8.1** WHEN no checks have run THE SYSTEM SHALL show a sentence in Settings → Doctor that names the ports check among the checks `navori doctor` runs (apps/macos/Navori/Features/Settings/SettingsView.swift:135, which port-discovery/design.md:353 cites as :132; the sentence does not name it today).
- **PSTOP-8.2** (guard) WHEN `navori doctor` runs THE SYSTEM SHALL CONTINUE TO report every row it reports today in today's order, with `ports` after `logs` (PORTS-9.4).

## 9. Open a running port in my browser

**Story:** As the user, I want to press `Open` on a port in a project's child menu, so that my browser shows the running service without my typing its address.

The demoable act is `Open` (PSTOP-9.1, 9.2). PSTOP-9.3 and 9.4 are supporting controls of the same menu, not acts of this story.

- **PSTOP-9.1** WHEN the pointer rests on a port row THE SYSTEM SHALL show `Open` where `ports.open` accepts the port, and `Copy`.
- **PSTOP-9.2** WHEN `Open` is pressed THE SYSTEM SHALL dispatch `ports.open` for that port and open the URL it returns in the default browser (PORTS-6.1).
- **PSTOP-9.3** (supporting) WHEN `Copy` is pressed THE SYSTEM SHALL copy `localhost:<port>` to the pasteboard and send nothing to the Core (`agent-decided`: the text; the Seed lists copy as a Shell action).
- **PSTOP-9.4** (supporting) WHEN the path in the child menu's header is pressed THE SYSTEM SHALL reveal that folder in Finder (`agent-decided`: the Seed lists "reveal in Finder" and the locked brief gives it no control).

## 10. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** **PSTOP-10.1** THE SYSTEM SHALL keep the Running submenu open within 16 ms of the trigger and later menu opens within 50 ms, the GOAL-11 budgets of docs/standards/performance.md, verified by `cargo xtask perf`. **PSTOP-10.2** WHILE the menu is closed and no `kill` request is in flight THE SYSTEM SHALL start no timer faster than 30 s on behalf of this feature (ARCH-16), verified by the timing tests of modules/ports/tests/timing.rs staying green. The port scan budget (50 ms, `port_scan_ms`) is unchanged.
- **Security:** **PSTOP-10.3** THE SYSTEM SHALL meet THR-5 (docs/security/threat-model.md): a `kill` never signals another user's process, a system service, a protected database, Docker's backend or Navori itself on a group request, verified by tests with fakes for each (including kill-self and kill-ancestor), and the one explicit limit is PSTOP-1.17.
- **Reliability:** **PSTOP-10.4** THE SYSTEM SHALL answer every `kill` request within 26 s (the 25 s of PSTOP-1.11 plus 1 s; `agent-decided`), and a stale, reused or vanished pid SHALL cause no signal to another process, verified by tests with a fake clock and a fake process table that replaces a pid between the read and the signal.
- **Accessibility:** **PSTOP-10.5** THE SYSTEM SHALL meet the keyboard and VoiceOver rules of PSTOP-4.17, 5.8 and 6.8, taken from the locked brief; None beyond that: docs/standards has no accessibility document, so no house conformance target exists.

## Out of Scope

- Needs-you and AI-proposed stops, the policy engine and the approval gate for `ports.kill`: ROAD-37. The AI cannot reach `ports.kill` until it exists (ROAD-14 runs only `Read` Actions).
- The PR list and port-conflict notices.
- A configurable protected list (THR-5 "configurable"): deferred to a later ROAD because it needs a persisted config key. Recorded here and in the run report only; the roadmap is not edited in this run.
- Stopping a target's own child processes by walking its parent links (the peer's probe found dev servers re-parented to pid 1, so it would not reach them): deferred.
- Any Docker request besides listing running containers and `stop`, `DOCKER_HOST` and other Docker contexts.
- A `navori ports` subcommand: generic `navori dispatch` reaches `kill`.
- Persisting anything: no Owner, table, migration or config key.
- Making the status icon, the Running dot and the project rows share one rule for "working" (PSTOP-4.20).
- A mechanical gate on `Risk` in the kernel, FFI or CLI.
- Measuring the port scan with the Docker read (issue #24 owns the baseline).
- Notarization, other UIDs and EndpointSecurity.

## Open Questions

- Group behaviour of servers launched by Codex, Grok or a terminal (PSTOP-1.8) — operator, at review — 2026-11-04 — forbid-guess: only a Claude Code tool shell and one `lsof` pass were probed; do not claim leader or non-leader for other hosts. Check: a read-only probe of one such server.
- Accuracy of `kind` (PSTOP-2.1) — operator, at review — 2026-11-04 — forbid-guess: `kind` is an exe-name heuristic (modules/ports/src/kind.rs:14-33) and now carries a safety rule; do not widen the protected set from a guess. Check: a stop in real use that takes a process `kind` called `DevServer`.
- One "working" rule for the status icon, the Running dot and the project rows (PSTOP-4.20) — operator, at review — 2026-11-04 — forbid-guess: no "working" symbol was found in crates/ffi/src, so do not claim they agree. Check: a later ROAD that unifies them.
- Container databases not named by a known image (PSTOP-2.1) — operator, at review — 2026-11-04 — forbid-guess: PSTOP-2.1 protects a container whose image name is `postgres`, `mysql`, `mariadb`, `redis`, `valkey` or `mongo`; an image referenced by id, a custom image name or another database is stopped by a group stop like any other. Do not claim every container database is protected. Check: a stop in real use that takes a database container the list missed.
- `Other` and `Stop all ports…` (PSTOP-7.1) — operator, at review — 2026-11-04 — forbid-guess: `Other` is not reached by `Stop all ports…` (it is always idle once Awake is known, and its listeners are unplaced); do not widen it without the operator. Check: real use of `Other`'s own `Stop N`.
- Worktree and Harness on a port row (PSTOP-6.9) — operator, at review — 2026-11-04 — forbid-guess: GOAL-3 (docs/product/vision.md:20) and the MILE-4 outcome say every port is shown with its project, worktree and Harness; the locked running-menu brief (:54) draws a port row as kind mark, name and `:port`, with the Harness mark on sessions only. This spec shows worktree and Harness in the row's tooltip and VoiceOver sentence only. Do not change the brief's visible row without the operator. A Docker port never shows a Harness (PORTS-3.8).
- Unknown means no Stop (PSTOP-4.4, 4.21) — operator, at review — 2026-11-04 — forbid-guess: with Tracking agents off, Awake unreadable or any unlabeled live Lease, every project is Unknown and no `Stop` is offered anywhere, `Stop all ports…` included, and the note line says why. On a fresh machine with Tracking agents off the feature offers no Stop at all. Do not soften this fail-closed rule without the operator.
