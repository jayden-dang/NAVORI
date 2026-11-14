# Assessments: MILE-2 — Awake (R1)

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
**Candidate closing revision:** a44b083515bbf33a71879d69f2ee6db02365d361
**Roadmap revision assessed:** 7709b0bba260657991224c36595554615106a329 (working tree: clean)
**Assessed:** 2026-11-14

### Agent assessment

**Outcome verdict:** achieved
**Evidence:** Each member is bound to one feature, all `Implemented`, and the requirements files agree with the catalog: ROAD-5 → PERF, ROAD-6 → PROCS, ROAD-7 → KCONTRACT, ROAD-8 → AWAKE, ROAD-9 → HOOKS, ROAD-28 → SOAK, ROAD-34 → MENU, ROAD-35 → ICON, ROAD-41 → LID. Bindings come from `docs/specs/catalog/app.md`. HOOKS turns Claude Code, Codex and Grok hook events into session Leases. AWAKE holds a power assertion while a Lease is live and lets it go after the grace period. PROCS watches the session processes exit. Together these keep the Mac awake while a session works and let it sleep afterwards, with no manual action. The operator ran real sessions on 2026-11-05 (HOOKS `tasks.md` Task 15 Step 1): session Leases started and ended for Claude Code, Codex (8/8) and Grok (3/3) in the release Journal. The remaining hand checks were AWAKE Task Step 5 ×3, MENU Task Step 4 and the LID hardware checks. On 2026-11-14 the operator stated these were tested and done; there is no recording.
**Goal coverage:** GOAL-2 advanced — AWAKE, HOOKS and PROCS hold the Mac awake for a working session and release it after the grace period; live sessions of all three Harnesses were recorded. GOAL-11 advanced in part — PERF ships `cargo xtask perf` and SOAK ships the hang and leak check, but no baseline is recorded (#24; PERF Task Steps 1–3 unticked) and a green budget on a release is not shown. By the operator's rule, perf is measured once at the final milestone.
**Deferrals:** None
**Plan accuracy:** +0 added · 0 moved out · 0 deferred · 0 days. The commitment was recorded on 2026-11-14, after every member was delivered between 2026-11-01 and 2026-11-06. The baseline is the commit that introduced MILE-2's `**Commitment:** Committed` line, read with `git blame`. The pickaxe query returns `7709b0b` instead, because the identical line under MILE-4 was removed there.
**Findings:** the perf baseline is not recorded and GOAL-11 is not shown green → plan-milestones (perf runs once at the final milestone; #24) · the hand checks rest on the operator's word with no recording → plan-milestones, only if a recorded walk is wanted before a release
**Rationale:** Every member shipped, and the outcome sentence is about behavior that the real-session run of 2026-11-05 shows: Leases start on a prompt and end after the turn, for all three Harnesses. The open item is GOAL-11's budget. It is a goal citation, not part of the outcome sentence, and it is deferred by a standing operator rule rather than missed.

### Human disposition

**Current:** Accepted
**Close decision:** Close
**History:**
- 2026-11-14 Pending
- 2026-11-14 Accepted / Close — "skip phần chạy tay, tôi đã test và làm nó rồi - đánh dấu đã hoàn thiện và stamp implmeented luôn."
