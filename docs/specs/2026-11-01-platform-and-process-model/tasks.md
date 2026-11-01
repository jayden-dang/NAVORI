# Tasks: Platform layer and process model

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: PROCS
Status: Approved
Date: 2026-11-01
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

**Goal:** Give the Core a safe, measured view of the user's processes — snapshot, Agent sessions with Harness/project/worktree/branch/terminal, Harness lookup, child activity, exit watching — and a power-assertion wrapper, with all `unsafe` confined to one platform crate.

**Architecture:** `navori-macos` is a leaf crate over `libc` and `objc2-io-kit` that exposes safe traits (`ProcessSource`, `ExitSource`, `PowerAssertions`), their real implementations and, behind the `fake` feature, boundary fakes. `navori-procs` is `unsafe`-free and holds all logic (UID filter, process tree, verified reads, Harness table, git resolution, the token-filtered `ProcWatcher`). `xtask` gains dependency, lint-inheritance, lint-drift and public-`unsafe` rules plus a `lint-probe` that proves the lints bite.

**Tech Stack:** Rust 2024 (MSRV 1.90, `aarch64-apple-darwin`), `libc` 0.2.191, `objc2-io-kit` 0.3.2 (`pwr_mgt`), `objc2-core-foundation` 0.3.2 (`CFString`), `thiserror`, `tracing`, `tokio` (`sync`); tests with `cargo test` / `cargo nextest`, real temp git trees via the `git` CLI, `tempfile`, `tracing-subscriber` (test capture).

## Global Constraints

Source of truth (referenced, not copied — read before every task):

| Source | Content hash (`git hash-object`) |
|---|---|
| `docs/standards/INDEX.md` | `64e2dd72765c2732e1edec949ecd6a9d0e79a1f3` |
| `docs/standards/testing.md` | `f5fa88bb8dbfb80eacffcd7278c7b33d1d5191f3` |
| `docs/standards/errors-logging.md` | `c34f3f15f45007c52db395bdb2eb76b37f3232eb` |
| `docs/architecture/INDEX.md` (ARCH-N) | `4f414076e784c582c02473029ac46d25743f8e04` |

Compact delta every task inherits:
- **ARCH-9** — hand-written `unsafe` only in `navori-macos` (its `src/` and its `tests/`), each block preceded by a `// SAFETY:` comment, always inside a private function body in `src/`; nothing `pub` requires `unsafe` to call.
- **ARCH-6 / errors-logging** — no error variant or log line carries argv, environment, a working directory or an executable path; errors are `thiserror` enums with named struct variants; no `unwrap`/`expect` outside tests.
- **ARCH-16** — no timer, no polling, anywhere in these crates.
- **Testing** — test-first; tests in each crate's `tests/` through the public API only; the only fakes are `navori_macos::fake::*`; `resolve_location` and the Harness table are never faked; test names are behavior sentences; no requirement IDs in source or test names. A test that counts threads, file descriptors or latency lives alone in its own test binary (one `#[test]` per file) so `cargo test --workspace` cannot run it beside other tests.
- **Lints** — `missing_debug_implementations` is on: a type holding a `Box<dyn …>` gets a manual `impl Debug` that prints no closure. `libc::mach_timebase_info` is `#[deprecated]` in libc 0.2.191; its single call site carries `#[allow(deprecated)]` with a one-line reason.
- **libc conventions** — `proc_pidinfo` and `proc_pidpath` return `0` (not `-1`) on failure with `errno` set; `kevent.udata` is `*mut c_void` (store the token as `token as usize as *mut c_void`); `proc_vnodepathinfo.pvi_cdir.vip_path` is `[[c_char; 32]; 32]` — treat it as one 1024-byte NUL-terminated buffer.
- **Dependencies** — declared once in `[workspace.dependencies]`; crates use `x.workspace = true`.
- **Commands** — task gate: `cargo nextest run -p <crate> --test <file>`; before each commit: `cargo xtask ci`. Real-OS probes marked `#[ignore]` run with `cargo test -p <crate> --test <file> -- --ignored --nocapture`; their output goes into the task report.
- **Shared files** — after Task 1 no two tasks edit the same source file except where a `Depends-on` edge orders them; `lib.rs` files and `fake/mod.rs` are written once, in Task 1.
- **Team** — Solo band (`docs/agents/project.md` `## Team`): one contributor; no assignee or reviewer theater.

Retrieval (`load-subgraph`, schema 1.1, run once on the File Structure below): `blast_radius` → **CORE** (`crates/` is an ancestor of `crates/macos/`, `crates/procs/`; root `Cargo.toml` touches every crate) and **BUILD** + **PERF** (`xtask/`); `cluster(PROCS)` → PROCS alone (`owns_coverage` 0/6 — the neighborhood is only as complete as absent Files blocks); `observations: []`. **Collision:** PERF (`docs/specs/2026-11-01-perf-gate/`, In-progress) also edits `xtask/src/main.rs` — it adds `mod perf`, `Cmd::Perf` and touches `Profile`. This plan adds `Cmd::LintProbe`, `fn lint_probe`, rules inside `fn check_deps` and one call in `fn ci`; tasks cite these by **symbol**, never by line number, and rebase on whatever PERF has landed.

## File Structure

