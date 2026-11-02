# Tasks: Performance gate

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: PERF
Status: In-progress
Date: 2026-11-01
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

**Goal:** `cargo xtask perf [--record]` measures release code against the GOAL-7 budget and a recorded baseline and gives one pass/fail verdict, without touching the daily app.

**Architecture:** `xtask` gains an identity × flavour split for bundling and a `perf` module (judge, baseline, log-stream reader, perf home, launch guard, counter probe, runner) plus a compiled Swift counter helper. The Swift shell gains a `PerfProbe` that exists only in perf mode (`NAVORI_PERF=1` + `NAVORI_PERF_HOME`) and writes `.notice` lines (subsystem `dev.navori`, category `perf`).

**Tech Stack:** Rust 2024 (`xtask` bin crate; `anyhow`, `clap`, `serde_json` — no serde derive, use `serde_json::Value`; dev-dep `tempfile`), in-module tests run with `cargo test -p xtask perf::<module>`; Swift 6 AppKit (`os.Logger`, `DispatchSource`, `CATransaction`, `NSView.displayLink`); Darwin libproc via a `swiftc -O` helper; `/usr/bin/log stream`, `/bin/kill`, `/usr/bin/id -u`, `/bin/ps`.

## Global Constraints

- Standards SSOT: `docs/standards/INDEX.md` (sha1 `1dabbefb2951`), `docs/standards/testing.md` (`af8d6561e1c5`), `docs/standards/errors-logging.md` (`03217271c9a2`), `docs/standards/performance.md` (`ad01248c8211`). Gate before every commit: `cargo xtask ci` (clippy `-D warnings`; `unwrap`/`expect` allowed only in tests per `clippy.toml`). Conventional Commits, English, one logical change per commit.
- Invariants (`docs/architecture/INDEX.md`): ARCH-2, ARCH-9 (no `unsafe` in Rust — xtask included), ARCH-11, ARCH-14, ARCH-15, ARCH-16.
- Design decisions 1–11 in `./design.md` are binding: signals via `/bin/kill`; no process matched by name; every wait times out after 10 s; MB = MiB; perf mode needs both env vars; the uid comes from `/usr/bin/id -u`.
- Until Task 10 wires `Cmd::Perf`, `xtask/src/perf/mod.rs` starts with `#![allow(dead_code)]` so `ci` stays green; Task 10 removes it.
- Perf code never calls `quit_app`, `install` or `open`, and never writes outside `build/perf/` and `/tmp/navori-perf-<uid>/`. A failing verdict is returned as `Ok(false)` from `perf::run`, and `main` turns it into an `anyhow` error (exit 1) so drop guards run; never `process::exit`.
- Team: Solo (`docs/agents/project.md` `## Team`).

## File Structure

| File | Responsibility |
|---|---|
| `xtask/src/main.rs` (modify) | `Flavor` split from `Profile`; `bundle(root, profile, flavor, out_dir)`; `mod perf;`; `Cmd::Perf { record }` |
| `xtask/Cargo.toml` (modify) | dev-dependency `tempfile` |
| `xtask/src/perf/mod.rs` (create) | module list; `run(root, record)` orchestration |
| `xtask/src/perf/judge.rs` (create) | `METRICS`, `median`, `judge` → `Verdict` |
| `xtask/src/perf/baseline.rs` (create) | `Machine`, `Baseline` (types in Task 2; IO and regression in Task 3) |
| `xtask/src/perf/logstream.rs` (create) | `Mark`, `parse_line`, `LogStream` (child `log stream`, wait-for-mark with timeout) |
| `xtask/src/perf/home.rs` (create) | `PerfHome` prepare / template / fresh runs, socket-length check, pid file + stale sweep |
| `xtask/src/perf/probe.rs` (create) | compile + call the counter helper; `Sample` |
| `xtask/src/perf/launch.rs` (create) | `Launched` guard: exec app, `/bin/kill` signals, wait-exit with timeout, SIGKILL on drop |
| `xtask/assets/perf-probe.swift` (create) | counter helper: `proc_pid_rusage` + `proc_pidinfo` → one JSON line |
| `apps/macos/Navori/Platform/PerfProbe.swift` (create) | perf-mode signal handlers and `.notice` marks |
| `apps/macos/Navori/App/AppEnvironment.swift` (modify) | `perfMode`, `perfHome`, `coreHome` |
| `apps/macos/Navori/App/AppDelegate.swift` (modify) | call `PerfProbe.start(statusBar:store:)` after the status bar exists |
| `apps/macos/Navori/Core/CoreStore.swift` (modify) | pass `AppEnvironment.coreHome`; `moduleCount` (modules in state `running`) |
| `apps/macos/Navori/StatusBar/StatusBarController.swift` (modify) | first-draw mark hook; `openMenuFresh(trigger:)` |
| `apps/macos/Navori/StatusBar/StatusPanel.swift` (modify) | `show(relativeTo:onFirstFrame:)` |
| `docs/standards/performance.md`, `docs/standards/testing.md`, `README.md`, `AGENTS.md`, `docs/agents/project.md` (modify) | method, testing exemption, commands, release step |
| `perf-baseline.json` (create, by `--record`) | recorded medians and floors for this Mac |

