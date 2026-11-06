# Product Vision: Navori

Status: Approved
Date: 2026-11-01

## Problem

Everyday work on a Mac — including coding with several Harnesses in parallel across projects and worktrees — is split across many apps, tabs and single-purpose menubar utilities. The Mac sleeps mid-run and stalls an Agent session; orphaned dev servers hold ports; pull-request pressure and CI failures sit scattered on GitHub; mail and chat that need an answer sit in yet more apps; and no assistant sees the whole picture, so nothing tells the user "this is what needs your decision now" or helps proactively. Each fix today is another utility that costs memory, wake-ups and attention.

## Users

One user: the author — a developer on macOS (Apple Silicon) who runs multiple agentic harnesses (Claude Code, Codex, Grok Build) side by side, reads Vietnamese and English, and builds Navori from source. Navori is both this user's **AI assistant** and their **personal utility app**.

Other developers install it themselves, for their own use, with `brew install --cask navori` from the Navori tap (v0.1.0, MILE-9; macOS 26 on Apple Silicon only). They are users of their own copy, not a second persona Navori is designed around: features are still chosen for the author, but their upgrades must keep their data, hooks and Keychain access (Compat obligation External in `docs/agents/project.md`).

## Goals

- **GOAL-1** Every decision that needs the user — from Navori AI, an Agent session, or a module — reaches exactly one place, **Needs you**, and can be answered with one click from the menubar; no `Destructive` action ever runs without the user's explicit approval.
- **GOAL-2** The Mac never sleeps while a Claude Code, Codex or Grok session is working, and returns to normal sleep within 5 minutes after every session has stopped — with no manual action.
- **GOAL-3** Every port the user has open is shown with the project, worktree and harness that started it, and a whole group can be freed in at most 2 actions.
- **GOAL-4** Pull requests needing review, failing checks and merge conflicts are counted in the menubar and refreshed within 2 minutes of a change on GitHub; open, approve and merge are reachable from the submenu.
- **GOAL-5** The user can ask Navori with any model they configure (Claude, OpenAI, DeepSeek, Gemini, Grok, OpenRouter, or a custom OpenAI-compatible endpoint), and the AI can call every action of every module through the same action surface — no per-module AI code.
- **GOAL-6** The AI sees the full context of what the user is doing, yet no secret (per the Sensitive Data Shield rules) ever leaves the machine or is written to disk; the user can see what the AI has seen.
- ~~**GOAL-7** The hardcore performance budget stays green in `cargo xtask perf` on every release: idle 0% CPU, ≤ 1 wakeup/s and ≤ 30 MB footprint; ≤ 60 MB with all features and the menu open; cold start to menubar icon ≤ 200 ms; menu open ≤ 50 ms; release `.app` ≤ 25 MB.~~ Superseded by GOAL-11 on 2026-11-02. On this Mac the first menu open measured 47.3–63.7 ms, so 50 ms sat inside the run-to-run spread. Later opens stayed under 11 ms.
- **GOAL-11** The hardcore performance budget stays green in `cargo xtask perf` on every release: idle 0% CPU, ≤ 1 wakeup/s and ≤ 30 MB footprint; ≤ 60 MB with all features and the menu open; cold start to menubar icon ≤ 200 ms; first menu open after launch ≤ 80 ms; later menu opens in the same process ≤ 50 ms; Core submenu open ≤ 16 ms; release `.app` ≤ 25 MB.
- **GOAL-8** Every release ships one complete capability that meets the Definition of Done and is used daily by the user for at least 3 days without a blocking defect.
- **GOAL-9** Navori AI proactively puts Decisions and Updates into Needs you when it is worth the interruption (CI failed, an Agent session finished, a port conflict), within a daily token budget and quiet hours.
- **GOAL-10** Claude Code, Grok Build and Codex can call Navori's tools over MCP, under the same approval policy as Navori's own AI.
- **GOAL-12** The user can connect their own accounts on external services beyond GitHub (such as Gmail, Slack and Telegram) and revoke each one from Settings; what needs the user from them reaches Needs you, and Navori AI reads and acts on them through the same Actions and approval policy as every Module.

## Non-goals

- Replacing the IDE, terminal, GitHub web or any specialised app — Navori coordinates; it is not where the work happens.
- Building its own coding agent or agent harness.
- Windows, Linux, or Intel Macs.
- ~~Keeping the Mac running with the lid closed (needs a privileged root helper).~~ Superseded on 2026-11-06: lid tests on this Mac showed the lid setting can be turned off without root. ROAD-41 keeps the Mac awake with the lid closed on the power adapter while a Lease is held; root and helper processes stay out.
- Letting the AI perform a `Destructive` action without approval.
- Telemetry, analytics, or any Navori-owned account or server — the only network calls go to GitHub, the AI providers the user configures and the external services the user explicitly connects (a Connected account or an MCP server).

## Scope boundaries

- **In scope now:** releases R1–R6 (Awake, AI Assistant, Ports, GitHub, Proactive, MCP) on macOS 26+ / Apple Silicon, built for the author and installable by other developers through Homebrew; R0 Foundation and the first public release (v0.1.0) have shipped.
- **Deferred:**
  - Developer ID signing + notarization, a download page, auto-update inside the app, and the official `homebrew/cask` (self-signed public releases through the personal tap `jayden-dang/homebrew-navori` shipped as v0.1.0: MILE-9, ADR 0007; the code is MIT, see `LICENSE`).
  - "Life" utility modules beyond the developer workflow (calendar, notes, clipboard…) — each must pass a check against this vision before it is planned.
  - Multiple GitHub accounts.
  - Connected accounts beyond GitHub (Gmail, Slack, Telegram…): direction accepted in ADR 0005, not yet scheduled. Reach each service through the MCP client (R6) first; build a native Module only when the service must be counted in the menubar or trigger proactively.
- **Hard constraints:**
  - Local-first: no Navori server; data leaves the machine only through the Context Gateway or a module's declared API client.
  - Navori's secrets live only in the macOS Keychain.
  - The GOAL-11 performance budget applies to every feature.
  - No App Sandbox (Navori needs process inspection, kill and IOKit power assertions).
  - Self-signed code signing with one stable identity, also for public releases, until a Developer ID replaces it (ADR 0004, ADR 0007).
