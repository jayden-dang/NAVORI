# Performance

Status: Approved
Date: 2026-11-01

"Light, fast, hardcore-optimized" (vision) is a measured budget, not a feeling. GOAL-11 states it; ARCH-15 – ARCH-18 prevent the usual causes at design time. `cargo xtask perf` checks the GOAL-11 budgets. It does not check ARCH-15 through ARCH-18.

## Budget (GOAL-11)

| Metric | Budget | Measured on |
|---|---|---|
| CPU, idle (menu closed) | 0% | 5 × 12 s windows; CPU time ÷ wall time, compared after rounding to one decimal |
| Wake-ups, idle | ≤ 1 / s | same windows; `ri_pkg_idle_wkups`, equal to `top` IDLEW |
| Memory footprint, idle | ≤ 30 MB | same windows; `proc_pid_rusage` phys_footprint, equal to `footprint` |
| Memory footprint, all features + menu open | ≤ 60 MB | phys_footprint with the menu open |
| Cold start → menubar icon visible | ≤ 200 ms | process start to status item drawn |
| First menu open after launch | ≤ 80 ms | trigger to first frame, first open in the process |
| Later menu opens | ≤ 50 ms | trigger to first frame, later opens in the same process |
| Submenu open | ≤ 16 ms | trigger to first frame, Core Submenu |
| Submenu open from pointer entry | ≤ 80 ms | pointer entering the Core row to the first frame of its Submenu, intent delay included |
| Release `.app` size | ≤ 25 MB | perf bundle on disk |
| Process snapshot with Agent session resolution | ≤ 10 ms | PROCS-8.4; median of 5 in one test run, debug build on this Mac, the pessimistic case |
| Port scan (process list, sockets, environment reads; no Docker read) | ≤ 50 ms | median of 5 `Scanner::scan` calls in one test run, debug build on this Mac |

GOAL-7's single 50 ms menu-open line was retired on 2026-11-02. First opens on this Mac (`Mac17,9`, macOS 27.0.1) measured 47.3–63.7 ms. 80 ms is the round budget above that high sample. Later opens measured 8.6–10.5 ms and keep 50 ms.

R0 baseline (debug build, 2026-11-01): 0.0% CPU, 0 wake-ups/s, 21 MB footprint, 6 threads. That row predates the gate; it is not a `perf-baseline.json` record.

## Method

