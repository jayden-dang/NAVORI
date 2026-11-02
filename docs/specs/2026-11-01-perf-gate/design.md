# Design: Performance gate

Feature code: PERF
Status: In-progress
Date: 2026-11-01
Requirements: ./requirements.md

## Context

Today the GOAL-7 budget exists only as a table in `docs/standards/performance.md`; nothing measures it. `xtask` is one 685-line `xtask/src/main.rs` with no modules and no tests, depending on `anyhow`, `clap`, `serde_json` and `toml`. Its `Profile` enum (`main.rs:50-93`) fuses two facets: *code flavour* (`cargo_dir`, `swift_config`, the `== Release` checks at `main.rs:173, 246`) and *identity* (`app_name`, `icon_file`, `bundle_id`, `name` — the last also names `build/<name>/` and writes `NavoriProfile`). The Swift shell logs through `os.Logger` (subsystem `dev.navori`, categories `app`, `core`), starts the Core synchronously in `applicationDidFinishLaunching` before creating the status item, and builds the panel lazily on first open (`StatusBarController.swift:10`, `StatusPanel.swift:19, 70`). It installs no signal handlers.

The binding constraint is the workspace lint `unsafe_code = "forbid"`, which applies to `xtask` too: the per-process counters the gate needs (`proc_pid_rusage`: footprint, peak footprint, logical writes, CPU time, idle wake-ups; thread count; process start time) cannot be read from Rust without `unsafe` or a new crate. Probing on this Mac (macOS 27.0.1, `Mac17,9`; `.skills/PERF/scan.md`) showed `proc_pid_rusage(RUSAGE_INFO_V4)` works for a same-user process without sudo and matches `footprint` and `top` exactly, and that a `swiftc -O`-compiled helper answers in under 10 ms (`swift script.swift` takes ~0.7 s per call). That rules out the alternative of shelling out to `footprint` + `/usr/bin/top` for every sample: two tools, two text formats, ~1.5 s per `top` sample, `top`'s memory column only in whole MiB, and still no source for disk writes. One small compiled Swift helper reads every counter from the platform API instead.

The second shaping fact is that both UI numbers must come from inside the app (frame-change decision 1), so the gate is two halves: a Shell-side perf probe that only exists in perf mode and only writes `.notice` log lines, and an `xtask perf` runner that builds, launches, signals, samples and judges. The probe found that `.notice` entries are persisted and readable without `--info`; `log show` costs 0.7–1.8 s per call, so the runner reads one `log stream` for the whole run instead of polling.

Neighbors (catalog `docs/specs/catalog/app.md`, OWNS coverage 0 of 5 cards — no `tasks.md` yet): `BUILD` owns `xtask/` (extended here), `SHELL` owns `apps/macos/` (small perf-mode-only additions), `CORE` owns `crates/` (unchanged — the home is passed through the existing `CoreConfig.home_dir`). No existing card measures performance.

Known before the first run: the installed release app idles at **42 MB** phys_footprint with 6 threads, above the 30 MB `idle_footprint_mb` budget. The gate will report that miss; fixing it is out of scope (requirements, Out of Scope).

Measured on this Mac (`Mac17,9`, macOS 27.0.1) at `dab5a48` on 2026-11-02: `idle_footprint_mb` was about 19 MB, inside the 30 MB budget. `menu_open_after_launch_ms` landed at 49.2, 51.8, 51.9 and 49.4 across four runs, and earlier samples on the same Mac reached 63.7. That spread made the 50 ms line fail some runs and pass others, so no baseline was committed. GOAL-11 then set the first-open budget to 80 ms. Later opens stay at 50 ms. `menu_reopen_ms` on those runs was about 8–11 ms.

## Decisions