---

### Task 1: Split build flavour from Navori Profile

**Files:**
- Modify: `xtask/src/main.rs`  # Profile at lines 50–93; `--release` checks at 173 and 246; bundle at 241–311; build_core callers at 99, 242, 679
- Test: none — behaviour-preserving refactor of build tooling (exempt per `docs/standards/testing.md`), verified by the existing gates in Step 3

**Reuse:** rung 2 — extends `Profile`, `build_core`, the Swift build and `bundle` in `xtask/src/main.rs`

**Interfaces:**
- Consumes: —
- Produces: `enum Flavor { Debug, Release }` with `cargo_dir()`, `swift_config()`; `Profile::default_flavor() -> Flavor`; `build_core(root: &Path, flavor: Flavor) -> Result<()>`; `bundle(root: &Path, profile: Profile, flavor: Flavor, out_dir: &Path) -> Result<PathBuf>`

**Depends-on:** none

- [x] **Step 1: Capture the before-state.** `cargo xtask bundle --profile dev` and `--profile release`; save `plutil -p` of both `Contents/Info.plist` and `codesign -dvv` output to `/tmp/perf-t1-before/`.
- [x] **Step 2: Implement the split.** Move `cargo_dir`/`swift_config` to `Flavor`; replace both `profile == Profile::Release` checks with the flavour; existing callers pass `profile.default_flavor()` and `root.join("build").join(profile.name())`.
- [x] **Step 3: Prove nothing changed.** Rebundle both profiles and `diff` against Step 1 (only `CFBundleVersion` may differ); `cargo xtask ci` → `✓ ci passed`; `cargo xtask install`, then `/Applications/Navori.app/Contents/Helpers/navori doctor` → every check ✓.
- [x] **Step 4: Commit** — `git commit -m "refactor(xtask): split build flavour from Navori profile"`

_Requirements: PERF-5.1, PERF-5.2, PERF-5.6_

---

### Task 2: Judge measurements against budgets and the skip list

**Files:**
- Create: `xtask/src/perf/mod.rs` (`#![allow(dead_code)]` + `mod` lines), `xtask/src/perf/judge.rs`, `xtask/src/perf/baseline.rs` (types only: `Machine`, `Baseline`)
- Modify: `xtask/src/main.rs` (`mod perf;`), `xtask/Cargo.toml` (`[dev-dependencies] tempfile.workspace = true`)
- Test: `xtask/src/perf/judge.rs` (`#[cfg(test)] mod tests`)

**Reuse:** rung 5 — `serde_json` (already an xtask dependency) for `perf-baseline.json`; the rest is new code (rung 7)

**Interfaces:**
- Consumes: —
- Produces: `enum Kind { Gated { budget: f64 }, BaselineOnly, Skipped { reason: &'static str } }`; `struct MetricSpec { name: &'static str, unit: &'static str, kind: Kind, floor: f64 }`; `const METRICS: &[MetricSpec]` (13 entries: the 8 gated of PERF-1.2, the 4 baseline-only of PERF-1.3, `submenu_open_ms` skipped "no submenu until the A5 menu"; floors per PERF-2.5); `type Measurements = BTreeMap<String, f64>` (metric names plus the informational key `modules_running`); `fn median(xs: &[f64]) -> Option<f64>`; `struct Verdict { lines: Vec<String>, warnings: Vec<String>, passed: bool }`; `struct Machine { model: String, macos: String }`; `struct Baseline { machine: Machine, revision: String, recorded: String, metrics: BTreeMap<String, (f64, f64)> }` (median, floor); `fn judge(m: &Measurements, baseline: Option<&Baseline>, machine: &Machine) -> Verdict` (this task implements budgets, skips and missing lines; regression lands in Task 3)