| Path | New / changed | Responsibility |
|---|---|---|
| `Cargo.toml` | changed | members `crates/macos`, `crates/procs`; path deps `navori-macos`, `navori-procs`; external deps `objc2-io-kit`, `objc2-core-foundation`; `[workspace.lints.clippy] undocumented_unsafe_blocks = "warn"` |
| `Cargo.lock` | changed | lock the two new external crates |
| `crates/macos/Cargo.toml` | new | `navori-macos`; own `[lints.rust]` (workspace minus `unsafe_code`) and `[lints.clippy]` (= workspace); feature `fake`; dev-deps `tempfile`, self with `fake` |
| `crates/macos/src/lib.rs` | new | crate doc; `mod error; mod process; mod procargs; mod exits; mod power;` each `pub use …::*;`; `#[cfg(feature = "fake")] pub mod fake;` |
| `crates/macos/src/error.rs` | new | `StartTime`, `PlatformError`, `ReadError`, `WatchError` |
| `crates/macos/src/process.rs` | new | `ProcEntry`, `ProcessSource`, `MacProcesses` |
| `crates/macos/src/procargs.rs` | new | `parse_procargs`, the `KERN_PROCARGS2` read behind `MacProcesses::argv` |
| `crates/macos/src/exits.rs` | new | `ExitSink`, `ExitSource`, `KqueueExits` |
| `crates/macos/src/power.rs` | new | `AssertionKind`, `PowerAssertions`, `AssertionGuard`, `PowerError`, `MacPower` |
| `crates/macos/src/fake/mod.rs` | new | `mod processes; mod exits; mod power;` each `pub use …::*;` |
| `crates/macos/src/fake/processes.rs` | new | `FakeProcesses`, `FakeProcess` |
| `crates/macos/src/fake/exits.rs` | new | `FakeExitsHandle` (+ private `FakeExits`) |
| `crates/macos/src/fake/power.rs` | new | `FakePower` |
| `crates/macos/tests/process_source.rs` | new | real-OS listing, per-pid reads, offsets cross-check, own argv |
| `crates/macos/tests/procargs.rs` | new | parser layout and property tests |
| `crates/macos/tests/exits.rs` | new | real-OS: already-exited, zombie, unwatch |
| `crates/macos/tests/exits_latency.rs` | new | real-OS exit probe with latency (one test) |
| `crates/macos/tests/exits_drop.rs` | new | real-OS fd/thread release on drop (one test) |
| `crates/macos/tests/exits_idle.rs` | new | real-OS 10 s idle measurement (one `#[ignore]` test) |
| `crates/macos/tests/power.rs` | new | real-OS assertion lifecycle probe, `from_create`, `FakePower` |
| `crates/procs/Cargo.toml` | new | `navori-procs`; `[lints] workspace = true`; dev-deps `navori-macos` (`fake`), `tempfile`, `tracing-subscriber`, `tokio` |
| `crates/procs/src/lib.rs` | new | crate doc; `mod error; mod snapshot; mod location; mod harness; mod lookup; mod activity; mod watch;` each `pub use …::*;` |
| `crates/procs/src/error.rs` | new | `ProcsError`, `Unavailable`, `Field<T>`, `NotRunning` |
| `crates/procs/src/snapshot.rs` | new | `ProcId`, `Process`, `Snapshot`, crate-private verified-read helper |
| `crates/procs/src/location.rs` | new | `Location`, `Branch`, `resolve_location` |
| `crates/procs/src/harness.rs` | new | `Harness`, `TerminalHost`, recognition table, `pub(crate) fn recognize`, `AgentSession`, `sessions` |
| `crates/procs/src/lookup.rs` | new | `HarnessLookup`, `harness_of` |
| `crates/procs/src/activity.rs` | new | `ChildActivity`, `activity` |
| `crates/procs/src/watch.rs` | new | `Exited`, `ProcWatcher` |
| `crates/procs/tests/snapshot.rs` | new | Story 1 over `FakeProcesses` |
| `crates/procs/tests/location.rs` | new | Story 2 git resolution over real temp trees |
| `crates/procs/tests/sessions.rs` | new | Story 2 sessions |
| `crates/procs/tests/harness_of.rs` | new | Story 3 |
| `crates/procs/tests/activity.rs` | new | Story 5 |
| `crates/procs/tests/watcher.rs` | new | Story 4 `ProcWatcher` over fakes |
| `crates/procs/tests/no_threads.rs` | new | PROCS-8.1 (one test) |
| `crates/procs/tests/real_os.rs` | new | PROCS-8.4 timing; `#[ignore]` real-OS session listing |
| `crates/procs/tests/no_leaks.rs` | new | PROCS-8.5 |
| `xtask/src/main.rs` | changed | `fn check_deps` rules (a)–(e); `Cmd::LintProbe`, `fn lint_probe`; `fn ci` calls it |
| `docs/architecture/system.md` | changed | Platform FFI / Process model rows built |
| `docs/architecture/INDEX.md` | changed | ARCH-9 enforcement row |
| `docs/codebase/map.md` | changed | built crates |
| `docs/codebase/modules.md` | changed | built crates |
| `docs/codebase/dependencies.md` | changed | Current rows, FFI deps row |
| `docs/ARCHITECTURE.md` | changed | process-model seed, Awake seed (hook ↔ CLI) |

---

### Task 1: Add the two crates under the dependency and lint rules

**Files:**
- Modify: `Cargo.toml` (members, `[workspace.dependencies]`)
- Modify: `Cargo.lock`
- Create: `crates/macos/Cargo.toml`, `crates/macos/src/lib.rs`, `crates/macos/src/error.rs`, `crates/macos/src/process.rs`, `crates/macos/src/procargs.rs`, `crates/macos/src/exits.rs`, `crates/macos/src/power.rs`, `crates/macos/src/fake/mod.rs`, `crates/macos/src/fake/processes.rs`, `crates/macos/src/fake/exits.rs`, `crates/macos/src/fake/power.rs`
- Create: `crates/procs/Cargo.toml`, `crates/procs/src/lib.rs`, `crates/procs/src/error.rs`, `crates/procs/src/snapshot.rs`, `crates/procs/src/location.rs`, `crates/procs/src/harness.rs`, `crates/procs/src/lookup.rs`, `crates/procs/src/activity.rs`, `crates/procs/src/watch.rs`
- Modify: `xtask/src/main.rs` (`fn check_deps`)
- Test: `cargo xtask check-deps` (xtask is exempt from unit tests — `docs/standards/testing.md` "Exempt")

**Reuse:** rung 2 — `check_deps` in `xtask/src/main.rs:554-621` (reads `cargo metadata`, first-match `else if` rule chain) and `xtask`'s existing `toml` dependency; rung 4 — rustc `unsafe_code` and clippy `undocumented_unsafe_blocks`

**Interfaces:**
- Consumes: none
- Produces: packages `navori-macos` (feature `fake = []`) and `navori-procs`; workspace deps `navori-macos = { path = "crates/macos" }`, `navori-procs = { path = "crates/procs" }`, `objc2-io-kit = { version = "0.3.2", default-features = false, features = ["std", "pwr_mgt", "libc"] }`, `objc2-core-foundation = { version = "0.3.2", default-features = false, features = ["std", "CFString"] }`; `crates/macos/Cargo.toml`: deps `libc`, `objc2-io-kit`, `objc2-core-foundation`, `thiserror`, `tracing`; dev-deps `tempfile.workspace = true` and `navori-macos = { path = ".", features = ["fake"] }` (if cargo rejects the self dev-dependency, use instead `[[test]] name = "power"` with `required-features = ["fake"]` and run that file with `--features fake`); `[lints.rust]` = `missing_debug_implementations = "warn"`, `unused_must_use = "deny"`; `[lints.clippy]` = the workspace clippy table verbatim; `crates/procs/Cargo.toml`: deps `navori-macos`, `thiserror`, `tracing`, `tokio` (workspace), dev-deps `navori-macos = { workspace = true, features = ["fake"] }`, `tempfile`, `tracing-subscriber`, `tokio`; `[lints] workspace = true`; every `src` module file exists with a `//!` doc line only and is wired in `lib.rs` / `fake/mod.rs` as listed in File Structure

**Depends-on:** none

- [x] **Step 1:** Create both manifests (shape of `crates/redact/Cargo.toml`) and every module file, wired as in File Structure. Run `cargo build -p navori-macos -p navori-procs --all-features` — expect: builds.
- [x] **Step 2:** Show the gaps: temporarily add `navori-macos.workspace = true` to `crates/kernel/Cargo.toml` `[dependencies]`, run `cargo xtask check-deps` — expect: `✓ dependency rules hold` (no rule yet). Revert.
- [x] **Step 3:** Add to `fn check_deps`, each failure naming crate and rule: (a) a non-dev dependency on `navori-macos` only from `navori-procs`, `navori-ffi` or a package whose manifest path contains `/modules/`; (b) `navori-macos` has no internal non-dev dependency; (c) every member manifest (`manifest_path`, parsed with `toml`) has `[lints] workspace = true` unless the package is `navori-ffi` or `navori-macos`; (d) `crates/macos/Cargo.toml` `[lints.rust]` equals root `[workspace.lints.rust]` minus `unsafe_code`, and its `[lints.clippy]` equals root `[workspace.lints.clippy]`; (e) no line in `crates/macos/src/**/*.rs` matches `pub unsafe fn`, `pub unsafe trait`, `pub(crate) unsafe trait` or `pub unsafe extern`.
- [x] **Step 4:** Prove each rule with a temporary edit, reverting after each, and paste the outputs into the task report: `navori-kernel → navori-macos` (rule a); `navori-macos → navori-redact` (rule b); delete `[lints] workspace = true` from `crates/procs/Cargo.toml` (rule c); drop `todo` from `crates/macos/Cargo.toml` `[lints.clippy]` (rule d); add `pub unsafe fn probe() {}` to `crates/macos/src/error.rs` (rule e); `navori-cli → navori-macos` (existing thin-client rule still fires); `navori-procs → navori-ffi` as a normal dependency (existing ffi rule still fires).
- [x] **Step 5:** `cargo xtask ci` — expect: pass. Commit `build(procs): add navori-macos and navori-procs under dependency and lint rules`.

