# Engineering Standards: Navori

Status: Approved
Date: 2026-11-01

The single source of engineering rules. Feature work, reviews and "done" claims check against these files. Architecture invariants (ARCH-N) live in `docs/architecture/INDEX.md`; this layer covers how code is written, tested, shipped and documented.

| File | Covers |
|---|---|
| [testing.md](testing.md) | test-first, seams, fakes, naming |
| [errors-logging.md](errors-logging.md) | error types, messages, CLI exit codes, logging and redaction |
| [performance.md](performance.md) | the GOAL-11 budget, how it is measured, regression policy |

## Code

- **Rust:** edition 2024, a toolchain pinned to an exact version in `rust-toolchain.toml`, bumped by a deliberate commit that passes `cargo xtask ci`, target `aarch64-apple-darwin`. Every crate inherits `version`, `edition`, `rust-version` and `[lints]` from the workspace.
- **Lints:** `unsafe_code = "forbid"` everywhere except where ARCH-9 allows it; `clippy::all` warns; `unwrap_used` / `expect_used` / `dbg_macro` / `todo` warn. `unwrap`/`expect` are allowed in tests only (`clippy.toml`).
- **Dependencies:** declared once in `[workspace.dependencies]`, pinned to the latest stable release; check drift with `cargo xtask outdated`. Prefer the standard library and existing workspace crates over new dependencies.
- **Swift:** the app target builds in Swift 6 language mode; the UniFFI-generated `NavoriCore` target stays in Swift 5 mode until UniFFI's async code is fully `Sendable` (uniffi-rs #2448). Only `CoreStore` touches the bindings (ARCH-2).
- **UI:** follows `docs/design/2026-11-01-menubar-ui-brief.md` — monochrome SF Symbols, semantic color only as a 7 pt status dot, status never by color alone, Reduce Motion / Reduce Transparency honored. Never write `*/` in a Rust doc comment exported through UniFFI (it is copied into a Swift block comment).
- **Deployment target:** `MACOSX_DEPLOYMENT_TARGET` (`.cargo/config.toml`), `LSMinimumSystemVersion` and `Package.swift` platforms must all say macOS 26.0.

## Gates

- **Before every commit:** `cargo xtask ci` — `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, `cargo xtask check-deps`, Swift build.
- **Before every release:** `cargo xtask perf` green (see [performance.md](performance.md)).
- Hosted CI is the release workflow (`.github/workflows/release.yml`, ADR 0007): on a `v*` tag it runs `cargo xtask ci`. `cargo xtask release-check` (ci + perf) stays the local gate before tagging, and perf never runs on CI. Otherwise the local gates are authoritative.

## Commits

- Conventional Commits (`feat(scope):`, `fix:`, `docs:`, `build:`, `test:`…), English, imperative subject.
- One logical change per commit, and every commit builds and passes the tests of the crates it adds.
- Secret-shaped test fixtures are split at compile time (`concat!`) so secret scanners (GitHub push protection) do not flag them.
- Never commit build output, generated bindings (`apps/macos/Generated`), signing material, or `/draft-ui` scaffolding.

## Documentation

- **Language:** English everywhere — docs, code, comments, commits, UI strings, IDs.
- Product north star: `docs/product/vision.md` (GOAL-N). Invariants: `docs/architecture/INDEX.md` (ARCH-N). Glossary: `CONTEXT.md`. System narrative: `docs/ARCHITECTURE.md`.
- A hard-to-reverse or surprising decision gets an ADR in `docs/adr/` (via `define-domain`).
- Each shipped feature gets a short note in `docs/features/<feature>.md`: what it does, settings, CLI commands, known limits.

## Definition of Done

A release (one capability) is done only when all of these hold. Trimmed for one author whose users are developers installing their own copy (vision: Users).

1. **Complete** — the capability described in its spec works end to end; nothing deferred "to the next version".
2. **Every failure is visible** — offline, quota, expired token, missing macOS permission: the UI says what is wrong and how to fix it; no endless spinner, no silent failure.
3. **Settings** — every option of the feature is in Settings; no hand-editing files required.
4. **Permissions** — first use walks through the macOS permissions the feature needs (the only onboarding Navori has).
5. **AI-ready** — actions with schema, LLM-readable description and Risk (ARCH-4); the module's context fragment; Events with a one-sentence summary; at least one eval case.
6. **Tested** — per [testing.md](testing.md); `cargo xtask ci` green.
7. **Observable** — enough `tracing` to debug from logs alone, and an entry in `navori doctor`.
8. **Fast** — `cargo xtask perf` green against the GOAL-11 budget (measured, not assumed).
9. **Documented** — `docs/features/<feature>.md`.
10. **Dogfooded** — used daily for at least 3 days with no blocking defect (GOAL-8).
11. **Upgrade-safe** — data, config keys, hook entries and Keychain names the feature adds or changes keep working for people who installed an earlier release: forward-only migrations with a backup (ARCH-12), and a rename is a migration (Compat obligation External, `docs/agents/project.md`).