**Depends-on:** Task 1

- [x] **Step 1: Failing tests:** `median_of_odd_and_even_counts` (`[3,1,2]`→2, `[4,1,3,2]`→2.5, `[]`→None); `all_within_budget_passes`; `over_budget_fails_naming_metric_value_and_budget` (`cold_start_ms` 201 → line contains `cold_start_ms`, `201`, `200`); `idle_cpu_rounds_to_one_decimal` (0.04 passes, 0.05 fails — `(x * 10.0).round() / 10.0`); `skipped_metric_prints_reason_and_does_not_fail`; `missing_enabled_metric_fails` (no `menu_reopen_ms` → `no line for an enabled metric: menu_reopen_ms`); `baseline_only_metric_is_labelled_and_never_budget_fails`; `menu_open_footprint_line_shows_running_modules` (`modules_running` 1 → line contains `modules: 1`). Run `cargo test -p xtask perf::judge` — expect failures.
- [x] **Step 2: Implement** `METRICS`, `median`, the type definitions and `judge` (budget part). Run `cargo test -p xtask perf::judge` — expect pass.
- [x] **Step 3: Gate** — `cargo xtask ci` → `✓ ci passed`.
- [x] **Step 4: Commit** — `git commit -m "feat(xtask): judge perf metrics against the GOAL-7 budget"`

_Requirements: PERF-1.2, PERF-1.3, PERF-1.4, PERF-1.7, PERF-1.8, PERF-1.9_

---

### Task 3: Baseline file and regression rule

**Files:**
- Modify: `xtask/src/perf/baseline.rs`, `xtask/src/perf/judge.rs`
- Test: `xtask/src/perf/baseline.rs`, `xtask/src/perf/judge.rs` (`#[cfg(test)]`)

**Reuse:** rung 5 — `serde_json` (already an xtask dependency) for `perf-baseline.json`; the rest is new code (rung 7)

**Interfaces:**
- Consumes: `Measurements`, `METRICS`, `Verdict`, `judge`, `Machine`, `Baseline` (Task 2)
- Produces: `Machine::current() -> Result<Machine>` (`sysctl -n hw.model`, `sw_vers -productVersion`); `Baseline::load(path: &Path) -> Result<Option<Baseline>>`; `Baseline::from_run(m: &Measurements, machine: Machine, revision: String, recorded: String) -> Baseline`; `Baseline::save(&self, path: &Path) -> Result<()>`; `fn finish(path: &Path, verdict: &Verdict, record: bool, b: &Baseline) -> Result<bool>` (writes only when `record && verdict.passed`; returns whether it wrote)

**Depends-on:** Task 2

- [x] **Step 1: Failing tests:** `regression_beyond_ten_percent_fails` (baseline 100, floor 10 → 110 passes, 111 fails); `floor_wins_for_small_values` (baseline 20, floor 5 → 25 passes, 25.1 fails); `zero_baseline_uses_floor` (baseline 0, floor 0.2 → 0.2 passes, 0.3 fails); `other_machine_or_macos_checks_budget_only_with_warning`; `missing_baseline_warns_and_checks_budget_only`; `baseline_round_trips_through_json` (temp dir); `record_writes_only_when_verdict_passed` (failed verdict + `record` → existing file byte-identical); `without_record_never_writes` (passed verdict, `record` false → file absent). Run `cargo test -p xtask perf::` — expect failures.
- [x] **Step 2: Implement** regression in `judge`, `Machine::current`, `Baseline::load/from_run/save`, `finish`. Run `cargo test -p xtask perf::` — expect pass.
- [x] **Step 3: Gate** — `cargo xtask ci` → `✓ ci passed`.
- [x] **Step 4: Commit** — `git commit -m "feat(xtask): record a perf baseline and fail on regressions"`

_Requirements: PERF-2.1, PERF-2.2, PERF-2.3, PERF-2.4, PERF-2.5, PERF-2.6_