_Requirements: PROCS-7.1, PROCS-7.3, PROCS-7.5, PROCS-7.6, PROCS-7.7, PROCS-7.8_

---

### Task 2: Prove the lints bite with `cargo xtask lint-probe`

**Files:**
- Modify: `Cargo.toml` (`[workspace.lints.clippy]` gains `undocumented_unsafe_blocks = "warn"`)
- Modify: `crates/macos/Cargo.toml` (same line in its `[lints.clippy]`)
- Modify: `xtask/src/main.rs` (`enum Cmd` gains `LintProbe`; `fn main` dispatch; new `fn lint_probe`; `fn ci` calls it right after `check_deps(root)?`)
- Test: `cargo xtask lint-probe`

**Reuse:** rung 2 — `check_deps` in `xtask/src/main.rs:554-621` (reads `cargo metadata`, first-match `else if` rule chain) and `xtask`'s existing `toml` dependency; rung 4 — rustc `unsafe_code` and clippy `undocumented_unsafe_blocks`

**Interfaces:**
- Consumes: Task 1's lint tables and drift rule (d)
- Produces: `cargo xtask lint-probe` — exit 0 only when fixture `a` reports exactly the error code `unsafe_code` and fixture `b` reports exactly `clippy::undocumented_unsafe_blocks`

**Depends-on:** Task 1

- [ ] **Step 1:** Implement `fn lint_probe(root: &Path) -> Result<()>`: temp dir `std::env::temp_dir().join(format!("navori-lint-probe-{}", std::process::id()))`, removed on every exit path; root `Cargo.toml` = `[workspace] resolver = "3"`, `members = ["a", "b"]`, `[workspace.package] edition = "2024"`, plus the real root `[workspace.lints]` tables copied from `Cargo.toml`; member `a` copies the `[lints]` of `crates/procs/Cargo.toml` and member `b` the `[lints.*]` of `crates/macos/Cargo.toml`; both `src/lib.rs` hold one function — `a`: `pub fn probe() -> u8 { let x = 1u8; // SAFETY: x is a live local u8.` then `unsafe { std::ptr::read(&x) } }`; `b`: the same body without the `// SAFETY:` line; run `cargo clippy --message-format=json -- -D warnings` in each member dir and collect `message.code.code` of every `reason == "compiler-message"` with `message.level == "error"`.
- [ ] **Step 2:** `cargo xtask lint-probe` — expect: failure "fixture b: expected [clippy::undocumented_unsafe_blocks], got []".
- [ ] **Step 3:** Add `undocumented_unsafe_blocks = "warn"` to both clippy tables. `cargo xtask lint-probe` — expect: pass. `cargo xtask check-deps` — expect: pass.
- [ ] **Step 4:** Wire `lint_probe(root)?` into `fn ci` after `check_deps(root)?`. `cargo xtask ci` — expect: pass. Commit `build(xtask): add lint-probe proving unsafe_code and SAFETY-comment lints fail CI`.

_Requirements: PROCS-7.1, PROCS-7.2, PROCS-7.4_

---

### Task 3: List processes and read exe, cwd and CPU time from the OS

**Files:**
- Modify: `crates/macos/src/error.rs`, `crates/macos/src/process.rs`
- Test: `crates/macos/tests/process_source.rs`

**Reuse:** rung 5 — `libc` 0.2.191 (already in the workspace) for `sysctl`, `proc_pidpath`, `proc_pidinfo`, `proc_pid_rusage`, `mach_timebase_info`; new code (rung 7) only for the `kinfo_proc` field reads and the `KERN_PROCARGS2` parser, which `libc` does not provide

**Interfaces:**
- Consumes: none
- Produces: `pub struct StartTime(pub u64)` (µs since the epoch; `Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash`); `pub enum PlatformError { Sysctl { errno: i32 }, Kqueue { op: &'static str, errno: i32 }, Layout { len: usize } }`; `pub enum ReadError { Gone, Denied, Malformed }` (`Debug, Clone, Copy, PartialEq, Eq`); `pub enum WatchError { Gone, Kqueue { errno: i32 } }`; `pub struct ProcEntry { pub pid: i32, pub ppid: i32, pub ruid: u32, pub start: StartTime, pub zombie: bool }`; `pub trait ProcessSource: Send + Sync { fn current_uid(&self) -> u32; fn list(&self) -> Result<Vec<ProcEntry>, PlatformError>; fn entry(&self, pid: i32) -> Result<ProcEntry, ReadError>; fn exe(&self, pid: i32) -> Result<PathBuf, ReadError>; fn argv(&self, pid: i32) -> Result<Vec<String>, ReadError>; fn cwd(&self, pid: i32) -> Result<PathBuf, ReadError>; fn cpu_time(&self, pid: i32) -> Result<Duration, ReadError>; }`; `pub struct MacProcesses` with `pub fn new() -> Result<MacProcesses, PlatformError>` (fails with `Layout` unless pid 1 parses to `ppid == 0 && ruid == 0`). `MacProcesses::argv` returns `Err(ReadError::Malformed)` until Task 4.

**Depends-on:** Task 1

- [ ] **Step 1:** Write `tests/process_source.rs`: `listing_reports_a_spawned_child_with_its_parent_and_owner` (spawn `sleep 30`; entry `ppid == std::process::id() as i32`, `ruid == current_uid()`, `zombie == false`); `listing_matches_the_kernel_view_for_every_owned_process_and_launchd` (for every entry whose `libc::proc_pidinfo(PROC_PIDTBSDINFO)` succeeds — called in the test inside `unsafe` with `// SAFETY:` — `ppid`, `ruid`, `start` (`pbi_start_tvsec * 1_000_000 + pbi_start_tvusec`) are equal; pid 1 has `ppid == 0`, `ruid == 0`); `a_spawned_child_has_its_executable_and_working_directory` (spawn `sleep 30` with `current_dir` = a `tempfile` dir; `exe` == `/bin/sleep`; `cwd` == the dir canonicalized); `cpu_time_of_a_busy_child_is_close_to_its_wall_time` (`sh -c 'yes > /dev/null'` for 1 s; `cpu_time` in 0.5–1.5 s); `reading_a_pid_that_does_not_exist_reports_gone` (pid 999999: `entry`, `exe`, `cwd` → `Gone`); `reading_the_working_directory_of_launchd_is_denied` (pid 1 `cwd` → `Denied`). Run `cargo nextest run -p navori-macos --test process_source` — expect: compile failure (`MacProcesses` not found).
- [ ] **Step 2:** Implement per design "navori-macos — process source" and Decision 2: `list` via `sysctl([CTL_KERN, KERN_PROC, KERN_PROC_ALL])` (probe size, allocate probe + probe/8, up to 3 tries on `ENOMEM`, `Layout { len }` unless `len % 648 == 0`), records via `chunks_exact(648)`, fields read with `from_ne_bytes` over `get(..)` slices at offsets 0 (`tv_sec` i64), 8 (`tv_usec` i32), 36 (`p_stat` u8; zombie when 5), 40 (`p_pid` i32), 392 (`p_ruid` u32), 560 (`e_ppid` i32); `entry` via `[CTL_KERN, KERN_PROC, KERN_PROC_PID, pid]` (returned len 0 → `Gone`); `exe` via `proc_pidpath` (`PROC_PIDPATHINFO_MAXSIZE`); `cwd` via `proc_pidinfo(PROC_PIDVNODEPATHINFO)`; `cpu_time` via `proc_pid_rusage(RUSAGE_INFO_V2)`, `(ri_user_time + ri_system_time) * numer / denom` with the timebase in a `std::sync::OnceLock`; errno `ESRCH → Gone`, `EPERM → Denied`. Run — expect: pass.
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `feat(macos): list processes and read exe, cwd and CPU time from the kernel`.

