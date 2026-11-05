# Roadmap: Navori

Status: Approved
Date: 2026-11-05

<!--
The program layer: milestone INTENT, above any single feature and below the product
vision. `plan-milestones` authors it; `refresh-roadmap-status` derives progress from it
without writing anything.

This file owns intent only: outcomes, priority, ordering, membership, dependencies,
commitments, blockers, deferrals, goal dispositions. It never records how far a feature has
got — that lives once, as `Status:` in the feature's own requirements.md, mirrored into its
docs/specs/INDEX.md row. Every heading below is a REQUIRED slot — filled or `None`.

Structural rules S1–S9, the Focus rules and the ID rules are those of the `plan-milestones`
roadmap template. In short: MILE-N / ROAD-N are flat, repo-wide and permanent (retire by
strikethrough, never renumber or reuse); priority lives only in ## Focus; order between
items is the After: field; GOAL-N is defined in docs/product/vision.md and only cited here;
feature codes are never written here — a ROAD-N is a program slot that a feature binds to
through the "Roadmap item" cell of docs/specs/catalog/*.md.

Release names R0–R6 in docs/ARCHITECTURE.md map to MILE-1 … MILE-7.
-->

## Focus

**Now:** ROAD-14, ROAD-13, ROAD-12, MILE-2
**Next:** ROAD-27, ROAD-37, ROAD-11, ROAD-38, ROAD-19, ROAD-20, ROAD-21, ROAD-15, ROAD-16

| ID | Milestone | Outcome | Depends-on | Commitment |
|---|---|---|---|---|
| MILE-1 | Foundation (R0) | The user installs Navori with one command, it starts at login as a menubar icon, and `navori doctor` reports every check green. | none | Closed |
| MILE-2 | Awake (R1) | The Mac stays awake while any Claude Code, Codex or Grok session is working and goes back to normal sleep after they stop, with no manual action. | MILE-1 | Planned |
| MILE-3 | AI Assistant (R2) | The user asks Navori AI anything from the menubar, a hotkey or the terminal with any configured model, and answers its Decisions in Needs you before anything destructive runs. | MILE-2 | Planned |
| MILE-4 | Ports (R3) | The user sees every listening port grouped by project, worktree and harness, and frees a whole group in at most 2 actions. | MILE-2 | Planned |
| MILE-5 | GitHub (R4) | The user sees pull requests needing review, failing checks and conflicts counted in the menubar, and opens, approves or merges them from the submenu. | MILE-3 | Planned |
| MILE-6 | Proactive (R5) | Navori AI tells the user on its own, within a daily budget and quiet hours, when CI fails, a review arrives, an Agent session finishes or a port conflicts. | MILE-4, MILE-5 | Planned |
| MILE-7 | MCP (R6) | Claude Code, Codex and Grok call Navori's tools over MCP under the same approval policy, and Navori AI uses tools from external MCP servers. | MILE-3 | Planned |
| MILE-8 | Auto Review | When someone directly requests the user's review on a pull request in a repository the user enabled, that pull request gets a started comment, then findings and an approval or a change request by a fixed severity rule with no action from the user, and a re-request on new commits reviews it again. | MILE-5, MILE-6 | Planned |
| MILE-9 | Public release | Anyone on an Apple Silicon Mac with macOS 26 installs Navori with `brew install --cask` from the Navori tap, and every later release reaches them through `brew upgrade` with their config, database, hooks and Keychain access intact. | MILE-2, MILE-4 | Planned |

## MILE-1 — Foundation (R0)

**Outcome:** The user installs Navori with one command, it starts at login as a menubar icon, and `navori doctor` reports every check green.
**Goals:** GOAL-11, GOAL-8
**Members:**
- **ROAD-1** core-foundation — After: none — Surfaces: `crates/protocol/`, `crates/redact/`, `crates/config/`, `crates/store/`, `crates/kernel/`, `crates/ipc/`, `crates/ffi/`
- **ROAD-2** menubar-shell — After: ROAD-1 — Surfaces: `apps/macos/`
- **ROAD-3** cli-and-doctor — After: ROAD-1 — Surfaces: `apps/cli/`
- **ROAD-4** build-sign-install — After: none — Surfaces: `xtask/`, `.cargo/config.toml`
**Depends-on:** none
**Commitment:** Closed 2026-11-01
**Closed:** 5e66b16a85d2776ca98afbf0606c9c1f73a366b7 (assessment: `assessments/MILE-1.md` Assessment 2, achieved)
**Deferred:** None
**Blockers:** None

## MILE-2 — Awake (R1)

**Outcome:** The Mac stays awake while any Claude Code, Codex or Grok session is working and goes back to normal sleep after they stop, with no manual action.
**Goals:** GOAL-2, GOAL-11
**Members:**
- **ROAD-5** perf-gate — After: none — Surfaces: `xtask/`
- **ROAD-6** platform-and-process-model — After: none — Surfaces: `crates/macos/`, `crates/procs/`
- **ROAD-7** kernel-contract-extensions — After: none — Surfaces: `crates/kernel/`, `crates/protocol/`, `crates/ffi/`
- **ROAD-8** awake-leases — After: ROAD-6, ROAD-7 — Surfaces: `modules/awake/`, `crates/ffi/`, `apps/macos/Navori/Features/`, `apps/cli/`
- **ROAD-9** harness-hooks — After: ROAD-8 — Surfaces: `apps/cli/`, `modules/integrations/`, `crates/ffi/`, `apps/macos/Navori/Features/Settings/` <!-- surfaces edited by an agent, agent-decided, 2026-11-03 (harness-hooks run); not the operator's choice -->
- **ROAD-28** soak-and-hang-check — After: ROAD-5 — Surfaces: `xtask/`, `apps/macos/Navori/`
- **ROAD-34** menubar-visual-foundation — After: none — Surfaces: `apps/macos/Navori/StatusBar/`, `apps/macos/Navori/Features/Panel/`, `apps/macos/Navori/Features/Settings/`, `apps/macos/NavoriPanel/`, `apps/macos/Package.swift`, `.cargo/config.toml`, `xtask/`
- **ROAD-35** status-icon-states — After: ROAD-7, ROAD-8, ROAD-34 — Surfaces: `modules/awake/`, `crates/kernel/`, `crates/protocol/`, `crates/ffi/`, `apps/cli/`, `apps/macos/Navori/Core/`, `apps/macos/Navori/StatusBar/`, `apps/macos/NavoriPanel/`, `xtask/assets/`
**Depends-on:** MILE-1
**Commitment:** Planned
**Closed:** None
**Deferred:** None
**Blockers:** None

## MILE-3 — AI Assistant (R2)

**Outcome:** The user asks Navori AI anything from the menubar, a hotkey or the terminal with any configured model, and answers its Decisions in Needs you before anything destructive runs.
**Goals:** GOAL-1, GOAL-5, GOAL-6
**Members:**
- **ROAD-10** keychain-secrets — After: none — Surfaces: `crates/secrets/`
- **ROAD-36** ask-walking-skeleton — After: ROAD-10 — Surfaces: `ai/llm/`, `ai/context/`, `crates/ipc/`, `crates/protocol/`, `crates/ffi/`, `apps/cli/`, `apps/macos/Navori/Features/Settings/` — Delivers text-only `navori ask` end to end: one adapter, one key saved from Settings, and a minimal Context Gateway (Shield on text, audit log, fail-closed). First consumer of `crates/secrets/`, so it takes the handoffs the SECRETS spec wrote for ROAD-11.
- **ROAD-11** providers-and-model-roles — After: ROAD-36 — Surfaces: `ai/llm/`, `ai/models/`, `crates/ffi/`, `apps/macos/Navori/Features/Settings/` <!-- surfaces edited by an agent (keychain-secrets run, 2026-11-03); approved by the operator 2026-11-04 --> — Also records the estimated cost of each Turn from the catalog's prices. <!-- cost deferred here from ROAD-14 by an agent (agent-loop run, 2026-11-05; Lock L19/L20 partly deferred, close-package D3, S-cost); pending the operator's confirmation -->
- **ROAD-12** shield-and-context-gateway — After: ROAD-36 — Surfaces: `ai/context/`, `crates/store/`, `crates/ffi/` — Delivers a way for the user to read the Context audit log that ROAD-36 writes (`context_audit`), and its retention, so GOAL-6's "the user can see what the AI has seen" is met. Keeps the text Shield ROAD-36 shipped; the screen-facing Shield layers moved to ROAD-39. <!-- narrowed by the operator 2026-11-05 (ADR 0008 session): app denylist, path blocks, OCR and screenshots deferred until Navori AI sees the screen. The reader's surface is set by the spec. -->
- **ROAD-13** needs-you-inbox — After: none — Surfaces: `modules/inbox/`, `apps/macos/Navori/Features/Panel/`
- **ROAD-14** agent-loop-and-session — After: ROAD-36 — Surfaces: `ai/agent/`, `crates/store/`, `crates/ffi/` — Ports dsh's agent loop, session log, tool registry and guards to Rust on Navori's Actions and Journal (ADR 0008), using the one adapter ROAD-36 shipped. Records what started each turn (THR-11) and usage from the first model call, and sends every tool call through one dispatch point. Runs only `Read` Actions until ROAD-37.
- **ROAD-37** policy-and-approvals — After: ROAD-13, ROAD-14 — Surfaces: `ai/agent/`, `crates/protocol/`, `crates/ffi/`, `apps/macos/Navori/Features/Panel/` — Ports dsh's user approval and permission presets (ADR 0008).
- **ROAD-38** ai-eval-harness — After: ROAD-14 — Surfaces: `xtask/`, `ai/agent/` — Delivers `cargo xtask eval` (`docs/standards/testing.md`).
- **ROAD-15** chat-and-quick-ask — After: ROAD-37 — Surfaces: `apps/macos/Navori/Features/`, `apps/cli/` — Also moves `navori ask` onto the agent loop, and streams replies (`eventsource-stream`), its first consumer. <!-- streaming deferred ASK → ROAD-14 → ROAD-15 by an agent (agent-loop run, 2026-11-05; ROAD-14 close-package D2); not yet approved by the operator --> — Also shows past Conversations and owns retention of the `ai_session_*` tables. <!-- added by an agent (agent-loop run, 2026-11-05; ROAD-14 final review item 2, Weigh W17); pending the operator, who may prefer widening ROAD-12 instead -->
- **ROAD-16** memory — After: ROAD-37 — Surfaces: `ai/memory/`, `crates/store/`
**Depends-on:** MILE-2
**Commitment:** Planned
**Closed:** None
**Deferred:**
- ROAD-39 shield-screen-layers → unscheduled (2026-11-05, app denylist, path blocks, OCR and screenshots wait until Navori AI sees the screen; split from ROAD-12)
- ROAD-40 computer-use → unscheduled (2026-11-05, Navori AI controlling other apps comes after the Agent loop; dsh has no usable base for it, ADR 0008)
**Blockers:** None

## MILE-4 — Ports (R3)

**Outcome:** The user sees every listening port grouped by project, worktree and harness, and frees a whole group in at most 2 actions.
**Goals:** GOAL-3
**Members:**
- **ROAD-17** port-discovery — After: none — Surfaces: `modules/ports/`, `crates/procs/`, `crates/macos/`
- **ROAD-18** ports-panel-and-stop — After: ROAD-17 — Surfaces: `modules/ports/`, `apps/macos/Navori/Features/`, `apps/macos/NavoriPanel/`
**Depends-on:** MILE-2
**Commitment:** Planned
**Closed:** None
**Deferred:** None
**Blockers:** None

## MILE-5 — GitHub (R4)

**Outcome:** The user sees pull requests needing review, failing checks and conflicts counted in the menubar, and opens, approves or merges them from the submenu.
**Goals:** GOAL-4
**Members:**
- **ROAD-19** github-device-auth — After: ROAD-36 — Surfaces: `modules/github/`, `crates/secrets/`, `crates/ffi/`, `apps/macos/Navori/Features/Settings/` <!-- surfaces edited by an agent (keychain-secrets run, 2026-11-03); approved by the operator 2026-11-04 -->
- **ROAD-20** pull-request-inbox — After: ROAD-19 — Surfaces: `modules/github/`, `apps/macos/Navori/Features/`
- **ROAD-21** pull-request-actions — After: ROAD-20 — Surfaces: `modules/github/`, `apps/macos/Navori/Features/`
**Depends-on:** MILE-3
**Commitment:** Planned
**Closed:** None
**Deferred:** ROAD-26 github-multi-account → unscheduled (2026-11-01, vision defers multiple GitHub accounts; the locked UI brief has no account switcher)
**Blockers:** None

## MILE-6 — Proactive (R5)

**Outcome:** Navori AI tells the user on its own, within a daily budget and quiet hours, when CI fails, a review arrives, an Agent session finishes or a port conflicts.
**Goals:** GOAL-9, GOAL-1
**Members:**
- **ROAD-22** triggers-and-budget — After: none — Surfaces: `ai/triggers/`, `crates/kernel/`
- **ROAD-23** proactive-notifications — After: ROAD-22 — Surfaces: `ai/triggers/`, `modules/inbox/`, `apps/macos/Navori/Platform/`
**Depends-on:** MILE-4, MILE-5
**Commitment:** Planned
**Closed:** None
**Deferred:** None
**Blockers:** None

## MILE-7 — MCP (R6)

**Outcome:** Claude Code, Codex and Grok call Navori's tools over MCP under the same approval policy, and Navori AI uses tools from external MCP servers.
**Goals:** GOAL-10
**Members:**
- **ROAD-24** mcp-server — After: none — Surfaces: `ai/mcp/`, `apps/cli/`, `integrations/`
- **ROAD-25** mcp-client — After: none — Surfaces: `ai/mcp/`, `ai/agent/`
**Depends-on:** MILE-3
**Commitment:** Planned
**Closed:** None
**Deferred:** None
**Blockers:** None

## MILE-8 — Auto Review

**Outcome:** When someone directly requests the user's review on a pull request in a repository the user enabled, that pull request gets a started comment, then findings and an approval or a change request by a fixed severity rule with no action from the user, and a re-request on new commits reviews it again.
**Goals:** GOAL-4, GOAL-11
**Members:**
- **ROAD-29** auto-review-standing-approval — After: none — Surfaces: `docs/adr/`, `docs/product/vision.md`, `docs/architecture/INDEX.md`
- **ROAD-30** agent-dispatch-runner — After: ROAD-9, ROAD-29 — Surfaces: proposed `modules/dispatch/`, `integrations/`
- **ROAD-31** review-trigger-and-repo-switch — After: ROAD-20, ROAD-22, ROAD-30 — Surfaces: `modules/github/`, proposed `modules/dispatch/`, `apps/macos/Navori/Features/Settings/`
- **ROAD-32** review-verdict-posting — After: ROAD-21, ROAD-31 — Surfaces: `modules/github/`, proposed `modules/dispatch/`
- **ROAD-33** re-review-on-request — After: ROAD-32 — Surfaces: `modules/github/`, proposed `modules/dispatch/`
**Depends-on:** MILE-5, MILE-6
**Commitment:** Planned
**Closed:** None
**Deferred:** None
**Blockers:** ROAD-30 to ROAD-33 wait on ROAD-29: approving without a human conflicts with GOAL-1, ARCH-5 and the vision's non-goal on `Destructive` actions, and needs a decision of record.

## MILE-9 — Public release

**Outcome:** Anyone on an Apple Silicon Mac with macOS 26 installs Navori with `brew install --cask` from the Navori tap, and every later release reaches them through `brew upgrade` with their config, database, hooks and Keychain access intact.
**Goals:** GOAL-8, GOAL-11
**Members:**
- **ROAD-27** open-source-distribution — After: none — Surfaces: `xtask/`, `.github/workflows/`, `modules/integrations/`, `docs/adr/`, `docs/agents/project.md`, external repo `jayden-dang/homebrew-navori` — A self-signed release built and published by GitHub Actions on a `v*` tag that matches the workspace version, a zip and sha256 on the NAVORI GitHub Release, and a cask in the personal tap; a local `cargo xtask release-check` (ci, perf, version) before tagging; a hook command that does nothing once the app is gone. Un-deferred from MILE-1 on 2026-11-04.
**Depends-on:** MILE-2, MILE-4
**Commitment:** Planned
**Closed:** None
**Deferred:** None
**Blockers:** None

## Goal dispositions

Every live `GOAL-N` in `docs/product/vision.md` that no milestone cites belongs here, so that a goal is never silently dropped (S6).

| Goal | Disposition | Date | Reason |
|---|---|---|---|
| GOAL-12 | Deferred | 2026-11-02 | Returns when Navori AI, its approvals, Needs you and the MCP client (ROAD-14, ROAD-37, ROAD-13, ROAD-25) exist and the user picks the first service beyond GitHub (ADR 0005). |