1. **Split `Profile` into identity and flavour.** `Profile` keeps identity (name, bundle id, app name, icon, `NavoriProfile`); a new `Flavor { Debug, Release }` carries cargo/Swift build config. Existing commands pass `profile.default_flavor()`, so their output is unchanged; perf builds `(Profile::Dev, Flavor::Release)` into `build/perf/`.
2. **One compiled Swift helper reads process counters.** `xtask/assets/perf-probe.swift`, compiled once per run with `swiftc -O` into `build/perf/perf-probe`, prints one JSON object per pid from `proc_pid_rusage` + `proc_pidinfo(PROC_PIDTASKINFO)`. No `footprint`/`top` parsing; no `unsafe` in Rust (ARCH-9).
3. **The runner reads one `log stream` per run**, predicate `subsystem == "dev.navori" AND category == "perf"`, `--style ndjson`, skipping the banner line, and routes lines by `processID`.
4. **The probe logs at `.notice`** with fixed message shapes: `statusItemDrawn ms=<f64>` and `menuOpened ms=<f64> first=<bool>`.
5. **SIGUSR1 means "open fresh"**: if the panel is open it is closed first and the close is allowed to finish; only then is the trigger time taken and the panel shown, so `menu_reopen_ms` and `menu_open_after_launch_ms` measure the same interval (trigger → first frame). One signal per open, so reopens need no second signal kind.
6. **Signals go through `/bin/kill`; cleanup has three layers.** `std` can only SIGKILL a child and `unsafe` is forbidden, so the runner sends SIGUSR1/SIGTERM with `/bin/kill -USR1|-TERM <pid>` (no new dependency). Cleanup: (a) apps are launched by direct exec, so they share the terminal's foreground process group and Ctrl-C's SIGINT ends them by default action — xtask itself also dies by default action, so its drop guard does **not** run on Ctrl-C; (b) a drop guard SIGKILLs every launched pid on error returns and panics; (c) every launched pid is appended to `/tmp/navori-perf-<uid>/pids`, and the next run first kills any listed pid whose executable is still the perf bundle (checked with `ps -o comm= -p`), covering an xtask killed outright. Processes are never matched by name (`quit_app`'s `pgrep -f` would also match a running real Navori Dev).
7. **The perf home is `/tmp/navori-perf-<uid>/`** (0700, created with `DirBuilder::mode`; an existing path is removed only if `symlink_metadata` shows a real directory owned by the current uid, otherwise the run aborts): `template/` from a warm-up launch that must exit cleanly — it gets up to 15 s, because Core shutdown may take a 5 s module grace plus 1 s runtime — and must leave no `session.marker`; then `run-<n>/` is copied fresh for each measured process. The runner asserts the derived socket path is ≤ 103 bytes so it never falls back to the shared `/tmp/navori-<uid>/dev.sock`.
8. **Pure judging logic gets unit tests inside `xtask`** (new seam): medians, thresholds, skip list, baseline matching, log-line parsing and home preparation are tested without launching anything. This changes `docs/standards/testing.md:24` ("xtask is verified by use"), which is updated in the same change.
9. **Every wait has a timeout.** Waiting for a log line times out after 10 s (the metric then has no value → PERF-1.9 failure, not a hang — e.g. if `StatusPanel.show` returns early because the button has no window); a probe taken after `menuOpened` waits 250 ms first so the Core's 33 ms snapshot pump has settled.
10. **Units:** MB means MiB (1,048,576 bytes); idle wake-ups are `ri_pkg_idle_wkups` (what `top` shows as IDLEW), not interrupt wake-ups; `idle_cpu_pct` = CPU-time delta ÷ wall-time delta of the window, so 0.0 means under ~6 ms of CPU per 12 s window.
11. **Perf mode needs both variables.** With `NAVORI_PERF=1` but no `NAVORI_PERF_HOME`, the app logs a fault and quits instead of running against the real home.

No ADR: each decision is cheap to reverse inside `xtask`/the probe and none is surprising once this design is read (define-domain gate).

## Architecture

Vocabulary for every section below: **module**, **interface**, **implementation**, **seam** (public surface where behavior is observable and substitutable). Prefer deep modules — an interface much simpler than what it hides.

### xtask build: identity × flavour

Satisfies: PERF-1.1, PERF-5.1, PERF-5.2, PERF-5.6
Reuse: rung 2 — extends `Profile`, `build_core`, the Swift build and `bundle` in `xtask/src/main.rs`
Respects: ARCH-14
Surface:
- `build_core(root, profile)` callers — `Cmd::BuildCore`, `bundle`, `ci` (`main.rs:99, 242, 679`) — **replace**: becomes `build_core(root, flavor)` (the core build depends only on the flavour); callers pass `profile.default_flavor()`, so output is byte-for-byte the same build.
- `bundle(root, profile)` callers — `Cmd::Bundle`, `run_dev`, `install` — **replace**: take `(profile, flavor, out_dir)`; existing callers pass the default flavour and `build/<name>/`.
- `Profile::cargo_dir`, `Profile::swift_config` — **replace**: move to `Flavor`; no other readers (grep).
- The two `profile == Profile::Release` checks that add `--release` (`main.rs:173` core, `main.rs:246` CLI) — **replace**: test the flavour instead.
- Generated `Info.plist` — **frozen** for dev and release bundles: same keys and values (PERF-5.1); the perf bundle writes the dev values.
Interface: `enum Flavor { Debug, Release }` with `cargo_dir()`, `swift_config()`; `Profile::default_flavor()`; `bundle(root, profile, flavor, out_dir) -> Result<PathBuf>`.
Depth: n/a — extends `Profile`/`bundle` in `xtask/src/main.rs`.
Locality: `xtask/src/main.rs` only — extend; `ci`, `run`, `install` keep their behavior through the default flavour.

### xtask perf runner

Satisfies: PERF-1.5, PERF-1.6, PERF-1.10, PERF-1.11, PERF-1.12, PERF-3.1, PERF-3.2, PERF-3.3, PERF-3.4, PERF-3.5, PERF-4.6, PERF-6.1, PERF-6.3, PERF-6.5
Reuse: none — new code (rung 7); nothing in the repo launches the app, samples it or reads the unified log
Respects: ARCH-11, ARCH-14
Security: TB-2, THR-4
Interface: `cargo xtask perf [--record]` → exit status + printed report; internally `run(root, record) -> Result<Report>`.
Depth: if this module vanished, a caller would still only need "build the perf bundle, give me per-metric medians for phase A and phase B"; the process lifecycle, home copying, signal timing and log routing stay hidden.
Locality: new `xtask/src/perf/` (`mod.rs` orchestration, `home.rs`, `launch.rs`, `logstream.rs`); one `Cmd::Perf` arm in `main.rs`; `sh` in `main.rs` pipes tool stdout to stderr so the build line is first on stdout (PERF-1.11) — leave everything else.

Flow:
1. `bundle(root, Dev, Release, build/perf)`; compile `perf-probe` once; print build time.
2. Sweep stale pids from a previous run's `pids` file (decision 6c). `home::prepare(uid)`: replace any old `/tmp/navori-perf-<uid>/` (PERF-6.5, with the ownership/symlink check of decision 7), create it `0700` (PERF-6.3), check the socket path for `run-9/` ≤ 103 bytes.
3. Start `log stream` (child, killed by the drop guard).
4. Warm-up: exec `build/perf/Navori Dev.app/Contents/MacOS/Navori` with `NAVORI_PERF=1`, `NAVORI_PERF_HOME=<root>/template`; wait for its `statusItemDrawn`; SIGTERM; wait ≤ 15 s for exit; fail if `template/…/session.marker` exists (an unclean exit would poison every copy).
5. Phase A × 5: copy `template/` → `run-n/`; exec; wait for `statusItemDrawn` (→ `cold_start_ms`); probe rusage; SIGUSR1; wait for `menuOpened first=true` (→ `menu_open_after_launch_ms`); wait 250 ms, probe (→ `footprint_menu_open_mb` with the `modules=` count from the log line, `menu_open_cpu_ms` delta); 5 × (SIGUSR1 → `menuOpened first=false`) (→ `menu_reopen_ms`, CPU deltas); final probe (→ this process's lifetime peak footprint); SIGTERM, ≤ 5 s, else SIGKILL (PERF-3.4).
6. Phase B × 1: fresh `run-6/`; exec; wait for `statusItemDrawn`; sleep 10 s; probe at t0 and after each of 5 × 12 s windows → per-window CPU %, idle wake-ups/s, footprint, logical-write delta, threads; final probe gives this process's lifetime peak; SIGTERM. `peak_footprint_mb` is the maximum lifetime peak across all six measured processes.
7. `app_size_mb` = recursive byte sum of the perf bundle (std `fs::metadata`).
8. Hand the measurements to the judge; print phase timings (PERF-1.10).

It never calls `quit_app`, `install`, or `open`, and never touches a path outside `build/perf/` and `/tmp/navori-perf-<uid>/` (PERF-3.1, PERF-3.2). Each launch is a direct exec, so Ctrl-C reaches the apps through the process group; a `Launched` guard SIGKILLs any pid still alive on drop (PERF-3.5).

### xtask perf judge and baseline

Satisfies: PERF-1.2, PERF-1.3, PERF-1.4, PERF-1.7, PERF-1.8, PERF-1.9, PERF-2.1, PERF-2.2, PERF-2.3, PERF-2.4, PERF-2.5, PERF-2.6
Reuse: rung 5 — `serde_json` (already an xtask dependency) for `perf-baseline.json`; the rest is new code (rung 7)
Interface: `METRICS: &[MetricSpec]` (name, unit, kind `Gated(budget) | BaselineOnly | Skipped(reason)`, floor); `median(&[f64]) -> f64`; `judge(&Measurements, Option<&Baseline>, &Machine) -> Verdict` (lines + pass/fail + warnings); `Baseline::load/save`.
Depth: if this module vanished, callers would still need only "the metric table and a function from measurements to a verdict"; thresholds, rounding, missing-line detection and machine matching stay inside.
Locality: new `xtask/src/perf/judge.rs` and `baseline.rs`; the metric table is the single place a metric is added, enabled or skipped (PERF-1.4) — leave everything else.

`judge` walks `METRICS` in order: `Skipped` prints `skipped: <reason>`; an enabled metric with no value fails `no line for an enabled metric: <name>` (PERF-1.9); `Gated` fails over budget (PERF-1.8; `idle_cpu_pct` compares the value rounded to one decimal against 0.0); every enabled metric fails when a matching baseline exists and `value > median + max(0.10 × median, floor)` (PERF-2.4, floors from PERF-2.5); a missing baseline or a different `hw.model`/macOS version adds the warning and skips only the regression check (PERF-2.6). `--record` saves `{machine, macos, revision, recorded, metrics{name:{median,floor}}}` only when the verdict passes (PERF-2.1, PERF-2.2); without it the file is never opened for writing (PERF-2.3). Machine identity comes from `sysctl -n hw.model` and `sw_vers -productVersion`.

### Process counter helper

Satisfies: PERF-1.3, PERF-1.6, PERF-6.2
Reuse: rung 4 — `proc_pid_rusage(RUSAGE_INFO_V4)` and `proc_pidinfo(PROC_PIDTASKINFO)` (Darwin libproc), called from Swift because Rust may not use `unsafe` here
Respects: ARCH-9
Interface: `perf-probe <pid>` → one JSON line `{"t_ns":u64,"footprint":u64,"peak":u64,"logical_writes":u64,"cpu_ns":u64,"idle_wakeups":u64,"threads":u32}` — `t_ns` from `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)` so window lengths come from the same clock as the counters; `peak` = `ri_lifetime_max_phys_footprint`; `idle_wakeups` = `ri_pkg_idle_wkups`; CPU times converted with `mach_timebase_info` (125/3 on this Mac).
Depth: if this helper vanished, callers would still only need "six counters for a pid as JSON"; the libproc structs and timebase conversion stay inside.
Locality: new `xtask/assets/perf-probe.swift`, compiled into `build/perf/` by the runner — leave the app untouched (it observes from outside, so it adds nothing to the app; PERF-6.2).

### Shell perf probe

Satisfies: PERF-4.1, PERF-4.2, PERF-4.3, PERF-4.4, PERF-4.5, PERF-5.3, PERF-5.4, PERF-5.5
Reuse: rung 4 — `os.Logger`, `DispatchSource.makeSignalSource`, `sysctl(KERN_PROC_PID)`, `CATransaction` completion; the existing `Logger(subsystem: "dev.navori")` pattern (`AppDelegate.swift:9`)
Respects: ARCH-2, ARCH-15, ARCH-16
Surface:
- `CoreStore.start()` home argument (`CoreStore.swift:32`) — **replace**: reads `AppEnvironment.coreHome`, which is `NSHomeDirectory()` unless perf mode and `NAVORI_PERF_HOME` are both set (PERF-5.4 keeps the default).
- `AppDelegate.applicationDidFinishLaunching` — **replace**: after creating the status bar, calls `PerfProbe.start(statusBar:)`, which does nothing unless perf mode is on (PERF-5.3); launch order stays Core → status item (PERF-5.4).
- `StatusBarController` click handling — **frozen** for users: left click still toggles, right/control click still shows the context menu (PERF-5.5); a new `openMenuFresh()` entry point is used only by the probe.
- `StatusPanel.show(relativeTo:)` (`StatusPanel.swift:57-74`) — **replace**: accepts an optional completion that fires on the first committed frame; existing callers pass none.
- `Platform/LoginItem.swift`, `Platform/CLIInstaller.swift` — **frozen**: keep using the real home; reached only from Settings, never during a perf run.
Interface: `enum PerfProbe { static var isEnabled: Bool; static func start(statusBar:store:); static func markStatusItemDrawn(); static func markMenuOpened(since:first:modules:) }`; `AppEnvironment.coreHome: String`.
Depth: if this module vanished, callers would still know only "start it after the status bar exists"; signal plumbing, process-start time and the log message format stay inside.
Locality: new `apps/macos/Navori/Platform/PerfProbe.swift`; small edits in `AppEnvironment.swift`, `CoreStore.swift`, `AppDelegate.swift`, `StatusBarController.swift`, `StatusPanel.swift` — extend.

`markStatusItemDrawn` runs once, from a `CATransaction` completion scheduled right after the status item's first image is set, and logs `statusItemDrawn ms=<now − process start>` where process start is `kp_proc.p_un.__p_starttime` (µs precision). With perf mode on, `start` ignores SIGUSR1/SIGTERM at the signal level and attaches main-queue `DispatchSource` handlers: SIGUSR1 → `statusBar.openMenuFresh()`, which closes the panel before it takes the trigger; SIGTERM → `NSApp.terminate(nil)` (normal `applicationWillTerminate` → `store.stop()`). `openMenuFresh` closes the panel if visible and lets the close finish, then takes the trigger time and shows the panel with a completion that calls `markMenuOpened(since: trigger, first: !builtBefore, modules: store.moduleCount)` (modules whose state is `running`), which logs `menuOpened ms=<f64> first=<bool> modules=<n>` (`n` from the current Core snapshot). With `NAVORI_PERF=1` but no `NAVORI_PERF_HOME`, `start` logs a fault and terminates the app (decision 11). Nothing here creates a timer or IPC call (ARCH-16, PERF-6.2).

Calibration task (open question in requirements; a spike before the runner relies on the marks): (1) confirm a SIGUSR1-opened panel in the inactive accessory app stays visible and is not closed at once by `resignKey`; (2) compare each `CATransaction`-completion mark with a frame-presentation reference — a one-shot `NSView.displayLink` callback on the panel's and the status button's views (the first presented frame after the change) — and accept the marks if they are within 5 ms; otherwise use the display-link time as the mark. `CGWindowList` is not used: it shows that a window was ordered front, not that a frame was rendered.

Risk: the A5 brief may replace `StatusPanel` with an `NSMenu` (`docs/design/2026-11-01-menubar-ui-brief.md:8, 117`). Then the menu mark moves to `NSMenuDelegate.menuWillOpen`/first draw, and `openMenuFresh` changes accordingly; the runner, judge and metric names (`menu_open_after_launch_ms`, `menu_reopen_ms`) stay the same.

### Documentation

Satisfies: none — infrastructure (keeps `docs/standards/performance.md` true to the shipped method)
Reuse: rung 2 — edits the existing standard
Interface: n/a
Depth: n/a — extends `docs/standards/performance.md`
Locality: `docs/standards/testing.md:24` — xtask's pure perf logic is now unit-tested; `README.md`, `AGENTS.md` (commands) and the release steps in `docs/agents/project.md` gain `cargo xtask perf`; `docs/standards/performance.md` — note that perf runs share the `dev.navori.app.debug` preferences domain (unused by the app today), and reword "Measured on" / Method: release *code* packaged as the dev identity, counters via `proc_pid_rusage` (equal to `footprint` and `top idlew`), 5 × 12 s idle windows, `perf-baseline.json` written only by `--record`, the floors rule, baseline-only metrics, and the skip list; replace "to be built with the next release".

## Seams for testing

| Seam | Kind | Covers |
|---|---|---|
| `perf::judge` — `judge()`, `METRICS`, `median()` (pure, in-crate unit tests) | unit | PERF-1.2, PERF-1.3, PERF-1.4, PERF-1.7, PERF-1.8, PERF-1.9, PERF-2.4, PERF-2.5, PERF-2.6 |
| `perf::measurements` / `idle_window` — median of per-process reopen medians, idle-window divisors (pure, in-crate unit tests) | unit | PERF-1.3, PERF-1.6 |
| `perf::baseline` — load / save / record-only-on-pass (temp dir) | unit | PERF-2.1, PERF-2.2, PERF-2.3 |
| `perf::logstream` — ndjson line → `(pid, Mark)` parser, banner/finished lines ignored, other pids dropped | unit | PERF-4.6 (filtering), PERF-1.9 (missing line) |
| `perf::home::prepare` — temp root: creates 0700, replaces a stale tree it owns, refuses a symlink or foreign-owned path, rejects a socket path > 103 bytes | integration (filesystem) | PERF-3.3, PERF-6.3, PERF-6.5 |
| `cargo xtask perf` on the author's Mac — full run, `--record`, three consecutive runs, Ctrl-C mid-run, daily app and Navori Dev left running | e2e (manual, recorded in the task report) | PERF-1.1, PERF-1.5, PERF-1.6, PERF-1.10, PERF-1.11, PERF-1.12, PERF-3.1, PERF-3.2, PERF-3.4, PERF-3.5, PERF-4.1–4.6, PERF-6.1, PERF-6.2, PERF-6.4 |
| `xtask` `tool_stdout` / `sh` — child stdout is a pipe forwarded to stderr (in-crate unit test) | unit | PERF-1.11 |
| Calibration — panel stays open after SIGUSR1; in-process marks vs one-shot display-link frame reference | e2e (manual spike, once) | PERF-4.1, PERF-4.2, PERF-4.3 |
| Existing gates — `cargo xtask ci`, `cargo xtask install` + `navori doctor`, `plutil -p` on both bundles, manual left/right/control click | e2e (manual) + existing `ci` | PERF-5.1, PERF-5.2, PERF-5.3, PERF-5.4, PERF-5.5, PERF-5.6 |

## Coverage check

Every requirement ID appears in a `Satisfies:` line: PERF-1.1–1.12, 2.1–2.6, 3.1–3.5, 4.1–4.6, 5.1–5.6, 6.1–6.5. PERF-1.3 and PERF-1.6 appear in two sections by necessity (the runner collects, the helper reads, the judge reports). PERF-1.11 is also covered by the `tool_stdout` unit seam. PERF-6.4 (three consecutive runs, same verdict, no regression failure) is verified by the e2e seam, not owned by one module. No ID is deliberately unmapped.
