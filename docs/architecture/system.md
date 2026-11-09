# Architecture domain: System

Status: Approved
Date: 2026-11-01

## Purpose and boundary

The major parts of Navori and how they relate: the app process, the Rust Core (kernel, modules, AI runtime), the Swift Shell, the `navori` CLI and the external actors around them. It records both what exists today (R0 Foundation) and the committed shape of the planned parts (R1–R6, per `docs/roadmap/INDEX.md` and `docs/product/vision.md`).

Not covered here, by design:
- Binding rules — `docs/architecture/INDEX.md` (ARCH-N).
- Threads, lifecycle, startup and shutdown — `docs/architecture/runtime.md`.
- Storage, schemas, retention — `docs/architecture/data.md`.
- External APIs and system frameworks in detail — `docs/architecture/integrations.md`.
- Where code goes and which crate may depend on which — `docs/codebase/`.
- Per-feature design (Awake, Ports, GitHub) — the feature's spec; until then the seeds in `docs/ARCHITECTURE.md`.

## Context

Navori is **one macOS app process** (`Navori.app`, accessory app with no Dock icon) that hosts two halves linked as a static library through UniFFI:

- the **Shell** — Swift (AppKit + SwiftUI) on the main thread: the menubar item and its menu, Settings, and the Apple frameworks that need the app bundle or a user permission;
- the **Core** — Rust, with its own tokio runtime: the kernel and its modules, storage, the IPC server and, from R2, the AI runtime.

Around that process:

```
 user ── menubar / Settings ──► Shell ──(UniFFI)──► Core ◄──(Unix socket, JSON-RPC)── navori CLI ◄── user, scripts,
                                                     │                                              harness hooks (R1)
                                                     ├──► macOS: IOKit power, sysctl / proc_pidinfo / proc_pidpath, kqueue (R1, R3)
                                                     ├──► GitHub API (R4)
                                                     ├──► cloud LLM providers, via the Context Gateway only (R2)
                                                     └──◄ MCP clients: Claude Code, Codex, Grok via `navori mcp` (R6)
```

Main flows:
1. **Command:** the Shell, the CLI, Navori AI or an MCP client sends an Envelope `{module, action}` → the kernel deserializes it into the module's typed Action → the module actor answers (or the call times out).
2. **Change:** a module updates its state and emits Events → each Event passes the Sensitive Data Shield, is written to the Journal and broadcast → the Core pushes a coalesced snapshot to the Shell.
3. **AI (R2+):** a question or a trigger → the Context Engine assembles context → the Context Gateway redacts it → a provider streams the answer → tool calls run as module Actions under the Risk policy; Destructive ones become Decisions in Needs you.

## Components

Status: **Built** = in the tree at R0 · **Built (Rn)** = in the tree since that release · **Planned (Rn)** = committed in the roadmap, not yet in the tree.

