# UI brief — Navori menubar (level 0 + feature submenus)

> Locked 2026-11-01 via `draft-ui`. Screenshot: [`2026-11-01-menubar-ui-brief.png`](2026-11-01-menubar-ui-brief.png).
> **Provisional by design:** each feature (R1 Awake, R3 Ports, R4 GitHub, R2/R5 AI) will update its own section when it is implemented; the overall layout and the rules below are fixed.

## Decision

**Direction A — "Menu" (cascading submenus), variant A5.** A was chosen over B (Triage tabs) and C (per-project Cockpit), then refined over 4 review rounds. The menu is drawn on an `NSPanel`, not an `NSMenu` (Amendment 10).

The user's words (verbatim, in order; the Vietnamese quotes are kept as-is, with an English translation appended):
- "Tôi muốn đi theo hướng của A — nhưng A chưa đủ tốt, chưa đủ clean và modern" (EN: "I want to go with direction A — but A isn't good enough yet, not clean and modern enough")
- "icons … cần đơn sắc để đồng bộ với macos hơn" (EN: "icons … need to be monochrome to fit in better with macOS")
- reference to RepoBar: "vẫn cần tối ưu hơn nữa, đẹp hơn, gọn gàng hơn, dễ dùng hơn" (EN: "still needs more optimization, nicer, tidier, easier to use")
- "phần awake, ports, PR cần đơn giản hơn, 1 dòng với info quan trọng only — không cần switch account github"; "trong trường hợp nhiều PR, nhiều repo … cần hiển thị rõ hơn" (EN: "the awake, ports, PR parts need to be simpler, 1 line with important info only — no need to switch GitHub account"; "when there are many PRs, many repos … they need to be displayed more clearly")
- "Bỏ dòng đầu … needs you … tương lai sẽ là nơi hiển thị notification từ AI, thông tin, to do … needs you ở đây là quyết định của tôi và AI, không phải là PR của git" (EN: "Remove the first line … needs you … in the future it will be where notifications from AI, information, to-dos are shown … needs you here means decisions by me and the AI, not git PRs")
- "Chốt, tạm thời như thế — khi triển khai features sẽ update thêm." (EN: "Locked, for now like this — it will be updated further as features are implemented.")

## Grounding

- **Token source of record:** macOS system semantics — `labelColor`, `secondaryLabelColor`, `tertiaryLabelColor`, `separatorColor`, `controlAccentColor` / `selectedContentBackgroundColor`, `systemGreen/Red/Orange/Blue`; glass surface per Amendment 11. The repo has no `docs/standards/design-tokens.md` yet; no custom tokens are proposed.
- **Existing kit** (`apps/macos/Navori`): `Banner`, `SectionTitle`, status dot (`Circle().fill`), borderless buttons. Ladder used below: `rung 1 — <native control>` = built-in AppKit/SwiftUI; `rung 2 — <kit piece>` = an existing Navori component; `new (rung 7)` = a new component, with the reason given.

## Job (the working screen)

- **Question:** Is anything waiting on my decision right now, and what state is my machine in?
- **Finding:** (data on the lock date) "2 decisions pending: stop 3 Claude servers so they don't sit idle in openklynt-main; keep the machine awake for Codex in cargo-zip" — at the top of the menu, the first row under the Ask field.
- **Primary job:** answer the first decision with an inline button right on that row (`Stop 3` / `Keep`) — one click, nothing else opens.

## Signature

**Every detail is one hover away** — the level-0 menu contains only *Ask → Needs you (inbox) → 3 one-line feature rows*; everything else lives in submenus that flip left near the screen edge, at most 3 levels deep.

---

## Surface 1 — Level-0 menu

- **Layout:** 360pt wide, 5pt padding. Shipped order:
  1. Any banners (the core did not start, an unexpected quit, a config problem).
  2. **Awake** — one line: icon `cup.and.saucer` · name · a green dot while a Lease holds the system assertion, otherwise no dot · chevron. No text value (user, 2026-11-04). No submenu when the record is missing or the module has stopped.
  3. **Core** — one line, with the dev chip on the dev profile. Its submenu is unchanged.
     *(Amended 2026-11-05, operator decision: in production this row shows only while the Core needs attention or is down; the dev profile always shows it, with the chip. See Amendment 12.)*
  4. Separator → `Settings… ⌘,` · `About Navori` · `Quit Navori ⌘Q`.
  - Ask, Needs you, Ports and Pull Requests are not on level 0. They stay in the dev-only preview until those features ship.
  - No account, API quota, or "updated" row at level 0.
  - *(Amended 2026-11-08: when Needs you ships, a `Decide N` block of one-line Decision rows and a `Needs you ›` row sit above `Running`. See [`2026-11-08-needs-you-ui-brief.md`](2026-11-08-needs-you-ui-brief.md) Surface 1.)*
