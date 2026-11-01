# Design: Platform layer and process model

Feature code: PROCS
Status: Approved
Date: 2026-11-01
Requirements: ./requirements.md

## Context

Navori has no process-related code today: the only platform call is `std::process::id()`
(`crates/ffi/src/lib.rs:184`), `libc 0.2.191` sits unused in `[workspace.dependencies]`, and the
workspace forbids `unsafe` everywhere (`Cargo.toml:54-57`) with one opt-out, `navori-ffi`
(`crates/ffi/Cargo.toml:44-53`). The kernel runs Modules as supervised actors on a 2-worker tokio
runtime (`crates/ffi/src/lib.rs:171-176`); a Module's `run` loop is a `tokio::select!` over its
inbox and its own timers (`crates/kernel/src/system.rs:89-110`), and the composition root
`app::build_kernel` registers Modules with concrete values (`crates/ffi/src/app.rs:163-166`).
Nothing reads `.git`, shells out to `git`, or uses kqueue. Digests: `.skills/PROCS/scan.md`,
`.skills/PROCS/scan-design.md`; measurements: `.skills/PROCS/spike.md`; hook semantics:
`.skills/research/2026-11-01-harness-hook-blocking.md`; independent review:
`.skills/PROCS/design-review.md`.

The binding constraints are **ARCH-9** (`unsafe` only in the platform FFI crate) and the idle
budget — 0% CPU and ≤ 1 wakeup/s (`docs/standards/performance.md`), with no timer faster than 30 s
and periodic work only through the Scheduler (**ARCH-16**). Every OS call needs `unsafe`, and
nothing may poll. So the platform crate is a thin, leaf `navori-macos` over `libc` and
`objc2-io-kit`, exposing safe traits; all logic (UID filtering, trees, identity checks, Harness
recognition, git resolution) lives in `navori-procs`, which is `unsafe`-free and testable against
fakes of those traits. Process exits come from kqueue `NOTE_EXIT` on a thread that blocks in
`kevent` — a blocked thread, not a timer; measured at ~0.2 ms kill-to-event with no added package
wakeups while idle.

This rules out the seed's other shapes: a wrapper crate (`libproc`) that lacks `cwd` and `argv` on
macOS and needs bindgen; `sysinfo`, whose refresh reads argument buffers it does not need; any
periodic rescan inside ROAD-6; and walking the process tree from the CLI. It also rules out
`proc_listallpids` + `PROC_PIDTBSDINFO` as the listing call: the spike found it costs ~2.6 ms and
fails with `EPERM` for set-uid processes such as `/usr/bin/login`, which sits between Ghostty and
the user's shell (`Ghostty → login → -zsh`, verified with `ps` on this Mac; `login`'s real UID is
the user's). One `sysctl(KERN_PROC_ALL)` returns pid, parent pid, real UID, start time and state
for every process (~1,250 here) in ~0.11 ms, set-uid processes included.

## Decisions

1. **Two crates, one direction.** `navori-macos` (leaf: no internal dependency; `libc`,
   `objc2-io-kit` with `default-features = false, features = ["std", "pwr_mgt"]`,
   `objc2-core-foundation` for `CFString`, `thiserror`, `tracing`) and `navori-procs` (depends on
   `navori-macos`, `thiserror`, `tracing`, `tokio` with `sync`). `objc2-io-kit` and
   `objc2-core-foundation` are the new dependencies the user chose with approach A.
2. **Listing via `sysctl(KERN_PROC_ALL)`.** `libc` has no `kinfo_proc` for Apple, so
   `navori-macos` reads the five fields it needs at fixed offsets of each 648-byte record
   (`p_starttime` 0, `p_stat` 36, `p_pid` 40, `p_ruid` 392, `e_ppid` 560 — measured with
   `offsetof` on this Mac). The read probes the size, allocates the probe + 12.5 %, retries up to 3
   times on `ENOMEM`, and returns `PlatformError` for a length that is not a whole number of
   records — it never indexes past the buffer. `MacProcesses::new` checks once that pid 1 parses to
   `ppid == 0 && ruid == 0` and fails loudly otherwise, so an OS layout change cannot pass silently.
   Per-pid reads stay on `libc`: `proc_pidpath` (exe), `KERN_PROCARGS2` (argv),
   `PROC_PIDVNODEPATHINFO` (cwd), `proc_pid_rusage` (CPU time, mach ticks converted with
   `mach_timebase_info`).