- `cargo xtask perf` measures **release code packaged as the dev identity** (bundle id `dev.navori.app.debug`, `NavoriProfile=dev`) on this Mac. It does not measure `/Applications/Navori.app`. MB means MiB (1,048,576 bytes).
- `snapshot_ms` and `port_scan_ms` are the exception to release code: each runs an ignored real-OS test (`crates/procs/tests/real_os.rs`, `modules/ports/tests/real_os.rs`) in a debug build, so it does not touch the app under measurement. It needs an idle machine for the same reason the other metrics do: a compile running beside it pushed the median to 14–16 ms (2026-11-04).
- Counters come from `proc_pid_rusage`: phys_footprint (equal to `footprint`), peak footprint, logical writes, CPU time and idle wake-ups (equal to `top` IDLEW). Thread count comes from `proc_pidinfo`.
- Idle metrics are the median of 5 consecutive 12 s windows after a 10 s settle. Cold start, the first menu open and the menu-open footprint are the median of 5 processes. Menu reopen is the median across those processes of 5 reopens each. Core Submenu open is the median across those processes of 5 Core Submenu opens each. Submenu open from pointer entry (`submenu_intent_open_ms`) uses the same 5 × 5 shape: the probe closes the Submenus, sends the pointer into the Core row through the controller's hover path (`pointerOn`, the real 50 ms intent timer, then `open`) and stops the clock at the first presented frame, so `CascadeState.intentDelay` is part of the value and changing it is a budget change.
- A gated metric fails over its budget. Idle CPU is compared to 0.0 after rounding to one decimal. An enabled metric with no measurement fails. With a baseline from the same machine model and macOS version, every measured metric also fails when its value exceeds `baseline median + max(10% of that median, floor)`. The floor is the one listed below, not the copy stored in the baseline file. No baseline, or a different `hw.model` or macOS version, skips only the regression check and prints why.
- Floors: 10 ms `cold_start_ms`; 2 ms `snapshot_ms`; 5 ms `port_scan_ms`; 5 ms `menu_open_after_launch_ms`, `menu_reopen_ms`, `submenu_open_ms` and `menu_open_cpu_ms`; 10 ms `submenu_intent_open_ms`; 1 MB `footprint_menu_open_mb` and `idle_footprint_mb`; 2 MB `peak_footprint_mb`; 0.5 MB `app_size_mb`; 0.1 percentage points `idle_cpu_pct`; 0.2 per second `idle_wakeups_per_s`; 64 KiB `idle_disk_writes_kb`; 2 threads `thread_count`. `idle_disk_writes_kb` is kibibytes (1024 bytes).
- `cargo xtask perf --record` writes `perf-baseline.json` (machine model, macOS version, git revision, date, and each measured metric's median and floor) only when the run passes. Without `--record`, or when the run fails, the file is left unchanged.
- Baseline-only metrics have no budget and are still regression-checked: `peak_footprint_mb`, `idle_disk_writes_kb`, `menu_open_cpu_ms`, `thread_count`.
- Perf runs use the same `dev.navori.app.debug` preferences domain as `cargo xtask run`.
- Perf runs link the user's Docker socket into each run home and print `docker=<status>` from `ports.list` (`unavailable` when the app has no Ports module). With the socket linked, an `unreadable` or `unavailable` status warns and refuses `--record`; `not_seen` prints a note and records. A baseline needs the same Docker state as the runs it is compared with.

## Soak

`cargo xtask soak` drives an isolated release-flavor dev-identity Navori (perf mode, its own home under `/tmp/navori-soak-<uid>/`) under load and judges leaks and hangs. Every value below is agent-decided (specs `2026-11-05-soak-and-hang-check`, `requirements.md` and `design.md`); none was set by the operator.

- **Tiers:** the short soak is a 1 minute settle plus 20 minutes of load (SOAK-1.2, D7; inside the operator's ~30 minute bound on `release-check`, about 23 minutes with teardown, SOAK-9.1). `cargo xtask soak --long` runs the same schedule at one fifth of its rate for 2 hours (SOAK-5.1, D8, D9), `--for <duration>` overrides the 2 hours, and it always exits 0 whatever the verdict. `release-check` never runs the long tier (SOAK-5.5).
- **Load rates:** the SOAK-1.3 schedule (D9), including a 4 s write-lock hold on `navori.db` every 2 minutes. Samples of footprint, threads and fds every 10 s in the short tier and 60 s in the long tier (SOAK-1.5, SOAK-5.3, D10).
- **Hang threshold:** a Core `status` probe unanswered after 2 s, or a menu SIGUSR1 with no `menuOpened` mark after 2 s (SOAK-3.2, SOAK-3.4, D11). Four times Apple's 500 ms "proper hang" (WWDC23 session 10248) and below the store's 5 s `busy_timeout`.
- **Leak rule:** a signal is LEAK when median(last third) minus median(first third) of the post-settle samples exceeds max(10% of median(first third), floor) and median(middle third) is at least median(first third) (SOAK-2.1, D12). The rule is the perf regression rule from Method.
- **Floors:** 1 MiB footprint, 2 threads, 4 fds (SOAK-2.2, D12). Footprint and threads reuse the perf floors; fds has no precedent. Fewer than 9 post-settle samples fail the run (SOAK-2.5).
- **Release gate:** `release-check` runs the short soak after perf on `build/perf/Navori Dev.app` and prints the tag only when it passes (SOAK-4.1, SOAK-4.2).
- **Issue #21:** a known Core stall under the store lock is open as #21. The soak is expected to fail with `soak hang core` while it is open, so #21 blocks releases. There is no skip and no override (operator decision, 2026-11-05): fix #21 first.
- **Perf visibility edit:** the 3-line change that made perf's `home`, `launch` and `logstream` modules visible to the soak is not a perf-gate change (operator decision, 2026-11-05); budgets, floors and output are unchanged.

## Design rules (from the architecture spine)

- ARCH-15 — no WebView, Electron or JS runtime.
- ARCH-16 — menu closed: no timer faster than 30 s; periodic work goes through the Scheduler (adaptive: faster while the menu is open, a timer parks on uptime, and an overdue timer may fire during dark wake, refresh on wake).
- ARCH-17 — the AI runtime and provider HTTP clients start on first use.
- ARCH-18 — network data (avatars, GitHub) is cached on disk; opening the menu never refetches.
- Snapshots pushed to Swift are coalesced (≤ 30/s while the menu is open, slower when closed).

## Changing the budget

A feature that cannot meet the budget after optimization changes it explicitly through `define-project` (update mode) with the measured reason — never by quietly loosening a number.

## Exemption on hosted CI

Wall-clock budgets of millisecond size cannot hold on a shared GitHub-hosted runner (the kill-to-exit event measured 30 ms there). The 10 ms kill-to-exit assertion in `crates/macos/tests/exits_latency.rs` is therefore skipped when the `CI` environment variable is set. It still runs in `cargo xtask ci` and `cargo xtask release-check` on the maintainer's Mac, and on CI the test still checks the exit event, the pid and the token.

## Amendments

- 2026-11-05 — added `submenu_intent_open_ms`, ≤ 80 ms gated, floor 10 ms. Approved by the operator on 2026-11-05 (from `.skills/research/perf-gate-intent-metric-proposal.md`); `define-project` is a user-run skill, so this note records the approval instead. Reason: `submenu_open_ms` starts at the open call and skips the hover intent timer, so a 150 ms re-armed timer passed every gate. First measurements: median about 58 ms (50 ms intent plus open and vsync), p95 about 62 ms on a busy machine (load 7–9). 80 ms is the 50 ms intent delay, the 16 ms open budget and 14 ms of timer and busy-machine slack; it fails a 150 ms timer (about 158 ms) with no baseline needed. The 10 ms floor is the one already used for `cold_start_ms`; it exceeds 10% of the median (about 6 ms) and covers `asyncAfter` jitter. No baseline value is recorded yet: until `cargo xtask perf --record` runs on an idle machine, only the budget is enforced for this metric. A baseline recorded before this metric existed reports `submenu_intent_open_ms` as missing from the baseline until it is re-recorded on an idle machine. The metric is measured after the five `submenu_open_ms` opens, so it reflects warm opens; the real first hover on a cold panel can be a few ms slower.
