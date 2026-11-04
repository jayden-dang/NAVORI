# Codebase Modules

Status: Approved
Date: 2026-11-01

## Purpose and boundary

The inventory of code units in the repository — every Rust crate and Swift target, built or planned — with its path, what it is responsible for, the public surface other units may use, and where its tests live. "Module" here means a code unit; a *feature* Module in the glossary sense (an actor in the kernel) is marked as such. Does not cover folder placement rules (`docs/codebase/map.md`), allowed dependency edges (`docs/codebase/dependencies.md`) or runtime behaviour (`docs/architecture/`).

## Module inventory

**Built (R0)**

| Module | Path | Responsibility |
|---|---|---|
| `navori-protocol` | `crates/protocol/` | Wire types for every layer. Surface: `Envelope`, `ActionSpec`, `Risk`, `Event`, `Severity`, `ModuleState`, `ModuleInfo`, `CoreStatus`, `TrayState`, `TrayGlyph`, `Check`, `rpc::{Request, Response, Error, method, code}`. Tests: exercised through kernel, ipc, ffi and CLI tests. |
| `navori-redact` | `crates/redact/` | Sensitive Data Shield. Surface: `redact_text`, `redact_json`, `is_sensitive_path`, `Redacted`, `Finding`. Tests: `tests/{redact_text,redact_json,sensitive_paths}.rs`. |
| `navori-config` | `crates/config/` | Profiles, paths, `config.toml`. Surface: `Profile`, `Paths`, `Config`, `JournalConfig`, `ConfigLoad`, `JournalSweep`, `ConfigReport`, `WholeFileReason`. Tests: `tests/{paths,config_file}.rs`. |
| `navori-store` | `crates/store/` | SQLite store. Surface: `Store` (`open`, `migrate`, `kv_get/kv_set`, `journal_append/recent/prune`, `with_conn`), `Migration`, `MigrationReport`, `StoreError`. Tests: `tests/{migrations,kv_and_journal}.rs`. |
| `navori-kernel` | `crates/kernel/` | Module runtime. Surface: `Module`, `ModuleAction`, `ActionDoc`, `ModuleCtx`, `ModuleConfig`, `NoConfig`, `Request`, `ActionError`, `Kernel`, `KernelBuilder`, `RestartPolicy`, `DispatchError`, `EventBus`, `Scheduler`, `WaitHandle`, `Wake`, `TrayContribution`, `compose_tray`, and the built-in feature Module `System`. Tests: `tests/kernel.rs`, `tests/scheduler.rs`, `tests/tray.rs`. |
| `navori-ipc` | `crates/ipc/` | Local JSON-RPC. Surface: `Server`, `Client`, `Handler`, `IpcError`, `MAX_LINE_BYTES`. Tests: `tests/ipc.rs`. |
| `navori-ffi` | `crates/ffi/` | Composition root and UniFFI surface (static lib + `lib`). Surface: `NavoriCore` (`start`, `status`, `actions`, `doctor`, `dispatch`, `set_panel_visible`, `system_did_wake`, `shutdown`), `SnapshotListener`, `CoreConfig`, `CoreSnapshot` and view records (including `TrayView`, `TrayGlyphView`, and `RunningView` with its project, session and port records, built in `src/running.rs` from the Ports and Awake states), `ConfigNotice`, `CoreError`; `CoreConfig` carries `home_dir` and `helper_path`, which feed the Integrations Module. Tests: `tests/{core,config_privacy,integrations,running}.rs`. |
| `navori-cli` (binary `navori`) | `apps/cli/` | Command-line client over IPC, including `navori hook`, the Harness hook command (silent, always exit 0). Tests: `tests/cli.rs` (real binary against an in-process core). |
| `xtask` | `xtask/` | Build and repo tooling (`cargo xtask <command>`). Verified by use, not unit tests. |
| Swift `Navori` (app target) | `apps/macos/Navori/` | The Shell: `App/` lifecycle, `Core/CoreStore`, `StatusBar/`, `Features/`, `Platform/`. Swift 6 mode. Verified by build and launch. |
| Swift `NavoriCore` + `NavoriCoreFFI` | `apps/macos/Generated/` | Generated bindings (Swift 5 mode) and the Rust static library as an xcframework. Never edited by hand. |

**Built (R1)**