---

### Task 4: Read perf marks from the unified log

**Files:**
- Create: `xtask/src/perf/logstream.rs`
- Modify: `xtask/src/perf/mod.rs` (`mod logstream;`)
- Test: `xtask/src/perf/logstream.rs` (`#[cfg(test)]`)

**Reuse:** none — new code (rung 7); nothing in the repo launches the app, samples it or reads the unified log

**Interfaces:**
- Consumes: —
- Produces: `enum Mark { StatusItemDrawn { ms: f64 }, MenuOpened { ms: f64, first: bool, modules: u32 } }`; `fn parse_line(line: &str) -> Option<(u32, Mark)>` (pid from `processID`, text from `eventMessage`); `struct LogStream`; `LogStream::from_reader(r: impl BufRead + Send + 'static) -> LogStream` (reader thread → channel); `LogStream::start() -> Result<LogStream>` (spawns `/usr/bin/log stream --predicate 'subsystem == "dev.navori" AND category == "perf"' --style ndjson --level default` and returns only after its first output line, the banner); `fn wait_for(&mut self, pid: u32, want: fn(&Mark) -> bool, timeout: Duration) -> Option<Mark>`

**Depends-on:** Task 2

- [x] **Step 1: Failing tests** with literal ndjson lines shaped like `.skills/PERF/scan.md` §9: `parses_status_item_drawn` (`statusItemDrawn ms=142.5`); `parses_menu_opened` (`menuOpened ms=31.2 first=true modules=1`); `ignores_banner_and_finished_lines` (`Filtering the log data using …`, `{"count":3,"finished":1}`); `ignores_malformed_messages`; `wait_for_skips_marks_of_other_pids` (from_reader with pid 7 then pid 9 → `wait_for(9, …)` returns the pid-9 mark); `wait_for_times_out_with_none` (empty reader, 50 ms). Run `cargo test -p xtask perf::logstream` — expect failures.
- [x] **Step 2: Implement** `parse_line` (`serde_json::Value`), `LogStream::from_reader`, `start`, `wait_for`. Run tests — expect pass.
- [x] **Step 3: Gate** — `cargo xtask ci` → `✓ ci passed`.
- [x] **Step 4: Commit** — `git commit -m "feat(xtask): read perf marks from the unified log"`

_Requirements: PERF-4.6, PERF-1.9_

---

### Task 5: Private perf home and stale-process sweep

**Files:**
- Create: `xtask/src/perf/home.rs`
- Modify: `xtask/src/perf/mod.rs` (`mod home;`)
- Test: `xtask/src/perf/home.rs` (`#[cfg(test)]`, temp dirs via `tempfile::Builder::new().tempdir_in("/tmp")`)

**Reuse:** none — new code (rung 7); nothing in the repo launches the app, samples it or reads the unified log

**Interfaces:**
- Consumes: —
- Produces: `fn current_uid() -> Result<u32>` (`/usr/bin/id -u`); `struct PerfHome { root: PathBuf }`; `PerfHome::prepare(base: &Path, uid: u32) -> Result<PerfHome>` (root = `base/navori-perf-<uid>`, mode 0700; an existing root is removed only when `symlink_metadata` shows a real directory owned by `uid`); `fn template(&self) -> PathBuf`; `fn fresh_run(&self, n: u32) -> Result<PathBuf>` (recursive copy of `template/`); `fn check_socket_len(home: &Path) -> Result<()>` (`home/Library/Application Support/Navori Dev/run/navori.sock` ≤ 103 bytes); `fn record_pid(&self, pid: u32) -> Result<()>`; `fn sweep_stale(&self, exe: &Path) -> Result<Vec<u32>>` (kills, via `/bin/kill -KILL`, only pids in `pids` whose `ps -o comm= -p <pid>` equals `exe`)

**Depends-on:** Task 2

