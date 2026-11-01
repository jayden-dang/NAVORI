# UI brief — Navori menubar (level 0 + feature submenus)

> Locked 2026-11-01 via `draft-ui`. Screenshot: [`2026-11-01-menubar-ui-brief.png`](2026-11-01-menubar-ui-brief.png).
> **Provisional by design:** each feature (R1 Awake, R3 Ports, R4 GitHub, R2/R5 AI) will update its own section when it is implemented; the overall layout and the rules below are fixed.

## Decision

**Direction A — "Menu" (NSMenu + cascading submenus), variant A5.** A was chosen over B (Triage tabs) and C (per-project Cockpit), then refined over 4 review rounds.

The user's words (verbatim, in order; the Vietnamese quotes are kept as-is, with an English translation appended):
- "Tôi muốn đi theo hướng của A — nhưng A chưa đủ tốt, chưa đủ clean và modern" (EN: "I want to go with direction A — but A isn't good enough yet, not clean and modern enough")
- "icons … cần đơn sắc để đồng bộ với macos hơn" (EN: "icons … need to be monochrome to fit in better with macOS")
- reference to RepoBar: "vẫn cần tối ưu hơn nữa, đẹp hơn, gọn gàng hơn, dễ dùng hơn" (EN: "still needs more optimization, nicer, tidier, easier to use")
- "phần awake, ports, PR cần đơn giản hơn, 1 dòng với info quan trọng only — không cần switch account github"; "trong trường hợp nhiều PR, nhiều repo … cần hiển thị rõ hơn" (EN: "the awake, ports, PR parts need to be simpler, 1 line with important info only — no need to switch GitHub account"; "when there are many PRs, many repos … they need to be displayed more clearly")
- "Bỏ dòng đầu … needs you … tương lai sẽ là nơi hiển thị notification từ AI, thông tin, to do … needs you ở đây là quyết định của tôi và AI, không phải là PR của git" (EN: "Remove the first line … needs you … in the future it will be where notifications from AI, information, to-dos are shown … needs you here means decisions by me and the AI, not git PRs")
- "Chốt, tạm thời như thế — khi triển khai features sẽ update thêm." (EN: "Locked, for now like this — it will be updated further as features are implemented.")

## Grounding

- **Token source of record:** macOS system semantics — `labelColor`, `secondaryLabelColor`, `tertiaryLabelColor`, `separatorColor`, `controlAccentColor` / `selectedContentBackgroundColor`, `systemGreen/Red/Orange/Blue`; system menu material. The repo has no `docs/standards/design-tokens.md` yet; no custom tokens are proposed.
- **Existing kit** (`apps/macos/Navori`): `Banner`, `SectionTitle`, status dot (`Circle().fill`), borderless buttons. Ladder used below: `rung 1 — <native control>` = built-in AppKit/SwiftUI; `rung 2 — <kit piece>` = an existing Navori component; `new (rung 7)` = a new component, with the reason given.

## Job (the working screen)

- **Question:** Is anything waiting on my decision right now, and what state is my machine in?
- **Finding:** (data on the lock date) "2 decisions pending: stop 3 Claude servers so they don't sit idle in openklynt-main; keep the machine awake for Codex in cargo-zip" — at the top of the menu, the first row under the Ask field.
- **Primary job:** answer the first decision with an inline button right on that row (`Stop 3` / `Keep`) — one click, nothing else opens.

## Signature

**Every detail is one hover away** — the level-0 menu contains only *Ask → Needs you (inbox) → 3 one-line feature rows*; everything else lives in real macOS submenus that flip left near the screen edge, at most 3 levels deep.

---

## Surface 1 — Level-0 menu

- **Layout:** 360pt wide, 5pt padding. Fixed order:
  1. The **Ask Navori…** field (`⌥ Space`) — no title/caption row above it.
  2. Separator → **Needs you** header + total count (right-aligned).
  3. Up to **3** inbox items (see the inbox rules), then `Show all N ›` when N > 3.
  4. Separator → 3 feature rows, **one line each**: icon · name · *one* important piece of info (right-aligned) · `›`
     - **Awake** — `● On · Claude, Codex` (green dot when on; `Off` when off)
     - **Ports** — `17 open`
     - **Pull Requests** — `1 to review · ● 2 failing` (the failing part shows only when > 0)
  5. Separator → `Settings… ⌘,` · `About Navori` · `Quit Navori ⌘Q`.
  - No account, API quota, or "updated" row at level 0.
