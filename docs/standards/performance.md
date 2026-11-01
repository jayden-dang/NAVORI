# Performance

Status: Approved
Date: 2026-11-01

"Light, fast, hardcore-optimized" (vision) is a measured budget, not a feeling. GOAL-7 states it; ARCH-15 – ARCH-18 prevent the usual causes at design time.

## Budget (GOAL-7)

| Metric | Budget | Measured on |
|---|---|---|
| CPU, idle (menu closed) | 0% | release build, 60 s window |
| Wake-ups, idle | ≤ 1 / s | `top -stats idlew` |
| Memory footprint, idle | ≤ 30 MB | `footprint <pid>` (phys_footprint) |
| Memory footprint, all features + menu open | ≤ 60 MB | `footprint <pid>` |
| Cold start → menubar icon visible | ≤ 200 ms | process launch to status item drawn |
| Menu open | ≤ 50 ms | click to first frame |
| Submenu open | ≤ 16 ms | hover to first frame (one frame) |
| Release `.app` size | ≤ 25 MB | bundle on disk |

R0 baseline (debug build, 2026-11-01): 0.0% CPU, 0 wake-ups/s, 21 MB footprint, 6 threads.

## Method

- Always measure the **release** build (`cargo xtask bundle --profile release`), on this Mac, on AC power, after a 10 s settle.
- Take the **median of 5 runs**; record the baseline per metric in `perf-baseline.json` (written by the tool).
- `cargo xtask perf` (to be built with the next release) fails when a metric exceeds its budget **or** regresses by more than 10% against the baseline.

## Design rules (from the architecture spine)

- ARCH-15 — no WebView, Electron or JS runtime.
- ARCH-16 — menu closed: no timer faster than 30 s; periodic work goes through the Scheduler (adaptive: faster while the menu is open, paused during sleep, refresh on wake).
- ARCH-17 — the AI runtime and provider HTTP clients start on first use.
- ARCH-18 — network data (avatars, GitHub) is cached on disk; opening the menu never refetches.
- Snapshots pushed to Swift are coalesced (≤ 30/s while the menu is open, slower when closed).

## Changing the budget

A feature that cannot meet the budget after optimization changes it explicitly through `define-project` (update mode) with the measured reason — never by quietly loosening a number.