- [x] **Step 1: Failing tests:** `creates_root_owned_by_user_with_mode_0700`; `replaces_a_stale_tree_it_owns`; `refuses_a_symlinked_root` (target untouched); `fresh_run_copies_template_contents`; `rejects_a_home_whose_socket_path_exceeds_103_bytes`; `sweep_kills_only_recorded_pids_running_the_given_exe` (spawn `/bin/sleep 30` twice by absolute path, record one → `sweep_stale(Path::new("/bin/sleep"))` kills the recorded one only; a recorded pid running another exe survives). The foreign-owner branch cannot be built without root; it is covered by review. Run `cargo test -p xtask perf::home` — expect failures.
- [x] **Step 2: Implement.** Run tests — expect pass.
- [x] **Step 3: Gate** — `cargo xtask ci` → `✓ ci passed`.
- [x] **Step 4: Commit** — `git commit -m "feat(xtask): prepare a private perf home under /tmp"`

_Requirements: PERF-3.3, PERF-3.5, PERF-6.3, PERF-6.5_

---

### Task 6: Process counter helper

**Files:**
- Create: `xtask/assets/perf-probe.swift`, `xtask/src/perf/probe.rs`
- Modify: `xtask/src/perf/mod.rs` (`mod probe;`)
- Test: `xtask/src/perf/probe.rs` (`#[cfg(test)]` — parsing); helper checked manually in Step 3

**Reuse:** rung 4 — `proc_pid_rusage(RUSAGE_INFO_V4)` and `proc_pidinfo(PROC_PIDTASKINFO)` (Darwin libproc), called from Swift because Rust may not use `unsafe` here

**Interfaces:**
- Consumes: —
- Produces: helper CLI `perf-probe <pid>` → `{"t_ns":…,"footprint":…,"peak":…,"logical_writes":…,"cpu_ns":…,"idle_wakeups":…,"threads":…}`; `struct Sample { t_ns: u64, footprint: u64, peak: u64, logical_writes: u64, cpu_ns: u64, idle_wakeups: u64, threads: u32 }`; `fn parse_sample(s: &str) -> Result<Sample>`; `struct Probe { bin: PathBuf }`; `Probe::compile(root: &Path, out_dir: &Path) -> Result<Probe>` (`swiftc -O`); `fn sample(&self, pid: u32) -> Result<Sample>`

**Depends-on:** Task 2

- [x] **Step 1: Failing tests** `parses_a_helper_line` (literal JSON → exact field values) and `rejects_a_line_missing_a_field`. Run `cargo test -p xtask perf::probe` — expect failures.
- [x] **Step 2: Implement** the Swift helper (CPU ns via `mach_timebase_info`, `ri_pkg_idle_wkups`, `ri_lifetime_max_phys_footprint`, `pti_threadnum`, `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)`) and `probe.rs`. Run tests — expect pass.
- [x] **Step 3: Manual check** against the running daily app (read-only): `swiftc -O xtask/assets/perf-probe.swift -o /tmp/perf-probe && /tmp/perf-probe $(pgrep -f /Applications/Navori.app/Contents/MacOS/Navori)`; `footprint` equals `footprint -j` `phys_footprint`; `threads` equals `/usr/bin/top -l 1 -pid <pid> -stats th`.
- [x] **Step 4: Gate and commit** — `cargo xtask ci` → `✓ ci passed`; `git commit -m "feat(xtask): add a libproc counter helper for perf sampling"`

_Requirements: PERF-1.3, PERF-6.2_

---

### Task 7: Shell perf probe

**Files:**
- Create: `apps/macos/Navori/Platform/PerfProbe.swift`
- Modify: `apps/macos/Navori/App/AppEnvironment.swift`, `apps/macos/Navori/App/AppDelegate.swift`, `apps/macos/Navori/Core/CoreStore.swift` (home at line 32; add `moduleCount`), `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/Navori/StatusBar/StatusPanel.swift` (`show` at lines 57–74)
- Test: none — Swift AppKit layer is exempt (`docs/standards/testing.md`); verified manually in Steps 3–4

**Reuse:** rung 4 — `os.Logger`, `DispatchSource.makeSignalSource`, `sysctl(KERN_PROC_PID)`, `CATransaction` completion; the existing `Logger(subsystem: "dev.navori")` pattern (`AppDelegate.swift:9`)

