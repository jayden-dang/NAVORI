# Requirements: Performance gate

Feature code: PERF
Status: In-progress
Date: 2026-11-01

<!--
Rules:
- Every acceptance criterion gets a hierarchical ID: PERF-<story>.<criterion>.
- Criteria use EARS phrasing (WHEN / WHILE / IF…THEN / WHERE / ubiquitous).
- Guard requirements protect existing behavior: WHEN <condition> THE SYSTEM SHALL CONTINUE TO <behavior>.
- IDs are immutable once Status is Approved. Retire by strikethrough — never renumber.
-->

Roadmap item: ROAD-5 (MILE-2) · Tracker: #1 · Budget source: GOAL-11 in `docs/product/vision.md`, method in `docs/standards/performance.md`.

Terms used below:
- **Perf build** — the app built from release code (Rust `--release`, Swift `-c release`) and packaged with the dev identity: bundle id `dev.navori.app.debug`, `NavoriProfile=dev`.
- **The menu** — what a left click on the status item opens (today the `StatusPanel`; later the A5 NSMenu).
- **Gated metric** — has a GOAL-11 budget and is also regression-checked. **Baseline-only metric** — regression-checked, no budget yet.
- **Perf mode** — the app was started with the environment variable `NAVORI_PERF=1`.

## 1. Run the gate and get a verdict

**Story:** As the developer, I want to run `cargo xtask perf` and get one pass/fail verdict with every metric shown against its budget, so that I know whether a release is fast enough before shipping it.

- **PERF-1.1** WHEN `cargo xtask perf` runs THE SYSTEM SHALL build the Perf build and measure that bundle.
- **PERF-1.2** THE SYSTEM SHALL report these gated metrics with their GOAL-11 budgets: `cold_start_ms` ≤ 200 (process start to status item drawn), `menu_open_after_launch_ms` ≤ 80 (first open of the menu in a process, trigger to first frame), `menu_reopen_ms` ≤ 50 (later opens in the same process), `footprint_menu_open_mb` ≤ 60 (phys_footprint with the menu open, printed with the number of running modules), `idle_cpu_pct` = 0.0 (rounded to one decimal), `idle_wakeups_per_s` ≤ 1, `idle_footprint_mb` ≤ 30, `app_size_mb` ≤ 25 (bundle on disk).
- **PERF-1.3** THE SYSTEM SHALL report these baseline-only metrics, labelled as baseline-only: `peak_footprint_mb` (highest phys_footprint seen during the run), `idle_disk_writes_kb` (logical bytes written during the idle windows), `menu_open_cpu_ms` (CPU time consumed per menu open), `thread_count` (threads while idle).
- **PERF-1.4** THE SYSTEM SHALL list every metric it does not measure in a skip list declared in the tool, each with a reason, and print each as `skipped: <reason>`; initially `submenu_open_ms` with the reason "no submenu until the A5 menu".
- **PERF-1.5** WHEN measuring THE SYSTEM SHALL run phase A as 5 separate app processes — taking `cold_start_ms`, `menu_open_after_launch_ms` and `footprint_menu_open_mb` once per process and `menu_reopen_ms` as the median of 5 reopens per process — and report the median across the 5 processes for each.
- **PERF-1.6** WHEN measuring THE SYSTEM SHALL run phase B as one app process that settles for 10 s and is then sampled over 5 consecutive 12 s idle windows, reporting the median window for `idle_cpu_pct`, `idle_wakeups_per_s`, `idle_footprint_mb`, `idle_disk_writes_kb` and `thread_count`.
- **PERF-1.7** WHEN every gated metric is within budget and no metric regresses (story 2) THE SYSTEM SHALL exit with status 0.
- **PERF-1.8** IF any gated metric exceeds its budget THEN THE SYSTEM SHALL exit non-zero and print the metric, the measured value and the budget.
- **PERF-1.9** IF a metric that is not on the skip list has no measurement after its phase THEN THE SYSTEM SHALL exit non-zero with "no line for an enabled metric: <metric>".
- **PERF-1.10** WHEN a run finishes THE SYSTEM SHALL print the elapsed time of the build, phase A and phase B.
- **PERF-1.11** WHEN `cargo xtask perf` runs THE SYSTEM SHALL print `build <seconds>s` as the first line of stdout, with three digits after the decimal.
- **PERF-1.12** (guard) WHEN `cargo xtask perf` prints the build elapsed line THE SYSTEM SHALL CONTINUE TO print `phase A <seconds>s` and `phase B <seconds>s` after the verdict lines, each with three digits after the decimal.

## 2. Record a baseline and catch regressions

**Story:** As the developer, I want to record a baseline once and have later runs fail when any metric gets meaningfully worse, so that slow drift is caught long before it hits the budget.

