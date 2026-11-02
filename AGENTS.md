# AGENTS.md

Agent instructions for the Navori repo. Project overview and everyday commands: [README.md](README.md); architecture: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

Navori is an AI-first personal cockpit for macOS (Apple Silicon): a menubar app with a Rust core and a Swift (AppKit + SwiftUI) shell — single user (the author), local-first, with a hardcore performance budget.

## Read first

| What | Where |
|---|---|
| Product vision, goals (GOAL-N), non-goals, scope | `docs/product/vision.md` |
| Architecture invariants (ARCH-N) — cite as `Respects: ARCH-N` | `docs/architecture/INDEX.md` |
| Engineering standards, Definition of Done | `docs/standards/` |
| Glossary (use these names) | `CONTEXT.md` |
| Milestones MILE-N / roadmap items ROAD-N | `docs/roadmap/INDEX.md` |
| Decisions of record | `docs/adr/` |
| System design narrative, design seeds | `docs/ARCHITECTURE.md` |
| Menubar UI (locked brief) | `docs/design/2026-11-01-menubar-ui-brief.md` |

## Commands

```sh
cargo xtask ci          # gate before every commit: fmt, clippy -D warnings, tests, check-deps, Swift build
cargo xtask perf        # GOAL-11 budget; --record writes perf-baseline.json when the run passes
cargo xtask run         # build + launch "Navori Dev.app" (dev profile; separate data/logs/socket)
cargo xtask install     # release build → /Applications/Navori.app
cargo xtask check-deps  # crate dependency rules (ARCH-3)
cargo xtask outdated    # dependency versions vs latest stable
cargo test -p <crate>   # one crate's tests
```

Talk to a running dev build: `"build/dev/Navori Dev.app/Contents/Helpers/navori" --profile dev status|doctor|events|actions`.

## Rules of the road

- English everywhere: docs, code, comments, commits, UI strings.
- Test-first at agreed seams (`docs/standards/testing.md`); fakes only at system boundaries.
- Conventional Commits; one logical change per commit; every commit builds.
- Never commit `target/`, `build/`, `apps/macos/Generated/`, signing material or `/draft-ui`.
- Commit or push only when the user asks.

## Agent skills

This repo is configured for a spec-driven skill set.

- Feature flow: `frame-change` → `specify-behavior` → `design-solution` →
  `plan-tasks` → `build-in-waves`
- Vague ask you want turned into a prompt for a fresh session: `/forge-prompt` (user-run)
- Work you want an agent to keep doing unattended until it is verifiably done: `/forge-goal` (user-run)
- Bug on-ramp: `root-cause` (clear unexpected behavior first, then a guarded fix);
  deployed env: `debug-remote` then `root-cause`; telemetry readiness:
  `assess-observability`
- Capture a conversation/spec/idea into tracker issues: `/publish-issues` (user-run)
- Incoming issues and PRs: `/triage` (user-run)
- Traceability check: the docs-only `audit-trace` skill — run by `prove-claim` and `cut-release`;
  keep it clean
- Project docs (layer enabled): `/define-project` maintains
  `docs/product/vision.md`, the `docs/architecture/` invariant spine, and
  `docs/standards/` (engineering standards SSOT; no `docs/product/guidelines.md`);
  the feature skills consult them. Glossary: `CONTEXT.md`.

Repo config the skills read:

- verify commands, release steps, Remote environments: `docs/agents/project.md`
- cold-start drive (Launch / Doctor / Drive / Evidence): `docs/agents/verify.md`
- Team composition (roster, ownership notes, workflow band): `docs/agents/project.md` (`## Team`)
- Issue tracker operations: `docs/agents/issue-tracker.md`
- Triage label mapping: `docs/agents/triage-labels.md`
