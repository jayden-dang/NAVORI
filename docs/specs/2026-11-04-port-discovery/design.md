# Design: Port discovery

Feature code: PORTS
Status: Draft
Date: 2026-11-04
Requirements: ./requirements.md

<!--
Written in an unattended run under .skills/_pending-ports/auto/charter.md (`.skills/` is gitignored). The operator has
not reviewed this file. A value marked `agent-decided` was picked by an agent session. Scan digests used:
.skills/_pending-ports/auto/scan.md and scan-design.md (facts with file:line). Fresh retrieval (load-subgraph
fsubr-1.2, advisory, owns_coverage 9/14): PROCS shares crates/macos/src/{process,procargs}.rs and
crates/procs/src/location.rs; PERF and MENU share xtask/src/perf/mod.rs; AWAKE, ASK, HOOKS, ICON, KCONTRACT share
crates/ffi/src/app.rs. No feature owns modules/ports/.
-->

## Context

Navori already reads the user's process table without `lsof`. `navori-macos` wraps `sysctl` and libproc behind the
safe `ProcessSource` trait, with a fake (crates/macos/src/process.rs:34-55, fake/processes.rs). `navori-procs` turns
that into a `Snapshot` of the current UID's processes, walks ancestors to a Harness, and resolves a working directory
to project, worktree and branch (crates/procs/src/snapshot.rs:51, lookup.rs:28, location.rs:21). Awake is the only
Module that uses them, always inside `spawn_blocking` (modules/awake/src/infra/platform.rs:112). Nothing reads file
descriptors, sockets or a process's environment today, and nothing talks HTTP.

The binding constraint is GOAL-11 together with ARCH-16. Idle CPU must round to 0.0 %, which leaves about 15 ms of CPU
per 30 s for all idle work, and the Scheduler holds a closed-menu timer to 30 s
(crates/kernel/src/scheduler.rs:91,163). A throwaway spike on this Mac found the same 20 listening sockets as
`lsof` in 4.2 ms of CPU, and `Snapshot::take` costs about 4.3 ms in a debug build (xtask/src/perf/mod.rs:651). So
one scan reuses one `Snapshot`, and reads sockets for those processes only. Every expensive per-process read
(working directory, environment, short name) happens only for the few processes that hold a listener.

That rules out two alternatives. Running `lsof`, `ps` or `docker ps` costs a process spawn per scan and is excluded
by PORTS-1.1. A Docker client library (`bollard`, `hyper`) would be a new dependency; the workspace has none of them,
while tokio's `net` and `io-util` are already enabled (scan-design.md section 8). The Engine read is one GET with a
small chunked body, so this design writes that read on `tokio::net::UnixStream` and adopts no dependency.

## Decisions

1. One scan is one `Snapshot`, then socket reads for its processes, then enrichment for listeners only, then the
   Docker read, then one publish (PORTS-4.9). `agent-decided`.
2. Socket, environment and short-name reads join `ProcessSource` as required methods, so `MacProcesses`,
   `FakeProcesses` and the test wrapper `FirstRuid` (modules/awake/tests/command.rs:200) all implement them. A
   default body would hide a missing read behind a quiet `Denied`. `agent-decided`.
3. `socket_fdinfo` is laid out by byte offset from the macOS 27 SDK's `sys/proc_info.h:369-571`, as `kinfo_proc` is
   (process.rs:11-21). A read whose byte count is not the struct size is `Malformed`. A test in the test process
   binds loopback listeners and reads them back. The app never listens to check its layout (ARCH-11). `agent-decided`.
4. The environment is never copied out of the argument scratch. A new closure-taking read runs the matcher on the
   scratch under its lock and `ClearOnDrop`, and returns only which marker matched. It does not reuse `read_procargs`,
   which returns an unwiped copy and an empty buffer on truncation (procargs.rs:110-124). `agent-decided`.
5. Harness markers sit in `navori-procs` beside `CATALOG`, because naming a Harness is procs' job (L7). Placement,
   the home guard and `kind` sit in the Module, because they are Ports policy over procs' data. They are derived only
   from `Snapshot`, `Location` and the exe path (L7, review r1 of the close package). `agent-decided`.
6. `resolve_location` keeps its behaviour and becomes `find_repo` plus the old fallback. Awake keeps calling it
   (PORTS-9.1).