- **Components:**
  - Menu + submenu cascade: `rung 1 — NSMenu / NSMenuItem.submenu` (needs a spike, see Open questions).
  - Custom row: `new (rung 7) — MenuRowView` (SwiftUI in `NSMenuItem.view`): 20pt lead, title, end value, chevron, up to 2 secondary lines, inline action area. Reason: a standard NSMenuItem has no secondary line / inline buttons.
  - Ask field: `rung 1 — NSSearchField` in an item view.
  - Inbox row: `new (rung 7) — InboxRowView` (see Surface 2).
  - Status dot: `rung 2 — status dot`.
  - Icons: `rung 1 — SF Symbols` (monochrome rendering): `sparkle`, `cup.and.saucer`, `point.3.connected.trianglepath.dotted`, `arrow.triangle.pull`, `gearshape`, `bell`, `square`.
- **States:** default · hover/selected (accent background with 8pt corners, white text + icon) · empty inbox (`✓ You're all caught up`, the menu gets shorter) · core not running (`rung 2 — Banner` error style in the inbox position: "Navori core isn't running" + `Restart`) · GitHub offline (`Pull Requests  offline · 5 min ago`) · Awake off (`Off`, no dot).
- **Type & color:** SF Pro 14pt title/item, 13pt right-aligned value (`secondaryLabelColor`), 12pt secondary line. SF Mono 11.5pt for port numbers, `#PR`, pid. Monochrome icons in `secondaryLabelColor` (white when selected). Semantic color is **only** the 7pt dot (`systemGreen` on/pass, `systemRed` fail, `systemOrange` conflict, `systemBlue` review/decision). No color tiles, gradients, or multicolor icons.
- **A11y:** NSMenu-style keyboard navigation (↑↓, → opens a submenu, ← closes, Return, Esc); VoiceOver reads "title, secondary line, time" as one; state is never conveyed by color alone (always text: "2 failing", "On"); respects Reduce Transparency (opaque material) and Reduce Motion; inline button hit area ≥ 24pt tall.

## Surface 2 — Needs you (inbox) — on level 0 and in the `Show all` submenu

The inbox is **the shared decision place between the user and Navori AI** — it is *not* a PR list (PRs live only in Surface 5).

- **Layout:** each item: lead (20pt) · title (1 line, truncated …) · age right-aligned · secondary line = the *reason* (1 line) · [Decision] row of inline buttons.
  - Order: **Decision → To-do → Update**; within a kind, newest first (for Decisions, earliest expiry first).
  - Level 0: at most 3 items; `Show all N` submenu: header `Needs you N`, segmented **All · Decisions n · To-dos n · Updates n**, the full list, an **Add a to-do…** field, inline buttons **Clear updates** · **History**.
- **Item kinds:**

  | Kind | Lead | Behavior |
  |---|---|---|
  | **Decision** (AI asks, the person decides) | `sparkle` | 1 primary + 1 secondary inline button (e.g. `Stop 3` / `Keep`, `Keep 1 h` / `Let it end`). A `Risk::Destructive` action shows the note "needs your approval" and **never** runs on its own. Once answered → the item disappears and the result becomes an Update. |
  | **To-do** (created by the person or the AI) | clickable square | Tick → completed (disappears, with Undo in History). Secondary line: due date + who created it ("Due today · added by you", "Suggested by Navori AI"). |
  | **Update** (notification) | `bell`, dimmer | No buttons; click opens the context. Auto-hides after being viewed / `Clear updates`. |

- **Components:** `new (rung 7) — InboxRowView` (lead, title, age, reason, action slot); inline button: `new (rung 7) — CapsuleButton` (primary = accent, secondary = fill) — reason: a standard NSButton has no small capsule style in a menu; segmented: `rung 1 — NSSegmentedControl`; to-do field: `rung 1 — NSTextField`.
- **States:** empty · many (N > 3 → `Show all N`) · button running (spinner in the button, other buttons disabled) · action error (secondary line turns into the error + `Retry`) · Decision expired (automatically becomes an "expired" Update).
- **Type & color:** title 14pt regular (Update is lighter: lead `tertiaryLabelColor`); reason 12pt `secondaryLabelColor`; buttons 12.5pt medium, 24pt tall; when the row is selected: the primary button inverts to a white background with accent text, the secondary button gets a 22% white background.
- **A11y:** inline buttons are in the row's Tab order; VoiceOver reads the item kind ("Decision: Stop 3 idle servers…, needs your approval"); shortcuts: Return = primary button, ⌘⌫ = dismiss.

## Surface 3 — Awake submenu (+ Agent sessions, Integrations)