**Interfaces:**
- Consumes: —
- Produces: `AppEnvironment.perfMode: Bool`, `AppEnvironment.perfHome: String?`, `AppEnvironment.coreHome: String`; `CoreStore.moduleCount: Int` (modules in state `running`); `StatusPanel.show(relativeTo:onFirstFrame:)` (`onFirstFrame` defaults to nil); `StatusBarController.openMenuFresh(trigger: UInt64)`; `enum PerfProbe { static func start(statusBar: StatusBarController, store: CoreStore); static func markStatusItemDrawn(); static func markMenuOpened(since: UInt64, first: Bool, modules: Int) }`; log lines `statusItemDrawn ms=<%.1f>` and `menuOpened ms=<%.1f> first=<true|false> modules=<n>` at `.notice`, subsystem `dev.navori`, category `perf`

**Depends-on:** none

- [x] **Step 1: Implement** per design § Shell perf probe (env parsing; `CoreStore.start` uses `coreHome`; first-draw mark; `openMenuFresh` closes, lets the close finish, takes the trigger, shows with completion; SIGUSR1/SIGTERM sources only in perf mode; fault + terminate when `NAVORI_PERF=1` lacks `NAVORI_PERF_HOME`).
- [x] **Step 2: Gate** — `cargo xtask ci` → `✓ ci passed`.
- [x] **Step 3: Perf-mode check:** `cargo xtask bundle --profile dev`; `mkdir -m 700 /tmp/navori-perf-manual`; start `/usr/bin/log stream --predicate 'subsystem == "dev.navori" AND category == "perf"' --style ndjson` in one shell; run `NAVORI_PERF=1 NAVORI_PERF_HOME=/tmp/navori-perf-manual "$PWD/build/dev/Navori Dev.app/Contents/MacOS/Navori" &`; expect `statusItemDrawn`; `/bin/kill -USR1 <pid>` twice → `menuOpened … first=true` then `first=false`; Core data appears under `/tmp/navori-perf-manual/Library/`; `/bin/kill -TERM <pid>` → exits, no `session.marker` left.
- [x] **Step 4: Normal-mode check:** `cargo xtask run`; left click toggles the panel; right and control click show the context menu; a normal-mode instance receiving `/bin/kill -USR1` ends (default action); `NAVORI_PERF=1` without `NAVORI_PERF_HOME` → the app quits after a fault line.
- [x] **Step 5: Commit** — `git commit -m "feat(shell): add a perf-mode probe for launch and menu timings"`

_Requirements: PERF-4.1, PERF-4.2, PERF-4.3, PERF-4.4, PERF-4.5, PERF-5.3, PERF-5.4, PERF-5.5_

---

### Task 8: Calibrate the in-process marks (spike)

**Files:**
- Modify (only if the marks must move): `apps/macos/Navori/Platform/PerfProbe.swift`, `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/Navori/StatusBar/StatusPanel.swift`
- Test: none — manual spike; results recorded in `.skills/PERF/calibration.md`

**Reuse:** rung 4 — `os.Logger`, `DispatchSource.makeSignalSource`, `sysctl(KERN_PROC_PID)`, `CATransaction` completion; the existing `Logger(subsystem: "dev.navori")` pattern (`AppDelegate.swift:9`)

**Interfaces:**
- Consumes: `PerfProbe`, `openMenuFresh(trigger:)`, `show(relativeTo:onFirstFrame:)` (Task 7)
- Produces: confirmation that the marks are within 5 ms of the first presented frame, or marks moved to a one-shot `NSView.displayLink` callback (same log line shapes)

**Depends-on:** Task 7

- [x] **Step 1: Instrument temporarily** (not committed): one-shot `NSView.displayLink(target:selector:)` on the status button and on the panel content view, logging the first callback time beside each mark.
- [x] **Step 2: Run 10 launches and 10 SIGUSR1 opens** in perf mode; record each pair's difference and whether the panel stays visible after SIGUSR1.
- [x] **Step 3: Decide.** All differences ≤ 5 ms → keep the marks. Otherwise move the marks to the display-link callback and repeat Step 2. If the panel does not stay visible after SIGUSR1, stop and raise it with the user — the menu metrics cannot be measured as designed.
- [x] **Step 4: Record and commit** — write `.skills/PERF/calibration.md`; if code changed, `cargo xtask ci` then `git commit -m "fix(shell): align perf marks with the first presented frame"`.

_Requirements: PERF-4.1, PERF-4.2, PERF-4.3_

---

### Task 9: Launch guard

