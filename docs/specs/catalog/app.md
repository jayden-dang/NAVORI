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
