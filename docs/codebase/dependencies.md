# Codebase Dependencies

Status: Approved
Date: 2026-11-01

## Purpose and boundary

Which crate may depend on which inside the workspace, which edges are forbidden and why, and the policy for third-party dependencies. Covers Rust crates and the Swift package. Does not cover where folders live (`docs/codebase/map.md`), the module inventory (`docs/codebase/modules.md`), or runtime calls between components (`docs/architecture/system.md`). Dev-dependencies (tests) are exempt from the direction rules.

## Allowed dependency directions

**Today** (from `cargo metadata`)

| From | To | Rule |
|---|---|---|
| `navori-protocol` | — | allowed: none — stays free of internal dependencies |
| `navori-redact` | — | allowed: none |
| `navori-config` | — | allowed: none |
| `navori-store` | `navori-protocol`, `navori-redact` | allowed — the Journal redacts on write |
| `navori-kernel` | `navori-protocol`, `navori-redact`, `navori-store` | allowed |
| `navori-ipc` | `navori-protocol` | allowed |
| `navori-ffi` | every foundation crate (`config`, `ipc`, `kernel`, `protocol`, `redact`, `secrets`, `store`), `navori-mod-awake`, `navori-mod-integrations`, `navori-mod-ports`, `navori-llm` and `navori-context` | allowed — the composition root; later also other `modules/*`, `ai/*` |
| `navori` CLI (`navori-cli`) | `navori-config`, `navori-ipc`, `navori-protocol` | allowed — thin client; dev-dependencies for end-to-end tests only: `navori-ffi`, `navori-context`, `navori-secrets` (`fake` feature), `navori-store`, `futures` and `tokio` |
| `xtask` | — | allowed: none (tooling only) |
| `navori-macos` | — | allowed: none — leaf |
| `navori-procs` | `navori-macos` | allowed — through the safe traits |
| `navori-secrets` | — | allowed: none — leaf; consumed by `modules/*` and `ai/*` (rows below) and may be opened by the composition root `navori-ffi` (it may depend on any foundation crate) |
| `navori-llm` | `navori-secrets` | allowed — provider-neutral LLM client; takes only the `Secret` type from `navori-secrets`; no edge to `navori-context` or any Module |
| `navori-context` | `navori-redact`, `navori-secrets` | allowed — Context Gateway; `redact` is required (nothing reaches an LLM unredacted, ARCH-6); no edge to `navori-llm` (the `Sender` trait is wired in `navori-ffi`) |
| `navori-mod-awake` | `navori-kernel`, `navori-macos`, `navori-procs`, `navori-store` | allowed — feature Module; depends on no other Module |
| `navori-mod-ports` | `navori-kernel`, `navori-macos`, `navori-procs` | allowed — feature Module; depends on no other Module |
| `navori-mod-integrations` | `navori-kernel` | allowed — feature Module; depends on no other Module, and not on `navori-macos`, `navori-procs` or `navori-store` (the last only as a dev-dependency) |
| Swift `Navori` target | `NavoriCore` (generated) → `NavoriCoreFFI` (xcframework) | allowed — only `CoreStore` imports `NavoriCore` (ARCH-2) |

`modules/*` may name `navori-macos` for the traits `ProcessSource`, `ExitSource` and `PowerAssertions`. `navori-ffi` may too (composition root). No other crate may. `cargo xtask check-deps` enforces that edge.

**Planned**

| From | To | Rule |
|---|---|---|
| `modules/*` | `kernel`, `protocol`, `store`, `config`, `redact`, `procs`, `secrets` | allowed |
| `ai/agent` | `ai/llm`, `ai/models`, `ai/context`, `ai/memory`, `ai/mcp`, `kernel`, `protocol` | allowed |
| `ai/memory` | `navori-redact` | required — nothing reaches memory unredacted (ARCH-6) |
| `ai/*` | `navori-secrets` | allowed — provider keys from the Keychain |

## Forbidden edges

- **anything → `navori-ffi`** (except dev-dependencies): it is the composition root; depending on it would invert the layering.
- **`navori-protocol` → any internal crate:** wire types must stay at the bottom so every layer can share them.
- **`navori` CLI → anything beyond `config`, `ipc`, `protocol`:** the CLI talks to the running app over IPC; it must never link the Core.
- **`modules/*` → `modules/*`:** modules interact through the Event bus and Actions only (ARCH-3).
- **`modules/*` → `ai/*`** and **`ai/*` → `modules/*`:** the AI sees modules only through ActionSpecs and context fragments, so a new module becomes an AI tool without AI code changes.
- **foundation crates (`crates/*` except `ffi`) → `modules/*`:** shared infrastructure cannot know about features.
- **any crate other than `navori-macos` and `navori-ffi` using `unsafe`:** ARCH-9 (enforced by the workspace lint `unsafe_code = "forbid"`).
- **any Swift file other than `CoreStore` → `NavoriCore`:** ARCH-2.

## Third-party stack

- Every third-party crate is declared once in `[workspace.dependencies]`, pinned to the latest stable release, and inherited with `workspace = true`; `cargo xtask outdated` reports drift.
- Prefer the standard library and crates already in the workspace; a new crate needs a reason it cannot be done with what exists (performance budget, GOAL-11).
- No WebView, Electron or JavaScript runtime dependency of any kind (ARCH-15).

| Area | In use (R0) | Planned |
|---|---|---|
| Async | `tokio`, `tokio-util`, `futures` | — |
| Errors | `thiserror` (libraries), `anyhow` (edges) | — |
| Serialization, schema | `serde`, `serde_json` (`preserve_order` workspace-wide, so object key order survives a round trip), `toml`, `schemars` | — |
| IDs, time | `ulid` | `time` or `jiff` when dates are needed |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` | — |
| Storage | `rusqlite` (bundled SQLite) | `sqlite-vec` for memory search |
| Text matching | `regex` | — |
| FFI | `uniffi` 0.32, `libc`, `objc2-io-kit`, `objc2-core-foundation` | — |
| CLI | `clap` | — |
| Secrets | `keyring-core`, `apple-native-keyring-store` (feature `keychain`) | — |
| HTTP, SSE | `reqwest` 0.13.5 (default features and `json`; rustls TLS by default; used by `navori-llm`) | `eventsource-stream` (R2) |
| GitHub | — | `graphql_client` (R4) |
| MCP | — | `rmcp` (R6) |
| Runtime files | `tempfile` (atomic writes in the integrations Module) | — |
| Tests | `tempfile`, `assert_cmd`, `predicates` | `wiremock`, `insta`, `proptest` |
| Swift | AppKit, SwiftUI, Observation, ServiceManagement | `KeyboardShortcuts` (hotkey), a SwiftUI Markdown renderer, ScreenCaptureKit, Vision, CoreImage, UserNotifications (R2) |

## Not a runtime enforcer

This document guides design and planning. It is **not** a runtime import checker. The edges under "Forbidden edges" that concern Rust crates are checked at build time by `cargo xtask check-deps` (part of `cargo xtask ci`); the `unsafe` rule by the workspace lint; the Swift import rule by review.