3. **Identity is `(pid, start time)`** (spike Q5: stable across reads, unique over 200 rapid
   spawns). Every per-pid read of a known identity goes through one verified-read helper in
   `navori-procs`: check the live entry (same start time, same real UID) → read → check again;
   any mismatch or `Gone` reports `Unavailable::Exited`. Nothing is read for a pid whose live
   entry shows another real UID.
4. **The UID filter is on the real UID and lives in `navori-procs`**, so it is testable against a
   fake that contains other-UID processes.
5. **Exit watching = one kqueue per watcher, one blocking thread per watcher**, started when the
   watcher is created and stopped (via an `EVFILT_USER` event registered before the thread
   spawns) and joined when it is dropped. Each watch carries a generation token in `kevent.udata`;
   events are filtered by pid *and* token, so an in-flight event can never be attributed to a later
   identity on the same pid. Registration and removal use `EV_RECEIPT` with exactly one receipt
   slot, so they never dequeue another pid's event; `ESRCH` on registration — gone or zombie
   (spike Q4) — becomes one immediate exit event. Rejected: `tokio::io::unix::AsyncFd` on the
   kqueue fd (no extra thread, but pulls tokio into the platform crate and nests kqueues under mio
   — unmeasured).
6. **Snapshot is synchronous and point-in-time.** Snapshot plus session resolution costs a few
   milliseconds (spike Q1); callers in async code choose `spawn_blocking` or not. Caching and
   staleness are the consumer's (ROAD-8) concern — the Shell renders cached Core state, so no
   snapshot runs in the menu-open path by construction. The two answers that must be live —
   "not running" from `harness_of` and `activity` — re-check the live entry.
7. **Fakes ship in `navori-macos` behind a `fake` cargo feature** (`navori_macos::fake`:
   `FakeProcesses`, `FakeExits`, `FakePower`), used as a dev-dependency by `navori-procs` and later
   by ROAD-8. They are the only test doubles and fake only the system boundary the standards allow
   (`docs/standards/testing.md:16`); `resolve_location` and the Harness catalog are always real.
8. **Power assertions return a guard.** `hold` returns an `AssertionGuard` holding its release in
   an `Option`; explicit `release()` takes it and reports errors, `Drop` takes it if still present
   and logs a failure (PROCS-6.4) — never a double release. `objc2-io-kit` does not export the
   assertion *type* strings, so they are built with `CFString::from_static_str`.