- **Components:**
  - Menu + submenu cascade: `new (rung 7) — panel cascade` on `StatusPanel` (`NSPanel`). Reason: Amendment 10.
  - Glass surface: `rung 1 — NSGlassEffectView` (Amendment 11).
  - Custom row: `new (rung 7) — MenuRowView` (SwiftUI): 20pt lead, title, end value, chevron, up to 2 secondary lines, inline action area.
  - Ask field: `rung 1 — SwiftUI TextField` in a rounded fill.
  - Inbox row: `new (rung 7) — InboxRowView` (see Surface 2).
  - Status dot: `rung 2 — status dot`.
  - Icons: `rung 1 — SF Symbols` (monochrome rendering): `sparkle`, `cup.and.saucer`, `point.3.connected.trianglepath.dotted`, `arrow.triangle.pull`, `gearshape`, `bell`, `square`.
- **States:** default · hover/selected (accent background with 8pt corners, white text + icon) · empty inbox (`✓ You're all caught up`, the menu gets shorter) · core not running (`rung 2 — Banner` error style in the inbox position: "Navori core isn't running" + `Restart`) · GitHub offline (`Pull Requests  offline · 5 min ago`) · Awake on (green dot) · Awake off or not held (no dot) · Awake unavailable (no dot, no submenu).
- **Type & color:** SF Pro 14pt title/item, 13pt right-aligned value (`secondaryLabelColor`), 12pt secondary line. SF Mono 11.5pt for port numbers, `#PR`, pid. Monochrome icons in `secondaryLabelColor` (white when selected). Semantic color is **only** the 7pt dot (`systemGreen` on/pass, `systemRed` fail, `systemOrange` conflict, `systemBlue` review/decision). No color tiles, gradients, or multicolor icons.
- **A11y:** NSMenu-style keyboard navigation, built by the panel cascade (↑↓, → opens a submenu, ← closes, Return, Esc); VoiceOver reads "title, secondary line, time" as one; state is never conveyed by color alone (always text: "2 failing", "On"); respects Reduce Transparency (opaque material) and Reduce Motion; inline button hit area ≥ 24pt tall.

## Surface 2 — Needs you (inbox) — on level 0 and in the `Show all` submenu

> Superseded 2026-11-08 by [`2026-11-08-needs-you-ui-brief.md`](2026-11-08-needs-you-ui-brief.md): Decisions are compact rows on level 0 only, the submenu filters All · To-dos · Updates, a to-do's status is its date pill's color, and Keep awake raises no Decision (the `Keep 1 h` / `Let it end` example no longer applies).

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

## Surface 3 — Awake submenu

> The commands stay. The Holding list and the row name are replaced by [`2026-11-04-running-menu-ui-brief.md`](2026-11-04-running-menu-ui-brief.md) when ROAD-18 ships.

Locked 2026-11-04 as variant C1. The brief of record is [`docs/specs/2026-11-02-awake-leases/ui-brief.md`](../specs/2026-11-02-awake-leases/ui-brief.md).

- **Layout:** `Keep awake`, opening a third level of `Indefinitely` and lengths from `5 minutes` to `5 hours`, then `Tracking agents`; a problem line only when something is wrong; then Holding, grouped by `Held by you`, `Commands` and project, with each session named by its title or else its branch. No per-Lease release. No Agent sessions row and no Integrations row.
- **Components:** commands are `MenuRowView` rows with an icon and a check. Holding rows are not cascade nodes.
- **States:** off (the commands alone) · on (no status text) · system assertion refused · display assertion refused · action failed.
- **Type & color / A11y:** as Surface 1, with monochrome Harness marks. ↑ and ↓ reach the commands, → opens `Keep awake`. VoiceOver reads each command, the problem line and each Holding row as one sentence. The problem line and Holding rows take no keyboard stop.

## Surface 4 — Ports submenu (+ Project)

> Superseded 2026-11-04 by [`2026-11-04-running-menu-ui-brief.md`](2026-11-04-running-menu-ui-brief.md): no separate Ports row or submenu.

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
5. Level 0: Awake / Ports / Pull Requests are **one line, one important piece of info** each (2026-11-04: Awake and Ports become one `Running` row, showing only a dot, per [`2026-11-04-running-menu-ui-brief.md`](2026-11-04-running-menu-ui-brief.md)); drop the account & Switch account everywhere.
6. Handles heavy load: level 0 has at most 3 inbox items + `Show all N`; PRs are grouped and shown **By repository** (top 6 + All); each PR shows only 1 status.
    *(Amended by NEEDS-3.1 and NEEDS-3.9: open Decisions on level 0 are not capped at 3; the Decide block, the Needs you row, and `You're all caught up` ship in Release.)*
