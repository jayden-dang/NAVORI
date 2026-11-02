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
| Submenu open | ≤ 16 ms | skipped: no submenu until the A5 menu |
| Release `.app` size | ≤ 25 MB | perf bundle on disk |

GOAL-7's single 50 ms menu-open line was retired on 2026-11-02. First opens on this Mac (`Mac17,9`, macOS 27.0.1) measured 47.3–63.7 ms. 80 ms is the round budget above that high sample. Later opens measured 8.6–10.5 ms and keep 50 ms.

R0 baseline (debug build, 2026-11-01): 0.0% CPU, 0 wake-ups/s, 21 MB footprint, 6 threads. That row predates the gate; it is not a `perf-baseline.json` record.

## Method

- `cargo xtask perf` measures **release code packaged as the dev identity** (bundle id `dev.navori.app.debug`, `NavoriProfile=dev`) on this Mac. It does not measure `/Applications/Navori.app`. MB means MiB (1,048,576 bytes).
- Counters come from `proc_pid_rusage`: phys_footprint (equal to `footprint`), peak footprint, logical writes, CPU time and idle wake-ups (equal to `top` IDLEW). Thread count comes from `proc_pidinfo`.
- Idle metrics are the median of 5 consecutive 12 s windows after a 10 s settle. Cold start, the first menu open and the menu-open footprint are the median of 5 processes. Menu reopen is the median across those processes of 5 reopens each.
- A gated metric fails over its budget. Idle CPU is compared to 0.0 after rounding to one decimal. An enabled metric with no measurement fails. With a baseline from the same machine model and macOS version, every measured metric also fails when its value exceeds `baseline median + max(10% of that median, floor)`. The floor is the one listed below, not the copy stored in the baseline file. No baseline, or a different `hw.model` or macOS version, skips only the regression check and prints why.
- Floors: 10 ms `cold_start_ms`; 5 ms `menu_open_after_launch_ms`, `menu_reopen_ms` and `menu_open_cpu_ms`; 1 MB `footprint_menu_open_mb` and `idle_footprint_mb`; 2 MB `peak_footprint_mb`; 0.5 MB `app_size_mb`; 0.1 percentage points `idle_cpu_pct`; 0.2 per second `idle_wakeups_per_s`; 64 KiB `idle_disk_writes_kb`; 2 threads `thread_count`. `idle_disk_writes_kb` is kibibytes (1024 bytes).
- `cargo xtask perf --record` writes `perf-baseline.json` (machine model, macOS version, git revision, date, and each measured metric's median and floor) only when the run passes. Without `--record`, or when the run fails, the file is left unchanged.
- Baseline-only metrics have no budget and are still regression-checked: `peak_footprint_mb`, `idle_disk_writes_kb`, `menu_open_cpu_ms`, `thread_count`.
- Skip list, declared in the tool: `submenu_open_ms`, reason "no submenu until the A5 menu". The line is printed `submenu_open_ms skipped: no submenu until the A5 menu` and does not fail the run.
- Perf runs use the same `dev.navori.app.debug` preferences domain as `cargo xtask run`.

## Design rules (from the architecture spine)

- ARCH-15 — no WebView, Electron or JS runtime.
- ARCH-16 — menu closed: no timer faster than 30 s; periodic work goes through the Scheduler (adaptive: faster while the menu is open, a timer parks on uptime, and an overdue timer may fire during dark wake, refresh on wake).
- ARCH-17 — the AI runtime and provider HTTP clients start on first use.
- ARCH-18 — network data (avatars, GitHub) is cached on disk; opening the menu never refetches.
- Snapshots pushed to Swift are coalesced (≤ 30/s while the menu is open, slower when closed).

## Changing the budget

A feature that cannot meet the budget after optimization changes it explicitly through `define-project` (update mode) with the measured reason — never by quietly loosening a number.