9. **Who may name `navori-macos`:** `navori-procs`, `navori-ffi` (composition root) and crates
   under `modules/` (for `PowerAssertions` and the watcher traits); no other crate, and never
   `navori-cli`. `check-deps` enforces this, plus: `navori-macos` has no internal dependency; only
   `navori-ffi` and `navori-macos` may omit `[lints] workspace = true`; and `navori-macos`'s lint
   tables equal the workspace tables except `unsafe_code`, plus `undocumented_unsafe_blocks`
   (drift check — `navori-ffi`'s hand copy has already lost `dbg_macro` and `todo`).
10. **No new test dependency.** The property test for the argument-buffer parser uses a small
    deterministic xorshift generator inside the test file instead of adopting `proptest`.
11. **No ADR.** Every decision above is internal to two new crates and reversible behind their
    traits; the one cross-cutting change (the hook ↔ CLI boundary) is recorded by updating the
    Awake seed in `docs/ARCHITECTURE.md`.

Invariants this feature relies on: **ARCH-6** (everything persisted or sent off the machine passes
the Sensitive Data Shield — these crates go further and never put argv, environment or paths into
errors or logs at all), **ARCH-9**, **ARCH-10** (a restarted Module can build a fresh watcher),
**ARCH-16**. None is contradicted.

Retrieval (fresh `load-subgraph`, schema 1.1): neighbors of PROCS = **CORE** via `path`
(`crates/` is an ancestor of `crates/macos/` and `crates/procs/`); no term overlap; `owns_coverage`
0/6 (no feature has a `tasks.md` OWNS block yet); `observations: []`. No reuse target follows from
it — CORE owns no process, git or platform code (scan-design §7).

## Architecture

Vocabulary for every section below: **module**, **interface**, **implementation**,
**seam** (public surface where behavior is observable and substitutable). Prefer
deep modules — an interface much simpler than what it hides.

```
                         navori-procs (no unsafe)                         navori-macos (leaf, unsafe)
caller ─► Snapshot::take ─► uid filter, tree, verified reads ────────────► ProcessSource  ─► sysctl / proc_pid*
       ─► sessions / harness_of / activity ─► Harness table + resolve_location (std::fs)
       ─► ProcWatcher ─► tokens, identity re-check, tokio mpsc ──────────► ExitSource ─► kqueue thread
       ─► (ROAD-8) ─────────────────────────────────────────────────────► PowerAssertions ─► IOKit
```

Shared types (`navori-macos`): `StartTime` (microseconds since the epoch; `Copy + Eq + Ord + Hash`);
`PlatformError { Sysctl { errno }, Kqueue { op, errno }, Layout { len } }`;
`WatchError { Gone, Kqueue { errno } }`.

### navori-macos — process source

Satisfies: PROCS-1.8, PROCS-8.7
Reuse: rung 5 — `libc` 0.2.191 (already in the workspace) for `sysctl`, `proc_pidpath`, `proc_pidinfo`, `proc_pid_rusage`, `mach_timebase_info`; new code (rung 7) only for the `kinfo_proc` field reads and the `KERN_PROCARGS2` parser, which `libc` does not provide
Respects: ARCH-9
Interface:
```rust
pub struct ProcEntry { pub pid: i32, pub ppid: i32, pub ruid: u32, pub start: StartTime, pub zombie: bool }
pub enum ReadError { Gone, Denied, Malformed }             // Display: operation + pid only
pub trait ProcessSource: Send + Sync {
    fn current_uid(&self) -> u32;
    fn list(&self) -> Result<Vec<ProcEntry>, PlatformError>;    // every process, one sysctl
    fn entry(&self, pid: i32) -> Result<ProcEntry, ReadError>;   // KERN_PROC_PID, same record reader
    fn exe(&self, pid: i32) -> Result<PathBuf, ReadError>;
    fn argv(&self, pid: i32) -> Result<Vec<String>, ReadError>;  // argc strings only, env cut off
    fn cwd(&self, pid: i32) -> Result<PathBuf, ReadError>;
    fn cpu_time(&self, pid: i32) -> Result<Duration, ReadError>;
}
pub struct MacProcesses;                                    // the real implementation
pub fn parse_procargs(buf: &[u8]) -> Result<Vec<String>, ReadError>;  // safe, pure; tested directly
```
Depth: if this module vanished, callers would still need to know only "list processes with
parent, real UID and start time, and read exe/argv/cwd/CPU per pid, each read able to fail as
gone/denied/malformed" — the record offsets, retries, buffer probing, errno mapping and tick
conversion stay hidden.
Locality: new `crates/macos/src/process.rs` (+ `procargs.rs`); neighbors — leave.

errno mapping: `ESRCH` → `Gone`; `EPERM` → `Denied`; `EINVAL` (what `KERN_PROCARGS2` returns for
both a vanished and a foreign pid, spike Q2) → `Gone` only if `entry(pid)` also fails, else
`Denied`. `parse_procargs` reads `argc`, skips the exec path and its NUL padding, collects exactly
`argc` NUL-terminated strings and stops: whatever follows (the environment, present only for the
caller's own process — spike Q3) is never decoded (PROCS-1.8). Strings are decoded lossily
(`String::from_utf8_lossy`), so a non-UTF-8 argument cannot hide a Harness; `Malformed` is
reserved for structural damage — truncation, a missing NUL, an `argc` the buffer cannot hold —
and every index is bounds-checked (PROCS-8.7). `argv` probes the buffer size first and reuses one
buffer (largest seen: 9 KB).

### Exit source and watcher (navori-macos + navori-procs)

Satisfies: PROCS-4.1, PROCS-4.2, PROCS-4.3, PROCS-4.4, PROCS-4.5, PROCS-4.6, PROCS-8.1, PROCS-8.2, PROCS-8.3
Reuse: rung 4 — kqueue `EVFILT_PROC`/`NOTE_EXIT`, `EVFILT_USER`, `EV_RECEIPT` (platform feature) through `libc`; new code (rung 7) for the thread and the procs-side watcher
Respects: ARCH-9, ARCH-10, ARCH-16
Interface (`navori-macos`):
```rust
pub type ExitSink = Box<dyn Fn(i32, u64) + Send + Sync>;   // (pid, token) of the watch that fired
pub trait ExitSource: Send {
    fn watch(&mut self, pid: i32, token: u64) -> Result<(), WatchError>;   // Gone on ESRCH
    fn unwatch(&mut self, pid: i32);                                       // tolerates ENOENT
}
pub struct KqueueExits;  impl KqueueExits { pub fn start(sink: ExitSink) -> Result<Self, PlatformError> }
```
Interface (`navori-procs`):
```rust
pub struct ProcWatcher;
impl ProcWatcher {
    pub fn new(source: Arc<dyn ProcessSource>,
               exits: impl FnOnce(ExitSink) -> Result<Box<dyn ExitSource>, PlatformError>) -> Result<Self, ProcsError>;
    pub fn watch(&mut self, id: ProcId) -> Result<(), ProcsError>;   // Err only for WatchError::Kqueue; the entry is removed
    pub fn unwatch(&mut self, id: ProcId);
    pub async fn next(&mut self) -> Option<Exited>;    // Exited { id: ProcId }; cancel-safe
}
```
Depth: callers know only "watch an identity, get one `Exited` for it, unwatch to stop"; the
kqueue fd, receipts, tokens, the wake-up event, thread lifecycle, the start-time re-check and the
late-event filter stay hidden.
Locality: new `crates/macos/src/exits.rs` and `crates/procs/src/watch.rs`; neighbors — leave (ROAD-8 will add a `select!` arm, as `system.rs:89-110` does for its timer).

`ProcWatcher` keeps `watched: HashMap<pid, (ProcId, token)>`, a token counter, a `VecDeque` of
synthetic exits and the receiving end of a `tokio::sync::mpsc::unbounded_channel` whose sender
the sink owns. `watch(id)`: take the next token, insert, call `ExitSource::watch(pid, token)`
(`EV_ADD | EV_ONESHOT | EV_RECEIPT`, `NOTE_EXIT`, `udata = token`). On `Gone` → remove the entry and
queue `Exited { id }` (PROCS-4.2). On success → re-read `entry(pid)`; a different start time or
`Gone` means the process was replaced or exited around registration, so remove the entry,
`unwatch`, and queue `Exited { id }` — the kernel event that may still arrive is dropped by the
token filter, so exactly one event is delivered (PROCS-4.1, 4.3). After registration kqueue
tracks that exact process, so later reuse cannot slip in. `unwatch(id)` removes the entry and
calls `ExitSource::unwatch` (`EV_DELETE | EV_RECEIPT`, `ENOENT` ignored since a fired oneshot
auto-deletes). `next()` first pops the synthetic queue, then awaits the channel and delivers
`(pid, token)` only if `watched[pid].token == token`, removing it (PROCS-4.4); it holds no state
across the await, so it is cancel-safe in `select!`. The sink ignores `SendError` when the
receiver is gone.

`KqueueExits::start` creates the kqueue, registers the `EVFILT_USER` wake event, then spawns the
thread, so no thread exists until a watcher does (PROCS-8.1) and a drop right after `start`
cannot lose the wake. The thread blocks in `kevent` with no timeout and calls the sink with
`(ident, udata)`. Dropping triggers the wake event, joins the thread (a panicked join is ignored,
not unwrapped) and closes the fd (PROCS-4.5). A new watcher is fully independent of a dropped
one (PROCS-4.6), which is what a restarted Module needs (ARCH-10). Measured: kill → event
0.19–0.2 ms; an idle blocked thread adds no package wakeups over 10 s (spike Q4).

### navori-macos — power assertions

Satisfies: PROCS-6.1, PROCS-6.2, PROCS-6.3, PROCS-6.4, PROCS-6.5
Reuse: rung 5 — `objc2-io-kit` (`pwr_mgt`: `IOPMAssertionCreateWithName`, `IOPMAssertionRelease`, `kIOPMAssertionLevelOn`) and `objc2-core-foundation` (`CFString`), adopted with approach A
Respects: ARCH-9
Interface:
```rust
pub enum AssertionKind { System, Display }                 // PreventUserIdleSystemSleep / PreventUserIdleDisplaySleep
pub trait PowerAssertions: Send + Sync {
    fn hold(&self, kind: AssertionKind, reason: &str) -> Result<AssertionGuard, PowerError>;
}
pub struct AssertionGuard;   // pub fn new(release: impl FnOnce() -> Result<(), PowerError> + Send + 'static) -> Self
impl AssertionGuard { pub fn release(self) -> Result<(), PowerError> }   // Drop releases if not yet released
pub struct MacPower;
pub enum PowerError { Refused { kind: AssertionKind, code: i32 }, ReleaseFailed { code: i32 } }
impl PowerError { #[doc(hidden)] pub fn from_create(kind: AssertionKind, code: i32) -> Option<PowerError> }
```
Depth: callers know "hold a kind with a reason, keep the guard, release or drop it"; CFString
construction, the IOKit return codes and the assertion id stay hidden.
Locality: new `crates/macos/src/power.rs`; neighbors — leave.

`hold` builds the type and reason `CFString`s, calls `IOPMAssertionCreateWithName` with
`kIOPMAssertionLevelOn`, and maps the `IOReturn` through `PowerError::from_create` — a non-zero
code returns `Refused` with the code before any guard exists (PROCS-6.5). `Drop` on an unreleased
guard runs the same release and logs a failure with `tracing::warn!(code, ..)`. `IOPMAssertionRelease`
is a short Mach call, acceptable on a tokio worker. If the app dies, the OS drops the assertion
itself (spike Q6: gone within 1 s after `kill -9`).

### navori-procs — snapshot

Satisfies: PROCS-1.1, PROCS-1.2, PROCS-1.3, PROCS-1.4, PROCS-1.5, PROCS-1.6, PROCS-1.7, PROCS-8.4, PROCS-8.6
Reuse: none — new code (rung 7); no process model exists in the repo
Security: THR-5
Interface:
```rust
pub struct ProcId { pub pid: i32, pub start: StartTime }
pub enum Unavailable { Exited, PermissionDenied, Malformed }
pub type Field<T> = Result<T, Unavailable>;
pub struct Process { pub id: ProcId, pub parent: Option<i32>, pub exe: Field<PathBuf> }
pub struct Snapshot;
impl Snapshot {
    pub fn take(source: Arc<dyn ProcessSource>) -> Result<Snapshot, ProcsError>;
    pub fn processes(&self) -> impl Iterator<Item = &Process>;
    pub fn get(&self, pid: i32) -> Option<&Process>;
    pub fn ancestors(&self, id: ProcId) -> Vec<&Process>;     // nearest first
    pub fn descendants(&self, id: ProcId) -> Vec<&Process>;   // nearest first (breadth-first)
    pub fn argv(&self, id: ProcId) -> Field<Vec<String>>;     // verified read
    pub fn cwd(&self, id: ProcId) -> Field<PathBuf>;          // verified read
}
```
Depth: callers know "a point-in-time set of my processes with parent links, plus lazy argv/cwd
that may be unavailable"; the UID filter, zombie and vanished-pid handling, the verified-read
helper, the parent index and error mapping stay hidden.
Locality: new `crates/procs/src/snapshot.rs`; neighbors — leave.

`take` calls `list()`, keeps entries with `ruid == current_uid()` that are not zombies
(PROCS-1.1, 1.2, 1.6), reads `exe` for each kept pid, then calls `list()` a second time
(~0.11 ms) and drops every kept pid whose start time changed or that disappeared — so no exe is
attributed to a replaced process (PROCS-1.3, 1.6); any other exe failure stays as `Unavailable`
(PROCS-1.7). `argv`/`cwd` go through the verified-read helper (Decision 3): live entry same start
and same real UID → read only that pid → live entry same start (PROCS-1.3, 1.5, 8.6). `ancestors`
follows `parent` while the parent is in the snapshot, so it stops at launchd or at a process of
another real UID. Measured on this Mac (~1,250 processes, ~890 owned): list 0.11 ms, exe for all
owned ~2.4 ms, argv+cwd for 45 candidates 1.6 ms (debug build).

### navori-procs — project resolver

Satisfies: PROCS-2.3, PROCS-2.4, PROCS-2.5, PROCS-2.6, PROCS-2.7, PROCS-2.8
Reuse: rung 3 — `std::fs` / `std::path` only; no `git` subprocess, no git library
Interface:
```rust
pub struct Location { pub project: String, pub worktree: Option<String>, pub branch: Option<Branch> }
pub enum Branch { Named(String), Detached { abbrev: String } }   // abbrev = first 7 hex chars of HEAD
pub fn resolve_location(cwd: &Path) -> Location;
```
Depth: callers know "a directory → project, optional worktree, optional branch"; `.git` file vs
directory, `gitdir:` pointers, `commondir`, `worktrees/` vs `modules/`, and `HEAD` parsing stay
hidden.
Locality: new `crates/procs/src/location.rs`; neighbors — leave.

Walk up from `cwd` to the first ancestor containing `.git`. A `.git` **directory** → main
checkout: project = that directory's name, no worktree, branch from `.git/HEAD` (PROCS-2.3). A
`.git` **file** → read `gitdir: <path>`, resolving a relative path against the directory holding
the `.git` file. If the gitdir's parent directory is named `worktrees` → linked worktree: the common
dir is `<gitdir>/commondir` (content such as `../..`, resolved against the gitdir); project = the
parent directory name of a common dir ending in `/.git`, or the last component of a common dir under
`/.git/modules/` (a worktree of a submodule); worktree = name of the directory holding the `.git`
file; branch from `<gitdir>/HEAD` (PROCS-2.4). Otherwise, if the gitdir path has a `modules/`
component (e.g. `gitdir: ../.git/modules/lib`) → submodule: project = name of the directory holding
the `.git` file, no worktree (PROCS-2.5). `HEAD` = `ref: refs/heads/<b>` → named
branch; a bare commit id → `Detached` (PROCS-2.6). No `.git` up to `/` → project = name of `cwd`,
no worktree, no branch (PROCS-2.7). An unreadable `cwd` never reaches this function: `sessions`
sets `location` to `Unavailable` (PROCS-2.8).

### navori-procs — Harness catalog, sessions, lookup and activity

Satisfies: PROCS-2.1, PROCS-2.2, PROCS-2.9, PROCS-2.10, PROCS-3.1, PROCS-3.2, PROCS-3.3, PROCS-5.1, PROCS-5.2
Reuse: none — new code (rung 7)
Interface:
```rust
pub enum Harness { ClaudeCode, Codex, Grok }
pub enum TerminalHost { Terminal, ITerm2, Ghostty, WezTerm, Kitty, Alacritty, Warp, VsCode, Cursor, Zed }
pub struct AgentSession { pub harness: Harness, pub process: ProcId, pub location: Field<Location>, pub terminal: Option<TerminalHost> }
pub enum HarnessLookup { Found { harness: Harness, process: ProcId }, NotAHarness, NotRunning }
pub fn sessions(snapshot: &Snapshot) -> Vec<AgentSession>;
pub fn harness_of(snapshot: &Snapshot, pid: i32) -> HarnessLookup;
pub struct ChildActivity { pub process: ProcId, pub cpu_time: Duration }
pub fn activity(snapshot: &Snapshot, id: ProcId) -> Result<Vec<ChildActivity>, NotRunning>;
```
Depth: callers know "which sessions run, which Harness a pid belongs to, and what its children are
doing"; the recognition rules, runtime detection, nesting rule and terminal-bundle matching stay
hidden in one data table.
Locality: new `crates/procs/src/harness.rs` (table, recognition, `sessions`), `crates/procs/src/lookup.rs` (`harness_of`), `crates/procs/src/activity.rs` (`activity`); neighbors — leave.

Recognition (one table, from spike Q2 shapes; case-sensitive; the bare-basename rules are
deliberately permissive for a single-user Mac):

| Harness | Direct binary (exe path) | Under a runtime (`node`/`bun` exe, argv[1]) |
|---|---|---|
| Claude Code | contains `/claude/versions/`, or basename `claude` | contains `/@anthropic-ai/claude-code/` |
| Codex | basename `codex` | contains `/@openai/codex/` |
| Grok Build | basename `grok`, or basename starts `grok-` under `/.grok/` | — |

argv is read only for processes whose exe basename is `node` or `bun` (the candidates), never for
the rest. `sessions` keeps a matched process unless one of its ancestors matches the same Harness
(PROCS-2.2), resolves `location` from a verified `cwd` read (`Unavailable` → `location`
unavailable, PROCS-2.8), and sets `terminal` to the nearest ancestor whose exe path contains
`/<Bundle>.app/Contents/` for the bundles `Terminal`, `iTerm`, `Ghostty`, `WezTerm`, `kitty`,
`Alacritty`, `Warp`, `Visual Studio Code`, `Cursor`, `Zed` (PROCS-2.9) — `None` under tmux or
after re-parenting (PROCS-2.10; this Mac's own Claude Code session runs under tmux and reports
`None`). `harness_of` returns `NotRunning` when the pid is not in the snapshot **or** its live
entry no longer matches (a pid of another real UID is also "not running" — it is not the user's);
otherwise it checks the process and then its ancestors nearest first (PROCS-3.x). `activity`
re-checks the root's live entry (`NotRunning` if gone, PROCS-5.2), then returns its `descendants`
with `cpu_time` from a verified read per child, skipping a child that vanished (PROCS-5.1).

### Errors and logging (both crates)

Satisfies: PROCS-8.5
Reuse: rung 2 — the repo's convention: `thiserror` enums with named struct variants and
`tracing::warn!(error = %e, …)` (`docs/standards/errors-logging.md`)
Respects: ARCH-6
Security: THR-3
Interface: `navori_macos::{PlatformError, ReadError, WatchError, PowerError}`, `navori_procs::{ProcsError, Unavailable}` — variants carry pids, errno and IOKit codes only.
Depth: n/a — extends the repo's error and logging convention
Locality: every error type in the two crates; neighbors — leave.

No error variant stores argv, environment, a working directory or an executable path; `Display`
prints the operation, pid and errno or IOKit code. The crates log only pid, Harness and the
operation (unreadable candidates at `debug`, pid only). Nothing they emit can carry those values
into panic text, logs, Events or the Journal.

### Lint, dependency and guard checks (workspace + xtask)

Satisfies: PROCS-7.1, PROCS-7.2, PROCS-7.3, PROCS-7.4, PROCS-7.5, PROCS-7.6, PROCS-7.7, PROCS-7.8
Reuse: rung 2 — `check_deps` in `xtask/src/main.rs:554-621` (reads `cargo metadata`, first-match `else if` rule chain) and `xtask`'s existing `toml` dependency; rung 4 — rustc `unsafe_code` and clippy `undocumented_unsafe_blocks`
Respects: ARCH-9
Surface:
- `[workspace.lints.clippy]` (`Cargo.toml`) gains `undocumented_unsafe_blocks = "warn"` → every crate inheriting workspace lints: **replace** (no `unsafe` exists in the repo today, so no new warning — scan-design §1).
- `check_deps` rule chain → `cargo xtask ci` and `cargo xtask check-deps` callers: **replace** (new rules; existing rules untouched, guarded by PROCS-7.5/7.6).
- `docs/codebase/dependencies.md` tables, read by people and by `inspect-invariants`: **replace** (updated in the Documentation section).
Interface: `cargo xtask check-deps` (new failures, each naming the crate; also fails on any `pub unsafe fn`, `pub unsafe trait` or `pub unsafe extern` in `crates/macos/src`, PROCS-7.3), `cargo xtask lint-probe` (new; run by `cargo xtask ci` right after `check_deps`).
Depth: n/a — extends `xtask` check-deps and `ci`
Locality: `Cargo.toml` (`members`, `[workspace.lints.clippy]`), `crates/macos/Cargo.toml` (own `[lints.rust]` = workspace table minus `unsafe_code`; own `[lints.clippy]` = workspace table, which now includes `undocumented_unsafe_blocks`), `crates/procs/Cargo.toml` (`[lints] workspace = true`), `xtask/src/main.rs` — extend.

`check-deps` gains, reading each member manifest via `manifest_path` with `toml`: (a) a
dependency on `navori-macos` only from `navori-procs`, `navori-ffi` or a crate under `/modules/`;
(b) `navori-macos` has no internal dependency; (c) `[lints] workspace = true` in every member
except `navori-ffi` and `navori-macos` (PROCS-7.8); (d) `navori-macos`'s `[lints.*]` tables equal
the workspace tables except the missing `unsafe_code`. `lint-probe` builds a throwaway workspace
in a temp dir (removed on success and failure) whose root `[workspace.lints]` is copied from the
real `Cargo.toml` and whose two members take the real lint settings of `crates/procs` and
`crates/macos`: one member is a single `pub fn` with an `unsafe` block, the other a single
`pub fn` with an `unsafe` block lacking `// SAFETY:`. It runs
`cargo clippy --message-format=json -- -D warnings` on each and passes only when the first reports
an error with code `unsafe_code` and the second with code `clippy::undocumented_unsafe_blocks`,
and nothing else (PROCS-7.1, 7.2, 7.4 — the Story 7 guard probe). `navori-macos` exposes only safe
items (PROCS-7.3): every `unsafe` call sits inside a private function body.

### Documentation updates (infrastructure — no Satisfies)

Reuse: n/a — edits existing docs. Locality: docs only.
- `docs/architecture/system.md:61-62` — Platform FFI and Process model rows: Planned → built (R1), naming the traits.
- `docs/architecture/INDEX.md:53` — ARCH-9 enforcement row: exceptions `navori-ffi` (UniFFI) and `navori-macos` (platform FFI), plus `undocumented_unsafe_blocks`, the lint drift check and `lint-probe`.
- `docs/codebase/map.md:34-35`, `docs/codebase/modules.md:32-33` — describe the built crates.
- `docs/codebase/dependencies.md:27-35` — Current: `navori-procs → navori-macos`; `navori-macos` → none (leaf); `modules/*` may name `navori-macos` (traits); no other crate may. `:63` FFI row → `libc`, `objc2-io-kit`, `objc2-core-foundation` (drop `libproc`).
- `docs/ARCHITECTURE.md:272-278` (process-model seed) — ProcessTable via `sysctl(KERN_PROC_ALL)`; `:300-301` (Awake seed) — `navori hook` sends its pid/ppid over IPC and the Core walks the tree while the request is open; resolve the Harness on `UserPromptSubmit`/`Stop`, never on `SessionEnd` (1–1.5 s budgets, research note).

## Seams for testing

| Seam | Kind | Covers |
|---|---|---|
| `navori_macos::MacProcesses` (real OS): fields of every own-UID process, pid 1 and a spawned child cross-checked against `proc_pidinfo(PROC_PIDTBSDINFO)`; own argv buffer has no env | integration | PROCS-1.8, Decision 2 offsets |
| `navori_macos::parse_procargs` (xorshift-generated, truncated and synthetic buffers; non-UTF-8 strings) | unit (property) | PROCS-8.7 |
| `navori_macos::KqueueExits` (real OS) — the Story 4 exit probe: watch a spawned child, kill it, print the event; already-exited pid; drop releases fd and thread | integration | PROCS-4.2, PROCS-4.5, PROCS-8.3 |
| `KqueueExits` idle (real OS, 10 s): watch a sleeping child; zero sink calls, `ri_pkg_idle_wkups` delta 0 and process CPU-time delta < 2 ms via `proc_pid_rusage` ("no CPU time" = below measurement noise) | integration | PROCS-8.2 |
| one-test binary `crates/procs/tests/no_threads.rs` (nothing else runs in it): thread count via `ps -M -p <pid>` (header + one line per thread; `navori-procs` forbids `unsafe`) before/after `Snapshot::take`, `sessions`, `MacPower::hold` + release | integration | PROCS-8.1 |
| `navori_macos::MacPower` (real OS) — the Story 6 lifecycle probe: `pmset -g assertions` before / hold System / hold Display / release / drop | integration | PROCS-6.1, PROCS-6.2, PROCS-6.3, PROCS-6.4 |
| `navori_macos::PowerError::from_create` (the mapping `MacPower::hold` applies before creating a guard) | unit | PROCS-6.5 |
| `navori_procs::{Snapshot, sessions, harness_of, activity}` over `navori_macos::fake::FakeProcesses` (fake cwd values point at real temp git trees; the resolver is never faked) | unit | PROCS-1.1 – 1.7, PROCS-2.1, 2.2, 2.8, 2.9, 2.10, PROCS-3.1 – 3.3, PROCS-5.1, 5.2, PROCS-8.6 |
| `navori_procs::resolve_location` over real temp git trees (`git init`, `git worktree add`, `git submodule add`, detached checkout) | unit | PROCS-2.3 – 2.7 |
| `navori_procs::ProcWatcher` over `FakeProcesses` + `fake::FakeExits`, incl. unwatch-then-rewatch of a reused pid with an in-flight event, and an exit between registration and re-check (exactly one event) | unit | PROCS-4.1, 4.3, 4.4, 4.6 |
| `navori_procs` real-OS session check (`#[ignore]`, run by hand with Claude Code, Codex, Grok open) | integration | done signal 1 (PROCS-2.x on this Mac) |
| `navori_procs` snapshot timing (real OS, normal suite — debug, the pessimistic case — median of 5 under 10 ms; the release figure is recorded once in task evidence) | integration | PROCS-8.4 |
| `tracing` capture + `Display` of every error while resolving a fake process whose argv holds a fake token | unit | PROCS-8.5 |
| `cargo xtask check-deps` / `cargo xtask lint-probe` / `cargo xtask ci` (xtask is exempt from unit tests; verified by running; a deliberate `navori-cli → navori-macos` edge is shown to fail once, in task evidence) | build check | PROCS-7.1 – 7.8 |

## Coverage check

Every ID in requirements.md appears in exactly one `Satisfies:` line:
1.1–1.7 snapshot · 1.8 process source · 2.1, 2.2, 2.9, 2.10 harness · 2.3–2.8 resolver · 3.1–3.3
harness · 4.1–4.6 exit source and watcher · 5.1, 5.2 harness · 6.1–6.5 power · 7.1–7.8 lint checks ·
8.1–8.3 exit source and watcher · 8.4 snapshot · 8.5 errors and logging · 8.6 snapshot · 8.7
process source. Deliberately unmapped: none. The Documentation section carries no requirement
(infrastructure).
