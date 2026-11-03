# Project configuration (agent-facing)

Written by `configure-repo`. Skills read this file for repo-specific **machine config** —
commands, globs, paths — plus **posture** and **team** (below). The project-docs layer
(`/define-project`) is enabled: product vision and goals in `docs/product/vision.md`
(GOAL-N), architecture invariants in `docs/architecture/INDEX.md` (ARCH-N), and engineering
standards — including the Definition of Done — in `docs/standards/` (the standards SSOT;
`plan-tasks` sources guidelines from there). `docs/ARCHITECTURE.md` is the system design
narrative and rationale.

## Project posture

The project's standing intent, lifecycle phase, and compatibility obligation. Skills read
this instead of re-asking: `frame-change` and `clarify-decisions` right-size migration /
compat / deprecation against **Compat obligation**; `interpret-session` reuses all three.
`clarify-decisions` production coverage needs Delivery **Production** + Lifecycle
**Cut Released / Scaling / Maintenance** + operate/launch surface (or an ops ask); MVP,
absent, and Early/Active stay off. Edit these lines when the project moves phase.

**SSOT:** the **Compat obligation** values and what each one means live only here. Skills
carry a one-clause copy of the derivation because a repo configured before the field existed
has no Compat obligation text in this file for them to read — change the meanings here, and
the derivation in `frame-change`, `clarify-decisions`, and `interpret-session` with it.

Compat obligation values (the line is absent below, so it is **derived**): **None:** nothing
outside this repo depends on any current schema, endpoint, or file format. **Internal:** other
services or teams in the same org read them. **External:** users, customers, or third-party
clients depend on them. Lifecycle Idea / Early development / Active development → **None**;
Cut Released / Scaling / Maintenance → **External**. Add a `- **Compat obligation:**` line only
to override that default (e.g. once the installed app's data in
`~/Library/Application Support/Navori/` must survive upgrades).

- **Delivery intent:** `Production` — the quality bar the output must meet. A bar, not a release state: **Production** says how well the code must be built, never that the project has shipped.
- **Lifecycle stage:** `Active development` — where the project is in its life.
- **Default PR base:** `main` — `land-branch` reads it as the third rung of its base-resolution ladder.
- **Default landing action:** `merge`

These are distinct from product goals (what success looks like): posture is *how carefully
to build right now*, not *what to build*.

## Team

Who works on this repo and how skills should package collaboration.
Skills that plan, review, or hand off read this section when present and
right-size **packaging** only (Solo / Small / Multi) — Iron Law gates never
change. Edit freely; re-run `/configure-repo` to re-draft from git/CODEOWNERS.
If this section is absent, skills do not invent a team.

**SSOT:** **band** derivation and the **packaging** matrix live only here.
Consumers **read** this section; they do not re-copy these rules into skill bodies.

### Roster

- Contributor — Jayden Đặng

Suggested roles (freeform allowed): Tech Lead, Backend Engineer, Frontend Engineer, Full-stack Engineer, Designer, Product Manager, QA, DevOps/SRE, Docs.

### Ownership notes (optional)

- none

### Workflow band

- **Override (optional):** blank — skills derive the band (currently **Solo**).
- **Derive (when override blank):**
  1. Headcount from **Roster only**: each `Role — Name` = 1; each `N × Role` / `N Role(s)` adds N.
     Ignore Ownership notes and placeholders (`<…>`).
  2. Buckets: empty roster → **no band** (same packaging as Team absent); 1 → **Solo**;
     2–4 → **Small**; ≥5 → **Multi**.
  3. Specialty upgrade only: if **Small** and ≥3 distinct role titles (case-insensitive, trimmed),
     upgrade to **Multi**. Never downgrade Multi→Small.

### Packaging matrix

| Band | Packaging |
|---|---|
| **Solo** | Lean multi-person ritual language; no invented peer reviewers/assignees; agent-as-pair; full gates |
| **Small** | Design-review checkpoints; ownership boundaries via optional freeform notes; name people when roster has names |
| **Multi** | CODEOWNERS-aware review language when ownership notes exist; explicit review responsibilities as prose; write-handoff/docs emphasis |
| **(no band)** | Team absent, or empty roster with blank override — pre-feature default; do not invent a team; do not hard-fail |