**Files:**
- Create: `xtask/src/perf/launch.rs`
- Modify: `xtask/src/perf/mod.rs` (`mod launch;`)
- Test: `xtask/src/perf/launch.rs` (`#[cfg(test)]`, using `/bin/sleep` as the launched executable)

**Reuse:** none — new code (rung 7); nothing in the repo launches the app, samples it or reads the unified log

**Interfaces:**
- Consumes: `PerfHome::record_pid` (Task 5)
- Produces: `struct Launched { pid: u32 }`; `Launched::spawn(exe: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<Launched>`; `fn signal(&self, sig: &str) -> Result<()>` (`/bin/kill -<sig> <pid>`); `fn wait_exit(&mut self, timeout: Duration) -> Result<bool>`; `Drop` → `/bin/kill -KILL` if still alive

**Depends-on:** Task 5

- [x] **Step 1: Failing tests:** `wait_exit_times_out_while_the_process_runs` (`/bin/sleep 30`, 100 ms → false); `term_signal_ends_the_process` (signal `TERM` → `wait_exit(2 s)` true); `dropping_the_guard_kills_a_live_process` (pid gone after drop, checked with `/bin/kill -0`). Run `cargo test -p xtask perf::launch` — expect failures.
- [x] **Step 2: Implement.** Run tests — expect pass.
- [x] **Step 3: Gate and commit** — `cargo xtask ci` → `✓ ci passed`; `git commit -m "feat(xtask): add a launch guard for perf runs"`

_Requirements: PERF-3.4, PERF-3.5_

---

### Task 10: Run the gate end to end

**Files:**
- Modify: `xtask/src/perf/mod.rs` (`run`; remove `#![allow(dead_code)]`), `xtask/src/main.rs` (`Cmd::Perf { record: bool }`; `if !perf::run(&root, record)? { bail!("perf gate failed") }`)
- Test: e2e manual (Steps 3–4); pure parts covered by Tasks 2–6 and 9

**Reuse:** none — new code (rung 7); nothing in the repo launches the app, samples it or reads the unified log

**Interfaces:**
- Consumes: `bundle(root, Profile::Dev, Flavor::Release, out_dir)` (Task 1); `METRICS`, `median`, `judge`, `Measurements` (Task 2); `Machine::current`, `Baseline::load/from_run`, `finish` (Task 3); `LogStream::start`, `wait_for`, `Mark` (Task 4); `current_uid`, `PerfHome` (Task 5); `Probe`, `Sample` (Task 6); perf-mode app (Tasks 7–8); `Launched` (Task 9)
- Produces: `perf::run(root: &Path, record: bool) -> Result<bool>`; `cargo xtask perf [--record]`. `menu_open_cpu_ms` = median over every open (first and reopens, all phase-A processes) of `(cpu_ns 250 ms after menuOpened − cpu_ns just before SIGUSR1) / 1e6`

**Depends-on:** Task 1, Task 3, Task 4, Task 6, Task 8, Task 9

- [x] **Step 1: Implement** `run` per design § xtask perf runner flow: build into `build/perf/` → sweep stale pids → `PerfHome::prepare(Path::new("/tmp"), uid)` → `LogStream::start()` (banner read) → warm-up (≤ 15 s exit, fail if `session.marker` remains) → phase A × 5 → phase B → `app_size_mb` → `judge` → print report and build/phase times → `finish(…, record, …)`.
- [x] **Step 2: Gate** — `cargo xtask ci` → `✓ ci passed` (no dead-code allowance left).
- [x] **Step 3: Full run** with `/Applications/Navori.app` and a `cargo xtask run` dev app both running: `cargo xtask perf`; expect 13 metric lines (8 gated, 4 baseline-only, 1 skipped) and printed phase times with measuring ≤ 3 min. During phase A, `lsof -p <perf pid>` lists no path under `~/Library/Application Support/Navori`, `…/Navori Dev`, `~/Library/Logs/Navori` or `…/Navori Dev`; both daily apps keep running with unchanged pids.
- [x] **Step 4: Failure paths.** Ctrl-C during phase A → `pgrep -fl build/perf` empty; `kill -9` the xtask process mid-run, rerun → the stale pid is swept first; an over-budget metric (expected today: `idle_footprint_mb`) → exit 1 naming metric, value and budget.
- [x] **Step 5: Commit** — `git commit -m "feat(xtask): add cargo xtask perf"`