7. The Docker client is hand-written HTTP/1.1 over `tokio::net::UnixStream`: `GET /containers/json` with
   `Connection: close`, a status line and headers, then a `Content-Length` or chunked body. The Engine on this Mac
   answers chunked. Limits are 2 s and 1 MiB (`agent-decided`). The socket is `<home>/.docker/run/docker.sock` only,
   Docker Desktop's default on macOS (`agent-decided`). Neither `/var/run/docker.sock` nor `DOCKER_HOST` is read, so a
   test with a temporary home never reaches Docker.
8. Containers are deserialized into a struct with only id, names, ports and one label. The other fields exist only
   in the buffered body (≤ 1 MiB) until it is parsed. They are never stored, returned or logged, and the body buffer
   is zeroed before it is dropped (PORTS-4.4; `agent-decided`: the zeroing).
9. State is published once per scan. `ports.list` returns the published state, grouped as PORTS-1.7 says. Nothing is
   persisted, so there is no Owner and no migration (hard stop not reached).
10. Timing: the first scan comes 5 s after `run` starts (`agent-decided`, inside PORTS-7.5's 30 s). While a scan is
    in flight the deadline is `None`, so a due timer cannot fire again and nothing spins. When the scan finishes, Ok or
    Err, the next deadline is now + 3 s while the panel is visible and now + 30 s while it is not. On a `PanelChanged`
    to visible with no scan in flight, the deadline becomes at most now + 3 s. A scan runs as a spawned task, so the
    actor keeps answering.
11. A new perf metric `port_scan_ms` (budget 50 ms, floor 5 ms, `agent-decided`) follows `snapshot_ms`
    end to end (xtask/src/perf/mod.rs:35,55,608-630; judge.rs:98-103; performance.md:21,30,34).
12. Identity: a scan ends with one more `list()` and drops every listener process whose start time or real UID
    changed. This is the trailing check `Snapshot::take` already does (snapshot.rs:51-82), so one sysctl replaces a
    per-pid check. `agent-decided`.
13. The doctor row `ports` goes last, after `logs`, so the integrations test that pins `awake, integrations, logs` keeps
    its order (crates/ffi/tests/integrations.rs:65-77). `agent-decided`.

No ADR: nothing here is hard to reverse, and no invariant is contradicted. Spine invariants relied on: ARCH-1, ARCH-3,
ARCH-4, ARCH-6, ARCH-9, ARCH-10, ARCH-11, ARCH-13, ARCH-14, ARCH-16.

## Architecture

Vocabulary for every section below: **module**, **interface**, **implementation**,
**seam** (public surface where behavior is observable and substitutable). Prefer
deep modules — an interface much simpler than what it hides.

### navori-macos: sockets, environment markers, short name

Satisfies: PORTS-2.7, PORTS-3.4, PORTS-10.3
Reuse: rung 2 — extends `ProcessSource`, `MacProcesses`, the `sysctl`/`read_error` helpers and the procargs scratch
(process.rs:34-55,285-312; procargs.rs:30-127); rung 5 — `libc::proc_pidinfo`, `proc_pidfdinfo`, `proc_name`,
`PROC_PIDLISTFDS`, `PROX_FDTYPE_SOCKET` (libc 0.2.191 apple/mod.rs:3990-5502). libc lacks `PROC_PIDFDSOCKETINFO` (3),
`SOCKINFO_TCP` (2) and `TSI_S_LISTEN` (1); sockets.rs defines them from sys/proc_info.h:415,530,777.
Respects: ARCH-9
Security: THR-1, THR-3
Surface:
- `MacProcesses` (process.rs:74): replace — implements the three new methods.
- `FakeProcesses` (fake/processes.rs:196): replace — implements them from new builders; `reads()` records the kinds `fds`, `env`, `name`.
- `FirstRuid` test wrapper (modules/awake/tests/command.rs:200): replace — delegates the three methods.
- Every other user holds `Arc<dyn ProcessSource>` and calls none of the new methods: no edit, behaviour unchanged.
Interface:
```
fn listeners(&self, pid: i32) -> Result<Vec<Listener>, ReadError>;        // TCP sockets in LISTEN state
fn env_marker(&self, pid: i32, names: &[&str]) -> Result<Marker, ReadError>;
fn name(&self, pid: i32) -> Result<String, ReadError>;                  // the OS short name
pub struct Listener { pub addr: IpAddr, pub port: u16 }
pub enum Marker { None, One(usize), Several }
```
Depth: n/a — extends `ProcessSource`.
Locality: extend — new `crates/macos/src/sockets.rs`; procargs.rs gains the in-place matcher; process.rs gains three
trait methods and their `MacProcesses` bodies; the fake gains builders `.listen(addr, port)`, `.env(&[&str])`, `.name(&str)`.

`listeners` calls `proc_pidinfo(PROC_PIDLISTFDS)` with a buffer sized by a first size probe, retrying once if it grew.
For each fd of type `PROX_FDTYPE_SOCKET` it calls `proc_pidfdinfo(PROC_PIDFDSOCKETINFO)` into a 792-byte buffer and
keeps `soi_kind == SOCKINFO_TCP` with `tcpsi_state == TSI_S_LISTEN`. The address comes from `insi_vflag` (v4 or v6)
and the port from `insi_lport` in network order. ESRCH on any call is `Gone` and EPERM is `Denied` (read_error). A
socket fd closed between the list and the info call (EBADF) is skipped, not an error.

`env_marker` uses a new `with_procargs(mib, probed, |block: &[u8]| -> R)`, which locks the scratch, holds it under
`ClearOnDrop`, reads, and runs the closure on the block in place. A length past the probe, a missing slice, or a block
whose walk runs off the end is `Malformed`, never an empty block, so a failed read is never a remembered "no marker".
The closure skips argc, the exec path and the argc arguments with the same walk `parse_procargs` uses. It then
compares each remaining entry's text before `=` with the names, and returns `None`, `One(index)` or `Several`, never a
string. `parse_procargs` and `read_argv` are untouched (PORTS-9.2).

`name` uses `proc_name` into a fixed 2·MAXCOMLEN buffer.

### navori-procs: repo lookup, verified reads, Harness markers

Satisfies: PORTS-1.2, PORTS-1.9, PORTS-3.1, PORTS-3.2, PORTS-3.3, PORTS-3.7, PORTS-3.9, PORTS-9.1, PORTS-9.2, PORTS-9.3
Reuse: rung 2 — `resolve_location` (location.rs:21-43), `Snapshot::read_verified` (snapshot.rs:159), `harness_of`
(lookup.rs:28), `CATALOG` and `recognize` (harness.rs:59-120)
Respects: ARCH-9
Security: THR-1, THR-5
Surface:
- `resolve_location` callers — procs/src/harness.rs:137, modules/awake/src/infra/platform.rs:268 and :356, crates/procs/tests/location.rs: replace — the body is split into `find_repo` plus the unchanged fallback, so every caller receives the same `Location` as before. The existing location tests are the proof.
Interface:
```
pub fn find_repo(path: &Path) -> Option<Repo>;            // None when no .git is found walking up
pub struct Repo { pub root: PathBuf, pub location: Location }
impl Snapshot {
    pub fn listeners(&self, id: ProcId) -> Field<Vec<Listener>>;   // unverified per pid; see still_same
    pub fn still_same(&self) -> Result<HashSet<ProcId>, ProcsError>; // one trailing list()
    pub fn env_marker(&self, id: ProcId, names: &[&str]) -> Field<Marker>;
    pub fn name(&self, id: ProcId) -> Field<String>;
}
pub fn started_by(snapshot: &Snapshot, id: ProcId) -> Field<Option<Harness>>;  // ancestor, then marker
```
Depth: n/a — extends `location`, `Snapshot` and `harness`.
Locality: extend — location.rs, snapshot.rs, harness.rs. Awake: leave.

`listeners` reads sockets only for a pid the snapshot listed under the current UID (PORTS-1.2). After every socket
read of the scan, `still_same` lists processes once more. Any process whose start time or real UID changed, or that is
gone, is dropped with its result. Its error is not counted in `skipped` either, so a pid reused by another user
contributes nothing (PORTS-1.9). This is the trailing check `Snapshot::take` does, one sysctl for the whole scan
instead of about 1,700 per-pid `KERN_PROC_PID` calls. `env_marker` and `name` go through `read_verified` as it is
(PORTS-3.7).

`started_by` returns the nearest recognized live ancestor, including the process itself, as `harness_of` does. If
there is none, it reads the marker with `MARKERS = [("CLAUDE_CODE_CHILD_SESSION", ClaudeCode), ("CODEX_THREAD_ID",
Codex), ("GROK_AGENT", Grok)]`. `Several` gives `None`. A read error is returned as an error, so the caller can avoid
remembering it (PORTS-3.6). PORTS-3.9 is the manual check before ship and is recorded here because the markers live
here.

### modules/ports: scan and enrichment

Satisfies: PORTS-1.1, PORTS-1.4, PORTS-1.5, PORTS-1.6, PORTS-1.10, PORTS-2.1, PORTS-2.2, PORTS-2.3, PORTS-2.4,
PORTS-2.5, PORTS-2.6, PORTS-3.5, PORTS-3.6, PORTS-3.8, PORTS-5.1, PORTS-5.2, PORTS-5.3, PORTS-5.4, PORTS-10.4,
PORTS-10.5
Reuse: none — new code (rung 7). Nothing composes sockets, placement and kind today. It is built only from the procs
items above.
Respects: ARCH-3, ARCH-6
Security: THR-3, THR-5
Interface:
```
pub struct Scanner { source: Arc<dyn ProcessSource>, home: PathBuf }     // home canonicalized once
impl Scanner { pub fn scan(&self, memo: HarnessMemo) -> (Result<Scan, ProcsError>, HarnessMemo) }  // blocking; memo moved in and back
pub struct Scan { pub entries: Vec<PortEntry>, pub skipped: u32, pub backend: Vec<BackendPort> }
pub struct PortEntry { port: u16, addresses: Vec<IpAddr>, pid: i32, start_us: u64, name: String, exe: Option<PathBuf>,
    place: Placement, harness: Option<HarnessName>, kind: PortKind, container: Option<ContainerRef> }
pub struct Placement { project: Option<String>, worktree: Option<String>, branch: Option<BranchName>, missing_path: Option<PathBuf> }
pub struct BackendPort { pid: i32, start_us: u64, port: u16, addresses: Vec<IpAddr> }
enum PortKind { DevServer, Database, Docker, System, Other }     // serde snake_case
enum HarnessName { ClaudeCode, Codex, Grok }                    // serde; mapped from procs' Harness, which has no serde
enum BranchName { Named(String), Detached(String) }             // serde; mapped from procs' Branch
```
Depth: if it vanished, a caller would still need to know only "take a scan, get entries with project, harness and
kind". The walk order, the home guard, the path-gone rule, the marker memo and the kind precedence would all be lost.
Locality: new — modules/ports/src/scan.rs (walk), place.rs (placement and guard), kind.rs (kind). No existing module
is touched.

`scan` takes one `Snapshot` and calls `listeners` for each process. `Exited` drops the process. Any other error is
held until `still_same` has run: it adds one to `skipped` only if the process kept its identity (PORTS-1.10). For each
listener process the scan then reads `cwd` and takes the exe from the snapshot. It reads `name` only when the exe is
unread, since the name otherwise comes from the exe path's last part (PORTS-1.6).

Entries are per (pid, port), with a sorted set of addresses (PORTS-1.4). Backend ports (exe name
`com.docker.backend`) are not finished here. They are handed to the Docker stage as `BackendPort`.

Placement (place.rs), in order:
1. `find_repo(cwd)`, kept only if its `root` is strictly under `home` and not inside a dot-directory directly below
   `home` (PORTS-2.4). If cwd no longer exists, walk up to the first existing ancestor, take only the project, and
   put the removed path in `missing_path` (PORTS-2.5).
2. Otherwise `find_repo(exe.parent())` with the same guard (PORTS-2.2). An unread exe gives no join (PORTS-2.6).
3. Otherwise no project, no worktree and no branch (PORTS-2.3).

The project text is `Location.project`, the same text Awake's labels use (PORTS-2.1).

Harness: `HarnessMemo` is a `HashMap<ProcId, Option<Harness>>` owned by the actor. It is moved into the blocking scan
and returned with the result. It is filled only by a completed `started_by`, and pruned to the current listeners after
each scan (PORTS-3.5, PORTS-3.6). A scan that panics loses the memo, which only costs re-reads. Backend entries get none
(PORTS-3.8).

Kind (kind.rs) is a pure function of the exe and the placement, in PORTS-5.1's order, with PORTS-5.2's prefixes.
An unread exe matches no exe rule (PORTS-5.3). `system` entries stay in the list (PORTS-5.4).

No working directory, argv or `working_dir` text reaches a log, Event or panic: tracing lines name the outcome only,
as `location.rs` does (PORTS-10.4). Only processes the `Snapshot` listed are read, and `Snapshot::take` keeps the
current UID alone (snapshot.rs:51-63). A result from a pid that changed during the scan is dropped by `still_same`
(PORTS-10.5).

### modules/ports: Docker read

Satisfies: PORTS-4.1, PORTS-4.2, PORTS-4.3, PORTS-4.4, PORTS-4.5, PORTS-4.6, PORTS-4.7, PORTS-4.8, PORTS-4.9
Reuse: rung 5 — `tokio::net::UnixStream`, `tokio::time::timeout`, `serde_json` (all already in the workspace);
rung 7 for the HTTP/1.1 framing, because no HTTP crate is installed and adopting one is the operator's call.
Respects: ARCH-7, ARCH-11
Security: THR-3, THR-10
Interface:
```
pub trait Containers: Send + Sync { fn running(&self) -> BoxFuture<'_, Result<Vec<Container>, DockerError>>; }
pub struct DockerSocket { path: PathBuf }                 // impl Containers; DockerSocket::new(home.join(".docker/run/docker.sock"))
pub enum DockerError { Absent, Refused, Timeout, TooLarge, BadStatus(u16), BadReply }   // no text from the reply
pub struct Container { pub id: String, pub name: String, pub working_dir: Option<PathBuf>, pub ports: Vec<(IpAddr, u16)> }
pub fn attribute(backend: &[BackendPort], containers: &[Container], place: impl Fn(&Path) -> Placement) -> Vec<PortEntry>
```
Depth: if it vanished, a caller would still need to know "list running containers, or get a reason why not". The
socket choice, request, framing, chunked decoding, limits and field filtering would all be lost.
Locality: new — modules/ports/src/infra/docker.rs (client) and src/docker.rs (pure attribution).

The read is called only when the scan found a backend port (PORTS-4.1, PORTS-4.2). It sends exactly
`GET /containers/json HTTP/1.1`, `Host: docker` and `Connection: close` (PORTS-4.3). The whole exchange is under one
2 s timeout, connect included, and the body under a 1 MiB cap (PORTS-4.9).

Framing: the header block is capped at 16 KiB. With `Content-Length`, read exactly that many bytes. With
`Transfer-Encoding: chunked`, each size line is capped at 32 bytes, chunks are appended until the 0-size chunk, and
trailers are read and dropped. With neither, read to EOF. Every path counts against the 1 MiB cap, and passing it is
`TooLarge`.

The body is deserialized into a private struct whose `Labels` holds only `com.docker.compose.project.working_dir`
(PORTS-4.4). The name is the first entry of `Names` without its leading `/`.

`attribute` keys a Docker entry by (pid, port, container). It matches a published `(IP, PublicPort)` to a backend
bound address, treating `0.0.0.0` and `::` as matching either wildcard (PORTS-4.5). It places a container through the
same `place` with `working_dir` as the path (PORTS-4.6). An unmatched backend port, or a container with no usable
`working_dir`, keeps `docker` with no project (PORTS-4.7).

Any `DockerError` gives every backend port `docker` with no project, and the state's `docker` field holds the
variant's fixed text (PORTS-4.8). `DockerError` carries no text from the reply, so a serde message that quotes a
`working_dir` cannot reach state, logs or Events (PORTS-10.4). The read is never logged with its body.

### modules/ports: the Module, its Actions and its timing

Satisfies: PORTS-1.3, PORTS-1.7, PORTS-1.8, PORTS-1.11, PORTS-6.1, PORTS-6.2, PORTS-6.3, PORTS-6.4, PORTS-6.5,
PORTS-7.1, PORTS-7.2, PORTS-7.3, PORTS-7.4, PORTS-7.5, PORTS-7.6, PORTS-7.7, PORTS-8.1, PORTS-8.2, PORTS-8.6,
PORTS-9.6, PORTS-10.6
Reuse: rung 2 — the `Module` trait, `ModuleCtx`, `WaitHandle` and `TrayContribution`
(crates/kernel/src/module.rs:54-84,143-165; scheduler.rs:112), with the Awake actor's `select!` loop shape
(modules/awake/src/actor.rs:89-127) and the Integrations crate layout as the template.
Respects: ARCH-3, ARCH-4, ARCH-10, ARCH-13, ARCH-16
Interface:
```
crate navori-mod-ports:  pub struct Ports;  impl Module for Ports { const ID = "ports"; type Config = NoConfig; .. }
Ports::new(env: PortsEnv)
PortsEnv { home: PathBuf, processes: OpenProcesses, containers: Arc<dyn Containers> }
pub type OpenProcesses = Box<dyn FnMut() -> Result<Arc<dyn ProcessSource>, PlatformError> + Send>  // defined here; ARCH-3 forbids Awake's
PortsEnv::mac(home) -> PortsEnv      // MacProcesses opened lazily, DockerSocket under home
enum PortsAction { List, Open { port: u16, #[serde(default)] address: Option<IpAddr> } }   // both Risk::Read
State (Serialize + Deserialize): PortsState { scanned_at_ms: Option<u64>, groups: Vec<ProjectGroup>, skipped: u32,
                    docker: DockerStatus, problem: Option<String> }
ProjectGroup { project: Option<String>, worktrees: Vec<WorktreeGroup> }   WorktreeGroup { worktree: Option<String>, processes: Vec<ProcessGroup> }
ProcessGroup { pid, start_us, name, exe, harness, entries: Vec<PortEntry> }
enum DockerStatus { NotSeen, Read, Unreadable(String) }   // the String is DockerError's fixed text
```
Depth: n/a for the trait (rung 2). The crate's own deletion test: a caller would still need to know two Actions and
one state shape. The cadence and the in-flight rule would be lost.
Locality: new — modules/ports/src/{lib.rs, action.rs, actor.rs, state.rs}. The kernel is left as it is.

`run` first publishes `PortsState::default()`, which means not scanned yet. The kernel sends `initial_state` only at
launch (crates/kernel/src/runtime.rs:207), so this publish is what clears a crashed run's data from `ports.list`. The
doctor row also checks the module's lifecycle, since a Failed module's last state stays readable (PORTS-8.1,
PORTS-10.6). The loop is `select! { biased; done = in_flight; req = ctx.recv();
wake = wait.wait_until(deadline) }`.

- `Due`: spawn `spawn_blocking(scan)` followed by the Docker read, as one task, and set `deadline = None`. No `Due` can
  come while a scan is in flight, so a due time is never queued behind one (PORTS-7.7).
- `PanelChanged`: if now visible and nothing is in flight, `deadline = min(deadline, now + 3 s)` (PORTS-7.4). If a
  scan is in flight, its `done` sets the deadline.
- `done`: on `Ok`, group the entries and publish them with `scanned_at_ms` once, clearing `problem` (PORTS-1.7,
  PORTS-1.8). On `Err`, keep the groups and `scanned_at_ms` and set `problem` to the outcome text (PORTS-8.2). In both
  cases the next deadline is now + 3 s if visible, else now + 30 s (PORTS-7.1, PORTS-7.2, PORTS-7.3).
- `List` replies with the published state. It never scans (PORTS-1.3, PORTS-7.6).
- `Open` finds the port in the published state and builds its URL (PORTS-6.1, PORTS-6.3, PORTS-6.4). It opens
  nothing and connects to nothing (PORTS-6.2).

The first deadline is start + 5 s (PORTS-7.5). Both Actions are in `catalog()` with `Risk::Read`, a schema and a
description (PORTS-1.11, PORTS-6.5).

`tray` asks for `TrayGlyph::Problem` only while `problem` is `Some`. A Docker failure lives in `docker`, not in
`problem`, and the Module never sets `attention` (PORTS-8.6, PORTS-9.6).

Ordering is total: projects by case-insensitive name, then exact name, with the no-project group last; worktrees
the same way, with no worktree first; processes by pid; entries by port, then container name, then first address.

### crates/ffi: registration and the doctor row

Satisfies: PORTS-8.3, PORTS-8.4, PORTS-8.5, PORTS-9.4, PORTS-9.5, PORTS-9.7
Reuse: rung 2 — `build_kernel` (crates/ffi/src/app.rs:246), `awake_view` / `awake_check` (app.rs:313-370), the
`IntegrationsEnv` home pattern (ffi/src/lib.rs:260)
Respects: ARCH-2, ARCH-14
Surface:
- `build_kernel` caller ffi/src/lib.rs:267: replace — passes `PortsEnv::mac(PathBuf::from(&config.home_dir))`.
- Doctor row lists in crates/ffi/tests/core.rs:770-786 and crates/ffi/tests/integrations.rs:83-93 (exact lists): replace — `ports` after `logs`.
- `doctor_has_an_integrations_row_between_awake_and_logs` (crates/ffi/tests/integrations.rs:65-77): no edit — `ports` goes after `logs`, so its neighbours hold.
- crates/ffi tests that start a core with a temporary home: no edit. The Ports Module scans the real process table read-only from 5 s after start, and never reaches Docker, because the socket lives under the temporary home.
- `doctor_reports_a_healthy_core` (core.rs:427) needs every row ok on a real core: no edit. The new row passes while nothing has been scanned yet (PORTS-8.4).
- Module lists in core.rs:92,345,857 check ids `system` and `awake` only: no edit.
- CLI `navori status` and `doctor` print rows and modules generically (apps/cli/src/main.rs:487-512): no edit. A new row and module appear.
- Swift `CoreStore` counts running modules generically (CoreStore.swift:79): no edit. The count rises by one. The release menu has no Ports row and none is added (PORTS-9.7).
- The Core submenu's `ModulesSection` (apps/macos/Navori/Features/Panel/CoreSubmenuView.swift:23-35) lists modules generically: no edit. It shows a `ports` module row. That is a module status row, not the Ports menu row of PORTS-9.7.
- SettingsView.swift:132 names the doctor rows in a placeholder sentence: compat — the sentence misses `ports` until ROAD-18, which owns Swift in MILE-4, edits it.
- docs/agents/verify.md:46-49, :96-101 and :160-165, README.md:27, docs/codebase/{modules,map,dependencies}.md name rows, modules or crates: replace — add `ports` / `navori-mod-ports`.
Interface: `fn ports_check(down: bool, state: Option<&PortsState>) -> Check` · `build_kernel(.., ports: PortsEnv)`
Depth: n/a — extends `build_kernel` and `doctor`.
Locality: extend — crates/ffi/src/app.rs, crates/ffi/Cargo.toml, Cargo.toml workspace members and dependencies.

The row fails when the module is Failed or Stopped, as `awake_is_down` checks (app.rs:330), when the state is
unreadable (PORTS-8.5), or when `problem` is set (PORTS-8.3). Otherwise it passes with
`not scanned yet` or `N ports` (PORTS-8.4). The crate is named `navori-mod-ports` under `modules/`, so check-deps
treats it as a Module (xtask/src/main.rs:690-727; PORTS-9.5).

### xtask perf: `port_scan_ms`

Satisfies: PORTS-9.8, PORTS-10.1, PORTS-10.2
Reuse: rung 2 — the `snapshot_ms` path (xtask/src/perf/mod.rs:35,55,608-630; judge.rs:98-103), copied for a second
ignored real-OS test
Surface:
- judge.rs `METRICS` and its fixtures (`within_budget` :255-267, names list :291-305, table test :495-545): replace — a `port_scan_ms` row and fixture values.
- docs/standards/performance.md :21, :30, :34: replace — a budget row, the method line and the floor.
- `perf-baseline.json`: absent today; when recorded later it gains the metric, so no reader breaks.
- `prepare_home` and the run in xtask/src/perf/mod.rs:77,301-306: replace — the app runs with `NAVORI_PERF_HOME` as its home (AppEnvironment.swift:15-27), so the Docker socket under that home is absent. When `~/.docker/run/docker.sock` exists, `prepare_home` links `<perf home>/.docker/run/docker.sock` to it, inside the temporary perf home, and the run reports whether the `ports` state's `docker` was `Read` (`agent-decided`).
Interface: `cargo test -p navori-mod-ports --test real_os -- --ignored --exact port_scan_cost_for_the_perf_gate` prints `median <N> µs`
Depth: n/a — extends the perf gate.
Locality: extend — xtask/src/perf/{mod.rs (metric and home link), judge.rs}, docs/standards/performance.md; new modules/ports/tests/real_os.rs.

The test runs `Scanner::scan` 5 times with no Docker read and prints the median (PORTS-10.2). PORTS-10.1 is the gate
run before and after, with `com.docker.backend` listening and the socket linked into the perf home, both reported, with
the `docker` status read from `navori dispatch ports '"list"'` against the perf app. Under the perf home no repo passes
the guard, so the gate measures both walks per listener rather than the placed path; their cost is comparable. Existing metrics keep their code paths and
budgets (PORTS-9.8).

### Docs touched (no behaviour)

Infrastructure, no Satisfies line.
- docs/architecture/integrations.md:28: add the environment marker read and the short name. :34: Docker read through
  the Engine API's `GET /containers/json`.
- docs/architecture/system.md:65: Ports module status.
- docs/security/threat-model.md: THR-1/THR-3 mitigation gains the marker read (names only, wiped, never copied); a
  line for the Docker socket read as a local read-only client.

## Seams for testing

| Seam | Kind | Covers |
|---|---|---|
| `ProcessSource` on `MacProcesses`, real OS, in-process loopback listeners and a removed cwd (crates/macos/tests) | integration (ignored where it needs the real OS) | PORTS-2.7, PORTS-3.4, PORTS-10.3 |
| `FakeProcesses` + `procargs` unit tests (env layout, argv `GROK_AGENT=1`, two markers, wipe) | unit | PORTS-3.3, PORTS-3.4, PORTS-10.3 |
| navori-procs public API: `find_repo`, `Snapshot::{listeners, env_marker, name}`, `started_by` over `FakeProcesses`; existing location, snapshot and no_leaks tests | unit | PORTS-1.2, PORTS-1.9, PORTS-3.1, PORTS-3.2, PORTS-3.3, PORTS-3.7, PORTS-9.1, PORTS-9.2, PORTS-10.3, PORTS-10.5 |
| `Scanner::scan` over `FakeProcesses` and a temp home with repos, linked worktrees, a removed dir, `/opt`-style roots | unit | PORTS-1.1, PORTS-1.4, PORTS-1.5, PORTS-1.6, PORTS-1.10, PORTS-2.1–2.6, PORTS-3.5, PORTS-3.6, PORTS-3.8, PORTS-5.1–5.4, PORTS-10.4 |
| `attribute` (pure) and the Docker client against a test `UnixListener` serving canned chunked, sized, slow and oversized replies | unit + integration | PORTS-4.1–4.9 |
| The Module through `Kernel::dispatch` with a `Rig` copied from modules/awake/tests/common (FakeClock, paused tokio, fake source, fake containers) | integration | PORTS-1.3, PORTS-1.7, PORTS-1.8, PORTS-1.11, PORTS-6.1–6.5, PORTS-7.1–7.7, PORTS-8.1, PORTS-8.2, PORTS-8.6, PORTS-9.6, PORTS-10.6 |
| `doctor()` and `ports_check` in crates/ffi/tests, `cargo xtask check-deps`, existing MENU tests | integration | PORTS-8.3, PORTS-8.4, PORTS-8.5, PORTS-9.4, PORTS-9.5, PORTS-9.7 |
| Real-OS `ports` tests and `cargo xtask perf` (judge unit tests plus a run before and after) | integration (ignored) + manual | PORTS-9.3, PORTS-9.8, PORTS-10.1, PORTS-10.2 |
| Manual on this Mac: a listener started by Claude Code, Codex and Grok Build, then detached | manual | PORTS-3.9 |

New seams: `Containers` (one), plus three methods on the existing `ProcessSource`.

## Coverage check

Every ID in requirements.md appears in exactly one `Satisfies:` line:
- 1.1–1.11: procs 1.2, 1.9; scan 1.1, 1.4, 1.5, 1.6, 1.10; Module 1.3, 1.7, 1.8, 1.11.
- 2.1–2.7: scan 2.1–2.6; navori-macos 2.7.
- 3.1–3.9: procs 3.1, 3.2, 3.3, 3.7, 3.9; navori-macos 3.4; scan 3.5, 3.6, 3.8.
- 4.1–4.9: Docker read.
- 5.1–5.4: scan.
- 6.1–6.5: Module.
- 7.1–7.7: Module.
- 8.1, 8.2, 8.6: Module; 8.3–8.5: ffi.
- 9.1–9.3: procs; 9.4, 9.5, 9.7: ffi; 9.6: Module; 9.8: perf.
- 10.1, 10.2: perf; 10.3: navori-macos; 10.4, 10.5: scan; 10.6: Module.

None is unmapped.

Requirements changed by this design (upstream sync-back, re-surfaced for approval):
- PORTS-1.2 and PORTS-10.5: "read only of processes whose real UID is the current user's" cannot be guaranteed per
  read, because a pid can be reused between listing and reading. Reworded to what the design does: read only pids
  listed under the current UID, and drop any result whose process changed identity during the scan.
- Preface: "no default test touches Docker or the real process table" is false for crates/ffi tests, which start a
  real core. Reworded: they scan the real process table read-only and never reach Docker. One existing gap was seen and is outside
this feature. `read_procargs` returns an unwiped copy of the scratch that still holds the environment bytes
(procargs.rs:110, scan-design.md section 1). PROCS-1.8 holds, because the environment is never returned. The copy
itself is not wiped. Reported for the operator, not fixed here.
