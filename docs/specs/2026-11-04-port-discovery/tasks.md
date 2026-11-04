# Tasks: Port discovery

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: PORTS
Status: In-progress
Approved by the operator ("approve", 2026-11-04 16:34 +07, typed in the conductor session after reading the review sheet for 827b756; relayed in .skills/_pending-ports/auto/operator-reply.md).
Date: 2026-11-04
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

**Goal:** Add a `ports` Module that scans the user's listening TCP ports without `lsof` and answers `ports.list` and
`ports.open`. Each entry is placed under its project and worktree, with the Harness that started it, its kind, and its
container for Docker ports.

**Architecture:**
- `navori-macos` gains socket, environment-marker and short-name reads on `ProcessSource`.
- `navori-procs` gains `find_repo`, identity-checked `Snapshot` reads and `started_by`.
- A new crate `navori-mod-ports` (`modules/ports`) composes them into a blocking `Scanner`. It adds a hand-written
  Docker Engine read over `tokio::net::UnixStream`, and runs an actor timed by the Scheduler.
- `navori-ffi` registers the Module and adds a `ports` doctor row.
- `xtask perf` gains `port_scan_ms`.

**Tech Stack:** Rust 2024 (rust-version 1.90), `libc` 0.2.191, `tokio` 1.54.1 (`net`, `io-util`, `time`), `serde`,
`serde_json`, `schemars` 1.2.3, `futures`, `thiserror`, `tracing`, `tempfile`. No new dependency.
`cargo test -p <crate>`, `cargo xtask ci`, `cargo xtask perf`.

**Source classes:** every value in `requirements.md` and `design.md` is a Lock, `agent-decided`, or Unverified. A task
never promotes one to the operator's choice, and a code comment never says an `agent-decided` value was decided by the
user.

## Global Constraints

Source of truth (referenced, not copied — read before every task):

| Source | Content hash (`git hash-object`, first 12) |
|---|---|
| `docs/standards/INDEX.md` | `f83e09733ee4` |
| `docs/standards/testing.md` | `b1a9b42fc1d3` |
| `docs/standards/errors-logging.md` | `8696a9b77e1f` |
| `docs/standards/performance.md` | `c94919963687` |
| `docs/architecture/INDEX.md` (ARCH-N) | `2e318bd23ff5` |

Compact delta every task inherits:

- **Test-first at agreed seams** (`design.md` "Seams for testing"):
  - Test names are sentences in domain language.
  - No requirement ID goes in test or production source.
  - Expected values are literals, never recomputed.
  - Fakes only at system boundaries: `navori_macos::fake::FakeProcesses`, `FakeContainers`, `FakeClock`.
- **Temp paths are canonical.** macOS reports `/var/folders/...` as `/private/var/...`. Every temp home, repo and
  expected path in a test goes through `std::fs::canonicalize` first, as `ChildTree::spawn` does
  (crates/macos/tests/process_source.rs:14-40).
- **Secrets in fixtures:**
  - An environment entry that must not leak is `concat!("CLAUDE_CODE_MESSAGING_TOKEN=", "tok_", "abcdefghijklmnop")`.
  - Tests assert that neither the full entry nor the name `CLAUDE_CODE_MESSAGING_TOKEN` appears in any return value,
    error text, captured log line or published state.
- **Build limits:**
  - No step writes outside the worktree or a temporary directory.
  - Only the ignored real-OS tests and `cargo xtask perf` contact Docker, and they only read.
  - No step prints a process's environment value.
- **ARCH-1, 3, 4, 6, 9, 10, 11, 13, 14, 16** hold in every task:
  - `unsafe` only in `navori-macos`, each block with a `// SAFETY:` comment, and no `pub unsafe` item.
  - `[lints] workspace = true` on the new crate.
  - No `unwrap`/`expect` outside tests.
- **Gate:**
  - Every task: `cargo fmt --all`, then `cargo clippy --workspace --all-targets -- -D warnings`, then the task's
    `cargo test -p <crate>`.
  - `cargo xtask ci` runs in Tasks 2, 9 and 11, the tasks that touch `crates/ffi` or close the plan.
- **Shared files** (load-subgraph cluster(PORTS), advisory, owns_coverage 10/14):
  - PROCS shares 13 files under `crates/macos` and `crates/procs`.
  - AWAKE, HOOKS, KCONTRACT, ASK and ICON share `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs` and
    `crates/ffi/tests/core.rs`.
  - MENU and PERF share `xtask/src/perf/*`.
  - ASK and HOOKS are In-progress in the catalog, so Tasks 2 and 9 rebase onto `main` before editing `crates/ffi`.
- **Commits:** Conventional Commits, one logical change per commit, every commit builds. No `git push`.
- **Unverified, stay open.** No task turns these into an assertion about the world:
  - `CODEX_THREAD_ID` and `GROK_AGENT` on real listeners (Task 11 checks by hand).
  - The Docker read cost in Rust (Task 11 measures it).
  - The database names.
  - `com.docker.compose.project.working_dir` as a documented contract.

## File Structure

