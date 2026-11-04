# Tasks: Ports panel and stop

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: PSTOP
Status: Approved
Approved by the operator ("approve", 2026-11-04 21:17 +07, typed in the conductor session after reading RUN/sheet.html for 775cb62; relayed in .skills/_pending-ports-panel/auto/operator-reply.md).
Date: 2026-11-04
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

<!--
Written in an unattended run under .skills/_pending-ports-panel/auto/charter.md. The operator has not reviewed this file. Per the charter, plan-tasks did not publish an issue (ticket #25 is the only ticket) and offers no execution route in this run.
-->

**Goal:** Add a `ports.kill` Action that stops exactly the listeners it names, a Core view that joins Ports and Awake by project, and the `Running` menu with project rows, a child menu and `Stop`.

**Architecture:**
- `navori-macos` gains a `Signals` trait (kill, process group) with a fake; `modules/ports` gains the `kill` Action, a fresh-read resolver, a process stopper, a Docker `stop` call and a stop orchestration task inside its actor loop.
- `navori-ffi` composes a pure `running::build` view from the Ports and Awake states into `CoreSnapshot.running`, carrying ready-to-send request JSON.
- The Swift Shell renders that view: pure state types in `NavoriPanel/`, views in `Features/Panel/`, one new `CoreStore` call path.

**Tech Stack:** Rust 2024 (rust-version 1.90), `libc` 0.2.191 (already in `navori-macos`), `tokio`, `serde`, `serde_json`, `schemars`, `futures`, `thiserror`, `tracing`; Swift 6 (AppKit + SwiftUI, Swift Testing), UniFFI. No new dependency. `cargo test -p <crate>`, `cargo xtask ci`, `cargo xtask perf`.

**Source classes:** every value in `requirements.md` and `design.md` is a Lock, `(Weigh, card N)`, `agent-decided` or Unverified. A task never promotes one to the operator's choice, and a code comment never says an `agent-decided` value was decided by the user.

## Global Constraints

Source of truth (referenced, not copied — read before every task):

| Source | Content hash (`git hash-object`, first 12) |
|---|---|
| `docs/standards/INDEX.md` | `f83e09733ee4` |
| `docs/standards/testing.md` | `b1a9b42fc1d3` |
| `docs/standards/errors-logging.md` | `68aadebff863` |
| `docs/standards/performance.md` | `c4b26d17a09b` |
| `docs/architecture/INDEX.md` (ARCH-N) | `2e318bd23ff5` |

Compact delta every task inherits:

- **Test-first at agreed seams** (`design.md` "Seams for testing"):
  - Test names are sentences in domain language. No requirement ID goes in test or production source. Expected values are literals.
  - Fakes only at system boundaries: `FakeProcesses`, `FakeSignals`, `FakeContainers`, `FakeClock`. No default test signals a real process or touches Docker.
  - **No test in `crates/ffi/tests/` may dispatch `kill`:** `PortsEnv::mac` reads the real process table and signals for real. An e2e step that presses `Stop` or dispatches `kill` acts only on a throwaway process the step spawned itself, never on a listener it did not start.
- **Canonical temp paths:** every temp home, repo and expected path goes through `std::fs::canonicalize` (macOS `/var/folders` is `/private/var`).
- **Build limits:** no step writes outside the worktree or a temporary directory. No step prints a process's environment value, a container's working directory or a container's environment.
- **ARCH-1, 2, 3, 4, 14, 16** hold in every task:
  - `unsafe` only in `navori-macos`, each block with a `// SAFETY:` comment first inside it; `[lints] workspace = true` elsewhere; no `unwrap`/`expect` outside tests.
  - Only `CoreStore` imports `NavoriCore`; Swift renders and sends requests; no Module imports another.
- **Gate:** every Rust task: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, then the task's `cargo test -p <crate>`. Swift tasks: the **Swift test command** is `swift test --arch arm64 --package-path apps/macos --filter <Suite>` run after the core build that `cargo xtask ci` runs first (`build_core`; xtask/src/main.rs:1183-1184), so run `cargo xtask ci` once at the start of the Swift tasks to produce it; `cargo xtask ci` is the only command that compiles `apps/macos` as a whole. `docs/agents/verify.md` has no Swift test command.
- **Shared files** (load-subgraph, 2026-11-04, advisory, `owns_coverage` 11/16):
  - PORTS owns `modules/ports/**`; PROCS shares `crates/macos/**`; AWAKE, HOOKS, KCONTRACT, ASK and ICON share `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs` and `crates/ffi/tests/core.rs`; AWAKE, MENU, ICON and PERF share `apps/macos/Navori/Features/Panel/*`, `StatusBarController.swift` and `CoreStore.swift`. ASK and HOOKS are In-progress in the catalog and share with this plan `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs`, `CoreStore.swift`, `SettingsView.swift`, `docs/architecture/integrations.md` and `docs/codebase/modules.md` (load-subgraph, re-run 2026-11-04 after this file existed: PORTS shares 19 paths, AWAKE 12, MENU 7, HOOKS 6, ASK 5, KCONTRACT 5, ICON 4, PERF 3, PROCS 3; `owns_coverage` 12/16 = 0.75). So Tasks 3, 8, 10, 11, 12, 13 and 14 each rebase onto `main` before editing those files. PERF shares `PerfProbe.swift`, which Task 10 touches only if the probe can open a row by id.
- **Commits:** Conventional Commits, one logical change per commit, every commit builds. No `git push`.
- **Unverified, stay open.** No task turns these into an assertion: group behaviour of servers launched by Codex, Grok or a terminal; `kind` accuracy; one "working" rule for the icon; container databases the image list misses; that the daemon keeps stopping a container after the client drops the connection; `CascadeState.maxDepth` counting the root (Task 10 reads `CascadeState.swift` first); how `xtask perf` drives a submenu by row id (Task 10 reads it first).

## File Structure

Create:
- `crates/macos/src/signals.rs` — `Signals`, `SignalTarget`, `Signal`, `SignalError`, `MacSignals`.
- `crates/macos/src/fake/signals.rs` — `FakeSignals`.
- `crates/macos/tests/signals.rs` — real-OS group and kill read-back (ignored) and fake behaviour.
- `modules/ports/src/stop/mod.rs` — `StopTask`, deadline, result assembly, in-flight registry.
- `modules/ports/src/stop/resolve.rs` — `FreshRead`, `Guard`, `StopClass`, `stop_class`, `resolve`, `Resolved`, plans.
- `modules/ports/src/stop/process.rs` — `stop_process`, `reach_for`.
- `modules/ports/src/image.rs` — `database_image`.
- `modules/ports/tests/kill.rs`, `modules/ports/tests/stop_resolve.rs`, `modules/ports/tests/stop_process.rs`, `modules/ports/tests/stop_docker.rs`, `modules/ports/tests/stop_orchestration.rs`, `modules/ports/tests/image.rs`.
- `crates/ffi/src/running.rs` — `RunningView` records and `build`.
- `crates/ffi/tests/running.rs`, `crates/ffi/tests/fixtures/lease_labels.json`.
- `apps/macos/NavoriPanel/RunningState.swift` — `RunningInput`, `RunningRowState`, `ProjectDot`, `ProjectRowState`, `RunningSubmenuState`.
- `apps/macos/NavoriPanel/StopFlow.swift` — `StopFlow`, `StopResult`, `Decodable` reply types.
- `apps/macos/Navori/Features/Panel/RunningSubmenuView.swift`, `ProjectRowView.swift`, `ProjectChildView.swift`, `InlineConfirmation.swift`.
- `apps/macos/Tests/NavoriPanelTests/RunningStateTests.swift`, `StopFlowTests.swift`, `LeaseLabelFixtureTests.swift`.

Modify:
- `crates/macos/src/lib.rs`, `crates/macos/src/fake/mod.rs` — export the new items.
- `modules/ports/src/lib.rs` — `PortsEnv` gains `signals` and `helper`; `pub use`.
- `modules/ports/src/action.rs` — `Kill`, `Selector`, `Target`, `KillReply`, `Outcome`.
- `modules/ports/src/actor.rs` — the `Kill` arm, the join set, `rescan_after`, `with_docker` to `pub(crate)`.
- `modules/ports/src/state.rs`, `modules/ports/src/place.rs`, `modules/ports/src/docker.rs` — `Placement.root`, `ContainerRef.image`.
- `modules/ports/src/kind.rs` — `is_docker_backend` to `pub`.
- `modules/ports/src/infra/docker.rs` — `Containers::stop`, `Stopped`, status-set parameter on `parse_head`, `Image` field of `Listed`.
- `modules/ports/tests/common/mod.rs` — `Rig` gains `signals: Arc<FakeSignals>`; `FakeContainers` gains `stop`.
- `modules/ports/tests/place.rs`, `scan.rs`, `docker.rs`, `docker_client.rs` — `Placement` and `ContainerRef` literals.
- `crates/ffi/src/lib.rs`, `crates/ffi/src/app.rs` — `CoreSnapshot.running`, `module_is_down` use, helper path into `PortsEnv`.
- `apps/macos/Navori/Core/CoreStore.swift` — `running`, `stop`, `openPort`, `stopProgress`.
- `apps/macos/Navori/Features/Panel/PanelView.swift`, `AwakeSubmenuView.swift` — the `running` row; the Holding list leaves.
- `apps/macos/Navori/StatusBar/StatusBarController.swift` — factory, `menuTree()`, project-id observation.
- `apps/macos/NavoriPanel/CascadeState.swift` — focus to the parent on a vanished row.
- `apps/macos/Tests/NavoriPanelTests/CascadeStateTests.swift`, `AwakeSubmenuStateTests.swift`.
- `crates/ffi/tests/core.rs` — one snapshot test for `running` (no `kill`).
- `apps/macos/Navori/Features/Settings/SettingsView.swift` — the empty-state sentence.
- `docs/architecture/integrations.md` — BSD row and Docker row.
- `docs/specs/2026-11-02-awake-leases/requirements.md` — amendment notes on AWAKE-11.1, 11.7, 11.13, 11.15, 11.17, 17.7.
- `docs/specs/2026-11-04-port-discovery/requirements.md`, `design.md` — notes on PORTS-1.5 and the read-only-Docker lines.
- `docs/codebase/modules.md`, `docs/agents/verify.md` — Ports surface and the stale "no screen yet" gotcha.

Not touched: `crates/kernel/**` (its `running::` use is grepped in Task 8), `crates/protocol/**`, `crates/procs/**`, `crates/store/**`, `apps/cli/**`, `modules/awake/**`, `xtask/**` (read-only: Task 10 and Task 15 read `xtask/src/perf`).

Order: Tasks 3, 4, 5, 6, 7 share `modules/ports/src/actor.rs`, `action.rs` and `tests/common/mod.rs`; Tasks 9 to 13 share Swift files. Edges below are what the tasks need.

---

### Task 1: A signalling seam in `navori-macos`

**Files:**
- Create: `crates/macos/src/signals.rs`, `crates/macos/src/fake/signals.rs`, `crates/macos/tests/signals.rs`
- Modify: `crates/macos/src/lib.rs`, `crates/macos/src/fake/mod.rs`

**Reuse:** rung 5 — `libc` 0.2.191 is already a dependency of `navori-macos` only (`kill`, `getpgid`, `getpgrp`); new trait and fake: rung 7 for the trait shape.

**Interfaces:**
- Consumes: `ReadError` (error.rs), `FakeProcesses` (fake/processes.rs), the `// SAFETY:` block form of process.rs:129.
- Produces:
  - `pub trait Signals: Send + Sync { fn own_pid(&self) -> i32; fn own_group(&self) -> i32; fn group_of(&self, pid: i32) -> Result<i32, ReadError>; fn send(&self, to: SignalTarget, signal: Signal) -> Result<(), SignalError>; }`
  - `pub enum SignalTarget { Process(i32), Group(i32) }`, `pub enum Signal { Term, Kill }`, `pub enum SignalError { Gone, Denied, Failed }` (all `Debug, Clone, Copy, PartialEq, Eq`)
  - `MacSignals::new() -> Self`
  - `fake::FakeSignals`: `new(own_pid: i32, own_group: i32)`, `set_group(pid, pgid)`, `ignore_term(pid)`, `obeys(pid, &FakeProcesses)` (a signal exits the process), `deny(pid)`, `before_send(Box<dyn Fn() + Send + Sync>)`, `sent() -> Vec<(SignalTarget, Signal)>`

**Depends-on:** none

- [x] **Step 1: Write the failing tests** in `crates/macos/tests/signals.rs`:
  - `a_fake_signal_to_an_obedient_process_ends_it_and_is_recorded`
  - `a_fake_signal_to_a_process_that_ignores_term_leaves_it_running`
  - `a_denied_signal_reports_denied_and_is_not_recorded_as_delivered`
  - `a_group_signal_is_recorded_as_a_group_target`
  - `a_real_child_started_in_its_own_group_reports_that_group_and_dies_on_kill` (`#[ignore]`, spawns a `sleep` child with `process_group(0)`, reads `MacSignals::group_of`, sends `Kill` to `Group`, expects `Gone` on a second send)
- [x] **Step 2: Run** `cargo test -p navori-macos --features fake --test signals` — expect: compile errors for `Signals`, `FakeSignals`.
- [x] **Step 3: Implement `signals.rs`.** `send` maps `Process(p)` to `libc::kill(p, sig)` and `Group(g)` to `libc::kill(-g, sig)`; errno `ESRCH` → `Gone`, `EPERM` → `Denied`, else `Failed`. `group_of` is `libc::getpgid` with the `read_error` mapping. Every `unsafe` block carries `// SAFETY:`. No `unsafe` item is `pub`.
- [x] **Step 4: Implement `FakeSignals`** over a shared `FakeProcesses` handle; `before_send` runs just before a signal is recorded.
- [x] **Step 5: Run** `cargo test -p navori-macos --features fake && cargo test -p navori-macos --test signals -- --ignored && cargo xtask check-deps` — expect: pass.
- [x] **Step 6: Commit** `git commit -m "feat(macos): add a Signals seam for stopping processes"`

_Requirements: PSTOP-1.16_

---

### Task 2: Checkout path and container image on the Ports state

**Files:**
- Create: `modules/ports/src/image.rs`, `modules/ports/tests/image.rs`
- Modify: `modules/ports/src/state.rs`, `modules/ports/src/place.rs`, `modules/ports/src/docker.rs`, `modules/ports/src/infra/docker.rs`, `modules/ports/src/lib.rs`, `modules/ports/tests/place.rs`, `modules/ports/tests/scan.rs`, `modules/ports/tests/docker.rs`, `modules/ports/tests/docker_client.rs`, `docs/specs/2026-11-04-port-discovery/requirements.md`

**Reuse:** rung 2 — `place()` (place.rs:43-93, which holds `repo.root` inside `counted()` and discards it), `Placement` (state.rs:75-80), `ContainerRef`, `attribute` (docker.rs:12), the list parser's `Listed` struct (infra/docker.rs).

**Interfaces:**
- Consumes: existing `place`, `attribute`, `Container`.
- Produces:
  - `Placement.root: Option<PathBuf>` (counted repo root; `None` for a missing path and for a path outside the counted area)
  - `ContainerRef.image: Option<String>` and `Container.image: Option<String>`: the repository name with registry, tag and digest removed
  - `pub fn database_image(repository: &str) -> bool` — true when the last path segment equals one of `postgres`, `mysql`, `mariadb`, `redis`, `valkey`, `mongo` (exact match; `postgres-exporter` and `bitnami/postgresql` do not match: `agent-decided`, bitnami stays under Open Question 4)

**Depends-on:** none

- [x] **Step 1: Write the failing tests:**
  - `image.rs`: `database_image` true for `postgres`, `library/postgres`, `docker.io/library/redis`, `mongo`; false for `postgres-exporter`, `bitnami/postgresql`, `minio/minio`, an empty string.
  - `place.rs`: `a_listener_in_a_counted_repo_carries_the_repo_root`, `a_removed_directory_carries_no_root`.
  - `docker.rs`/`docker_client.rs`: a list reply with `"Image":"postgres:16-alpine"` parses to image `postgres`; `"Image":"sha256:abc…"` parses to `None`; the request bytes are unchanged.
- [x] **Step 2: Run** `cargo test -p navori-mod-ports --test image --test place` — expect: compile errors.
- [x] **Step 3: Implement** the repository-name extraction (strip a `sha256:` id to `None`, strip `@digest`, strip a `:tag` after the last `/`, keep the path) and `database_image`.
- [x] **Step 4: Implement** `Placement.root` in `place()` and `place_listener`, `ContainerRef.image` through `attribute`, and the `Image` field on `Listed`. Fix every `Placement` and `ContainerRef` literal and `assert_eq!` in the four test files named above.
- [x] **Step 5: Add** a one-line amendment note on PORTS-1.5 in the PORTS `requirements.md`: "(Amended 2026-11-04 by PSTOP: entries gain `root` and a container's `image`; both additive.)"
- [x] **Step 6: Run** `cargo test -p navori-mod-ports && cargo test -p navori-ffi` — expect: pass (the ffi re-deserialization still round-trips). Commit `git commit -m "feat(ports): carry the checkout path and the container image"`

_Requirements: PSTOP-2.1, PSTOP-3.9, PSTOP-4.6_

---

### Task 3: The `kill` Action, its request model and validation

**Files:**
- Modify: `modules/ports/src/action.rs`, `modules/ports/src/lib.rs`, `modules/ports/src/actor.rs`, `modules/ports/tests/common/mod.rs`, `crates/ffi/src/lib.rs`, `docs/codebase/modules.md`
- Create: `modules/ports/tests/kill.rs`

**Reuse:** rung 2 — extends `PortsAction` (action.rs:7-41) the way `Open` does: a `schemars::JsonSchema` + serde variant, hand-written `risk()` and `catalog()` arms; `PortsEnv::mac` (lib.rs:46).

**Interfaces:**
- Consumes: `Signals`, `MacSignals`, `FakeSignals` (Task 1).
- Produces:
  - `PortsAction::Kill { selector: Selector, targets: Vec<Target> }`, `risk()` = `Risk::Destructive` for `Kill` only
  - `enum Selector { Pid(i32), Worktree { project: String, worktree: String }, Project(Option<String>) }`
  - `enum Target { Process { pid: i32, start_us: u64 }, Container { id: String } }`
  - `struct KillReply { results: Vec<TargetResult> }`, `struct TargetResult { target: Target, result: Outcome, reason: Option<String> }`, `enum Outcome { Stopped, StoppedAlone, AlreadyGone, StillStopping, Stale, Protected, OutsideSelector, Failed }`
  - `PortsEnv { home, processes, containers, signals: Arc<dyn Signals>, helper: Option<PathBuf> }`; `PortsEnv::mac(home)` builds `MacSignals`; `PortsEnv::with_helper(self, path: PathBuf) -> Self`
  - Wire example: `{"kill":{"selector":{"project":"klynt"},"targets":[{"process":{"pid":412,"start_us":1730000000000000}}]}}`
  - `Rig` field `signals: Arc<FakeSignals>`; `ActionError::Invalid` texts for the rejections below

**Depends-on:** Task 1

- [x] **Step 1: Write the failing tests** in `modules/ports/tests/kill.rs` through `rig.dispatch`:
  - `an_empty_target_list_is_rejected_and_nothing_is_signalled`
  - `the_three_selector_forms_and_both_target_forms_are_accepted`
  - `a_pid_selector_with_two_targets_is_rejected`
  - `a_pid_selector_whose_target_has_another_pid_is_rejected`
  - `a_container_id_that_is_not_hex_is_rejected` (`"abc/../x"`, 65 hex digits, empty)
  - `the_catalog_lists_kill_as_destructive_and_list_and_open_unchanged`
  - `ports_list_and_open_still_reply_from_the_last_scan_without_scanning` (guard)
  - `the_ports_module_still_accepts_every_config_file_it_accepted_before` (guard: `NoConfig`)
- [x] **Step 2: Run** `cargo test -p navori-mod-ports --test kill` — expect: compile errors for `Kill`, `Selector`.
- [x] **Step 3: Implement** the types, the `catalog()` entry (a description written for people and LLMs), validation in the `recv` arm before anything is read, and the `PortsEnv` fields. Fix both struct-literal sites (`PortsEnv::mac`, `Rig::opening` at tests/common/mod.rs:230). The `Kill` arm replies `Failed` "Stopping is not wired yet" until Task 7 (no signal path exists yet).
- [x] **Step 4: Wire** `crates/ffi/src/lib.rs:299` to `PortsEnv::mac(home).with_helper(helper_path)` using `CoreConfig.helper_path`.
- [x] **Step 5: Update** `docs/codebase/modules.md:50` with the new exports. Run `cargo test -p navori-mod-ports && cargo test -p navori-ffi` — expect: pass.
- [x] **Step 6: Commit** `git commit -m "feat(ports): add the destructive kill action and its request model"`

_Requirements: PSTOP-1.1, PSTOP-1.2, PSTOP-1.4, PSTOP-1.18, PSTOP-2.5, PSTOP-2.6_

---

### Task 4: Re-read each target and decide protection

**Files:**
- Create: `modules/ports/src/stop/mod.rs`, `modules/ports/src/stop/resolve.rs`, `modules/ports/tests/stop_resolve.rs`
- Modify: `modules/ports/src/lib.rs`, `modules/ports/src/kind.rs`, `modules/ports/src/actor.rs`

**Reuse:** rung 2 — the fresh read is `Scanner::scan` (scan.rs:58) followed by `with_docker` (actor.rs:207), the same two steps `start_scan` runs; `kind()` (kind.rs:18), `place_listener` (place.rs:9) and `ProcessSource::entry` as they are; `is_docker_backend` (kind.rs:36) from `pub(crate)` to `pub`; `with_docker` to `pub(crate)`.

**Interfaces:**
- Consumes: `Selector`, `Target`, `Outcome`, `PortsEnv.signals`, `PortsEnv.helper` (Task 3); `database_image`, `ContainerRef.image` (Task 2).
- Produces:
  - `pub struct FreshRead { scan: Scan, entries: Vec<PortEntry>, docker: DockerStatus }`; every item below is `pub` and `pub use`d from `modules/ports/src/lib.rs` (`FreshRead`, `StopClass`, `stop_class`, `Guard`, `Resolved`, `Plan`, `ProcessPlan`, `ContainerPlan`, `resolve`, and the `Reach` enum `{ Group(Vec<(i32, u64)>), Alone }`, which `ProcessPlan` needs; Task 5 adds `reach_for` and `stop_process` to the list), because the tests are integration tests in `modules/ports/tests/`, `navori-ffi` calls `stop_class`, and `pub` items are not `dead_code` before Task 7 uses them
  - `pub fn stop_class(entry: &PortEntry, own_pid: i32, helper: Option<&Path>) -> StopClass`, `enum StopClass { Free, GroupProtected, Never }`
  - `Guard::new(signals: Arc<dyn Signals>, source: Arc<dyn ProcessSource>, helper: Option<PathBuf>)`, with ancestors read by walking `entry(pid).ppid`
  - `fn resolve(fresh: &FreshRead, guard: &Guard, selector: &Selector, targets: &[Target]) -> Vec<Resolved>`; `enum Resolved { Go(Plan), Refused(Outcome, &'static str) }`; `enum Plan { Process(ProcessPlan), Container(ContainerPlan) }`; `ProcessPlan { pid, start_us, members: Vec<(i32, u64)>, reach: Reach }` (`resolve` leaves `reach: Reach::Alone`; Task 7 fills it from `reach_for` before `stop_process`); `ContainerPlan { id: String, ports: Vec<u16> }`
  - Fixed `reason` literals, defined once here and asserted verbatim by every test (no trailing period beyond what is shown): `Stale` "The process changed" (start time or uid differs) and "The port closed" (no longer a listener) and "The container is not running"; `Protected` "This process is protected" and "This port is not stopped with a group"; `OutsideSelector` "It is not in that project"; `Failed` "Docker is not reachable", "The signal was refused", "It is still running", "The process list could not be read in time" and "Docker refused the stop"; `StillStopping` "It is still stopping"

**Depends-on:** Task 2, Task 3

- [x] **Step 1: Write the failing tests** in `modules/ports/tests/stop_resolve.rs` (pure `resolve` over a hand-built `FreshRead` and a fake `Guard`):
  - process: `a_gone_pid_is_already_gone`, `a_zombie_is_already_gone`, `a_reused_pid_with_another_start_time_is_stale`, `a_pid_of_another_user_is_stale`, `a_pid_that_stopped_listening_is_stale_with_the_port_closed_sentence`
  - protection: `navoris_own_pid_is_protected_under_every_selector` (kill-self), `an_ancestor_of_navori_is_protected_under_every_selector` (kill-ancestor), `the_helper_by_exe_path_is_protected`, `an_unclaimed_docker_backend_entry_is_protected`, `a_container_entry_whose_pid_is_the_backend_is_free_under_stop_class`, `a_process_target_naming_the_backend_pid_is_protected` (the container check precedes the pid check), `a_database_under_a_project_selector_is_protected_but_under_a_pid_selector_is_free`, `a_system_port_follows_the_same_rule`, `protection_comes_from_the_fresh_entry_not_the_request`
  - selector: `a_target_outside_the_named_worktree_is_outside_the_selector`, `a_project_selector_with_no_project_reaches_only_unplaced_targets`, `a_selector_never_adds_a_target_the_request_did_not_name`
  - container: `a_container_not_running_is_stale`, `a_container_that_no_longer_publishes_a_port_is_stale`, `a_container_in_another_project_is_outside_the_selector`, `a_database_image_container_is_protected_under_a_project_selector`, `unreachable_docker_fails_every_container_before_membership_is_checked`
- [x] **Step 2: Run** `cargo test -p navori-mod-ports --test stop_resolve` — expect: compile errors.
- [x] **Step 3: Implement** `stop_class` as the one predicate (a container entry is `Free` unless its image names a database, then `GroupProtected`; only an unclaimed Docker entry, a Docker-backend pid, own pid and the helper are `Never`) and `resolve` in the order of design section C, first hit wins.
- [x] **Step 4: Implement** the fresh read helper that runs `Scanner::scan` on `spawn_blocking` and `with_docker`, and `Guard`.
- [x] **Step 5: Run** `cargo test -p navori-mod-ports` — expect: pass. Commit `git commit -m "feat(ports): re-read and protect every stop target"`

_Requirements: PSTOP-1.3, PSTOP-1.5, PSTOP-1.6, PSTOP-2.1, PSTOP-2.2, PSTOP-2.3, PSTOP-2.4, PSTOP-3.1, PSTOP-3.2, PSTOP-3.6, PSTOP-10.3_

---

### Task 5: Stop one process

**Files:**
- Create: `modules/ports/src/stop/process.rs`, `modules/ports/tests/stop_process.rs`
- Modify: `modules/ports/src/stop/mod.rs`, `modules/ports/src/stop/resolve.rs`

**Reuse:** rung 2 — `tokio::time::sleep` (paused-clock friendly, as the cadence tests use), `ProcessSource::entry` for the identity check; signalling through the `Signals` seam (Task 1).

**Interfaces:**
- Consumes: `Signals`, `FakeSignals`, `FakeProcesses::replace`, `ProcessPlan`, `Guard`, `FreshRead` (Tasks 1, 4).
- Produces:
  - `async fn stop_process(plan: ProcessPlan, source: &dyn ProcessSource, signals: &dyn Signals) -> (Outcome, Option<&'static str>)`
  - `fn reach_for(plan: &ProcessPlan, fresh: &FreshRead, signals: &dyn Signals, guard: &Guard, confirmed: &[(i32, u64)]) -> Reach`, `enum Reach { Group(Vec<(i32, u64)>), Alone }`

**Depends-on:** Task 1, Task 4

- [x] **Step 1: Write the failing tests** in `modules/ports/tests/stop_process.rs` with `FakeProcesses`, `FakeSignals` and a paused tokio clock:
  - `a_process_that_obeys_term_is_gone_after_the_wait_and_gets_no_kill`
  - `a_process_that_ignores_term_gets_kill_after_three_seconds` (asserts the recorded sequence and that `Kill` follows `Term` by exactly 3 s of paused time)
  - `a_process_still_running_one_second_after_kill_fails_with_a_fixed_sentence`
  - `a_signal_refused_by_the_os_fails_and_names_no_path`
  - `a_leader_whose_group_holds_only_confirmed_listeners_is_signalled_as_a_group_and_each_member_answers_from_its_own_check`
  - `a_leader_whose_group_holds_an_unconfirmed_listener_is_signalled_alone` (table: unconfirmed, protected, unreadable group member)
  - `a_leader_in_navoris_own_group_or_an_ancestors_group_is_signalled_alone`
  - `a_non_leader_is_signalled_alone`
  - `a_pid_replaced_just_before_the_kill_check_gets_no_kill` (`before_send` hook replaces the pid; asserts the recorded signals)
  - `a_pid_replaced_between_term_and_the_wait_end_is_reported_gone_and_gets_no_kill`
  - `a_zombie_after_term_counts_as_gone`
  - `a_first_term_that_finds_no_such_process_is_already_gone`
- [x] **Step 2: Run** `cargo test -p navori-mod-ports --test stop_process` — expect: compile errors.
- [x] **Step 3: Implement** `reach_for` and `stop_process` (both `pub use`d from `lib.rs` here; `reach_for` returns the `Reach` that Task 7 stores in `ProcessPlan.reach`; `stop_process` reads `plan.reach` and never recomputes it) and the sequence of design section D: `Term`, 3 s, `entry`, `Kill`, 1 s, `entry`; no await between the second `entry` and `send(Kill)`.
- [x] **Step 4: Run** `cargo test -p navori-mod-ports` — expect: pass. Commit `git commit -m "feat(ports): stop a process with term, a wait and kill"`

_Requirements: PSTOP-1.7, PSTOP-1.8, PSTOP-1.9, PSTOP-1.10, PSTOP-10.4_

---

### Task 6: Stop a container through the Engine

**Files:**
- Create: `modules/ports/tests/stop_docker.rs`
- Modify: `modules/ports/src/infra/docker.rs`, `modules/ports/src/lib.rs`, `modules/ports/tests/common/mod.rs`, `modules/ports/tests/docker_client.rs`

**Reuse:** rung 2 — the framing, head parsing and `Wiped` buffer of the list call (infra/docker.rs:100-200) and `DockerError`'s fixed texts; the status check of `parse_head` (:157-159) factored into a function taking the accepted statuses; the test fake server `serve` of tests/docker_client.rs:15-60.

**Interfaces:**
- Consumes: `Containers`, `DockerSocket`, `DockerError`, `FakeContainers` (existing).
- Produces:
  - `Containers::stop(&self, id: &str) -> BoxFuture<'_, Result<Stopped, DockerError>>`; `enum Stopped { Stopped, AlreadyGone }`
  - request bytes `POST /containers/{id}/stop HTTP/1.1\r\nHost: docker\r\nContent-Length: 0\r\nConnection: close\r\n\r\n` (no `t`)
  - status map: 204 → `Stopped`; 304 and 404 → `AlreadyGone`; other → `BadStatus(n)`; body read and discarded
  - `FakeContainers::reply_stop(Result<Stopped, DockerError>)`, `stops() -> Vec<String>`, `delay_stop(Duration)`
  - the list call's request bytes, 2 s limit and 1 MiB cap unchanged

**Depends-on:** Task 2

- [x] **Step 1: Write the failing tests** in `modules/ports/tests/stop_docker.rs` against the Unix-socket fake server:
  - `a_stop_sends_exactly_the_post_with_no_t_value`
  - `a_204_reply_means_stopped`, `a_304_reply_means_already_gone`, `a_404_reply_means_already_gone`
  - `a_500_reply_fails_with_the_fixed_text_and_never_the_daemons_body`
  - `a_stop_reply_delayed_past_two_seconds_still_succeeds`
  - `the_list_request_bytes_the_two_second_limit_and_the_one_mebibyte_cap_are_unchanged` (guard)
  - `the_stop_writes_no_container_working_directory_or_environment_to_a_log` (guard: `LogCapture`)
  - `docker_is_sent_no_request_other_than_list_and_stop`
- [x] **Step 2: Run** `cargo test -p navori-mod-ports --test stop_docker` — expect: compile errors for `stop`.
- [x] **Step 3: Factor** `parse_head`'s status check to take accepted statuses (the list call passes `&[200]`); implement `DockerSocket::stop` with no client limit of its own (the request deadline of Task 7 is its limit), and the `FakeContainers` members.
- [x] **Step 4: Run** `cargo test -p navori-mod-ports` — expect: pass. Commit `git commit -m "feat(ports): stop a container through the Docker Engine"`

_Requirements: PSTOP-3.3, PSTOP-3.4, PSTOP-3.7, PSTOP-3.8, PSTOP-3.9_

---

### Task 7: Run a stop request end to end in the actor

**Files:**
- Create: `modules/ports/tests/stop_orchestration.rs`
- Modify: `modules/ports/src/actor.rs`, `modules/ports/src/stop/mod.rs`, `modules/ports/src/lib.rs`

**Reuse:** rung 2 — the `InFlight` abort-on-drop guard and `start_scan` (actor.rs:32-41, 174-194), `ModuleCtx::emit` (module.rs:160), `Request::respond` (module.rs:111).

**Interfaces:**
- Consumes: `resolve`, `Guard`, `FreshRead` (Task 4), `stop_process` (Task 5), `Containers::stop` (Task 6), the `Kill` arm placeholder (Task 3).
- Produces:
  - `StopTask::spawn(request, env clones, arrival)` held in a `JoinSet<StopDone>` local to `run_loop`; `StopDone { counts: [u32; 8] }`
  - one reply per request, in request order, within `arrival + 25 s` (the whole task body, fresh read included, is inside `tokio::time::timeout_at`)
  - one Event `ports.stopped` (`Severity::Info`, `Warn` when any `Failed`), summary = counts per outcome only
  - after a reply: `start_scan(now)` and `deadline = Some(now + cadence)` when no scan is in flight, else `rescan_after = true` handled in the `finished` arm
  - a loop-local registry of in-flight `(pid, start_us)` and container ids; an overlapping target answers `StillStopping` without a second signal

**Depends-on:** Task 3, Task 4, Task 5, Task 6

- [x] **Step 1: Write the failing tests** in `modules/ports/tests/stop_orchestration.rs` through `Rig` (paused clock, `FakeSignals`, `FakeContainers`):
  - `a_kill_request_is_answered_once_with_one_result_per_target_in_request_order`
  - `a_process_and_a_container_stop_at_the_same_time_inside_one_deadline`
  - `a_target_still_stopping_at_twenty_five_seconds_answers_still_stopping`
  - `a_stalled_process_read_fails_every_target_and_signals_nothing`
  - `ports_list_and_open_are_answered_while_a_stop_waits`
  - `the_scan_cadence_is_unchanged_while_a_stop_waits` (guard)
  - `a_valid_kill_from_the_cli_runs_without_a_further_approval` (dispatch through the kernel with no approval argument)
  - `unreachable_docker_does_not_stop_the_process_targets_from_stopping`
  - `a_container_stop_still_running_at_the_deadline_answers_still_stopping` (`FakeContainers::delay_stop`)
  - `a_module_restart_during_a_stop_sends_no_further_signal`
  - `after_the_reply_a_scan_starts_at_once_and_the_row_leaves_the_next_state`
  - `a_reply_that_arrives_while_a_scan_runs_starts_one_more_scan_after_it_and_never_aborts_it`
  - `one_event_with_counts_and_no_process_path_or_container_name_is_emitted`
  - `an_overlapping_second_request_does_not_signal_a_target_twice`
  - `no_timer_runs_while_the_menu_is_closed_and_no_stop_is_in_flight` (guard: `tests/timing.rs` assertions stay green)
  - `a_kill_in_one_profile_writes_its_event_only_to_that_profiles_journal` (guard)
- [x] **Step 2: Run** `cargo test -p navori-mod-ports --test stop_orchestration` — expect: failures (placeholder reply).
- [x] **Step 3: Implement** the `Kill` arm in the `ctx.recv()` branch of `run_loop`: record `arrival`, spawn the task, hold it in the `JoinSet`; the task body is fresh read, `resolve`, concurrent stops in a `FuturesUnordered`, result assembly, all inside the 25 s timeout. Containers are keyed by id so one `stop` per container.
- [x] **Step 4: Implement** the join-set branch of the `select!`: emit the Event, then rescan as above. Dropping the set on return aborts the tasks (`Request` dropped → caller sees `Unavailable`).
- [x] **Step 5: Run** `cargo test -p navori-mod-ports && cargo xtask check-deps` — expect: pass. Commit `git commit -m "feat(ports): carry out a kill request and rescan after it"`

_Requirements: PSTOP-1.17, PSTOP-1.11, PSTOP-1.12, PSTOP-1.13, PSTOP-1.14, PSTOP-1.15, PSTOP-1.19, PSTOP-1.20, PSTOP-3.5, PSTOP-10.2_

---

### Task 8: The Running view in the Core

**Files:**
- Create: `crates/ffi/src/running.rs`, `crates/ffi/tests/running.rs`, `crates/ffi/tests/fixtures/lease_labels.json`
- Modify: `crates/ffi/src/lib.rs`, `crates/ffi/src/app.rs`

**Reuse:** rung 2 — `awake_view` (app.rs:325-340), `ports_state` (app.rs:384), `module_is_down`, `AwakeView`/`LeaseView` (lib.rs:98-107), the snapshot pump (app.rs:535-565); the Lease label rule of `AwakeSubmenuState.place` (AwakeSubmenuState.swift:185-194) ported. The join is new code (rung 7).

**Interfaces:**
- Consumes: `PortsState`, `PortEntry`, `stop_class`, `StopClass`, `Selector`/`Target` JSON shape (Tasks 2-4); `AwakeView`.
- Produces:
  - `pub fn build(awake: Option<&AwakeView>, ports: Option<&PortsState>, own: Own) -> Option<RunningView>`; `Own { pid: i32, helper: Option<PathBuf> }`
  - `uniffi::Record`s: `RunningView { projects, hidden_system_ports, stop_all: Vec<String>, stop_all_ports, in_use_ports, other_ports_left }`, `RunningProject { name, is_other, state: ProjectState, path: Option<String>, up_since_ms: Option<i64>, sessions, ports, stoppable_ports, row_stop_request: Option<String> /* idle projects only */, child_stop_request: Option<String> /* every known project, working included */ }`, `RunningSession`, `RunningPort { port, name, kind, container, openable, protected_in_group, target: StopTargetView, stop_request: String }`, `StopTargetView` (`Process { pid, start_us }` or `Container { id }`), `uniffi::Enum ProjectState { Working, Idle, Unknown }`
  - `CoreSnapshot.running: Option<RunningView>` built in `snapshot()` (lib.rs:497); `ports` passed as `None` when the Ports Module is down
  - fixture `lease_labels.json`: `[{"label": "klynt/main", "project": "klynt"}, {"label": "klynt", "project": "klynt"}, {"label": "/main", "project": "/main"}, {"label": "klynt/", "project": "klynt/"}, {"label": null, "project": null}, {"label": "", "project": null}, {"label": "a/b/c", "project": "a"}]` (null project = unlabeled: Swift sends it to `Other`; Rust treats a live session Lease with an empty or missing label as unlabeled, so every project reads Unknown); the Swift test finds the file from `#filePath` because `Package.swift` has no `resources:`

**Depends-on:** Task 2, Task 4

- [x] **Step 1: Write the failing tests** in `crates/ffi/tests/running.rs` (pure `build` with fixture states):
  - join: `a_label_joins_the_project_before_its_first_slash` (reads the shared fixture), `a_session_lease_with_a_working_flag_makes_its_project_working`
  - `an_unknown_state_carries_its_reason_sentence_per_case_and_none_while_known` (four cases, the sentences verbatim, the Awake problem text passed through unchanged)
  - unknown, one test each: `awake_not_registered`, `awake_state_that_did_not_deserialize` (view `None`), `awake_with_a_problem_set`, `awake_module_down`, `tracking_agents_off`, `any_live_session_lease_without_a_label`; each leaves `row_stop_request` and `child_stop_request` empty for every project
  - `a_project_with_no_working_session_is_idle`
  - `the_ports_module_down_gives_no_stop_requests` (ports `None`)
  - order: `idle_and_unknown_projects_come_first_then_case_insensitive_name_then_other_last`
  - stoppable: `a_database_and_a_system_port_are_left_out_of_the_group_requests_but_have_their_own_pid_request`, `a_container_is_in_the_group_request_and_a_database_image_container_is_not`, `an_unclaimed_docker_entry_has_no_stop_request`
  - `stop_all_holds_each_idle_named_project_with_a_stoppable_port_and_never_other`, `the_counts_equal_the_ports_in_the_requests_sent`, `other_ports_left_counts_the_ports_in_other`
  - `the_view_carries_the_project_path_and_the_oldest_listeners_start`
  - `a_port_row_carries_its_worktree_and_harness_and_a_docker_port_carries_no_harness`
  - `two_ports_of_one_process_are_one_target_and_count_as_two_ports`
  - `a_repo_literally_named_other_stays_a_named_project_not_the_no_project_group`
  - `a_row_stop_request_exists_only_for_an_idle_project_and_a_child_stop_request_for_every_known_project`
  - `openable_is_true_for_a_listed_port_that_is_not_a_database_or_system_port` (`agent-decided`: `ports.open` accepts any listed port, so the Shell offers `Open` only where a page is plausible)
  - the tray and status icon not reading the view is checked by `rg -n "running::" crates/kernel` returning nothing (guard, a grep step)
  - a snapshot test in `crates/ffi/tests/core.rs` helpers style: `the_snapshot_carries_a_running_view_once_ports_has_scanned`, with no `kill` dispatch
- [x] **Step 2: Run** `cargo test -p navori-ffi --test running` — expect: compile errors.
- [x] **Step 3: Implement** `running.rs` per design section H; request strings are the exact `kill` JSON of Task 3 (a `Project` selector for a group and for a container, a `Pid` selector for a process port).
- [x] **Step 4: Wire** `snapshot()` and rebase onto `main` first (shared ffi files). Run `cargo test -p navori-ffi && cargo xtask ci` — expect: pass (it also builds the Swift shell, which must still compile with the new field unused).
- [x] **Step 5: Commit** `git commit -m "feat(ffi): join ports and awake into one running view"`

_Requirements: PSTOP-4.1, PSTOP-4.2, PSTOP-4.3, PSTOP-4.4, PSTOP-4.5, PSTOP-4.6, PSTOP-4.7, PSTOP-4.12, PSTOP-4.20, PSTOP-4.21, PSTOP-6.9, PSTOP-7.3_

---

### Task 9: Pure Swift state for the Running menu

**Files:**
- Create: `apps/macos/NavoriPanel/RunningState.swift`, `apps/macos/NavoriPanel/StopFlow.swift`, `apps/macos/Tests/NavoriPanelTests/RunningStateTests.swift`, `apps/macos/Tests/NavoriPanelTests/StopFlowTests.swift`, `apps/macos/Tests/NavoriPanelTests/LeaseLabelFixtureTests.swift`
- Modify: `apps/macos/Tests/NavoriPanelTests/AwakeSubmenuStateTests.swift`

**Reuse:** rung 2 — `StatusDot` (CoreRowState.swift), `AwakeRowState` for the row-state shape, `AwakeSubmenuState`'s value-type style; no generated types in `NavoriPanel/` (testable target).

**Interfaces:**
- Consumes: the field names of `RunningView` (Task 8) mirrored as plain Swift input structs.
- Produces:
  - `RunningInput`, `RunningProjectInput` (plain structs mirroring the record)
  - `RunningRowState(running: RunningInput?)`: `label`, `dot: StatusDot?`, `hasSubmenu`, `spoken`
  - `enum ProjectDot { filled, ring, none }`, `ProjectRowState { name, dot, spoken, stop: StopOffer? }`, `StopOffer { count: Int, request: String }` (built from `row_stop_request` only; the child's `Stop N ports` is built from `child_stop_request`)
  - `RunningSubmenuState(running:)`: `rows`, `stopAll: StopAllOffer?` (`StopAllOffer { requests: [String], ports: Int, projects: Int, inUse: Int, otherLeft: Int, systemHidden: Int }`), `nothingRunning: Bool`
  - `enum StopFlow { idle, asking(count: Int), running, failed([String]) }` with `press()`, `confirm()`, `dismiss()`, `finish([StopResult])`; a second `confirm()` while `running` is ignored
  - `struct StopResult: Decodable { target, result, reason }` and `KillReply: Decodable`

**Depends-on:** Task 8

- [x] **Step 1: Write the failing tests** (Swift Testing, `#expect`):
  - `RunningRowStateTests`: a green dot when anything runs, no dot otherwise, `Running, active` and `Running, nothing running`, no submenu when the running view is missing
  - `RunningSubmenuStateTests`: filled dot for working, ring for idle, no dot for unknown; the spoken sentences `klynt, working` and `openklynt-main, 3 idle ports`; `Nothing running` shows no `Stop all ports…`; the note line appears only while `unknown_reason` is set and an unknown project is spoken as `<name>, working state unknown`; a project row offers `Stop N` only when the input carries a `row_stop_request` (none on a working or unknown row); the submenu order is Keep awake, Tracking agents, the problem line, a separator, the projects, a separator, `Stop all ports…`; `Stop all ports…` absent when no named project is idle
  - `StopFlowTests`: nothing is sent before `confirm`; `dismiss` returns to idle; a second `confirm` while running is ignored; a result of stale keeps the row and carries the reason; stopped, stopped-alone and already-gone clear it
  - `LeaseLabelFixtureTests`: reads `crates/ffi/tests/fixtures/lease_labels.json` and asserts `AwakeSubmenuState`'s project for each label equals the fixture's (a null project means project `Other`)
- [x] **Step 2: Run** the Swift test command (Global Constraints) — expect: compile errors.
- [x] **Step 3: Implement** the types; keep every rule that decides whether a stop is offered in the Core's view (`StopOffer` is present only when the input carries a request).
- [x] **Step 4: Run** the Swift test command — expect: pass. Commit `git commit -m "feat(shell): add pure state for the running menu"`

_Requirements: PSTOP-4.8, PSTOP-4.9, PSTOP-4.10, PSTOP-4.11, PSTOP-4.13, PSTOP-4.14, PSTOP-4.17, PSTOP-5.1, PSTOP-7.1_

---

### Task 10: The Running row and submenu

**Files:**
- Create: `apps/macos/Navori/Features/Panel/RunningSubmenuView.swift`, `apps/macos/Navori/Features/Panel/ProjectRowView.swift`
- Modify: `apps/macos/Navori/Platform/PerfProbe.swift` (only if the probe can open a row by id), `apps/macos/Navori/Features/Panel/PanelView.swift`, `apps/macos/Navori/Features/Panel/AwakeSubmenuView.swift`, `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/NavoriPanel/CascadeState.swift`, `apps/macos/Tests/NavoriPanelTests/CascadeStateTests.swift`

**Reuse:** rung 2 — `MenuRowView`, `MenuRow`, the cascade (`CascadeController`, `CascadeState`, `SafeDiagonal`, `CascadePlacement`), `StatusDot`, `AwakeSubmenuView`'s `Keep awake`, `Tracking agents` and problem line (moved), `HarnessMarkImage`, the `CoreStore` snapshot-property pattern (CoreStore.swift:146-216). `ProjectRowView` and the ring dot are new (rung 7): a sibling-button layout, because a nested `Button` inside `MenuRowView`'s whole-row `Button` with `.accessibilityElement(children: .ignore)` breaks accessibility and pointer tracking.

**Interfaces:**
- Consumes: `RunningRowState`, `RunningSubmenuState`, `ProjectRowState` (Task 9); `CoreSnapshot.running` (Task 8).
- Produces:
  - `CoreStore.running: RunningInput?` computed from `snapshot.running`
  - row id `running` (replaces `awake`); submenu factory cases `running`, `awake.keep` (unchanged id), prefix `running.project.<name>` and `running.other`; `menuTree()` gains one `CascadeNode` per project row with no children yet (Task 12 gives them the child menu, so this task's dev app shows no chevron on a project row); an observation of the project-id list calls `cascade.replaceTree()` when it changes
  - `CascadeState.handleReplaceTree` moves focus to the parent when the focused row left
  - the Holding list (`HoldingList`, `HoldingLine`) leaves `AwakeSubmenuView`; `Keep awake`, `Tracking agents`, the problem line and `KeepAwakeSubmenuView` live in `RunningSubmenuView`

**Depends-on:** Task 9

- [x] **Step 1: Read** `CascadeState.swift` (`maxDepth` semantics, whether the root counts) and `xtask/src/perf/` (how `submenu_open_ms` selects a submenu by row id); note both in the task report. If `maxDepth: 3` does not fit Running → projects → child, raise it in `CascadeState` and test it.
- [x] **Step 2: Write the failing tests** in `CascadeStateTests.swift`: `a_row_that_vanishes_while_focused_moves_focus_to_its_parent`, `a_tree_with_project_children_opens_three_levels_deep`.
- [x] **Step 3: Implement** `RunningSubmenuView` and `ProjectRowView` (dot or ring, name only, no secondary text), the `PanelView` row `MenuRow(id: "running", icon: "cup.and.saucer" (the Awake row's existing icon, PanelView.swift:42), title: "Running", value: nil, dot:, opensSubmenu:)`, the factory and tree changes, the observation, and `handleReplaceTree`. Opening the submenu reads only the snapshot. Rebase onto `main` first (shared Swift files).
- [x] **Step 4: Verify** by hand in the dev app (`cargo xtask run`, then the cold-start drive of `docs/agents/verify.md`): the level-0 `Running` row, the project list, the ring and dot, `Nothing running`, the problem line; and the unchanged `Keep awake` and `Tracking agents` still work. Run `cargo xtask ci && cargo xtask perf` — expect: pass (guard: the existing metrics; no `--record`). Note: the probe drives the Core submenu (`openCoreSubmenuFresh`, PerfProbe.swift:96, StatusBarController.swift:188-194), not Running. Read both; if the probe can open another row by id, point a second probe at `running`; if timing Running needs a new xtask metric, do not add it here: record "PSTOP-10.1 for the Running submenu is not measured by `cargo xtask perf`" in the run report, never reporting it as met, and time the first frame by hand with the dev app.
- [x] **Step 5: Commit** `git commit -m "feat(shell): show running projects in one menu"`

_Requirements: PSTOP-4.15, PSTOP-4.16, PSTOP-4.18, PSTOP-10.1, PSTOP-10.5_

---

### Task 11: Stop a project and stop all, with a confirmation

**Files:**
- Create: `apps/macos/Navori/Features/Panel/InlineConfirmation.swift`
- Modify: `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/Navori/Features/Panel/ProjectRowView.swift`, `apps/macos/Navori/Features/Panel/RunningSubmenuView.swift`, `apps/macos/NavoriPanel/StopFlow.swift`

**Reuse:** rung 2 — `CoreStore.dispatch(module:actionJSON:)` (CoreStore.swift:73) and its failure pattern (`perform`, `AwakeActionFailure`, `CoreStartupBanner.menuFailure`, CoreStore.swift:201-216); the cascade hover `selection`. The inline confirmation is new (rung 7).

**Interfaces:**
- Consumes: `StopFlow`, `StopResult`, `KillReply`, `StopOffer`, `StopAllOffer` (Task 9); the project `row_stop_request` and `child_stop_request` strings (Task 8).
- Produces:
  - `CoreStore.stop(_ requests: [String]) async -> [StopResult]` (dispatches `ports` once per request, decodes the reply, keeps `stopProgress` per project and per port until a later snapshot no longer lists the target)
  - `InlineConfirmation` view: text names the ports (never processes), `Stop` and `Cancel`; focus lands on `Cancel`; `Esc`, `Cancel`, the pointer leaving or the menu closing send nothing
  - a ringed project row shows `Stop N` while selected; `Stop all ports…` row with its own confirmation naming port count, project count, the "in use" count and the ports left in `Other`

**Depends-on:** Task 9, Task 10

- [x] **Step 1: Write the failing tests** in `StopFlowTests.swift`: `stop_all_sends_one_request_per_idle_project_after_one_confirmation`, `a_failed_target_keeps_its_row_with_the_reason`, `fewer_stops_than_confirmed_read_as_results_not_as_an_error`, `a_confirmation_text_names_ports_and_the_in_use_and_other_counts`.
- [x] **Step 2: Run** the Swift test command — expect: failures.
- [x] **Step 3: Implement** `CoreStore.stop`, `InlineConfirmation`, the `Stop N` button as a sibling of the row's own button (Tab reaches it, VoiceOver reads `Stop 3 ports`, pressing it does not open the child), and `Stop all ports…`.
- [x] **Step 4: Verify** in the dev app on a throwaway listener this step spawns (`python3 -m http.server 0`, started in a fresh `git init` repo at `target/pstop-e2e-<n>` inside this worktree: `target/` is gitignored, lies under `$HOME` with a non-hidden first component, so `place()` counts it as project `pstop-e2e-<n>`; then a hand-made session Lease labelled `pstop-e2e-<n>/main` with `navori --profile dev dispatch awake` so the project joins and the row is working, and a second run with no Lease so it is idle): hover, press `Stop N`, confirm, and read the row leave on the next state; dismiss sends nothing (check with `navori --profile dev events`). Never press `Stop` on a listener the step did not start.
- [x] **Step 5: Run** `cargo xtask ci` — expect: pass. Commit `git commit -m "feat(shell): stop a project's ports or all idle ports"`

_Requirements: PSTOP-5.2, PSTOP-5.3, PSTOP-5.4, PSTOP-5.5, PSTOP-5.6, PSTOP-5.7, PSTOP-5.8, PSTOP-7.2, PSTOP-7.4, PSTOP-7.5_

---

### Task 12: The project child menu and stopping one port

**Files:**
- Create: `apps/macos/Navori/Features/Panel/ProjectChildView.swift`
- Modify: `apps/macos/Navori/Features/Panel/ProjectRowView.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/NavoriPanel/RunningState.swift`, `apps/macos/Tests/NavoriPanelTests/RunningStateTests.swift`

**Reuse:** rung 2 — `MenuRowView`, `HarnessMarkImage`, `InlineConfirmation` (Task 11), `CoreStore.stop`; the hover `selection` of the cascade.

**Interfaces:**
- Consumes: `RunningProjectInput.sessions/ports/path/upSinceMs/stopRequest`, `RunningPort.stopRequest`, `StopFlow` (Tasks 9, 11).
- Produces:
  - `ProjectChildState` (pure, in `RunningState.swift`): header (name, path, `up 2d 20h` only when no working session), `Sessions` and `Ports` sections (a section with no rows is left out), `Stop N ports` for any project whose stoppable list is not empty, `N system ports hidden` for `Other`, per-row spoken sentences with the state word
  - `ProjectChildView` at about 330 pt; port rows swap the port for `Stop` on hover (the `Open` and `Copy` controls of Task 13 sit beside it); `Stop` running and `Stop` failed with the reason

**Depends-on:** Task 10, Task 11

- [x] **Step 1: Write the failing tests** in `RunningStateTests.swift`: `the_header_shows_up_only_for_a_project_with_no_working_session`, `a_section_with_no_rows_is_left_out`, `a_working_project_still_has_stop_n_ports_in_its_child` (from `child_stop_request`), `other_says_how_many_system_ports_are_hidden`, `every_row_reads_as_one_sentence_with_its_state_word`, `a_port_stop_uses_the_pid_request_for_a_process_and_the_project_request_for_a_container`, `a_port_rows_tooltip_and_spoken_sentence_name_its_worktree_and_harness_and_none_for_docker`.
- [x] **Step 2: Run** the Swift test command — expect: failures.
- [x] **Step 3: Implement** `ProjectChildState` and `ProjectChildView`; register the child in the factory and `menuTree()`.
- [x] **Step 4: Verify** in the dev app on the throwaway listener of Task 11: open the child, press a port's `Stop`, confirm; a protected database port shows its own `Stop`; read `Stop` failed with the reason on a refused target.
- [x] **Step 5: Run** `cargo xtask ci` — expect: pass. Commit `git commit -m "feat(shell): add the project child menu and a single-port stop"`

_Requirements: PSTOP-6.1, PSTOP-6.2, PSTOP-6.3, PSTOP-6.4, PSTOP-6.5, PSTOP-6.6, PSTOP-6.7, PSTOP-6.8, PSTOP-6.9_

---

### Task 13: Open a port in the browser; copy and reveal

**Files:**
- Modify: `apps/macos/Navori/Core/CoreStore.swift`, `apps/macos/Navori/Features/Panel/ProjectChildView.swift`, `apps/macos/NavoriPanel/RunningState.swift`, `apps/macos/Tests/NavoriPanelTests/RunningStateTests.swift`

**Reuse:** rung 4 — `NSWorkspace.shared.open` and `activateFileViewerSelecting`, `NSPasteboard.general`; rung 2 — `CoreStore.dispatch` and the `ports.open` reply of PORTS-6.1 (`http://localhost:<port>`).

**Interfaces:**
- Consumes: `RunningPort.openable` (Task 8), `ProjectChildView` (Task 12).
- Produces:
  - `CoreStore.openPort(_ port: UInt16, address: String) async` dispatching `{"open":{"port":…,"address":…}}` to `ports` and opening the returned URL
  - pure `PortActions` helper: `copyText(port) -> String` (`localhost:<port>`), `hasOpen(port)`
  - header path is a button that reveals the folder

**Depends-on:** Task 12

- [x] **Step 1: Write the failing tests** in `RunningStateTests.swift`: `open_is_offered_only_where_the_core_says_the_port_is_openable`, `copy_text_is_localhost_and_the_port`.
- [x] **Step 2: Run** the Swift test command — expect: failures.
- [x] **Step 3: Implement** `openPort`, the controls and the reveal.
- [x] **Step 4: Verify** in the dev app on the throwaway listener: `Open` opens the browser at `http://localhost:<port>`; `Copy` puts `localhost:<port>` on the pasteboard and sends nothing (check `navori --profile dev events`); the path reveals in Finder.
- [x] **Step 5: Run** `cargo xtask ci` — expect: pass. Commit `git commit -m "feat(shell): open, copy and reveal from the project menu"`

_Requirements: PSTOP-9.1, PSTOP-9.2, PSTOP-9.3, PSTOP-9.4_

---

### Task 14: Documents, the Settings sentence and the Awake amendments

**Files:**
- Modify: `apps/macos/Navori/Features/Settings/SettingsView.swift`, `docs/architecture/integrations.md`, `docs/specs/2026-11-02-awake-leases/requirements.md`, `docs/specs/2026-11-04-port-discovery/requirements.md`, `docs/specs/2026-11-04-port-discovery/design.md`, `docs/codebase/modules.md`, `docs/agents/verify.md`

**Reuse:** rung 2 — the in-place amendment note style of the AWAKE spec (`*(Amended 2026-11-04: …)*`).

**Interfaces:**
- Consumes: the shipped behavior of Tasks 3 to 13.
- Produces: no code interface.
  - `SettingsView.swift:135` sentence names `ports`: "Run the checks to verify config, database, IPC, modules, awake, integrations, logs and ports."
  - `integrations.md` BSD row (:28) gains the leader-only group rule and "otherwise the process alone"; Docker row (:34) says Navori writes to Docker only by stopping a container, in place of "Nothing is written to Docker"
  - an amendment note naming PSTOP on AWAKE-11.1, 11.7, 11.13, 11.15, 11.17 and 17.7
  - port-discovery notes at `requirements.md:164` and `design.md:57,252,388` pointing to PSTOP-3.9 and 3.10
  - `verify.md:162` "no screen yet" gotcha replaced by a line on the Running menu

**Depends-on:** Task 13

- [x] **Step 1: Write the failing check** as a grep step: `rg -n "Nothing is written to Docker" docs/architecture docs/specs/2026-11-04-port-discovery` and `rg -n "integrations and logs\." apps/macos` return hits — expect: hits now (the PSTOP spec itself quotes the phrase and is out of the grep's scope).
- [x] **Step 2: Edit** the files above.
- [x] **Step 3: Run** the same greps — expect: no hit for either phrase, and `rg -n "Amended 2026-11-04 by PSTOP" docs/specs/2026-11-02-awake-leases/requirements.md` returns six lines (AWAKE-11.1, 11.7, 11.13, 11.15, 11.17, 17.7); `navori --profile dev doctor` still lists the same rows in the same order with `ports` after `logs`; `cargo test -p navori-ffi --test integrations` passes (guard for the doctor row order).
- [x] **Step 4: Run** `cargo xtask ci` — expect: pass. Commit `git commit -m "docs(PSTOP): amend the Docker row, the Awake criteria and the Doctor text"`

_Requirements: PSTOP-3.10, PSTOP-4.19, PSTOP-8.1, PSTOP-8.2_

---

### Task 15: Whole-feature verification

**Files:**
- Modify: `docs/specs/2026-11-04-ports-panel-and-stop/tasks.md` (tick the boxes; nothing else)

**Reuse:** existing — `cargo xtask ci`, `cargo xtask perf`, the cold-start drive of `docs/agents/verify.md`.

**Interfaces:**
- Consumes: Tasks 1 to 14.
- Produces: a verification note in the run report.

**Depends-on:** Task 14

- [ ] **Step 1: Run** `cargo xtask ci` — expect: pass (fmt, clippy `-D warnings`, tests, check-deps, Swift build).
- [ ] **Step 2: Run** `cargo xtask perf` on an idle machine, without `--record` — expect: pass for `submenu_open_ms`, `menu_reopen_ms`, `menu_open_cpu_ms`, `port_scan_ms` and idle CPU and wake-ups (the GOAL-11 budgets). If Task 10 found that the probe cannot open the Running submenu, report PSTOP-10.1 as "not measured for Running; existing metrics unchanged", never as met, and put that line in the run report.
- [ ] **Step 3: Drive** the dev app (Launch, Doctor, Drive, Evidence): spawn a throwaway listener in a throwaway git repo, confirm the project shows a ring, press `Stop N` and confirm, read the port released and the next state without the row; then `navori --profile dev dispatch ports '{"kill":{"selector":{"pid":<pid>},"targets":[{"process":{"pid":<pid>,"start_us":1}}]}}'` for the throwaway's pid (read from `ports.list`) returns `Stale` and leaves it running, and a dispatch naming the throwaway's own pid and start time stops it and returns `Stopped` or `StoppedAlone`. Kill-self and kill-ancestor are covered only by the fakes of Task 4: the Core holds no TCP listener, so a real dispatch naming it would answer `Stale`, and no step names a process it did not spawn.
- [ ] **Step 4: Check** the three Open Questions of `requirements.md` are still open and unverified items are not stated as facts anywhere in the diff.
- [ ] **Step 5: Commit** any tick-box change `git commit -m "docs(PSTOP): record the verification of the plan"`

_Requirements: PSTOP-10.1, PSTOP-10.2, PSTOP-10.3_