_Requirements: PROCS-1.1, PROCS-1.7_

---

### Task 4: Read argv from the OS argument buffer without the environment

**Files:**
- Modify: `crates/macos/src/procargs.rs`, `crates/macos/src/process.rs` (`MacProcesses::argv`)
- Test: `crates/macos/tests/procargs.rs`, `crates/macos/tests/process_source.rs`

**Reuse:** rung 5 — `libc` 0.2.191 (already in the workspace) for `sysctl`, `proc_pidpath`, `proc_pidinfo`, `proc_pid_rusage`, `mach_timebase_info`; new code (rung 7) only for the `kinfo_proc` field reads and the `KERN_PROCARGS2` parser, which `libc` does not provide

**Interfaces:**
- Consumes: `ProcessSource`, `ReadError`, `MacProcesses` (Task 3)
- Produces: `pub fn parse_procargs(buf: &[u8]) -> Result<Vec<String>, ReadError>`; `MacProcesses::argv` returns the `argc` strings decoded with `String::from_utf8_lossy`; `KERN_PROCARGS2` `EINVAL` → `Gone` when `entry(pid)` also fails, else `Denied`

**Depends-on:** Task 3

- [ ] **Step 1:** Write `tests/procargs.rs` (helper `buf(argc: i32, exec: &[u8], args: &[&[u8]], env: &[&[u8]]) -> Vec<u8>` = `argc.to_ne_bytes()` ++ exec ++ `\0\0\0\0` ++ each arg + `\0` ++ each env + `\0`): `the_arguments_are_returned_and_the_environment_is_not` (`buf(2, b"/bin/zsh", [b"zsh", b"-l"], [b"HOME=/Users/x", b"PATH=/bin"])` → `["zsh", "-l"]`); `a_non_utf8_argument_is_kept_lossily` (`buf(2, b"/usr/bin/node", [b"node", b"\xff/x.js"], [])` → `["node", "\u{fffd}/x.js"]`); `a_buffer_with_fewer_strings_than_argc_is_malformed` (`buf(3, b"/bin/sh", [b"sh", b"-c"], [])`); `an_unterminated_argument_is_malformed` (valid buffer with its final `\0` removed and no env); `an_empty_or_argc_only_buffer_is_malformed` (`[]`, `2i32.to_ne_bytes()`); `random_and_truncated_buffers_never_panic` (xorshift64 seeded `0x9E37_79B9_7F4A_7C15`, 10 000 buffers of length 0–512, plus every prefix of the first valid buffer; each call returns `Ok` or `Err`). Add to `tests/process_source.rs`: `a_spawned_child_reports_its_arguments` (`sleep 30` → `["sleep", "30"]`) and `the_test_process_own_arguments_exclude_its_environment` (`argv(std::process::id() as i32)` == `std::env::args().collect::<Vec<_>>()`). Run `cargo nextest run -p navori-macos --test procargs` — expect: compile failure (`parse_procargs` not found).
- [ ] **Step 2:** Implement `parse_procargs` (argc → skip exec path to its NUL → skip NUL padding → exactly `argc` NUL-terminated strings, every index through `get`) and `MacProcesses::argv` (`KERN_ARGMAX` once in a `OnceLock`, probe `KERN_PROCARGS2` size, read, parse). Run both files — expect: pass.
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `feat(macos): read process arguments and drop the environment`.

_Requirements: PROCS-1.8, PROCS-8.7_

---

### Task 5: Snapshot the user's processes with verified reads

**Files:**
- Modify: `crates/macos/src/fake/processes.rs`
- Modify: `crates/procs/src/error.rs`, `crates/procs/src/snapshot.rs`
- Test: `crates/procs/tests/snapshot.rs`

**Reuse:** none — new code (rung 7); no process model exists in the repo

**Interfaces:**
- Consumes: `ProcessSource`, `ProcEntry`, `StartTime`, `ReadError`, `PlatformError` (Task 3)
- Produces: `navori_macos::fake::FakeProcess { pub pid: i32, pub ppid: i32, pub ruid: u32, pub start: StartTime, pub zombie: bool, pub exe: Result<PathBuf, ReadError>, pub argv: Result<Vec<String>, ReadError>, pub cwd: Result<PathBuf, ReadError>, pub cpu: Result<Duration, ReadError> }` with `FakeProcess::new(pid: i32, ppid: i32, ruid: u32, start_us: u64, exe: &str)` (argv/cwd/cpu default `Err(ReadError::Denied)`) and builders `.argv(&[&str])`, `.cwd(impl Into<PathBuf>)`, `.cpu(Duration)`, `.zombie()`, `.exe_err(ReadError)`; `navori_macos::fake::FakeProcesses` (`ProcessSource`) with `new(current_uid: u32)`, `add(&self, FakeProcess)`, `exit(&self, pid: i32)`, `replace(&self, pid: i32, start_us: u64, ruid: u32)`, `replace_on_read(&self, pid: i32, kind: &'static str, start_us: u64, ruid: u32)` (applies `replace` at the moment `kind` is read), `reads(&self) -> Vec<(i32, &'static str)>` (`"exe" | "argv" | "cwd" | "cpu"`); `navori_procs::{ProcId { pub pid: i32, pub start: StartTime }, Unavailable { Exited, PermissionDenied, Malformed }, type Field<T> = Result<T, Unavailable>, Process { pub id: ProcId, pub parent: Option<i32>, pub exe: Field<PathBuf> }, Snapshot, ProcsError { Platform { source: PlatformError }, Watch { errno: i32 } }, NotRunning }`; `Snapshot::{take(Arc<dyn ProcessSource>) -> Result<Snapshot, ProcsError>, processes(&self) -> impl Iterator<Item = &Process>, get(&self, pid: i32) -> Option<&Process>, ancestors(&self, id: ProcId) -> Vec<&Process>, descendants(&self, id: ProcId) -> Vec<&Process>, argv(&self, id: ProcId) -> Field<Vec<String>>, cwd(&self, id: ProcId) -> Field<PathBuf>}`; crate-private `Snapshot::read_verified<T>(&self, id: ProcId, read: impl FnOnce(&dyn ProcessSource, i32) -> Result<T, ReadError>) -> Field<T>` and `Snapshot::source(&self) -> &dyn ProcessSource` for Tasks 7–9

**Depends-on:** Task 3