## verify commands

Run in this order; all must pass before any completion claim.

| Check | Command |
|---|---|
| Typecheck | `cargo check --workspace --all-targets` |
| Lint | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings` |
| Unit tests | `cargo nextest run --workspace` |
| E2E / smoke | none as a command — the boot-and-drive recipe is `docs/agents/verify.md` |

Single test file: `cargo nextest run -p <crate> --test <file>` — e.g.
`cargo nextest run -p navori-redact --test redact_json`. Builds only that test binary.

Task gate (changed units + their dependents; `none` when the whole suite is fast):
`none` — the whole suite is fast; the suite is the gate.

Wave gate (compile + lint every target after a parallel merge; `none` = typecheck + lint):
`cargo xtask ci` — fmt check, clippy `-D warnings`, `cargo test --workspace`, `check-deps`
(crate dependency rules), `build-core` (UniFFI bindings + XCFramework), Swift build. It is
the only command that compiles the Swift shell (`apps/macos`).

The whole suite runs once, at close. Each task runs its task gate instead, escalating
to the whole suite when the change reaches what the build graph cannot see (schema,
migrations, shared fixtures, manifests, generated contracts, a root package).

The traceability check is not a command here — the `audit-trace` skill runs it as
`grep`/`git` over `docs/specs/` (and optional architecture). It is **docs-only**
and does not grep application tests for requirement IDs.

Legacy annotations in consumer code (if any) are ignored; do not require new ones.

Specs directory override: (blank — `docs/specs/`)

## Run locally (dev)

How to start the app for user-facing acceptance checks (read by `validate-api`,
`validate-ui`, `validate-feature`). Full recipe: `docs/agents/verify.md`.

| Surface | Start command | Ready signal |
|---|---|---|
| Core over IPC (Unix socket) — the API the CLI, UI and AI call | `cargo xtask run` | `"build/dev/Navori Dev.app/Contents/Helpers/navori" --profile dev doctor` exits 0 with every check ✓ |
| Menubar UI (native SwiftUI/AppKit — not a browser) | same launch | the Navori Dev icon appears in the menu bar |

Native macOS app: there is no web frontend, so kimi-webbridge does not apply. Drive the
core through the CLI. Do not add a Playwright suite for acceptance.

## Remote environments

Read by `debug-remote` and `assess-observability`. Never put tokens here.

None — not deployed

## release steps

Run in order; `cut-release` executes exactly these and does not improvise.

1. `cargo xtask ci` — the full gate.
2. `cargo xtask perf` — the GOAL-11 budget on release code packaged as the dev identity
   (`docs/standards/performance.md`). `--record` is not part of a release.
3. `cargo xtask install` — builds the release bundle, signs it with `Navori Local Signing`,
   runs `codesign --verify --strict --deep`, installs to `/Applications/Navori.app`
   (the previous install is kept as `/Applications/Navori.app.previous`) and launches it.
4. Smoke: `/Applications/Navori.app/Contents/Helpers/navori doctor` — every check ✓, exit 0.
5. Tag: `git tag v<version> && git push origin v<version>` — `<version>` is
   `[workspace.package] version` in `Cargo.toml`.

No notarization or public distribution yet (no Apple Developer ID).

## Paths

- Specs: `docs/specs/`
- ADRs: `docs/adr/`
- Glossary: `CONTEXT.md`
- Out-of-scope KB: `.out-of-scope/`
- Product vision (GOAL-N): `docs/product/vision.md`
- Architecture invariants (ARCH-N spine): `docs/architecture/INDEX.md`
- Architecture narrative: `docs/ARCHITECTURE.md`
- Engineering standards (SSOT, incl. Definition of Done): `docs/standards/`
- UI briefs: `docs/design/`
- Library docs: Context7 MCP (preferred) — connected in the user's MCP config; fall back to official docs when absent