| Module | Path | Responsibility |
|---|---|---|
| `navori-macos` | `crates/macos/` | Platform FFI. Surface: traits `ProcessSource`, `ExitSource`, `PowerAssertions`, `Signals`; `MacProcesses`, `KqueueExits`, `MacPower`, `MacSignals`, `AssertionGuard`; errors `PlatformError`, `ReadError`, `WatchError`, `PowerError`. Feature `fake`: `FakeProcesses`, `FakeExitsHandle`, `FakePower`, `FakeSignals`. The only hand-written `unsafe`. Tests: `tests/{process_source,procargs,exits,exits_latency,exits_drop,exits_idle,power,signals}.rs`. |
| `navori-procs` | `crates/procs/` | Process model. No `unsafe`. Surface: `Snapshot`, `ProcId`, `resolve_location`, `Harness`, `sessions`, `harness_of`, `activity`, `ProcWatcher`. Tests: `tests/{snapshot,location,sessions,harness_of,activity,watcher,no_threads,real_os}.rs`. |
| `navori-mod-awake` | `modules/awake/` | Feature Module: Leases and the power assertion. Surface: `Awake`, `AwakeAction`, `AwakeState`, and its `tray()` contribution to the menubar icon; other units reach it through the kernel. Tests: `tests/{tray,duration,manual,command,session,stop,restore,display,refusal,restart,actions,idle,real_os}.rs`. |
| `navori-mod-integrations` | `modules/integrations/` | Feature Module: reports, installs, repairs and removes Navori's hooks in each Harness's Hook file. Surface: `Integrations`, `IntegrationsEnv`, `inspect`, `IntegrationsAction`, `Harness`, `IntegrationsState`, `HarnessStatus`, `State`, `Offer`, the `HOOK_*` event kinds; other units reach it through the kernel. Tests: `tests/{status,install,repair,remove}.rs`. |

**Built (ASK walking skeleton)**

| Module | Path | Responsibility |
|---|---|---|
| `navori-llm` | `ai/llm/` | Provider-neutral LLM client. Surface: `LlmProvider`, `OpenAiCompat`, `ChatRequest`, `ChatReply`, `Message`, `Role`, `LlmError`, `DEEPSEEK_BASE_URL`, `DEEPSEEK_PROVIDER`. Depends on `navori-secrets` for `Secret` only, and on `reqwest` for HTTP. |
| `navori-context` | `ai/context/` | Context Gateway: key read, Shield pass over the prompt, audit row, send; fails closed. Surface: `Gateway`, `Sender`, `AuditSink`, `AuditRow`, `Outcome`, `AskError`, `SendError`, `AuditError`. Depends on `navori-redact` and `navori-secrets`, not on `navori-llm`; `navori-ffi` wires them. |

**Planned**

| Module | Path | Responsibility |
|---|---|---|
| `navori-secrets` | `crates/secrets/` | Keychain store for tokens and API keys (landed; consumed by `navori-llm`, `navori-context` and `navori-ffi`; ROAD-11 and ROAD-19 still to come). |
| `navori-mod-inbox` | `modules/inbox/` | Feature Module: Needs you — Decisions, To-dos, Updates (R2). |
| `navori-mod-ports` | `modules/ports/` | Feature Module: lists the TCP ports the user's own processes listen on, grouped by project, worktree and process, and stops exactly the listeners and containers a `kill` request names (Action `kill`, Risk `destructive`; `src/stop/` holds the fresh-read resolver, the process stopper and the orchestration task). Surface: `Ports`, `PortsEnv`, `OpenProcesses`, `PortsAction`, `Selector`, `Target`, `KillReply`, `TargetResult`, `Outcome`, `Scanner`, `Scan`, `BackendPort`, `PortsState`, the group, entry and kind types, and the stop seam (`FreshRead`, `Guard`, `Resolved`, `StopClass`, `resolve`, `stop_process`, `database_image`, `Containers`, `Stopped`); other units reach it through the kernel. Tests: `modules/ports/tests/` (scan, kind, place, docker, docker client, open, kill, image, stop resolve, stop process, stop docker, stop orchestration, timing, failure, harness; `real_os.rs` is ignored). |
| `navori-mod-github` | `modules/github/` | Feature Module: pull requests, checks, approve/merge (R4). |
| `navori-models`, `navori-agent`, `navori-memory` | `ai/{models,agent,memory}/` | AI runtime: model routing, agent loop and policy, memory (R2). |
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