- [ ] **Step 1:** Write `tests/snapshot.rs` over `FakeProcesses::new(501)`: `the_snapshot_holds_every_process_of_the_user_with_parent_start_and_executable`; `processes_of_another_user_are_left_out` (ruid 0); `a_zombie_is_left_out`; `a_process_whose_executable_read_says_gone_is_left_out` (`.exe_err(ReadError::Gone)`); `a_process_replaced_during_the_snapshot_is_left_out` (`replace_on_read(pid, "exe", 999, 501)`); `an_unreadable_executable_is_kept_as_unavailable` (`.exe_err(ReadError::Denied)` → `Unavailable::PermissionDenied`); `a_reused_pid_gets_a_different_identity` (snapshot 1, `replace(10, 999, 501)`, snapshot 2: the two `ProcId`s for pid 10 differ); `ancestors_and_descendants_are_ordered_nearest_first` (10 → 11 → 12, and 11 → 13: `ancestors(12)` = [11, 10], `descendants(10)` = [11, 12, 13]); `argv_and_cwd_are_read_only_for_the_asked_process` (`reads()` filtered to kinds `argv`/`cwd` lists only that pid); `argv_of_a_process_replaced_since_the_snapshot_is_unavailable` (`replace` then `argv` → `Exited`); `argv_and_cwd_are_never_read_for_a_pid_now_owned_by_another_user` (`replace(pid, 999, 0)` then `argv`/`cwd` → `Exited`; `reads()` has no `argv`/`cwd` for it). Run `cargo nextest run -p navori-procs --test snapshot` — expect: compile failure.
- [ ] **Step 2:** Implement `FakeProcesses` (interior `Mutex`, manual `Debug`) and `Snapshot` per design "navori-procs — snapshot" (filter `ruid == current_uid()` and `!zombie`; read `exe`; `Gone` drops the pid, other errors stay `Unavailable`; second `list()` drops changed or vanished pids; `read_verified` = live entry same start and same ruid → read → live entry same start). Run — expect: pass.
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `feat(procs): snapshot the user's processes with identity-checked reads`.

_Requirements: PROCS-1.1, PROCS-1.2, PROCS-1.3, PROCS-1.4, PROCS-1.5, PROCS-1.6, PROCS-1.7, PROCS-8.6_

---

### Task 6: Resolve project, worktree and branch from a working directory

**Files:**
- Modify: `crates/procs/src/location.rs`
- Test: `crates/procs/tests/location.rs`

**Reuse:** rung 3 — `std::fs` / `std::path` only; no `git` subprocess, no git library

**Interfaces:**
- Consumes: none
- Produces: `pub struct Location { pub project: String, pub worktree: Option<String>, pub branch: Option<Branch> }`, `pub enum Branch { Named(String), Detached { abbrev: String } }`, `pub fn resolve_location(cwd: &Path) -> Location` (all `Debug, Clone, PartialEq, Eq`)

**Depends-on:** Task 1