- **PERF-2.1** WHEN `cargo xtask perf --record` completes a run that passes THE SYSTEM SHALL write `perf-baseline.json` at the repository root containing the machine model, macOS version, git revision, record date, and for every measured metric its median and floor.
- **PERF-2.2** IF a `--record` run does not pass THEN THE SYSTEM SHALL leave `perf-baseline.json` unchanged.
- **PERF-2.3** WHEN `cargo xtask perf` runs without `--record` THE SYSTEM SHALL leave `perf-baseline.json` unchanged.
- **PERF-2.4** WHEN a baseline recorded on the same machine model and macOS version exists THE SYSTEM SHALL fail every metric whose value exceeds `baseline median + max(10% × baseline median, floor)`, printing the metric, the value and the threshold.
- **PERF-2.5** THE SYSTEM SHALL use these floors: 10 ms for `cold_start_ms`; 5 ms for `menu_open_after_launch_ms`, `menu_reopen_ms` and `menu_open_cpu_ms`; 1 MB for `footprint_menu_open_mb` and `idle_footprint_mb`; 2 MB for `peak_footprint_mb`; 0.5 MB for `app_size_mb`; 0.1 percentage points for `idle_cpu_pct`; 0.2 per second for `idle_wakeups_per_s`; 64 KB for `idle_disk_writes_kb`; 2 threads for `thread_count`.
- **PERF-2.6** IF no baseline exists, or the baseline's machine model or macOS version differs from the current Mac THEN THE SYSTEM SHALL check budgets only and print a warning naming why the regression check was skipped.

## 3. Measure without disturbing the daily app

**Story:** As the developer, I want to run the gate while my installed Navori keeps running, so that measuring never quits my daily app or touches its data.

- **PERF-3.1** WHILE `cargo xtask perf` runs THE SYSTEM SHALL NOT quit, launch or write to `/Applications/Navori.app`, `~/Library/Application Support/Navori/` or `~/Library/Logs/Navori/`.
- **PERF-3.2** WHILE `cargo xtask perf` runs THE SYSTEM SHALL NOT quit a running `Navori Dev` app nor write to `~/Library/Application Support/Navori Dev/` or `~/Library/Logs/Navori Dev/`.
- **PERF-3.3** WHEN preparing a run THE SYSTEM SHALL launch the Perf build once as a warm-up with a fresh template home under `/tmp/navori-perf-<uid>/`, and start every measured process from a new copy of that template.
- **PERF-3.4** WHEN a measured process has been measured THE SYSTEM SHALL send it SIGTERM and, IF it has not exited within 5 s, kill it.
- **PERF-3.5** IF `cargo xtask perf` is interrupted (Ctrl-C) or fails mid-run THEN THE SYSTEM SHALL stop every app process it launched before exiting.

## 4. The app reports its own timings in perf mode

**Story:** As the developer, I want to start Navori with `NAVORI_PERF=1` and read its launch and menu-open times in the unified log, so that the gate measures what the app actually draws instead of guessing from outside.

- **PERF-4.1** WHEN the status item is drawn for the first time in a process THE SYSTEM SHALL write a unified-log entry (subsystem `dev.navori`, category `perf`) carrying the milliseconds since the process started.
- **PERF-4.2** WHEN the menu shows its first frame after being opened THE SYSTEM SHALL write a unified-log entry (subsystem `dev.navori`, category `perf`) carrying the milliseconds from the open trigger to that frame and whether it was the first open in the process.
- **PERF-4.3** WHERE perf mode is on THE SYSTEM SHALL open the menu when the process receives SIGUSR1.
- **PERF-4.4** WHERE perf mode is on THE SYSTEM SHALL quit through the normal application termination path, shutting the Core down, when the process receives SIGTERM.
- **PERF-4.5** WHERE perf mode is on and `NAVORI_PERF_HOME` is set THE SYSTEM SHALL pass that directory to the Core as its home, so the data directory, logs, database, config, backups and — while its path stays within 103 bytes — the socket all live under it.
- **PERF-4.6** WHEN `cargo xtask perf` measures a process THE SYSTEM SHALL send SIGUSR1 only after reading that process's status-item-drawn entry, and read entries of that process only (filtered by pid).

## 5. Existing builds and the app outside perf mode stay unchanged

**Story:** As the developer, I want my normal builds and the app I use every day to behave exactly as before, so that adding the gate costs nothing outside a perf run.

Files this change touches, with what each must keep doing:
- `xtask/src/main.rs` — `build-core`, `bundle`, `run`, `install`, `signing-setup`, `check-deps`, `outdated`, `icon`, `ci` (guards 5.1, 5.2, 5.6).
- `apps/macos/Navori/App/AppDelegate.swift`, `apps/macos/Navori/App/AppEnvironment.swift` — launch order and profile (guards 5.3, 5.4).
- `apps/macos/Navori/Core/CoreStore.swift` — Core config (guard 5.4).
- `apps/macos/Navori/StatusBar/StatusBarController.swift`, `apps/macos/Navori/StatusBar/StatusPanel.swift` — left click opens the panel, right click opens the context menu (guard 5.5).
- `docs/standards/performance.md` — no behavior to guard (wording only).
- `perf-baseline.json` (new), new perf code under `xtask/` — no behavior to guard (new files).