| Component | Status | Responsibility | Where |
|---|---|---|---|
| Shell | Built | Menubar item, menu/panel, Settings, launch at login; renders Core snapshots; holds no business logic (ARCH-1). | `apps/macos/Navori/` |
| CoreStore | Built | The only main-actor bridge between the Shell and the Core bindings (ARCH-2). | `apps/macos/Navori/Core/CoreStore.swift` |
| NavoriCore (composition root) | Built | Boots config → logging → store → kernel → IPC → session marker → snapshot pump; the UniFFI surface Swift sees; `doctor`. | `crates/ffi` |
| Kernel | Built | Module registry, Envelope routing with a dispatch timeout, supervision (restart with backoff, inbox kept), Event bus (redact → journal → broadcast). | `crates/kernel` |
| Module contract | Built | `Module` + `ModuleAction`: typed Actions with JSON schema, LLM-readable description and Risk; a state snapshot. | `crates/kernel/src/module.rs` |
| `system` module | Built | Liveness (`ping`) and Journal retention. | `crates/kernel/src/system.rs` |
| Store | Built | SQLite (WAL): per-Owner migrations with backup, KV, the Journal. | `crates/store` |
| Sensitive Data Shield | Built | Detects and masks secrets in text and JSON; flags credential paths. | `crates/redact` |
| Config | Built | Profiles (`release`/`dev`), filesystem layout, `config.toml`. | `crates/config` |
| IPC | Built | JSON-RPC 2.0 over a user-private Unix socket; doubles as the single-instance lock. | `crates/ipc` |
| Protocol | Built | Wire types shared by every layer (Envelope, ActionSpec, Event, status, RPC frames). | `crates/protocol` |
| `navori` CLI | Built | `status`, `doctor`, `events`, `actions`, `dispatch`, `awake`, `hook`, `ask`; later `mcp`. | `apps/cli` |
| xtask | Built | Build core + bindings, bundle, sign, install, CI gate, dependency rules. | `xtask` |
| Scheduler | Built | Periodic work for all modules: fast while the menu is open, slow in the background, a timer parks on uptime, and an overdue timer may fire during dark wake (ARCH-16). | `crates/kernel` |
| Platform FFI | Built (R1) | Safe traits `ProcessSource`, `ExitSource` and `PowerAssertions` (`MacProcesses`, `KqueueExits`, `MacPower`). The only hand-written `unsafe` (ARCH-9). | `crates/macos` |
| Process model | Built (R1) | Snapshot, project/worktree location, Harness table, child activity and process-exit watcher. No `unsafe`. Uses `ProcessSource` and `ExitSource`. | `crates/procs` |
| Secrets | Built (R2) — consumed by `navori-llm`, `navori-context` and `navori-ffi`; ROAD-11 and ROAD-19 still to come | Keychain storage for tokens and API keys (ARCH-8). | `crates/secrets` |
| Awake module | Built (R1) | Leases → at most one power assertion per kind; harness-session tracking. | `modules/awake` |
| Ports module | In progress (R3) | Listening ports by project/worktree/harness; `ports.list` replies with the last scan. Stop with approval is planned. | `modules/ports` |
| GitHub module | Planned (R4) | PR inbox, checks, approve/merge; OAuth device flow (ADR 0002). | `modules/github` |
| Needs you inbox | Built (R2) | Decisions, To-dos and Updates between the user and Navori AI; approvals for Destructive Actions. | `modules/needs_you`, `modules/todo` (formerly proposed as `modules/inbox`) |
| PlatformServices | Planned (R2) | Swift-implemented services the Core calls: notifications, frontmost app/AX, screen capture + OCR + blur, permissions. | `apps/macos/Navori/Platform/` |
| LLM providers | Partial (R2) — `openai_compat` for DeepSeek with tool calls, reasoning text and usage exists; no streaming or other adapters yet | Canonical chat/tool/stream format; adapters `anthropic`, `openai`, `openai_compat`, `gemini`. | `ai/llm` |
| Model router | Planned (R2) | Model catalog, roles (`chat`, `fast`, `vision`, `embedding`), fallback, usage and budget. | `ai/models` |
| Context Engine + Gateway | Partial (R2) — the Context Gateway exists (key read, Shield pass over text and structured chat requests, one audit row per model call, send); the Context Engine does not | Assembles the current situation; the only path to an LLM, redacted and audited (ARCH-7). | `ai/context` |
| Agent runtime | Partial (R2) — a Turn runs as an Agent loop over Read Actions only (`NavoriCore::run_agent_turn`); each Turn is kept in a Conversation after the Shield and ends with one `ai.turn_ended` Event. Limits: 8 Steps and 200 000 tokens per Turn, 10 s per Action; the token check is made when a Step ends, so the Step that reaches the limit still runs its tool calls. Approvals and non-Read Actions are not built | Agent loop; tool registry built from every module's ActionSpecs; Risk policy and approvals. | `ai/agent` |
| Memory | Planned (R2) | Facts, episodic summaries, embeddings. | `ai/memory` |
| Triggers | Planned (R5) | Proactive rules on the Event bus, within a token budget and quiet hours. | `ai/triggers` |
| MCP | Planned (R6) | MCP client (external tools) and server (`navori mcp`). | `ai/mcp` |

## Not the invariant spine

This narrative domain file does **not** redefine `docs/architecture/INDEX.md` ARCH-N invariants. It references them where a component exists to honor one (ARCH-1, ARCH-2, ARCH-7, ARCH-8, ARCH-9, ARCH-16); when this file and the spine disagree, the spine wins.