Create:
- `crates/macos/src/sockets.rs` — `Listener`, socket-fd reads, `socket_fdinfo` offsets, and the libc constants `PROC_PIDFDSOCKETINFO`, `SOCKINFO_TCP` and `TSI_S_LISTEN`.
- `crates/macos/tests/sockets.rs` — real-OS loopback read-back (ignored) and fake listener behaviour.
- `crates/procs/tests/listeners.rs` — `Snapshot::listeners`, `still_same`, `name`.
- `crates/procs/tests/markers.rs` — `started_by` and markers.
- `modules/ports/Cargo.toml` — crate `navori-mod-ports`.
- `modules/ports/src/lib.rs` — `Ports`, `PortsEnv`, `OpenProcesses`, `Module` impl, `tray`, the `pub use` surface.
- `modules/ports/src/action.rs` — `PortsAction`, risk and catalog.
- `modules/ports/src/state.rs` — `PortsState`, `ProjectGroup`, `WorktreeGroup`, `ProcessGroup`, `PortEntry`, `Placement`, `PortKind`, `HarnessName`, `BranchName`, `ContainerRef`, `DockerStatus`, grouping and order.
- `modules/ports/src/actor.rs` — the `select!` loop and deadlines.
- `modules/ports/src/scan.rs` — `Scanner`, `Scan`, `BackendPort`, `HarnessMemo`.
- `modules/ports/src/place.rs` — placement and the home guard.
- `modules/ports/src/kind.rs` — the `PortKind` rule.
- `modules/ports/src/docker.rs` — `attribute`.
- `modules/ports/src/infra/mod.rs`, `modules/ports/src/infra/docker.rs` — `Containers`, `Container`, `DockerSocket`, `DockerError`, HTTP/1.1 framing.
- `modules/ports/tests/common/mod.rs` — `Rig`, `FakeClock` (both copied shape from `modules/awake/tests/common/mod.rs`), `Gate` (a `ProcessSource` wrapper that can deny, block or panic once), `restarts` (copied from `modules/awake/tests/restart.rs`), `canonical_temp_home`, and from Task 6 `FakeContainers`.
- `modules/ports/tests/scan.rs`, `place.rs`, `harness.rs`, `kind.rs`, `docker.rs`, `docker_client.rs`, `open.rs`, `timing.rs`, `failure.rs`, `real_os.rs` — tests per story.

Modify:
- `crates/macos/src/process.rs` — three `ProcessSource` methods and `MacProcesses` bodies.
- `crates/macos/src/procargs.rs` — `with_procargs` and the in-place marker matcher.
- `crates/macos/src/lib.rs` — `mod sockets; pub use sockets::*;`.
- `crates/macos/src/fake/processes.rs` — builders `.listen`, `.fds_err`, `.env`, `.name`; read kinds `fds`, `env`, `name`.
- `crates/macos/tests/process_source.rs` — real-OS removed-cwd and environment-marker reads (ignored).
- `crates/procs/src/location.rs` — `find_repo`, `Repo`; `resolve_location` calls it.
- `crates/procs/src/snapshot.rs` — `listeners`, `still_same`, `env_marker`, `name`.
- `crates/procs/src/harness.rs` — `MARKERS`, `started_by`.
- `crates/procs/tests/location.rs` — `find_repo` cases.
- `modules/awake/tests/command.rs` — `FirstRuid` delegates the three new methods.
- `Cargo.toml` — workspace member `modules/ports`, dependency `navori-mod-ports`.
- `crates/ffi/Cargo.toml`, `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs` — register `Ports`, `module_is_down`, `ports_check`, the `ports` doctor row.
- `crates/ffi/tests/core.rs`, `crates/ffi/tests/integrations.rs` — exact doctor row lists gain `ports` after `logs`.
- `xtask/src/perf/mod.rs`, `xtask/src/perf/judge.rs`, `xtask/src/perf/home.rs` — `port_scan_ms`, a shared `median_ms` parser, the Docker socket link in each run's home copy.
- `docs/standards/performance.md` — budget row, method line, floor.
- `docs/architecture/integrations.md`, `docs/architecture/system.md`, `docs/security/threat-model.md` — reads and Ports status.
- `docs/agents/verify.md`, `README.md`, `docs/codebase/modules.md`, `docs/codebase/map.md`, `docs/codebase/dependencies.md` — the `ports` row, module and crate.