7. Drop the caption row "Navori · N need you · time" — the menu opens with the Ask field.
8. **Needs you = the decision inbox between the user and the AI** (Decision / To-do / Update), containing no PRs.
9. The brief is provisional: each feature updates its own surface when it ships; constraints 1–8 stay unless re-reviewed.
10. **The menu is an `NSPanel`, not an `NSMenu`** (user, 2026-11-02, replacing "NSMenu" in constraint 1). The cascade, edge flip, keyboard navigation and VoiceOver are built in the Shell. See the spike evidence below.
11. **The surface is `NSGlassEffectView` with style `.clear` and a tint** (user, 2026-11-02). Chosen tint: black at 69% in dark appearance, white at 57% in light. Measured WCAG 2 on Preview screenshots: dark on white, primary 5.94:1, secondary 3.04:1; light on black, primary 4.88:1, secondary 3.03:1. Starting 65/55 missed secondary 3:1; 1% steps found these first passers. Text on glass uses `labelColor` and `secondaryLabelColor`. `tertiaryLabelColor` was not remeasured at this pair. Reduce Transparency falls back to an opaque surface. Requires macOS 26 or later.
12. **Until a feature's data exists, level 0 shows only real Core state** (user, 2026-11-02): banners, a `Core` row, then `Settings…`, `About Navori`, `Quit Navori`. The `Core` row is not in the layout above; its submenu holds module states and recent activity, so the cascade runs on real data before Awake. Whether it stays is decided when the Awake row ships. Ask, Needs you and the feature rows appear only in a dev-only preview until their features ship.
    *(Amended 2026-11-05, operator decision: "Whether it stays" is answered. The Running row exists, so in production the `Core` row shows only while the Core is `Needs attention` or `Not running`; the dev profile keeps it always. Requirement MENU-3.11. The Core submenu's Events list is the only in-app view of the counts-only `ports.stopped` Events, so they are now visible in the app only while the Core needs attention.)*
    *(Amended by NEEDS-3.1 and NEEDS-3.9: open Decisions on level 0 are not capped at 3; the Decide block, the Needs you row, and `You're all caught up` ship in Release. Ask, Ports, and Pull Requests stay out of Release.)*

## Spike evidence (2026-11-02)

A throwaway AppKit app, driven with real input events on `Mac17,9`, macOS 27.0.1, with no Rust core. The numbers compare the options; they are not the GOAL-11 result.

| Measure | Budget | `NSMenu` | `NSPanel` + glass |
|---|---|---|---|
| Typing in Ask, inline buttons | — | works | works |
| Footprint with the menu open | ≤ 60 MB | 131 MB for about 1.2 s, then 33–35 MB; 26 MB with no text field | 28 MB |
| Footprint after closing | ≤ 30 MB idle | 32 MB | 28 MB |
| First open, median | ≤ 80 ms | 94–100 ms | 68–95 ms, noisy |
| Later opens, median | ≤ 50 ms | 27–32 ms, one sample 54 ms | 8–12 ms |
| Submenu open, first frame | ≤ 16 ms | 10–11 ms | 12–14 ms, simple child panel |

- `NSStatusItem.button.performClick` blocks the main queue while the `NSMenu` is open (0 of 60 ticks); a real click does not (60 of 60).
- The cause of the 131 MB peak was not found.
- Plain `.clear` glass with no tint leaves dark-appearance text unreadable on a white background. Estimated contrast on that background (label colors taken as 85% and 55% white): 65% tint gives 4.9:1 primary and 3.1:1 secondary; 75% gives 6.8:1 and 3.9:1; 85% gives 9.5:1 and 5.0:1. Light-appearance figures on a dark background were sampled over blurred text, so treat them as rough.
- Not tested: Reduce Transparency, VoiceOver, the real app with the Rust core, and a full panel cascade.

Tint comparison: [`2026-11-02-clear-glass-tints.png`](2026-11-02-clear-glass-tints.png).

## Open questions for `design-solution`

- Inbox data (Decision/To-do/Update): answered. The modules are `navori-mod-needs-you` and `navori-mod-todo` (formerly proposed as `navori-mod-inbox`), receiving Decisions from the AI runtime via `ActionSpec` + `Risk`.