- **Layout:** quick commands (`Keep awake for 1 hour`, `Keep awake until I turn it off`, `Let my Mac sleep now`) → **status block** `Awake · since 11:38` + "3 leases · system sleep blocked · display may sleep" with inline buttons `Pause 15m · Display on · Release all` → header **Holding**: each lease on 2 lines (`Claude · NAVORI/main` / `working · pid 23719`, time right-aligned; hover → `Release` button) → `Agent sessions [6] ›`, `Integrations [3] ›`.
  - Level 3 **Agent sessions:** every open harness session (claude/codex/grok), status, time; hover → `Show terminal`, `Keep awake for this session`.
  - Level 3 **Integrations:** Claude Code / Grok Build / Codex with the hook file path and `● installed` status; click → Repair. Note: Grok also reads Claude's hooks, so events are de-duplicated per session.
- **Components:** `new (rung 7) — StatusBlockView` (icon, title + subtitle, description line, inline button row — shared with Ports/project); count capsule `new (rung 7) — CountBadge`; hover actions `new (rung 7) — RowActionButton` (icon 26×22).
- **States:** off (status block "Awake off" + a prominent `Keep awake for 1 hour`) · no Agent session Leases · hook not installed (Integrations shows `● not installed` + an inbox To-do "Install the Grok hook").
- **Type & color / A11y:** as Surface 1; hover buttons must be keyboard-reachable (Tab within the row).

## Surface 4 — Ports submenu (+ Project)

- **Layout:** `Open all web servers [5]`, `Stop all ports…` → header **Projects** (each project on 2 lines: name + port-count badge; secondary line `⑂ main · by Claude · :5173 :3101 :3100` or `2 worktrees`) → note "4 macOS system ports hidden · refreshed 3 s ago".
  - Level 3 **Project:** header with name + path (mono); a worktree filter segmented control when > 1 (`All · dev · feat-lmedia`); each process on 2 lines (`postgres` / `docker · klynt-postgres`, port in mono right-aligned; hover → `Open` (if web) · `Copy` · `Stop`); a final inline row `Terminal · Finder · Stop N`.
- **Components:** as Surface 3 + `rung 1 — NSSegmentedControl`.
- **States:** no ports · ports of other UIDs/system always hidden · `Stop` running / failed (secondary line shows the error) · `Stop all` always goes through confirmation (or a Decision in the inbox if proposed by the AI).

## Surface 5 — Pull Requests submenu (+ lists, By repository)

- **Layout:** `Open pull requests on GitHub`, `Open notifications [4]` → groups **Review requested · My pull requests · Failing checks · Merge conflicts** (badge + `›`) → header **By repository N**: top 6 repos by PR count (badge + `›`), then `All N repositories ›` → note "@account · updated 40 s ago". **No** account block / Switch account.
  - Level 3 **PR list:** header + segmented control when useful (`All · Ready · Drafts`); each PR on 2 lines: avatar · title · age right-aligned / `repo #n · author` (mono) + **one** most important status (priority order: checks fail › checks running › conflicts › review requested › approved › draft › checks pass); hover → `Open on GitHub` · `Copy link`.
- **Components:** avatar `new (rung 7) — AvatarView` (GitHub image, fallback initial letter on a fill background); the rest is shared.
- **States:** not signed in (a single row `Sign in to GitHub…` replaces the whole submenu) · offline/rate-limited (the footer note changes to the reason + retry time) · empty list ("Nothing here.").

---

## Amendments (constraints decided in review)

1. Direction **A** (NSMenu + submenus) — no tabs (B) or per-project dashboard (C).
2. Tidy & modern: one-line rows where possible, actions shown only on hover, SF Symbols-style icons.
3. **Monochrome icons** — no color tiles, no gradients; semantic color is only the 7pt dot; switches use the system accent.
4. RepoBar patterns: multi-level submenus (≤ 3), status block with inline buttons, gray count capsules, dot + text states, real avatars, 14pt text.
5. Level 0: Awake / Ports / Pull Requests are **one line, one important piece of info** each; drop the account & Switch account everywhere.
6. Handles heavy load: level 0 has at most 3 inbox items + `Show all N`; PRs are grouped and shown **By repository** (top 6 + All); each PR shows only 1 status.
7. Drop the caption row "Navori · N need you · time" — the menu opens with the Ask field.
8. **Needs you = the decision inbox between the user and the AI** (Decision / To-do / Update), containing no PRs.
9. The brief is provisional: each feature updates its own surface when it ships; constraints 1–8 stay unless re-reviewed.

## Open questions for `design-solution`

- **Real NSMenu or a custom-drawn NSPanel cascade?** NSMenu gives native behavior for free (keyboard, material, submenu flipping) but input fields (Ask, Add a to-do) and inline buttons inside `NSMenuItem.view` need a spike to confirm focus/tracking. R0 currently uses an `NSPanel` — this decision may replace `StatusPanel`.
- Inbox data (Decision/To-do/Update) needs a new module in the kernel (proposed `navori-mod-inbox`, receiving Decisions from the AI runtime via `ActionSpec` + `Risk`).
