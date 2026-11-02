# Codebase Modules

Status: Approved
Date: 2026-11-01

## Purpose and boundary

The inventory of code units in the repository — every Rust crate and Swift target, built or planned — with its path, what it is responsible for, the public surface other units may use, and where its tests live. "Module" here means a code unit; a *feature* Module in the glossary sense (an actor in the kernel) is marked as such. Does not cover folder placement rules (`docs/codebase/map.md`), allowed dependency edges (`docs/codebase/dependencies.md`) or runtime behaviour (`docs/architecture/`).

## Module inventory

**Built (R0)**

| Module | Path | Responsibility |
|---|---|---|
| `navori-protocol` | `crates/protocol/` | Wire types for every layer. Surface: `Envelope`, `ActionSpec`, `Risk`, `Event`, `Severity`, `ModuleState`, `ModuleInfo`, `CoreStatus`, `Check`, `rpc::{Request, Response, Error, method, code}`. Tests: exercised through kernel, ipc, ffi and CLI tests. |
| `navori-redact` | `crates/redact/` | Sensitive Data Shield. Surface: `redact_text`, `redact_json`, `is_sensitive_path`, `Redacted`, `Finding`. Tests: `tests/{redact_text,redact_json,sensitive_paths}.rs`. |
| `navori-config` | `crates/config/` | Profiles, paths, `config.toml`. Surface: `Profile`, `Paths`, `Config`, `JournalConfig`, `ConfigLoad`, `JournalSweep`, `ConfigReport`, `WholeFileReason`. Tests: `tests/{paths,config_file}.rs`. |
| `navori-store` | `crates/store/` | SQLite store. Surface: `Store` (`open`, `migrate`, `kv_get/kv_set`, `journal_append/recent/prune`, `with_conn`), `Migration`, `MigrationReport`, `StoreError`. Tests: `tests/{migrations,kv_and_journal}.rs`. |
| `navori-kernel` | `crates/kernel/` | Module runtime. Surface: `Module`, `ModuleAction`, `ActionDoc`, `ModuleCtx`, `Request`, `ActionError`, `Kernel`, `KernelBuilder`, `RestartPolicy`, `DispatchError`, `EventBus`, and the built-in feature Module `System`. Tests: `tests/kernel.rs`. |
| `navori-ipc` | `crates/ipc/` | Local JSON-RPC. Surface: `Server`, `Client`, `Handler`, `IpcError`, `MAX_LINE_BYTES`. Tests: `tests/ipc.rs`. |
| `navori-ffi` | `crates/ffi/` | Composition root and UniFFI surface (static lib + `lib`). Surface: `NavoriCore` (`start`, `status`, `actions`, `doctor`, `dispatch`, `set_panel_visible`, `system_did_wake`, `shutdown`), `SnapshotListener`, `CoreConfig`, `CoreSnapshot` and view records, `ConfigNotice`, `CoreError`. Tests: `tests/core.rs`. |
| `navori-cli` (binary `navori`) | `apps/cli/` | Command-line client over IPC. Tests: `tests/cli.rs` (real binary against an in-process core). |
| `xtask` | `xtask/` | Build and repo tooling (`cargo xtask <command>`). Verified by use, not unit tests. |
| Swift `Navori` (app target) | `apps/macos/Navori/` | The Shell: `App/` lifecycle, `Core/CoreStore`, `StatusBar/`, `Features/`, `Platform/`. Swift 6 mode. Verified by build and launch. |
| Swift `NavoriCore` + `NavoriCoreFFI` | `apps/macos/Generated/` | Generated bindings (Swift 5 mode) and the Rust static library as an xcframework. Never edited by hand. |

**Built (R1)**

| Module | Path | Responsibility |
|---|---|---|
| `navori-macos` | `crates/macos/` | Platform FFI. Surface: traits `ProcessSource`, `ExitSource`, `PowerAssertions`; `MacProcesses`, `KqueueExits`, `MacPower`, `AssertionGuard`; errors `PlatformError`, `ReadError`, `WatchError`, `PowerError`. Feature `fake`: `FakeProcesses`, `FakeExitsHandle`, `FakePower`. The only hand-written `unsafe`. Tests: `tests/{process_source,procargs,exits,exits_latency,exits_drop,exits_idle,power}.rs`. |
| `navori-procs` | `crates/procs/` | Process model. No `unsafe`. Surface: `Snapshot`, `ProcId`, `resolve_location`, `Harness`, `sessions`, `harness_of`, `activity`, `ProcWatcher`. Tests: `tests/{snapshot,location,sessions,harness_of,activity,watcher,no_threads,real_os}.rs`. |

**Planned**

| Module | Path | Responsibility |
|---|---|---|
| `navori-secrets` | `crates/secrets/` | Keychain store for tokens and API keys (R2). |
| `navori-mod-awake` | `modules/awake/` | Feature Module: Leases and the power assertion (R1). |
| `navori-mod-inbox` | `modules/inbox/` | Feature Module: Needs you — Decisions, To-dos, Updates (R2). |
| `navori-mod-ports` | `modules/ports/` | Feature Module: listening ports by project/worktree/harness (R3). |
| `navori-mod-github` | `modules/github/` | Feature Module: pull requests, checks, approve/merge (R4). |
| `navori-llm`, `navori-models`, `navori-context`, `navori-agent`, `navori-memory` | `ai/{llm,models,context,agent,memory}/` | AI runtime: providers, model routing, context and gateway, agent loop and policy, memory (R2). |
| `navori-triggers` | `ai/triggers/` | Proactive rules (R5). |
| `navori-mcp` | `ai/mcp/` | MCP client and server (R6). |

## Boundaries

- Other units use a crate only through what its `lib.rs` exports; internal modules (`mod`) are private.
- A feature Module exposes itself to the rest of the system only through the kernel: its Actions (with schema and Risk), its state snapshot, its Events and, for the AI, its context fragment. No other crate calls into a feature Module directly.
- Feature Modules never import each other or the AI crates; the AI crates never import feature Modules (see `docs/codebase/dependencies.md`).
- Feature Modules and services are wired into the app only in `navori-ffi`; nothing depends on `navori-ffi` except tests.
- A feature Module owns its Store tables and KV namespace; it does not read another Module's tables (`docs/architecture/data.md`).
- On the Swift side, only `CoreStore` imports `NavoriCore`; views read `CoreStore` state and call its methods.

## Not feature registry

This document is **not** the feature registry (`docs/specs/INDEX.md`) and **not** the architecture invariant spine.