- [ ] **Step 1:** Write `tests/location.rs`; each test builds a real tree under `tempfile::tempdir()` with `git -c user.name=t -c user.email=t@example.com` (`init -b main`, `commit --allow-empty -m init`): `a_directory_inside_a_main_checkout_reports_its_project_and_branch` (cwd `navori/src` → `{ project: "navori", worktree: None, branch: Some(Named("main")) }`); `a_linked_worktree_reports_the_main_project_and_the_worktree_name` (`git worktree add ../navori-feat -b feat` → `{ "navori", Some("navori-feat"), Some(Named("feat")) }`); `a_submodule_is_its_own_project_and_not_a_worktree` (`git -c protocol.file.allow=always submodule add <path to repo lib> lib`, cwd `navori/lib` → `project == "lib"`, `worktree == None`); `a_detached_head_reports_the_abbreviated_commit` (`git checkout --detach` → `Detached { abbrev }` == `git rev-parse --short=7 HEAD`); `a_directory_outside_any_repository_reports_its_own_name` (→ `{ <dir name>, None, None }`). Run `cargo nextest run -p navori-procs --test location` — expect: compile failure.
- [ ] **Step 2:** Implement `resolve_location` per design "navori-procs — project resolver" (relative `gitdir:` resolved against the `.git` file's directory; `commondir` resolved against the gitdir). Run — expect: pass.
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `feat(procs): resolve project, worktree and branch from .git without running git`.

_Requirements: PROCS-2.3, PROCS-2.4, PROCS-2.5, PROCS-2.6, PROCS-2.7_

---

### Task 7: List the running Agent sessions

**Files:**
- Modify: `crates/procs/src/harness.rs`
- Test: `crates/procs/tests/sessions.rs`

**Reuse:** none — new code (rung 7)

**Interfaces:**
- Consumes: `Snapshot`, `Process`, `ProcId`, `Field`, `Unavailable`, `read_verified` (Task 5); `resolve_location`, `Location` (Task 6); `FakeProcesses`, `FakeProcess` (Task 5)
- Produces: `pub enum Harness { ClaudeCode, Codex, Grok }`, `pub enum TerminalHost { Terminal, ITerm2, Ghostty, WezTerm, Kitty, Alacritty, Warp, VsCode, Cursor, Zed }`, `pub struct AgentSession { pub harness: Harness, pub process: ProcId, pub location: Field<Location>, pub terminal: Option<TerminalHost> }`, `pub fn sessions(snapshot: &Snapshot) -> Vec<AgentSession>`; `pub(crate) fn recognize(snapshot: &Snapshot, process: &Process) -> Option<Harness>`

**Depends-on:** Task 5, Task 6

- [ ] **Step 1:** Write `tests/sessions.rs` (fake cwds point at real temp git trees built as in Task 6): table-driven `each_harness_shape_is_recognized` — exe `/Users/u/.local/share/claude/versions/2.1.316` → ClaudeCode; `/opt/homebrew/Caskroom/codex/0.166.0/bin/codex` → Codex; `/Users/u/.grok/downloads/grok-1.0.70-macos-aarch64` → Grok; `/opt/homebrew/Cellar/node/24.1.0/bin/node` with argv `["node", "/opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js"]` → ClaudeCode; `/Users/u/.bun/bin/bun` with argv `["bun", "/Users/u/node_modules/@openai/codex/bin/codex.js"]` → Codex; node with argv `["node", "/srv/app.js"]` → none; `argv_is_read_only_for_node_and_bun_processes` (`reads()` has no `argv` for the native rows); `a_harness_started_by_the_same_harness_is_not_a_second_session`; `a_session_reports_the_location_of_its_working_directory`; `a_session_whose_working_directory_cannot_be_read_has_an_unavailable_location`; table-driven `the_nearest_terminal_app_is_the_terminal_host` — ancestor exes `/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal` → Terminal, `/Applications/iTerm.app/Contents/MacOS/iTerm2` → ITerm2, `/Applications/Ghostty.app/Contents/MacOS/ghostty` → Ghostty, `/Applications/WezTerm.app/Contents/MacOS/wezterm-gui` → WezTerm, `/Applications/kitty.app/Contents/MacOS/kitty` → Kitty, `/Applications/Alacritty.app/Contents/MacOS/alacritty` → Alacritty, `/Applications/Warp.app/Contents/MacOS/stable` → Warp, `/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper (Plugin).app/Contents/MacOS/Code Helper (Plugin)` → VsCode, `/Applications/Cursor.app/Contents/MacOS/Cursor` → Cursor, `/Applications/Zed.app/Contents/MacOS/zed` → Zed, `/Applications/Cloudflare WARP.app/Contents/MacOS/Cloudflare WARP` → none; `a_session_under_tmux_has_no_terminal_host` (`tmux(2627, ppid 1) → -zsh → claude`, pid 1 absent). Run `cargo nextest run -p navori-procs --test sessions` — expect: compile failure.
- [ ] **Step 2:** Implement the recognition table (case-sensitive), `recognize`, terminal matching (`/<Bundle>.app/Contents/`) and `sessions` per design. Run — expect: pass.
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `feat(procs): list Agent sessions with Harness, location and terminal host`.

_Requirements: PROCS-2.1, PROCS-2.2, PROCS-2.8, PROCS-2.9, PROCS-2.10_

---

### Task 8: Find the Harness behind a calling process

**Files:**
- Modify: `crates/procs/src/lookup.rs`
- Test: `crates/procs/tests/harness_of.rs`

**Reuse:** none — new code (rung 7)

**Interfaces:**
- Consumes: `recognize`, `Harness` (Task 7); `Snapshot`, `ProcId`, `Snapshot::source` (Task 5); `FakeProcesses` (Task 5)
- Produces: `pub enum HarnessLookup { Found { harness: Harness, process: ProcId }, NotAHarness, NotRunning }`, `pub fn harness_of(snapshot: &Snapshot, pid: i32) -> HarnessLookup`

**Depends-on:** Task 7

- [ ] **Step 1:** Write `tests/harness_of.rs`: `a_hook_shell_resolves_to_its_claude_ancestor` (`claude(100) → sh(101) → navori(102)`: `harness_of(102)` → `Found { ClaudeCode, process.pid == 100 }`); `the_harness_process_itself_resolves_to_itself`; `the_nearest_harness_wins_when_two_are_nested` (`codex → sh → claude → sh → navori` → ClaudeCode); `a_process_with_no_harness_ancestor_is_not_a_harness`; `a_pid_not_in_the_snapshot_is_not_running`; `a_pid_that_exited_since_the_snapshot_is_not_running` (`exit(102)` after `take`). Run `cargo nextest run -p navori-procs --test harness_of` — expect: compile failure.
- [ ] **Step 2:** Implement `harness_of` (snapshot membership; live `entry` with the same start, else `NotRunning`; self then ancestors nearest first through `recognize`). Run — expect: pass.
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `feat(procs): resolve the Harness behind a calling process`.

_Requirements: PROCS-3.1, PROCS-3.2, PROCS-3.3_

---

### Task 9: Report the live descendants of a process with their CPU time

**Files:**
- Modify: `crates/procs/src/activity.rs`
- Test: `crates/procs/tests/activity.rs`

**Reuse:** none — new code (rung 7)

**Interfaces:**
- Consumes: `Snapshot::descendants`, `read_verified`, `Snapshot::source`, `NotRunning` (Task 5); `ProcessSource::cpu_time` (Task 3)
- Produces: `pub struct ChildActivity { pub process: ProcId, pub cpu_time: Duration }`, `pub fn activity(snapshot: &Snapshot, id: ProcId) -> Result<Vec<ChildActivity>, NotRunning>`

**Depends-on:** Task 5

- [ ] **Step 1:** Write `tests/activity.rs`: `activity_lists_every_live_descendant_with_its_cpu_time` (`claude(100) → zsh(101, 10 ms) → cargo(102, 3 s)` and `zsh → node(103, 250 ms)` → `[101, 102, 103]` with those CPU times, nearest first); `a_descendant_that_vanished_is_skipped` (`exit(103)` after `take`); `activity_of_a_process_that_exited_is_not_running` (`exit(100)`); `activity_of_a_pid_not_in_the_snapshot_is_not_running`. Run `cargo nextest run -p navori-procs --test activity` — expect: compile failure.
- [ ] **Step 2:** Implement `activity` (live root re-check → `NotRunning`; `descendants`; `read_verified` with `cpu_time`; skip `Exited`). Run — expect: pass.
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `feat(procs): report live descendants of a process with CPU time`.

_Requirements: PROCS-5.1, PROCS-5.2_

---

### Task 10: Watch processes for exit with kqueue

**Files:**
- Modify: `crates/macos/src/exits.rs`
- Test: `crates/macos/tests/exits.rs`, `crates/macos/tests/exits_latency.rs`, `crates/macos/tests/exits_drop.rs`, `crates/macos/tests/exits_idle.rs`

**Reuse:** rung 4 — kqueue `EVFILT_PROC`/`NOTE_EXIT`, `EVFILT_USER`, `EV_RECEIPT` (platform feature) through `libc`; new code (rung 7) for the thread and the procs-side watcher

**Interfaces:**
- Consumes: `PlatformError`, `WatchError` (Task 3)
- Produces: `pub type ExitSink = Box<dyn Fn(i32, u64) + Send + Sync>`; `pub trait ExitSource: Send { fn watch(&mut self, pid: i32, token: u64) -> Result<(), WatchError>; fn unwatch(&mut self, pid: i32); }`; `pub struct KqueueExits` (manual `Debug`) with `pub fn start(sink: ExitSink) -> Result<KqueueExits, PlatformError>` and `impl ExitSource`; `Drop` triggers the wake event, joins the thread (a panicked join is ignored) and closes the fd

**Depends-on:** Task 3

- [ ] **Step 1:** Write the tests, each sink feeding a `std::sync::mpsc::channel`: `tests/exits.rs` — `watching_a_process_that_already_exited_reports_gone` (spawn `true`, `wait`, `watch` → `Err(WatchError::Gone)`); `watching_an_unreaped_zombie_reports_gone` (spawn `true`, sleep 100 ms, no `wait`); `an_unwatched_process_produces_no_event` (watch, unwatch, kill, nothing within 200 ms; `unwatch` of an already-fired pid returns normally). `tests/exits_latency.rs` (one test) — the exit probe `a_killed_child_is_reported_with_its_token_within_ten_milliseconds` (spawn `sleep 30`, `watch(pid, 7)`, `kill`, receive `(pid, 7)`, elapsed < 10 ms, print it). `tests/exits_drop.rs` (one test) — `dropping_the_watcher_closes_its_kqueue_and_ends_its_thread` (fd count = entries of `std::fs::read_dir("/dev/fd")`, thread count = `pti_threadnum` from `libc::proc_pidinfo(PROC_PIDTASKINFO)` in an `unsafe` block with `// SAFETY:`; both equal their before values after drop, and the thread count was before + 1 while the watcher lived). `tests/exits_idle.rs` (one `#[ignore]` test) — `an_idle_watcher_causes_no_wakeups_and_no_cpu_time` (watch `sleep 60`, 10 s; `proc_pid_rusage(RUSAGE_INFO_V2)` of the test process: `ri_pkg_idle_wkups` delta 0, `ri_user_time + ri_system_time` delta < 2 ms after timebase conversion; no sink call). Run `cargo nextest run -p navori-macos --test exits` — expect: compile failure.
- [ ] **Step 2:** Implement `KqueueExits` per design "Exit source and watcher": `kqueue()`; register `EVFILT_USER` (`EV_ADD | EV_CLEAR`) before `std::thread::Builder::new().name("navori-exits".into()).spawn`; thread loop `kevent(kq, null, 0, events, 16, null)` → for `EVFILT_PROC` call `sink(ident as i32, udata as usize as u64)`, exit on `EVFILT_USER`; `watch` = one change `EV_ADD | EV_ONESHOT | EV_RECEIPT`, `fflags = NOTE_EXIT`, `udata = token as usize as *mut c_void`, a one-slot eventlist, `EV_ERROR` with `data == ESRCH` → `Gone`, other `data` → `Kqueue { errno }`; `unwatch` = `EV_DELETE | EV_RECEIPT`, `ENOENT` ignored; `Drop` = `NOTE_TRIGGER` on the user event, join, `close`. Run `exits`, `exits_latency`, `exits_drop` — expect: pass. Run `cargo test -p navori-macos --test exits_idle -- --ignored --nocapture` — expect: pass; paste the numbers.
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `feat(macos): watch process exits with kqueue on one blocking thread`.

_Requirements: PROCS-4.2, PROCS-4.5, PROCS-8.2, PROCS-8.3_

---

### Task 11: Deliver exactly one exit per watched identity

**Files:**
- Modify: `crates/macos/src/fake/exits.rs`
- Modify: `crates/procs/src/watch.rs`
- Test: `crates/procs/tests/watcher.rs`

**Reuse:** rung 4 — kqueue `EVFILT_PROC`/`NOTE_EXIT`, `EVFILT_USER`, `EV_RECEIPT` (platform feature) through `libc`; new code (rung 7) for the thread and the procs-side watcher

**Interfaces:**
- Consumes: `ExitSink`, `ExitSource`, `WatchError`, `PlatformError` (Tasks 3, 10); `ProcessSource`, `FakeProcesses`, `ProcId`, `ProcsError` (Task 5)
- Produces: `navori_macos::fake::FakeExitsHandle` (`Clone + Send + Sync`, manual `Debug`) — `new() -> FakeExitsHandle`; `factory(&self) -> impl FnOnce(ExitSink) -> Result<Box<dyn ExitSource>, PlatformError>` (stores the sink, returns a private `FakeExits` sharing state with the handle); `fire(&self, pid: i32)` (calls the sink with the most recent token ever registered for `pid`, even after `unwatch`); `fire_with(&self, pid: i32, token: u64)`; `fail_next_watch(&self, pid: i32, err: WatchError)`; `watched(&self) -> Vec<i32>`; `tokens(&self, pid: i32) -> Vec<u64>`; `navori_procs::{Exited { pub id: ProcId }, ProcWatcher}` — `ProcWatcher::new(source: Arc<dyn ProcessSource>, exits: impl FnOnce(ExitSink) -> Result<Box<dyn ExitSource>, PlatformError>) -> Result<ProcWatcher, ProcsError>`, `watch(&mut self, id: ProcId) -> Result<(), ProcsError>` (`WatchError::Kqueue { errno }` → `ProcsError::Watch { errno }`, entry removed), `unwatch(&mut self, id: ProcId)`, `async fn next(&mut self) -> Option<Exited>` (cancel-safe)

**Depends-on:** Task 5, Task 10

- [ ] **Step 1:** Write `tests/watcher.rs` (`#[tokio::test]`, `FakeProcesses::new(501)` + `FakeExitsHandle`): `an_exit_of_a_watched_process_is_delivered_once` (`fire(pid)` twice → one `Exited`); `a_process_already_gone_is_reported_at_once` (`fail_next_watch(pid, WatchError::Gone)`); `a_kqueue_failure_is_returned_and_nothing_is_watched` (`fail_next_watch(pid, WatchError::Kqueue { errno: 9 })` → `Err(ProcsError::Watch { errno: 9 })`, `watched()` empty); `a_pid_reused_before_registration_reports_the_old_identity_as_exited` (`replace` before `watch` → one `Exited { old id }`, `watched()` lacks the pid); `an_exit_between_registration_and_recheck_is_delivered_once` (`exit(pid)` on the fake before `watch`, so registration succeeds but the re-check sees `Gone`; then `fire(pid)` → exactly one `Exited` within 100 ms); `an_unwatched_process_produces_no_event` (`unwatch`, then `fire(pid)` with the old token → no event within 100 ms); `rewatching_a_reused_pid_ignores_the_previous_identitys_late_exit` (watch A, unwatch A, `replace` to B, watch B, `fire_with(pid, tokens(pid)[0])` → no event; `fire(pid)` → `Exited { B }`); `a_new_watcher_after_dropping_one_works_the_same`; `next_is_cancel_safe` (two `tokio::select!` rounds against `std::future::ready(())`, then the queued event still arrives). Run `cargo nextest run -p navori-procs --test watcher` — expect: compile failure.
- [ ] **Step 2:** Implement `FakeExitsHandle` and `ProcWatcher` per design (token map, synthetic `VecDeque`, unbounded channel, pid + token filter, sink ignores `SendError`). Run — expect: pass.
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `feat(procs): deliver exactly one exit per watched identity`.

_Requirements: PROCS-4.1, PROCS-4.3, PROCS-4.4, PROCS-4.6_

---

### Task 12: Hold and release power assertions

**Files:**
- Modify: `crates/macos/src/power.rs`, `crates/macos/src/fake/power.rs`
- Test: `crates/macos/tests/power.rs`

**Reuse:** rung 5 — `objc2-io-kit` (`pwr_mgt`: `IOPMAssertionCreateWithName`, `IOPMAssertionRelease`, `kIOPMAssertionLevelOn`) and `objc2-core-foundation` (`CFString`), adopted with approach A

**Interfaces:**
- Consumes: Task 1's dependencies and the self dev-dependency with `fake`
- Produces: `pub enum AssertionKind { System, Display }` (`Debug, Clone, Copy, PartialEq, Eq`); `pub trait PowerAssertions: Send + Sync { fn hold(&self, kind: AssertionKind, reason: &str) -> Result<AssertionGuard, PowerError>; }`; `pub struct AssertionGuard` (manual `Debug`; holds `Option<Box<dyn FnOnce() -> Result<(), PowerError> + Send>>`) with `pub fn new(release: impl FnOnce() -> Result<(), PowerError> + Send + 'static) -> AssertionGuard`, `pub fn release(mut self) -> Result<(), PowerError>`, `Drop`; `pub enum PowerError { Refused { kind: AssertionKind, code: i32 }, ReleaseFailed { code: i32 } }` with `#[doc(hidden)] pub fn from_create(kind: AssertionKind, code: i32) -> Option<PowerError>`; `pub struct MacPower;` (`impl PowerAssertions`); `navori_macos::fake::FakePower` (`PowerAssertions`) — `new()`, `refuse(&self, code: i32)`, `held(&self) -> Vec<(AssertionKind, String)>` (currently held), `releases(&self) -> usize` (total releases performed)

**Depends-on:** Task 1

- [ ] **Step 1:** Write `tests/power.rs`: the lifecycle probe `a_held_assertion_is_listed_by_pmset_until_released` (reason `format!("navori-test system {}", std::process::id())`; while held, a line of `pmset -g assertions` contains both `PreventUserIdleSystemSleep` and the reason; after `release()` no line contains the reason; then the same for `Display` / `PreventUserIdleDisplaySleep`); `dropping_a_guard_releases_its_assertion`; `a_refused_create_is_an_error_carrying_the_code` (`from_create(AssertionKind::System, 0)` → `None`; `from_create(AssertionKind::System, -536870206)` → `Some(PowerError::Refused { kind: AssertionKind::System, code: -536870206 })`); `the_fake_releases_each_guard_exactly_once` (`FakePower`: hold → `held().len() == 1`; `release()` then the guard is gone → `releases() == 1`; hold and drop → `releases() == 2`); `a_refusing_fake_returns_the_code` (`refuse(5)` → `Err(Refused { code: 5, .. })`). Run `cargo nextest run -p navori-macos --test power` — expect: compile failure.
- [ ] **Step 2:** Implement `power.rs` per design (`CFString::from_static_str("PreventUserIdleSystemSleep" | "PreventUserIdleDisplaySleep")`, `CFString::from_str(reason)`, `IOPMAssertionCreateWithName(Some(&ty), kIOPMAssertionLevelOn, Some(&name), &mut id)` in an `unsafe` block with `// SAFETY:`, `from_create`, guard `Option::take` in `release` and `Drop`, `Drop` logs a failed release with `tracing::warn!(code, "power assertion release failed")`) and `FakePower`. Run — expect: pass.
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `feat(macos): hold and release IOKit power assertions behind a guard`.

_Requirements: PROCS-6.1, PROCS-6.2, PROCS-6.3, PROCS-6.4, PROCS-6.5_

---

### Task 13: Start no thread or timer without a watcher

**Files:**
- Test: `crates/procs/tests/no_threads.rs`

**Reuse:** rung 4 — kqueue `EVFILT_PROC`/`NOTE_EXIT`, `EVFILT_USER`, `EV_RECEIPT` (platform feature) through `libc`; new code (rung 7) for the thread and the procs-side watcher

**Interfaces:**
- Consumes: `MacProcesses` (Task 3), `Snapshot` (Task 5), `sessions` (Task 7), `MacPower`, `AssertionKind`, `PowerAssertions` (Task 12)
- Produces: none

**Depends-on:** Task 7, Task 12

- [ ] **Step 1:** Write `tests/no_threads.rs` with exactly one `#[test]` `taking_a_snapshot_resolving_sessions_and_holding_an_assertion_starts_no_thread`: thread count = lines of `ps -M -p <std::process::id()>` output minus the header line; count, run `Snapshot::take(Arc::new(MacProcesses::new()?))`, `sessions`, `MacPower.hold(AssertionKind::System, "navori-test threads")` and `release()`, count again, assert equal. To see it fail first, add a temporary `std::thread::spawn(|| std::thread::park())` before the second count, run `cargo nextest run -p navori-procs --test no_threads` — expect: assertion failure; remove it.
- [ ] **Step 2:** Run without the temporary thread — expect: pass. If a thread appears that `navori-macos`/`navori-procs` created, remove its source; if the extra thread is IOKit's own, stop and hand to `reroute-plan` (PROCS-8.1 would need re-wording).
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `test(procs): prove no thread starts without a watcher`.

_Requirements: PROCS-8.1_

---

### Task 14: Measure snapshot cost and list sessions on this Mac

**Files:**
- Test: `crates/procs/tests/real_os.rs`

**Reuse:** none — new code (rung 7); no process model exists in the repo

**Interfaces:**
- Consumes: `MacProcesses` (Task 3), `Snapshot` (Task 5), `sessions` (Task 7)
- Produces: none

**Depends-on:** Task 7

- [ ] **Step 1:** Write `tests/real_os.rs`: `a_snapshot_with_session_resolution_takes_under_ten_milliseconds` (5 runs of `Snapshot::take` + `sessions` on `MacProcesses`; median < 10 ms; print the median and the process count); `#[ignore]` `the_running_agent_sessions_on_this_mac_are_listed` (prints each session as `<harness> · <project>/<worktree or branch> · pid <pid> · <terminal or unknown>`; asserts at least one session). Run `cargo nextest run -p navori-procs --test real_os` — expect: pass (debug). Run `cargo test --release -p navori-procs --test real_os -- --nocapture` and `cargo test -p navori-procs --test real_os -- --ignored --nocapture` with Claude Code, Codex and Grok open; paste the release median and the session lines into the task report.
- [ ] **Step 2:** If the debug median is ≥ 10 ms, profile `take` (the exe pass dominates per spike Q1) and fix. Run — expect: pass.
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `test(procs): bound snapshot cost and list real sessions on this Mac`.

_Requirements: PROCS-8.4_

---

### Task 15: Keep argv, environment and paths out of errors and logs

**Files:**
- Test: `crates/procs/tests/no_leaks.rs`
- Modify: `crates/macos/src/error.rs`, `crates/procs/src/error.rs` (only if the test finds a leak)

**Reuse:** rung 2 — the repo's convention: `thiserror` enums with named struct variants and `tracing::warn!(error = %e, …)` (`docs/standards/errors-logging.md`)

**Interfaces:**
- Consumes: `FakeProcesses` (Task 5), `Snapshot`, `sessions`, `harness_of`, `activity` (Tasks 5, 7, 8, 9), `ProcWatcher`, `FakeExitsHandle` (Task 11), `FakePower`, `AssertionGuard`, `PowerError` (Task 12)
- Produces: none

**Depends-on:** Task 8, Task 9, Task 11, Task 12

- [ ] **Step 1:** Write `tests/no_leaks.rs`: a fake process with argv `["node", "/x/@anthropic-ai/claude-code/cli.js", concat!("--token=ghp_", "FAKEFAKEFAKEFAKEFAKEFAKEFAKEFAKEFAKE00")]`, cwd `/Users/u/secret-project`, exe `/Users/u/bin/node`; capture `tracing` at `TRACE` with `tracing_subscriber::fmt().with_writer(<Arc<Mutex<Vec<u8>>> writer>)` as the default subscriber for the test; exercise `take`, `sessions`, `harness_of`, `activity`, every `Unavailable` path (exit, replace, denied), a `ProcWatcher` kqueue-failure path and an `AssertionGuard::new(|| Err(PowerError::ReleaseFailed { code: 1 }))` dropped unreleased; assert neither the captured log nor `format!("{e}")` / `format!("{e:?}")` of every returned error contains `ghp_`, `secret-project` or `/Users/u/bin`. Run `cargo nextest run -p navori-procs --test no_leaks` — expect: pass, or a failure naming the leaking text.
- [ ] **Step 2:** Fix any leaking variant or log field. Run — expect: pass.
- [ ] **Step 3:** `cargo xtask ci` — expect: pass. Commit `test(procs): prove argv, environment and paths never reach errors or logs`.

_Requirements: PROCS-8.5_

---

### Task 16: Update the architecture and codebase docs

**Files:**
- Modify: `docs/architecture/system.md` (Platform FFI and Process model rows, currently lines 61–62), `docs/architecture/INDEX.md` (ARCH-9 enforcement row, currently line 53), `docs/codebase/map.md` (crate rows, lines 34–35), `docs/codebase/modules.md` (crate rows, lines 32–33), `docs/codebase/dependencies.md` (Planned rows, lines 27–35; FFI row, line 63), `docs/ARCHITECTURE.md` (process-model seed, lines 272–278; Awake seed "Harness identification" bullet, lines 300–301)

**Reuse:** n/a — edits existing docs

**Interfaces:**
- Consumes: the built crate names, traits and rules from Tasks 1–12
- Produces: none

**Depends-on:** Task 2, Task 7, Task 8, Task 9, Task 11, Task 12

- [ ] **Step 1:** Apply the edits listed in design "Documentation updates".
- [ ] **Step 2:** Check: `rg -n "libproc" docs/codebase/dependencies.md docs/ARCHITECTURE.md` — expect: no match; `rg -n "walking up the process tree" docs/ARCHITECTURE.md` — expect: no match; `rg -n "navori-macos" docs/architecture/INDEX.md` — expect: the ARCH-9 enforcement row. `cargo xtask ci` — expect: pass.
- [ ] **Step 3:** Commit `docs(architecture): record the built platform layer and process model`.

_Requirements: none — infrastructure (design.md "Documentation updates" carries no requirement)_
