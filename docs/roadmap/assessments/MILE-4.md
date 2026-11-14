# Assessments: MILE-4 — Ports (R3)

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
**Committed baseline:** 74cd5dee1fc22edff9d074003074babdfa75d8cf
**Candidate closing revision:** e6c0258ef0444ff219a9888e6156f226496133d4
**Roadmap revision assessed:** 74cd5dee1fc22edff9d074003074babdfa75d8cf (working tree: clean)
**Assessed:** 2026-11-14

### Agent assessment

**Outcome verdict:** achieved
**Evidence:** Members ROAD-17 → PORTS and ROAD-18 → PSTOP, both `Implemented`. Bindings come from `docs/specs/catalog/app.md`, the shard the router points to, and the requirements files agree. PORTS scans the listening TCP ports without `lsof` and places each port under its project and worktree. It names the Harness that started the port (PORTS-3.1), attributes Docker ports to their containers, and since `93590e3` answers a list asked for before the first scan. PSTOP's `Running` menu groups ports by project. Hovering a project opens its child menu, and pressing `Stop N ports` then confirming frees the whole group: two actions (PSTOP-5.1, amended 2026-11-05). The `ports` module tests pass at the candidate (19 suites, 0 failed). The operator drove the menu half by hand on 2026-11-05 (`tasks.md` Task Step 3, no recording kept). The CLI half was driven by an agent: a stale `kill` stayed `Stale`, and a matching one returned `stopped_alone`. No live run at the candidate: Navori was not running during this assessment.
**Goal coverage:** GOAL-3 advanced — project grouping and the two-action group stop are on screen. Worktree and Harness are shown only in each port row's tooltip and VoiceOver sentence, not on the visible row (PSTOP-6.9, operator decision recorded in the spec). A Docker port never names a Harness (PORTS-3.8, a known limit).
**Deferrals:** None
**Plan accuracy:** +0 added · 0 moved out · 0 deferred · 0 days. The commitment was recorded on 2026-11-14, after both members were delivered on 2026-11-04 and 2026-11-05.
**Findings:** the time to the first frame of the `Running` submenu (PSTOP-10.1) is not measured by `cargo xtask perf`, and is tied to #24 → plan-milestones (perf runs once at the final milestone) · a visible worktree and Harness on a port row would need the locked running-menu brief to change → amend-feature, only if the operator wants it
**Rationale:** Both members shipped and their behavior matches the outcome sentence. The two-action count holds after the 2026-11-05 amendment. "Grouped by project, worktree and harness" is met through project grouping on screen, with worktree and Harness carried per port in the tooltip and VoiceOver. The operator chose this split, so it is a deliberate shape rather than a gap. The weakest evidence is that the menu drive was by hand and unrecorded, and nothing was re-driven at the candidate.

### Human disposition

**Current:** Accepted
**Close decision:** Close
**History:**
- 2026-11-14 Pending
- 2026-11-14 Accepted / Close
