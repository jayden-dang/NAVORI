# App catalog

Recognized feature cards for this domain. Add a row **before** writing
`requirements.md` for a new feature. Status is one of
`Draft | Approved | In-progress | Implemented | Shipped | Recognized`.
`Owner` is the team or person a PR that changes this feature's spec or surface should
notify (`@org/team`). A blank cell means nobody is notified (solo repo).

| Code | Feature | Spec | Status | Owner | Roadmap item | Match terms | Surface roots |
|---|---|---|---|---|---|---|---|
| CORE | The Rust Core: kernel and Modules, SQLite store and Journal, config and Profiles, Sensitive Data Shield, IPC socket and the UniFFI surface | — | Shipped |  | ROAD-1 | kernel, module, action, event, journal, store, migration, profile, redact, ipc, ffi | `crates/` |
| SHELL | The macOS menubar Shell: status item, panel, Settings window, launch at login and CLI install | — | Shipped |  | ROAD-2 | menubar, status item, panel, settings, login item, CoreStore | `apps/macos/` |
| CLI | The `navori` command-line client over IPC: status, doctor, events, actions | — | Shipped |  | ROAD-3 | cli, doctor, status, events, actions, exit code | `apps/cli/` |
| BUILD | Build, sign, bundle, install and CI tooling run as `cargo xtask` | — | Shipped |  | ROAD-4 | xtask, bundle, signing, install, ci, check-deps, outdated | `xtask/` |
| PERF | Performance gate: `cargo xtask perf` measures release code against the GOAL-11 budget and a recorded baseline | [2026-11-01-perf-gate](../2026-11-01-perf-gate/) | Implemented |  | ROAD-5 | perf, budget, baseline, footprint, wakeups, cold start, menu open, regression | `xtask/` |
| PROCS | Platform layer and process model: snapshot of the user's processes resolved to Harness, project, worktree, branch and terminal host; process-exit watcher; child-tree activity; IOKit power-assertion wrapper | [2026-11-01-platform-and-process-model](../2026-11-01-platform-and-process-model/) | Implemented |  | ROAD-6 | process, pid, harness, agent session, project, worktree, branch, terminal, kqueue, libproc, power assertion, iokit | `crates/macos/`, `crates/procs/` |
| KCONTRACT | Kernel contract extensions: a config section per Module, a wait handle for absolute deadlines inside `run` (panel and wake aware), per-value config fallback and a Journal sweep that pauses when retention cannot be trusted | [2026-11-02-kernel-contract-extensions](../2026-11-02-kernel-contract-extensions/) | In-progress |  | ROAD-7 | scheduler, deadline, wait handle, timer, wake pulse, panel visible, module config, config fallback, retention, sweep | `crates/kernel/`, `crates/config/`, `crates/ffi/` |