- **PERF-5.1** (guard) WHEN `cargo xtask build-core`, `bundle`, `run` or `install` runs THE SYSTEM SHALL CONTINUE TO build the dev profile from debug code (cargo debug, `swift build -c debug`) and the release profile from release code (`--release`, `swift build -c release`), with the same bundle ids, names, signing and `Info.plist` keys as before; in particular `Info.plist` SHALL CONTINUE TO declare no URL types.
- **PERF-5.2** (guard) WHEN `cargo xtask install` runs THE SYSTEM SHALL CONTINUE TO install and launch `/Applications/Navori.app` with the app icon, and `navori doctor` SHALL CONTINUE TO report every check passing.
- **PERF-5.3** (guard) WHILE perf mode is off THE SYSTEM SHALL CONTINUE TO install no handler for SIGUSR1 or SIGTERM, keeping the operating system's default action for both.
- **PERF-5.4** (guard) WHILE perf mode is off THE SYSTEM SHALL CONTINUE TO pass the user's home directory to the Core and to start the Core before creating the status item.
- **PERF-5.5** (guard) WHEN the user left-clicks the status item THE SYSTEM SHALL CONTINUE TO toggle the panel, and WHEN the user right-clicks or control-clicks it THE SYSTEM SHALL CONTINUE TO show the context menu (Open <app name>, Settings…, Open Logs Folder, Quit <app name>).
- **PERF-5.6** (guard) WHEN `cargo xtask ci` runs THE SYSTEM SHALL CONTINUE TO run fmt, clippy with warnings denied, the Rust tests, `check-deps` and a debug Swift build, and `signing-setup`, `check-deps`, `outdated` and `icon` SHALL CONTINUE TO behave as before.

## 6. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** **PERF-6.1** WHEN `cargo xtask perf` runs THE SYSTEM SHALL finish measuring (phases A and B, excluding the build) within 3 minutes on the author's Mac — verified by the elapsed times PERF-1.10 prints. **PERF-6.2** THE SYSTEM SHALL add no timer, polling loop or IPC traffic to the app for measurement, in or out of perf mode (ARCH-16) — verified by review and by `idle_wakeups_per_s` staying within its baseline.
- **Security:** **PERF-6.3** WHEN the tool creates `/tmp/navori-perf-<uid>/` THE SYSTEM SHALL create it owned by the current user with mode `0700`, so the perf socket stays private (ARCH-11; threat THR-4 at boundary TB-2 in `docs/security/threat-model.md`) — verified by a test on the directory's mode and owner.
- **Reliability:** **PERF-6.4** WHEN `cargo xtask perf` runs 3 times in a row on the same revision with no code change THE SYSTEM SHALL give the same verdict and the same set of failing metrics in all 3 runs, with no regression failure — verified manually once the gate is built, and repeated whenever floors change. **PERF-6.5** IF a previous run left `/tmp/navori-perf-<uid>/` behind THEN THE SYSTEM SHALL replace it with a fresh template before measuring — verified by a test that seeds a stale directory.
- **Accessibility:** None — developer command-line tool; the perf probe adds no visible UI.

## Out of Scope

- A new Profile value, or any change to the glossary's Profile or ARCH-14.
- Any action, FFI function, IPC method, CLI command or URL scheme that exists only for measurement.
- `powermetrics`, `sudo`, or synthetic clicks through Accessibility.
- Absolute budgets for the four baseline-only metrics (set later through `define-project`, which amends GOAL-11).
- Hang detection and long-running leak/soak checks (ROAD-28).
- Prebuilding the panel or any optimisation to make a metric pass — this feature measures; fixing a miss is separate work.
- Measuring the installed `/Applications/Navori.app` itself, or any machine other than the author's Mac.
- Hosted CI integration.

## Open Questions

- Whether a callback after `show()` lines up with the menu's first frame on screen, and whether a SIGUSR1-opened panel stays open in the inactive app — owner: design-solution (calibration spike: compare against a one-shot display-link frame reference, ≤ 5 ms apart) — 2026-11-01 — forbid-guess.
- Floors in PERF-2.5 are estimates — owner: the author, after the first 3 runs (PERF-6.4) — 2026-11-01 — forbid-guess. On 2026-11-02 the three runs at `dab5a48` did not agree (exits 1, 1, 0) because `menu_open_after_launch_ms` sat on both sides of 50 (51.8, 51.9, 49.4). That was the absolute budget, not a regression past a floor, so the floors were left as written.
