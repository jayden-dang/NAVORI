# Assessments: MILE-1 — Foundation (R0)

<!--
Outcome truth for one milestone, written only by `assess-milestone` to
docs/roadmap/assessments/<MILE-N>.md. One file per milestone.

This file records whether the milestone DELIVERED. docs/roadmap/INDEX.md keeps recording
what was INTENDED, and is written only by `plan-milestones`. Neither writes into the other.

Structural rules A1–A7, the append-only rule and the disposition states are those of the
`assess-milestone` template: ordinals unique and ascending; full 40-hex SHAs; one Agent
assessment and one Human disposition per block; Current is one of Pending, Deferred,
Accepted, Overridden; a terminal Current carries a Close decision of Close or Hold; every
block after the first names what it supersedes; earlier blocks are never edited. A later
human disposition appends to the block's History.
-->

## Assessment 1

**Supersedes:** None — first assessment of this milestone
**Committed baseline:** 6d3b8bca0d3c10827fe61c9c6d2bc93269ba6c15
**Candidate closing revision:** bff12aa67b1f9998c02a4a84baed010d48c6a7b5
**Roadmap revision assessed:** 6d3b8bca0d3c10827fe61c9c6d2bc93269ba6c15 (working tree: clean)
**Assessed:** 2026-11-01

### Agent assessment

**Outcome verdict:** not achieved
**Evidence:** Members ROAD-1 → CORE, ROAD-2 → SHELL, ROAD-3 → CLI, ROAD-4 → BUILD (all Recognized; bindings read from `docs/specs/catalog/app.md`, the shard the router points to). On macOS 27.0.1 (ld-27106) `cargo xtask install` at the candidate revision fails: rustc cannot dlopen the stripped release proc-macro dylibs ("mis-aligned LINKEDIT string pool"), so no one-command install exists at this revision. With an uncommitted fix (`[profile.release.build-override] strip = "none"`) the install succeeds: `/Applications/Navori.app` is signed by `Navori Local Signing`, passes `codesign --verify --strict --deep`, runs as a menubar app, and `navori doctor` reports 6/6 ✓ (app, config, database, ipc, modules, logs). Launch at login is an opt-in Settings toggle (`SMAppService.mainApp`, LaunchAgent fallback) and was not exercised on this machine. The dev profile was already green (`navori --profile dev doctor` 6/6 ✓).
**Goal coverage:** GOAL-7 not advanced — `cargo xtask perf` does not exist yet (ROAD-5, MILE-2), so the budget is unmeasured. GOAL-8 not advanced — the release build was never installed before this assessment, so no 3 days of daily use, and the install defect above is blocking.
**Deferrals:** ROAD-27 open-source-distribution → unscheduled (2026-11-01, vision defers open source) — reason recorded, but no milestone destination: parked in the vision's Deferred scope.
**Plan accuracy:** +0 added · 0 moved out · 0 deferred · 0 days
**Findings:** release build fails on macOS 27 because proc-macros are stripped → amend-feature (BUILD); `xtask install` prints "previous kept at Navori.app.previous" when no previous app existed → amend-feature (BUILD); ROAD-27 has no milestone destination → plan-milestones
**Rationale:** The outcome sentence promises a one-command install; at the candidate revision that command fails on the author's OS, so the outcome is not met even though every member's code exists and the dev profile is healthy. The fix is a one-line profile change; once committed, a new assessment against that revision (with the login toggle exercised) is expected to change the verdict, which is material evidence and warrants a new block rather than a disposition on this one.

### Human disposition

**Current:** Pending
**Close decision:** None — required only once Current is terminal
**History:**
- 2026-11-01 Pending
