# Architecture: Navori

Status: Approved
Date: 2026-11-01

<!--
The architecture spine: the small set of INVARIANTS that keep independently-built
features consistent. Each invariant is a bold **ARCH-N** ID plus ONE imperative rule.
Feature design.md files cite the ones they rely on as `Respects: ARCH-N`, and the
`audit-trace` check verifies those citations point at a live invariant.
IDs are flat, unique forever, never renumbered or reused; retire one by strikethrough
via an ADR (write "struck ARCH-N superseded by ARCH-M" in prose only).
Rationale and the full system narrative: ../ARCHITECTURE.md.
-->

## Invariants

### Core & boundaries
- **ARCH-1** Keep business logic in the Rust core; the Swift shell only renders and calls Apple frameworks.
- **ARCH-2** Cross from Swift into Rust only through `navori-ffi`, and import its generated bindings only in `CoreStore`.
- **ARCH-3** Never let a module depend on another module; modules interact only through the event bus and dispatched actions.
- **ARCH-4** Expose every module capability as a typed action with a JSON schema, an LLM-readable description and a Risk level, served identically to UI, CLI, IPC, AI and MCP.
- **ARCH-10** Run every module as a supervised actor that owns its state, so a crash restarts it without losing queued requests.

### Safety & privacy
- **ARCH-5** Run a `Destructive` action only after explicit human approval.
- **ARCH-6** Pass everything that is persisted or sent off the machine through the Sensitive Data Shield first.
- **ARCH-7** Let data leave the machine only through the Context Gateway (to an LLM) or a module's declared API client.
- **ARCH-8** Store Navori's own secrets only in the macOS Keychain.
- **ARCH-9** Confine `unsafe` code to the platform FFI crate and UniFFI-generated scaffolding.
- **ARCH-11** Use only a Unix socket private to the user for IPC; never listen on the network.

### Data & runtime
- **ARCH-12** Version each storage owner's tables with transactional migrations, and back up existing data before every upgrade.
- **ARCH-13** Never stop the app on a missing or invalid config; fall back to defaults and report the error.
- **ARCH-14** Never share data, logs or sockets between the `release` and `dev` profiles.

### Performance (GOAL-7)
- **ARCH-15** Ship no WebView, Electron or JavaScript runtime anywhere in the app.
- **ARCH-16** While the menu is closed, fire no timer more often than every 30 seconds, and register all periodic work through the Scheduler.
- **ARCH-17** Create the AI runtime and provider HTTP clients on first use, never at startup.
- **ARCH-18** Cache network-sourced data on disk, and never refetch it just because the menu opened.

## Domains

None — all invariants live in this file. Split into `docs/architecture/<domain>.md` files if the set grows past ~25; the ARCH-N namespace stays flat.

## Enforcement today

| Invariant | Mechanical check |
|---|---|
| ARCH-3, ARCH-2 (crate side) | `cargo xtask check-deps` |
| ARCH-9 | workspace lint `unsafe_code = "forbid"` (exceptions: `navori-ffi` for UniFFI scaffolding, `navori-macos` for platform FFI); clippy `undocumented_unsafe_blocks = "warn"`; `cargo xtask check-deps` lint-drift check (`navori-macos` lint tables match the workspace tables minus `unsafe_code`; every member except `navori-ffi` and `navori-macos` inherits workspace lints) and rejects a public `unsafe` item in `crates/macos/src`; `cargo xtask lint-probe` |
| ARCH-6 | `navori-store` / `EventBus` redact on write; tests in `crates/store`, `crates/kernel` |
| ARCH-10, ARCH-12, ARCH-13, ARCH-14 | tests in `crates/kernel`, `crates/store`, `crates/config`, `crates/ffi` |
| ARCH-11 | tests in `crates/ipc` |
| ARCH-15 – ARCH-18 | `cargo xtask perf` + design review |
| ARCH-1, ARCH-4, ARCH-5, ARCH-7, ARCH-8 | design review (`inspect-invariants`) |