_Requirements: PERF-1.1, PERF-1.5, PERF-1.6, PERF-1.10, PERF-3.1, PERF-3.2, PERF-6.1, PERF-6.2_

---

### Task 11: Document the gate

**Files:**
- Modify: `docs/standards/performance.md`, `docs/standards/testing.md`, `README.md`, `AGENTS.md`, `docs/agents/project.md`, `docs/architecture/INDEX.md`
- Test: none — documentation

**Reuse:** rung 2 — edits the existing standard

**Interfaces:**
- Consumes: `cargo xtask perf [--record]` (Task 10)
- Produces: documented command and method

**Depends-on:** Task 10

- [x] **Step 1: Edit** `performance.md` (release code packaged as the dev identity; `proc_pid_rusage` counters, equal to `footprint` and `top` IDLEW; 5 × 12 s windows; `--record`; floors rule; baseline-only metrics; skip list; the shared `dev.navori.app.debug` preferences domain; drop "to be built with the next release"); `testing.md` Exempt (xtask's perf logic is unit-tested); `README.md` and `AGENTS.md` command lists; `docs/agents/project.md` release steps (`cargo xtask perf` after `ci`).
- [x] **Step 2: Commit** — `git commit -m "docs: document cargo xtask perf"`

_Requirements: PERF-1.1_

---

### Task 12: Record the baseline and prove stability

**Files:**
- Create: `perf-baseline.json` (written by `cargo xtask perf --record`)
- Test: e2e manual

**Reuse:** none — new code (rung 7); nothing in the repo launches the app, samples it or reads the unified log

**Interfaces:**
- Consumes: `cargo xtask perf [--record]` (Task 10)
- Produces: committed `perf-baseline.json`

**Depends-on:** Task 10

- [ ] **Step 1: Record** — `cargo xtask perf --record`. On 2026-11-02 at `dab5a48` one `--record` run exited 0 (`menu_open_after_launch_ms` 49.2, `idle_footprint_mb` 18.7) and wrote the file. The file was then removed and not committed, because Step 2 did not hold. While `menu_open_after_launch_ms` is over 50, stop this task and report the miss (optimise first, or amend GOAL-7 through `define-project`); never loosen a budget here.
- [ ] **Step 2: Stability** — three consecutive `cargo xtask perf` runs on the same revision give the same verdict with no regression failure; if one regresses without a code change, report the metric and values and raise Open Question 2 (floors) instead of editing floors. On 2026-11-02 at `dab5a48` the three runs exited 1, 1, 0 with `menu_open_after_launch_ms` 51.8, 51.9, 49.4. No regression line was printed. Floors were not edited.
- [ ] **Step 3: Commit** — `git commit -m "chore: record the perf baseline"`

_Requirements: PERF-2.1, PERF-6.4_

---

### Task 13: Keep the build line first on stdout

**Files:**
- Modify: `xtask/src/main.rs` (`tool_stdout`, `sh`)
- Test: `xtask` unit test `tool_stdout_is_piped_so_report_stdout_stays_clean`

**Reuse:** rung 2 — changes the existing `sh` helper

**Interfaces:**
- Consumes: child tool stdout from `cargo` / `swift` / `xcodebuild` during `cargo xtask perf`
- Produces: report stdout whose first line is `build <seconds>s`, with tool progress on stderr; `phase A` and `phase B` lines unchanged

**Depends-on:** Task 10

- [x] **Step 1: Failing test, then pipe** — `tool_stdout()` returns a pipe; `sh` copies that pipe to stderr. `cargo test -p xtask tool_stdout_is_piped_so_report_stdout_stays_clean` passes (1 passed, 2026-11-01).
- [x] **Step 2: Full run** — `cargo xtask perf` stdout begins with `build <n.nnn>s` and still ends with `phase A <n.nnn>s` and `phase B <n.nnn>s`. Observed on 2026-11-02: record run `build 9.500s` / `phase A 10.748s` / `phase B 70.262s`, and three later runs the same shape. Tool progress stayed on stderr.
- [x] **Step 3: Commit** — `f8ee2c8` `fix(xtask): keep perf report stdout free of tool progress`

_Requirements: PERF-1.11, PERF-1.12_
