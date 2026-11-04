# Codebase Map

Status: Approved
Date: 2026-11-01

## Purpose and boundary

Where things live in this repository and where new code belongs. Covers the top-level folders (existing and planned) and the placement rules for each kind of code. Does not cover which crate may depend on which (`docs/codebase/dependencies.md`), the module inventory (`docs/codebase/modules.md`), or what the components do at runtime (`docs/architecture/system.md`).

## Top-level layout

**Exists today**

| Path | Purpose |
|---|---|
| `crates/protocol/` | `navori-protocol` — wire types shared by every layer; no internal dependencies. |
| `crates/redact/` | `navori-redact` — the Sensitive Data Shield. |
| `crates/config/` | `navori-config` — Profiles, paths, `config.toml`. |
| `crates/store/` | `navori-store` — SQLite store, migrations, KV, Journal. |
| `crates/kernel/` | `navori-kernel` — module runtime, Event bus, supervision, built-in `system` module. |
| `crates/ipc/` | `navori-ipc` — Unix-socket JSON-RPC server and client. |
| `crates/ffi/` | `navori-ffi` — the UniFFI surface Swift sees and the app's composition root; also the `uniffi-bindgen-swift` bin. |
| `apps/cli/` | `navori` command-line client. |
| `apps/macos/` | The Swift Shell (SwiftPM package `Package.swift`); `Navori/` sources: `App/`, `Core/`, `StatusBar/`, `Features/`, `Platform/`, `main.swift`. |
| `apps/macos/Generated/` | Generated and gitignored: UniFFI Swift bindings (`NavoriCore/`) and `NavoriCoreFFI.xcframework`. Never edit by hand. |
| `xtask/` | Build, bundle, sign, install, CI, dependency rules (`cargo xtask <command>`). |
| `crates/macos/` | `navori-macos` — platform FFI behind safe traits `ProcessSource`, `ExitSource` and `PowerAssertions`. The only hand-written `unsafe`. |
| `crates/procs/` | `navori-procs` — process snapshot, project/worktree resolver, Harness table, child activity and exit watcher. No `unsafe`. |
| `ai/llm/` | `navori-llm` — provider-neutral LLM client (one `openai_compat` adapter, DeepSeek endpoint). |
| `ai/context/` | `navori-context` — Context Gateway: Shield pass, audit hand-off and the `Sender` trait. |
| `modules/awake/` | `navori-mod-awake` — feature Module: Leases and the power assertion. |
| `modules/integrations/` | `navori-mod-integrations` — feature Module: Navori's hooks in each Harness's config. |
| `docs/` | Product (`product/`), architecture (`architecture/`, `ARCHITECTURE.md`), codebase (`codebase/`), standards (`standards/`), UI briefs (`design/`), specs (`specs/`), ADRs (`adr/`), agent config (`agents/`). |
| `docs/features/` | One short user-facing note per shipped feature. |
| Root files | `Cargo.toml` (workspace, dependencies, lints), `rust-toolchain.toml`, `.cargo/config.toml` (xtask alias, deployment target), `clippy.toml`, `AGENTS.md`, `CONTEXT.md`, `README.md`. |

**Planned** (created by the release that first needs them — never as empty folders)

| Path | Purpose |
|---|---|
| `crates/secrets/` | `navori-secrets` — Keychain (landed; library only, no consumer yet, ROAD-11 and ROAD-19 consume it). |
| `modules/` | Remaining feature modules: `inbox/` (R2), `ports/` (R3), `github/` (R4). |
| `ai/` | Remaining AI runtime crates: `models/`, `agent/`, `memory/` (R2), `triggers/` (R5), `mcp/` (R6). |
| `integrations/` | Files Navori installs into other tools: `claude-code/`, `codex/`, `grok/` hook and MCP snippets (R1, R6). |

## Placement rules

- **A new feature module** goes in `modules/<name>/` as crate `navori-mod-<name>`, laid out as: `lib.rs` (the `Module` impl), `actor.rs` (the actor loop), `action.rs` (the Action enum with LLM-readable doc comments and Risk), `state.rs` (the snapshot), `context.rs` (state → context fragment for the AI), `event.rs` (domain Events with a one-sentence summary), `domain/` (pure logic, heavily tested), `infra/` (IO: HTTP, auth, its own SQLite tables).
- **Code that needs `unsafe` or calls a C/Objective-C system API** goes in `crates/macos/` behind a safe trait; nowhere else.
- **Logic shared by several modules but owned by none** (process model, secrets) goes in its own `crates/<name>/`, not inside a module and not inside the kernel.
- **AI runtime code** goes under `ai/`; it reaches modules only through ActionSpecs and context fragments, never by importing a module crate.
- **A type that crosses a process or language boundary** (IPC, CLI output, FFI) goes in `crates/protocol/` if it is wire data; FFI-only view types and the UniFFI exports go in `crates/ffi/`.
- **Wiring a new module or service into the app** happens only in `crates/ffi/` (the composition root); the CLI never links the Core.
- **A new CLI command** goes in `apps/cli/src/` and talks to the running app over IPC.
- **Swift UI for a feature** goes in `apps/macos/Navori/Features/<Feature>/`; menubar and panel chrome in `StatusBar/`; Apple-framework services the Core calls (PlatformServices) and system integrations in `Platform/`; only `Core/CoreStore.swift` imports the generated bindings.
- **Tests** go in each crate's `tests/` folder and use the public API (see `docs/standards/testing.md`); shared test helpers stay inside the test files of that crate.
- **Build, packaging and repo-maintenance commands** go in `xtask/`, not in shell scripts.
- **Documentation** goes under the matching `docs/` folder; a decision that is hard to reverse goes in `docs/adr/`.

## Not spine / not feature registry

This document is **not** the architecture invariant spine (`docs/architecture/INDEX.md`) and **not** the feature registry (`docs/specs/INDEX.md`). It only guides folder placement and layout.

## Blockers

None.