Not touched: `apps/macos/**` (SettingsView.swift:132's sentence is `compat` until ROAD-18), `apps/cli/**`, `crates/kernel/**`.

Order: Tasks share `scan.rs`, `state.rs`, `actor.rs` and `tests/common/mod.rs`, so the edges below form one chain. That
is what the tasks need, not a tidy-up.

---

### Task 1: Read listening sockets of the user's processes

**Files:**
- Create: `crates/macos/src/sockets.rs`
- Modify: `crates/macos/src/process.rs`, `crates/macos/src/lib.rs`, `crates/macos/src/fake/processes.rs`, `crates/procs/src/snapshot.rs`, `modules/awake/tests/command.rs`
- Test: `crates/macos/tests/sockets.rs`, `crates/procs/tests/listeners.rs`

**Reuse:**
- navori-macos: rung 2 — extends `ProcessSource`, `MacProcesses`, the `sysctl`/`read_error` helpers and the procargs scratch (process.rs:34-55,285-312; procargs.rs:30-127); rung 5 — `libc::proc_pidinfo`, `proc_pidfdinfo`, `proc_name`, `PROC_PIDLISTFDS`, `PROX_FDTYPE_SOCKET` (libc 0.2.191 apple/mod.rs:3990-5502). libc lacks `PROC_PIDFDSOCKETINFO` (3), `SOCKINFO_TCP` (2) and `TSI_S_LISTEN` (1); sockets.rs defines them from sys/proc_info.h:415,530,777.
- navori-procs: rung 2 — `resolve_location` (location.rs:21-43), `Snapshot::read_verified` (snapshot.rs:159), `harness_of` (lookup.rs:28), `CATALOG` and `recognize` (harness.rs:59-120)

**Interfaces:**
- Consumes: `ProcEntry`, `ReadError`, `read_error` (process.rs:306), `Snapshot::read_verified`.
- Produces:
  - `ProcessSource::listeners(&self, pid: i32) -> Result<Vec<Listener>, ReadError>`
  - `ProcessSource::name(&self, pid: i32) -> Result<String, ReadError>`
  - `pub struct Listener { pub addr: IpAddr, pub port: u16 }` deriving `Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord`
  - On `FakeProcess`: `listen(self, addr: &str, port: u16) -> Self`, `fds_err(self, err: ReadError) -> Self`, `name(self, name: &str) -> Self`, and read kinds `"fds"`, `"name"`
  - `Snapshot::listeners(&self, id: ProcId) -> Field<Vec<Listener>>`
  - `Snapshot::still_same(&self) -> Result<HashSet<ProcId>, ProcsError>`
  - `Snapshot::name(&self, id: ProcId) -> Field<String>`

**Depends-on:** none

- [x] **Step 1: Write the failing tests.**
  - In `crates/macos/tests/sockets.rs`, `#[ignore]` `a_loopback_listener_in_this_process_is_read_back`:
    - Bind `127.0.0.1:0` and `[::1]:0` with `std::net::TcpListener`.
    - Connect one `TcpStream` to the first.
    - Expect exactly the two listening `(addr, port)` pairs from `MacProcesses::new()?.listeners(std::process::id() as i32)`.
  - In `crates/procs/tests/listeners.rs`:
    - `only_processes_listed_under_this_user_have_their_sockets_read`: a uid-0 listener in `FakeProcesses::new(501)`; `reads()` has no `(pid, "fds")` for it.
    - `a_pid_taken_over_by_another_user_during_the_scan_is_dropped`: `replace_on_read(pid, "fds", 2, 0)`; `still_same()` excludes the old `ProcId`.
    - `a_process_that_exits_while_its_sockets_are_read_is_dropped`: `exit(pid)` after the read; `still_same()` excludes it.
    - `a_refused_socket_read_is_reported_as_an_error`: `.fds_err(ReadError::Denied)` gives `Err(Unavailable::PermissionDenied)`.
- [x] **Step 2: Run** `cargo test -p navori-procs --test listeners` — expect: compile errors for `listeners`, `still_same`, `listen`.
- [x] **Step 3: Implement `sockets.rs`.**
  - Probe the `PROC_PIDLISTFDS` size, read, and retry once if it grew.
  - For each `PROX_FDTYPE_SOCKET` fd, call `proc_pidfdinfo(.., PROC_PIDFDSOCKETINFO)` into 792 bytes. A short read is `Malformed`. `EBADF` skips the fd.
  - Keep `SOCKINFO_TCP` with `TSI_S_LISTEN`.
  - `name` uses `proc_name`.
  - Add both methods to `ProcessSource`, `MacProcesses`, `FakeProcesses` and `FirstRuid`.
- [x] **Step 4: Implement** the three `Snapshot` reads:
  - `listeners` is a plain read for a listed pid.
  - `name` goes through `read_verified`.
  - `still_same` makes one `list()` and keeps the `ProcId`s whose start time and real UID are unchanged.
- [x] **Step 5: Run** `cargo test -p navori-macos && cargo test -p navori-procs && cargo test -p navori-mod-awake && cargo test -p navori-macos --test sockets -- --ignored` — expect: pass.
- [x] **Step 6: Commit** `git commit -m "feat(macos): read the listening TCP sockets of a process"`

_Requirements: PORTS-1.2, PORTS-1.9, PORTS-10.5_

---

### Task 2: List my listening ports

**Files:**
- Create: `modules/ports/Cargo.toml`, `modules/ports/src/lib.rs`, `modules/ports/src/action.rs`, `modules/ports/src/state.rs`, `modules/ports/src/actor.rs`, `modules/ports/src/scan.rs`, `modules/ports/tests/common/mod.rs`, `modules/ports/tests/scan.rs`
- Modify: `Cargo.toml`, `crates/ffi/Cargo.toml`, `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs`, `docs/architecture/system.md`, `docs/codebase/modules.md`, `docs/codebase/map.md`, `docs/codebase/dependencies.md`, `README.md`

**Reuse:**
- Scan: none — new code (rung 7). Nothing composes sockets, placement and kind today. It is built only from the navori-procs items in this design.
- Module: rung 2 — the `Module` trait, `ModuleCtx`, `WaitHandle` and `TrayContribution` (crates/kernel/src/module.rs:54-84,143-165; scheduler.rs:112), with the Awake actor's `select!` loop shape (modules/awake/src/actor.rs:89-127) and the Integrations crate layout as the template.

**Interfaces:**
- Consumes: `Snapshot::listeners`, `still_same`, `name` (Task 1).
- Produces:
  - `modules/ports/Cargo.toml`:
    - name `navori-mod-ports`, `[lints] workspace = true`.
    - deps (all `.workspace = true`): `anyhow`, `futures`, `navori-kernel`, `navori-macos`, `navori-procs`, `schemars`, `serde`, `serde_json`, `thiserror`, `tokio`, `tracing`.
    - dev-deps: `navori-macos` with `features = ["fake"]`, `navori-store`, `tempfile`, `tokio` with `features = ["test-util"]`.
  - `pub use` from `lib.rs`: `Ports`, `PortsEnv`, `OpenProcesses`, `PortsAction`, `Scanner`, `Scan`, `BackendPort`, and every type in `state.rs`.
  - `Ports::new(env: PortsEnv)`. `PortsEnv { home: PathBuf, processes: OpenProcesses }`, where Task 6 adds `containers`. `PortsEnv::mac(home: PathBuf)` canonicalizes `home` once.
  - `pub type OpenProcesses = Box<dyn FnMut() -> Result<Arc<dyn ProcessSource>, PlatformError> + Send>`.
  - `PortsAction::List`.
  - `Scanner::new(source: Arc<dyn ProcessSource>, home: PathBuf)`; `Scanner::scan(&self) -> Result<Scan, ProcsError>`, where Task 4 adds the memo.
  - Every `state.rs` type, complete, with `Serialize + Deserialize`. Values not yet computed: `Placement::default()` (all `None`), `harness: None`, `kind: PortKind::Other`, `container: None`, `docker: DockerStatus::NotSeen`.

**Depends-on:** Task 1

- [x] **Step 1: Write the failing tests** in `modules/ports/tests/scan.rs` with `Rig` over `FakeProcesses`:
  - `listing_returns_one_entry_per_process_and_port_with_both_addresses`: `127.0.0.1:3000` and `::1:3000` give one entry with 2 addresses.
  - `listing_before_any_scan_says_nothing_was_scanned_and_starts_no_scan`: `"list"` at t=0 gives `scanned_at_ms` null, and `list_calls()` is unchanged.
  - `the_first_scan_comes_five_seconds_after_start`: `Rig::advance(Duration::from_secs(5)).await`.
  - `listing_counts_a_process_whose_sockets_could_not_be_read`: `.fds_err(ReadError::Denied)` gives `skipped` 1.
  - `a_process_whose_executable_cannot_be_read_is_named_by_the_os_short_name`: `.exe_err(..)` with `.name("node")` gives name `node`.
  - `the_list_is_grouped_by_project_then_worktree_then_process_in_a_stable_order`.
  - `list_is_a_read_action_with_a_schema_and_description`: via `kernel.actions()`.
  - `the_list_states_when_its_scan_was_taken`.
- [x] **Step 2: Run** `cargo test -p navori-mod-ports` — expect: the crate does not build.
- [x] **Step 3: Implement** the crate.
  - Follow the design sections "modules/ports: scan and enrichment" (walk, no placement) and "the Module" (`List`).
  - One scan comes at start + 5 s, awaited inside the loop. Task 8 adds the cadence and the in-flight task.
  - The process name is the exe's last part, else `Snapshot::name`.
  - `Gate` and `canonical_temp_home` go in `tests/common/mod.rs`.
- [x] **Step 4: Register** the Module.
  - Rebase on `main`.
  - In `build_kernel`, register it with `PortsEnv::mac(PathBuf::from(&config.home_dir))`.
  - Add the workspace member and dependency.
  - Update the module, crate and `navori doctor` lines in the docs under Files.
- [x] **Step 5: Run** `cargo test -p navori-mod-ports && cargo xtask ci` — expect: pass, with `check-deps` accepting `navori-mod-ports`.
- [x] **Step 6: Commit** `git commit -m "feat(ports): list the user's listening ports through ports.list"`

_Requirements: PORTS-1.1, PORTS-1.3, PORTS-1.4, PORTS-1.5, PORTS-1.6, PORTS-1.7, PORTS-1.8, PORTS-1.10, PORTS-1.11, PORTS-7.5, PORTS-9.5, PORTS-9.7_

---

### Task 3: Place each port under its project and worktree

**Files:**
- Create: `modules/ports/src/place.rs`, `modules/ports/tests/place.rs`
- Modify: `crates/procs/src/location.rs`, `crates/procs/tests/location.rs`, `modules/ports/src/scan.rs`, `modules/ports/src/lib.rs`, `crates/macos/tests/process_source.rs`

**Reuse:**
- Scan: none — new code (rung 7). Nothing composes sockets, placement and kind today. It is built only from the navori-procs items in this design.
- navori-procs: rung 2 — `resolve_location` (location.rs:21-43), `Snapshot::read_verified` (snapshot.rs:159), `harness_of` (lookup.rs:28), `CATALOG` and `recognize` (harness.rs:59-120)

**Interfaces:**
- Consumes: `Snapshot::cwd`, `Process::exe` (existing); `Placement` (Task 2).
- Produces:
  - `pub fn find_repo(path: &Path) -> Option<Repo>`, with `pub struct Repo { pub root: PathBuf, pub location: Location }`.
  - `pub fn place(path: &Path, home: &Path) -> Placement`.
  - `pub fn place_listener(cwd: Field<PathBuf>, exe: Field<PathBuf>, home: &Path) -> Placement`.

**Depends-on:** Task 2

- [x] **Step 1: Write the failing `find_repo` tests** in `crates/procs/tests/location.rs`:
  - `a_directory_with_no_repository_above_it_has_no_repo`.
  - `a_linked_worktree_reports_its_checkout_as_the_root`.
  - Every existing location test stays as it is.
- [x] **Step 2: Write the failing table test** `each_listener_is_placed_by_the_first_rule_that_holds` in `modules/ports/tests/place.rs`.
  - Fixture: a canonical temp home with `git init` repos, and a linked worktree made inside its repo by `git worktree add <repo>/.worktrees/lane`. The `/opt`-like root is a separate canonical temp dir.
  - Rows:
    - cwd in a repo under home gives project, worktree and branch, with the text `resolve_location` gives.
    - cwd in no repo, exe in a repo, gives the exe's repo.
    - Neither gives no project.
    - A repo root outside home places nothing.
    - A repo root in `home/.dotdir` places nothing.
    - After `rm -rf <repo>/.worktrees/lane`, a cwd inside it gives the project, no worktree, no branch, and `missing_path` set.
    - An unread exe makes no exe join.
- [x] **Step 3: Write** `#[ignore]` `a_removed_working_directory_is_still_read_as_its_old_path` in `crates/macos/tests/process_source.rs`.
  - Use `ChildTree::spawn`'s canonical temp dir. Start `sleep 5` there and remove the dir.
  - Expect `MacProcesses::cwd` to return the canonical removed path.
- [x] **Step 4: Run** `cargo test -p navori-procs --test location && cargo test -p navori-mod-ports --test place` — expect: `find_repo` and `place` not found.
- [x] **Step 5: Implement** the placement.
  - Move the walk out of `resolve_location` into `find_repo`. `resolve_location` becomes `find_repo(cwd).map(|r| r.location)` with the old `dir_name(cwd)` fallback.
  - Write `place.rs` per the design and call it from `scan.rs`.
  - Logs name outcomes only, never paths.
- [x] **Step 6: Run** `cargo test -p navori-procs && cargo test -p navori-mod-ports && cargo test -p navori-mod-awake && cargo test -p navori-macos --test process_source -- --ignored` — expect: pass.
- [x] **Step 7: Commit** `git commit -m "feat(ports): place each port under its project and worktree"`

_Requirements: PORTS-2.1, PORTS-2.2, PORTS-2.3, PORTS-2.4, PORTS-2.5, PORTS-2.6, PORTS-2.7, PORTS-9.1_

---

### Task 4: Name the Harness that started a port

**Files:**
- Create: `crates/procs/tests/markers.rs`, `modules/ports/tests/harness.rs`
- Modify: `crates/macos/src/procargs.rs`, `crates/macos/src/process.rs`, `crates/macos/src/fake/processes.rs`, `crates/macos/tests/process_source.rs`, `crates/procs/src/snapshot.rs`, `crates/procs/src/harness.rs`, `modules/awake/tests/command.rs`, `modules/ports/src/scan.rs`, `modules/ports/src/actor.rs`, `modules/ports/src/lib.rs`, `docs/architecture/integrations.md`, `docs/security/threat-model.md`

**Reuse:**
- navori-macos: rung 2 — extends `ProcessSource`, `MacProcesses`, the `sysctl`/`read_error` helpers and the procargs scratch (process.rs:34-55,285-312; procargs.rs:30-127); rung 5 — `libc::proc_pidinfo`, `proc_pidfdinfo`, `proc_name`, `PROC_PIDLISTFDS`, `PROX_FDTYPE_SOCKET` (libc 0.2.191 apple/mod.rs:3990-5502). libc lacks `PROC_PIDFDSOCKETINFO` (3), `SOCKINFO_TCP` (2) and `TSI_S_LISTEN` (1); sockets.rs defines them from sys/proc_info.h:415,530,777.
- navori-procs: rung 2 — `resolve_location` (location.rs:21-43), `Snapshot::read_verified` (snapshot.rs:159), `harness_of` (lookup.rs:28), `CATALOG` and `recognize` (harness.rs:59-120)

**Interfaces:**
- Consumes: `Scanner::scan` (Task 2).
- Produces:
  - `ProcessSource::env_marker(&self, pid: i32, names: &[&str]) -> Result<Marker, ReadError>` and `pub enum Marker { None, One(usize), Several }`.
  - `fn with_procargs<R>(mib: &mut [i32], probed: usize, f: impl FnOnce(&[u8]) -> Result<R, ReadError>) -> Result<R, ReadError>`, crate-private in `procargs.rs`.
  - `FakeProcess::env(self, entries: &[&str]) -> Self`, which takes whole `NAME=value` entries, and read kind `"env"`.
  - `Snapshot::env_marker(&self, id: ProcId, names: &[&str]) -> Field<Marker>`.
  - `pub const MARKERS: [(&str, Harness); 3]`.
  - `pub fn started_by(snapshot: &Snapshot, id: ProcId) -> Field<Option<Harness>>`.
  - `pub type HarnessMemo = HashMap<ProcId, Option<Harness>>`.
  - `Scanner::scan(&self, memo: HarnessMemo) -> (Result<Scan, ProcsError>, HarnessMemo)`, replacing Task 2's signature.

**Depends-on:** Task 3

- [x] **Step 1: Write the failing `procargs` unit tests**, beside `parse_procargs`, on hand-built blocks:
  - `GROK_AGENT=1` as an argument, not an environment entry, gives `Marker::None`.
  - Two marker names give `Several`.
  - A block truncated mid-environment is `Malformed`.
  - The scratch holds no environment byte afterwards.
  - `argv` of the same block still returns the arguments only.
  - `the_argument_scratch_does_not_keep_a_previous_environment_byte` stays unchanged.
- [x] **Step 2: Write the failing tests at the other seams.**
  - Two `#[ignore]` tests in `crates/macos/tests/process_source.rs`. Both spawn their child with `.env_clear()` first, since a cargo run started by a Harness passes its own marker to children.
    - `an_environment_marker_is_found_in_a_real_process`: `.env_clear().env("GROK_AGENT", "1")`, args `["5"]`. Expect `MacProcesses::env_marker(pid, &["CLAUDE_CODE_CHILD_SESSION","CODEX_THREAD_ID","GROK_AGENT"])` to be `One(2)`.
    - `a_marker_given_only_as_an_argument_is_not_found`: `Command::new("/bin/sh").env_clear().args(["-c", "sleep 5; :", "sh", "GROK_AGENT=1"])`. The `; :` keeps `sh` from replacing itself with `sleep`, so `GROK_AGENT=1` stays in `sh`'s arguments. Read the `sh` pid and expect `Marker::None`.
  - In `crates/procs/tests/markers.rs`:
    - A live Claude ancestor wins over a marker.
    - With no ancestor, each marker gives its Harness.
    - Two markers give none.
    - A pid reused mid-read gives `Exited`.
  - In `modules/ports/tests/harness.rs`:
    - A listener whose `.env(..)` holds the fixture token entry and `CLAUDE_CODE_CHILD_SESSION=1` shows Claude Code, and the token text and name are absent from state, errors and captured logs.
    - `Gate` denies one env read; the next scan reads again.
    - A completed read is not repeated (`reads()` has one `"env"` per `ProcId`).
    - Backend ports have no Harness.
- [x] **Step 3: Run** `cargo test -p navori-macos && cargo test -p navori-procs --test markers` — expect: compile errors for `env_marker`, `Marker`.
- [x] **Step 4: Implement** the marker read.
  - `with_procargs` runs under the scratch lock and `ClearOnDrop`, then the matcher, then the trait method on all three implementers.
  - Add `started_by` and the memo, moved into and back out of the blocking scan.
  - Update `docs/architecture/integrations.md:28` and the THR-1 and THR-3 mitigation lines.
- [x] **Step 5: Run** `cargo test -p navori-macos && cargo test -p navori-procs && cargo test -p navori-mod-ports && cargo test -p navori-mod-awake && cargo test -p navori-macos --test process_source -- --ignored` — expect: pass.
- [x] **Step 6: Commit** `git commit -m "feat(ports): name the Harness that started a listener"`

_Requirements: PORTS-3.1, PORTS-3.2, PORTS-3.3, PORTS-3.4, PORTS-3.5, PORTS-3.6, PORTS-3.7, PORTS-3.8, PORTS-9.2, PORTS-10.3_

---

### Task 5: Say what kind of thing holds a port

**Files:**
- Create: `modules/ports/src/kind.rs`, `modules/ports/tests/kind.rs`
- Modify: `modules/ports/src/scan.rs`, `modules/ports/src/lib.rs`

**Reuse:** none — new code (rung 7). Nothing composes sockets, placement and kind today. It is built only from the navori-procs items in this design.

**Interfaces:**
- Consumes: `Placement` (Tasks 2 and 3), `Process::exe`.
- Produces: `pub fn kind(exe: Option<&Path>, placed: bool) -> PortKind`.

**Depends-on:** Task 4

- [x] **Step 1: Write the failing table test** `each_port_gets_the_first_kind_that_applies`. Rows, as `(exe, placed) → kind`:

  | Exe | Placed | Kind |
  |---|---|---|
  | `/Applications/Docker.app/Contents/MacOS/com.docker.backend` | either | `docker` |
  | `/opt/homebrew/bin/postgres` | either | `database` |
  | `/System/Library/CoreServices/ControlCenter.app/Contents/MacOS/ControlCenter` | no | `system` |
  | `/usr/libexec/rapportd` | no | `system` |
  | `/usr/sbin/sshd` | no | `system` |
  | `/usr/libexec/rapportd` | yes | `dev_server` |
  | `/usr/bin/ruby` | no | `other` |
  | `/usr/local/bin/caddy` | no | `other` |
  | `/System/Volumes/Data/Users/u/bin/y` | no | `other` |
  | unread | yes | `dev_server` |
  | unread | no | `other` |

  Add `system_ports_are_listed_like_any_other`.
- [x] **Step 2: Run** `cargo test -p navori-mod-ports --test kind` — expect: `kind` not found.
- [x] **Step 3: Implement** `kind.rs` per PORTS-5.1 and PORTS-5.2, and call it from `scan.rs`.
- [x] **Step 4: Run** `cargo test -p navori-mod-ports` — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(ports): label each port's kind"`

_Requirements: PORTS-5.1, PORTS-5.2, PORTS-5.3, PORTS-5.4_

---

### Task 6: Place Docker ports under their containers

**Files:**
- Create: `modules/ports/src/docker.rs`, `modules/ports/src/infra/mod.rs`, `modules/ports/src/infra/docker.rs`, `modules/ports/tests/docker.rs`, `modules/ports/tests/docker_client.rs`
- Modify: `modules/ports/src/lib.rs`, `modules/ports/src/actor.rs`, `modules/ports/src/scan.rs`, `modules/ports/tests/common/mod.rs`, `docs/architecture/integrations.md`, `docs/security/threat-model.md`

**Reuse:** rung 5 — `tokio::net::UnixStream`, `tokio::time::timeout`, `serde_json` (all already in the workspace); rung 7 for the HTTP/1.1 framing, because no HTTP crate is installed and adopting one is the operator's call.

**Interfaces:**
- Consumes: `BackendPort` (Task 2), `place` (Task 3), `PortKind::Docker` (Task 5).
- Produces:
  - `PortsEnv.containers: Arc<dyn Containers>`.
  - `pub trait Containers: Send + Sync { fn running(&self) -> BoxFuture<'_, Result<Vec<Container>, DockerError>>; }`.
  - `Container { id, name, working_dir: Option<PathBuf>, ports: Vec<(IpAddr, u16)> }`.
  - `DockerSocket::new(path: PathBuf)`.
  - `DockerError { Absent, Refused, Timeout, TooLarge, BadStatus(u16), BadReply }`.
  - `pub fn attribute(backend: &[BackendPort], containers: &[Container], place: impl Fn(&Path) -> Placement) -> Vec<PortEntry>`.
  - `FakeContainers` (with `calls()`) in `tests/common/mod.rs`.

**Depends-on:** Task 5

- [x] **Step 1: Write the failing `attribute` table test** in `modules/ports/tests/docker.rs`:
  - A container on `127.0.0.1:5432` with a live worktree `working_dir` gives that worktree.
  - A removed `working_dir` gives the project, no worktree and `missing_path`.
  - Two containers publishing 5432 on `127.0.0.1` and `127.0.0.2` give two entries.
  - A publish on `0.0.0.0` matches a backend bound to `0.0.0.0`.
  - An unmatched port keeps `docker` with no project.
  - One container publishing `0.0.0.0:5432` and `:::5432`, with the backend bound to both, gives one entry with two addresses.
  - The same container with the backend bound only to `:::5432` gives one entry.
- [x] **Step 2: Write the failing Rig tests.**
  - No backend listener: `FakeContainers::calls()` stays 0.
  - With a backend, one call per scan.
  - `Timeout` keeps `docker` with no project, and `docker` is `Unreadable`, with no `problem`.
  - `each_scan_publishes_once`: `Kernel::revision()` moves by 1 per scan, with the Docker result in that one publish.
  - `no_working_directory_or_container_path_reaches_a_log_or_event`: captured tracing output and `recent_events` hold no fixture path.
- [x] **Step 3: Write the client tests** in `modules/ports/tests/docker_client.rs`, against a `tokio::net::UnixListener` in a short `/tmp` temp dir. Replies to serve:
  - a `Content-Length` reply;
  - a chunked reply with trailers;
  - a reply closed at EOF;
  - a header block over 16 KiB, which gives `BadReply`;
  - a body over 1 MiB, which gives `TooLarge`;
  - a silent server, which gives `Timeout` after 2 s with `start_paused`;
  - a missing socket, which gives `Absent`.

  Also assert:
  - The request line is exactly `GET /containers/json HTTP/1.1`.
  - A reply whose JSON holds `"Command"` and `"Mounts"` yields only the 4 kept fields.
- [x] **Step 4: Run** `cargo test -p navori-mod-ports --test docker --test docker_client` — expect: compile errors.
- [x] **Step 5: Implement** per the design section "modules/ports: Docker read".
  - `PortsEnv::mac` builds `DockerSocket::new(home.join(".docker/run/docker.sock"))`.
  - Zero the body buffer before dropping it.
  - Update `integrations.md:34` and add the threat-model line.
- [x] **Step 6: Run** `cargo test -p navori-mod-ports` — expect: pass.
- [x] **Step 7: Commit** `git commit -m "feat(ports): attribute Docker ports to their containers"`

_Requirements: PORTS-4.1, PORTS-4.2, PORTS-4.3, PORTS-4.4, PORTS-4.5, PORTS-4.6, PORTS-4.7, PORTS-4.8, PORTS-4.9, PORTS-10.4_

---

### Task 7: Get a URL for a port

**Files:**
- Create: `modules/ports/tests/open.rs`
- Modify: `modules/ports/src/action.rs`, `modules/ports/src/actor.rs`

**Reuse:** rung 2 — the `Module` trait, `ModuleCtx`, `WaitHandle` and `TrayContribution` (crates/kernel/src/module.rs:54-84,143-165; scheduler.rs:112), with the Awake actor's `select!` loop shape (modules/awake/src/actor.rs:89-127) and the Integrations crate layout as the template.

**Interfaces:**
- Consumes: `PortsState` (Task 2).
- Produces:
  - `PortsAction::Open { port: u16, #[serde(default)] address: Option<IpAddr> }`.
  - Reply `{"url": "<string>"}`.
  - `ActionError::Invalid` naming the port, or listing the addresses.

**Depends-on:** Task 6

- [x] **Step 1: Write the failing table test** `open_returns_the_url_for_a_listed_port`:

  | Bound address | URL |
  |---|---|
  | `127.0.0.1:5173` | `http://localhost:5173` |
  | `0.0.0.0:7000` | `http://localhost:7000` |
  | `::1:3000` | `http://localhost:3000` |
  | `192.168.1.5:8080` | `http://192.168.1.5:8080` |
  | `2001:db8::5:8080` | `http://[2001:db8::5]:8080` |

  Add these tests:
  - `opening_an_unlisted_port_names_the_port`.
  - `a_port_bound_to_two_addresses_with_different_urls_asks_for_one`.
  - `open_is_a_read_action`.
  - `open_connects_to_nothing`:
    - Bind a real `std::net::TcpListener` on `127.0.0.1:0` and list its port in the fake.
    - Dispatch `open`.
    - Then `set_nonblocking(true)`. `accept()` returns `WouldBlock`.
- [x] **Step 2: Run** `cargo test -p navori-mod-ports --test open` — expect: unknown variant `open`.
- [x] **Step 3: Implement** `Open` in `action.rs` (risk, catalog) and its reply in the actor.
- [x] **Step 4: Run** `cargo test -p navori-mod-ports` — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(ports): answer ports.open with the port's URL"`

_Requirements: PORTS-6.1, PORTS-6.2, PORTS-6.3, PORTS-6.4, PORTS-6.5_

---

### Task 8: Rescan on the panel's cadence

**Files:**
- Create: `modules/ports/tests/timing.rs`
- Modify: `modules/ports/src/actor.rs`, `modules/ports/tests/common/mod.rs`

**Reuse:** rung 2 — the `Module` trait, `ModuleCtx`, `WaitHandle` and `TrayContribution` (crates/kernel/src/module.rs:54-84,143-165; scheduler.rs:112), with the Awake actor's `select!` loop shape (modules/awake/src/actor.rs:89-127) and the Integrations crate layout as the template.

**Interfaces:**
- Consumes: `Gate` (Task 2); `kernel.scheduler().set_panel_visible(bool)`; `restarts` and `RestartPolicy`.
- Produces: no new public names. The scan becomes an in-flight task in the `select!`, and `deadline` is `None` while it runs.

**Depends-on:** Task 7

- [x] **Step 1: Write the failing tests**, `#[tokio::test(start_paused = true)]` with `Rig::advance(..).await`:
  - `with_the_panel_closed_scans_come_every_thirty_seconds`. Today there is no second scan, so it fails.
  - `with_the_panel_open_scans_come_every_three_seconds`.
  - `opening_the_panel_brings_the_next_scan_within_three_seconds_but_not_at_once`: `list_calls()` at +0 s and at +3 s.
  - `a_scan_that_comes_due_during_a_slow_scan_is_skipped`: `Gate` blocks one `listeners` call.
  - `list_answers_while_a_scan_is_running`: same `Gate`.
  - `after_a_crash_the_module_says_nothing_was_scanned_and_scans_again`: `Gate` panics once.
- [x] **Step 2: Run** `cargo test -p navori-mod-ports --test timing` — expect: the cadence and in-flight tests fail.
- [x] **Step 3: Implement** the deadline rules of the design's Decision 10. `run` publishes `PortsState::default()` first.
- [x] **Step 4: Run** `cargo test -p navori-mod-ports` — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(ports): rescan every 3 s with the panel open and 30 s closed"`

_Requirements: PORTS-7.1, PORTS-7.2, PORTS-7.3, PORTS-7.4, PORTS-7.6, PORTS-7.7, PORTS-10.6_

---

### Task 9: Say when the list cannot be trusted

**Files:**
- Create: `modules/ports/tests/failure.rs`
- Modify: `modules/ports/src/lib.rs`, `modules/ports/src/actor.rs`, `crates/ffi/src/app.rs`, `crates/ffi/tests/core.rs`, `crates/ffi/tests/integrations.rs`, `docs/agents/verify.md`, `README.md`

**Reuse:** rung 2 — `build_kernel` (crates/ffi/src/app.rs:246), `awake_view` / `awake_check` (app.rs:313-370), the `IntegrationsEnv` home pattern (ffi/src/lib.rs:260)

**Interfaces:**
- Consumes: `PortsState.problem` (Task 2); `FakeContainers` (Task 6).
- Produces:
  - `fn module_is_down(kernel: &Kernel, id: &str) -> bool`, generalized from `awake_is_down` (app.rs:330), with its call site at app.rs:322 replaced.
  - `fn ports_check(down: bool, state: Option<&PortsState>) -> Check`.
  - `Ports::tray`.
  - Doctor row `ports` after `logs`.

**Depends-on:** Task 8

- [x] **Step 1: Write the failing tests.**
  - In `modules/ports/tests/failure.rs`:
    - `a_failed_scan_keeps_the_last_good_list_and_says_why`: `fail_list(true)` before the second scan.
    - `a_failed_scan_asks_for_the_problem_glyph`.
    - `a_docker_failure_does_not_ask_for_the_problem_glyph`.
    - `port_counts_never_turn_on_attention`.
  - In `crates/ffi/tests/core.rs`:
    - The exact-list test (:770-786) expects `["config","database","ipc","modules","awake","integrations","logs","ports"]`.
    - Add `the_ports_row_passes_before_the_first_scan`.
    - Table-test `ports_check` for down, unreadable, `problem`, not scanned, and `N ports`.
  - In `crates/ffi/tests/integrations.rs`, the exact-list test (:83-93) gains `ports` after `logs`.
- [x] **Step 2: Run** `cargo test -p navori-mod-ports --test failure && cargo test -p navori-ffi` — expect: failures on the row list and the glyph.
- [x] **Step 3: Implement** the failure reporting.
  - Rebase on `main`.
  - Add `problem` handling and `tray` in the Module.
  - Add `module_is_down` and `ports_check`, and append the row after `logs`.
  - Update `docs/agents/verify.md` (:46-49, :96-101, :160-165) and `README.md:27`.
- [x] **Step 4: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(ports): report scan failures in ports.list, doctor and the status icon"`

_Requirements: PORTS-8.1, PORTS-8.2, PORTS-8.3, PORTS-8.4, PORTS-8.5, PORTS-8.6, PORTS-9.4, PORTS-9.6_

---

### Task 10: Measure the scan in the perf gate

**Files:**
- Create: `modules/ports/tests/real_os.rs`
- Modify: `xtask/src/perf/mod.rs`, `xtask/src/perf/judge.rs`, `xtask/src/perf/home.rs`, `docs/standards/performance.md`

**Reuse:** rung 2 — the `snapshot_ms` path (xtask/src/perf/mod.rs:35,55,608-630; judge.rs:98-103), copied for a second ignored real-OS test

**Interfaces:**
- Consumes: `Scanner` with its memo (Tasks 2 and 4).
- Produces:
  - Ignored test `port_scan_cost_for_the_perf_gate`, printing `median <N> µs; listeners <n>`.
  - `fn median_ms(output: &str) -> Result<f64>`, replacing `snapshot_median_ms` (mod.rs:623), with both metrics using it.
  - Metric `port_scan_ms`: gated, budget 50.0, floor 5.0.
  - Each run's home copy (home.rs `fresh_run`) links `.docker/run/docker.sock` to `~/.docker/run/docker.sock` when that exists. The template is left without the link.

**Depends-on:** Task 9

- [x] **Step 1: Write the failing tests.**
  - In `judge.rs`:
    - `within_budget` gains `port_scan_ms`.
    - The table test gains `("port_scan_ms", 5.0, Some(50.0), "≤ 50 ms")`.
    - A new assertion that `performance.md` contains `` `port_scan_ms` `` in its floors line, since `≤ 50 ms` already appears for `menu_reopen_ms`.
  - In `mod.rs`, `median_ms` parses `"median 4308 µs; listeners 20"` to 4.308.
  - In `home.rs`, `a_run_copy_links_the_docker_socket_when_it_exists` and `a_run_copy_has_no_docker_link_when_there_is_none`. Both use a fake home root in a temp dir.
- [x] **Step 2: Run** `cargo test -p xtask` — expect: the missing metric, `median_ms` and link fail.
- [x] **Step 3: Implement** the metric.
  - The ignored test runs 5 `Scanner::scan` calls with no Docker read and prints the median.
  - Add `measure_port_scan_ms`, the METRICS row and the link.
  - Update the `performance.md` lines :21, :30 and :34.
- [x] **Step 4: Run** `cargo test -p xtask && cargo test -p navori-mod-ports --test real_os -- --ignored --nocapture && cargo test -p navori-procs --test real_os` — expect: pass, with the median printed and PROCS-8.8 still holding.
- [x] **Step 5: Commit** `git commit -m "feat(perf): gate the port scan at 50 ms"`

_Requirements: PORTS-9.3, PORTS-9.8, PORTS-10.2_

---

### Task 11: Acceptance on this Mac

**Files:**
- Modify: none, unless an acceptance failure points at an earlier task's files.

**Reuse:** none — acceptance only; no design section builds here.

**Interfaces:**
- Consumes:
  - The dev build from `cargo xtask run`, driven by `"build/dev/Navori Dev.app/Contents/Helpers/navori" --profile dev`.
  - A temporary worktree, `git worktree add /tmp/ports-before <commit before Task 2>`.
- Produces: an acceptance note in the execute report.

**Depends-on:** Task 10

- [ ] **Step 1: Run** `cargo xtask ci` — expect: pass.
- [ ] **Step 2: Run** `cargo xtask perf` in `/tmp/ports-before`, then in this worktree, with `com.docker.backend` listening.
  - Report both results, and the `docker` status that `ports.list` shows during the second run.
  - A red run before the Module is reported, not blamed on it.
  - Remove `/tmp/ports-before` with `git worktree remove`.
- [ ] **Step 3: Drive** `navori --profile dev dispatch ports '"list"'` 35 s after `cargo xtask run`.
  - Compare with `lsof -nP -iTCP -sTCP:LISTEN` for this UID: the same (pid, port) set.
  - Check each entry by rule against what `lsof`, the cwd and `git worktree list` say at that moment. The close package's signal 2 names are a guide to what to expect, not fixed values.
- [ ] **Step 4: Check by hand** a listener started by each Harness, then detached.
  - Start a listener from Claude Code, from Codex and from Grok Build, and detach each.
  - Each must show that Harness. Record which marker fired.
  - A failure reopens that marker. Do not edit the marker list to pass.
- [ ] **Step 5: Record** the results in the execute report. No commit unless a fix was needed.

_Requirements: PORTS-3.9, PORTS-10.1_
