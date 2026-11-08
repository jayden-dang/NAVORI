# UI brief: Needs you

> Locked 2026-11-08 via `draft-ui` for ROAD-13 (issue #35). Screenshot: [`2026-11-08-needs-you-ui-brief.png`](2026-11-08-needs-you-ui-brief.png). The Decide rows and To-dos in it are sample content.
> Amends Surface 1 (level-0 order) and replaces Surface 2 (Needs you) of [`2026-11-01-menubar-ui-brief.md`](2026-11-01-menubar-ui-brief.md). Waits for the spec that adopts it. Nothing here is built yet.

## Decision

**Direction D, a hybrid. Level 0 carries C's Decide idea, reduced to compact one-line rows, then one `Needs you` row. The submenu takes A's filter and Add a to-do field, with C's one-line to-do rows below. A to-do's status is the color of its date pill. Keep awake is not part of Needs you.** Chosen over A (brief-faithful three two-line items on level 0 and `Show all N`), B (one quiet `Needs you` row, everything grouped one hover away) and C (a large Decision card with a countdown on level 0).

The user's words, in order (Vietnamese verbatim, with an English translation):
- "Thứ nhất tôi không cần đưa keep awake vào AI feature, nên bỏ nó ra khỏi thiết kế - decide ở đây nó sẽ được define sau focus vào các công việc của cá nhân tôi, như review PR - … tôi muốn trong sub-menu sẽ có phân loại của A - nhưng layout bên dưới của C … để đơn giản hóa lại - phân decide sử dụng C - nhưng không đưa phần keep awake vào phần decide này, các decide này chỉ là những thứ quang trọng như Review PR, check remimder, todo, etc..." (EN: "First, I don't need Keep awake in the AI feature, so take it out of the design. What Decide holds will be defined later; it focuses on my own work, like reviewing PRs. I want the submenu to have A's categories but C's layout below them, to keep it simple. Decide uses C, but without Keep awake. Decisions are only the important things: review a PR, check a reminder, a to-do, etc.")
- "lỗi deuplicated" (EN: "duplication bug" — Decisions showed both on level 0 and in the submenu's All tab)
- "các decide bên ngoài đang quá to, cần làm đơn giản lại, compact hơn" (EN: "the Decide items outside are too big; make them simpler and more compact")
- "về phần status của todo, tôi muốn tích hợp nó vào time thay vì 1 badget nằm riêng - sử dụng màu làm status, ví dụ overdue thì nó sẽ chỉ hiển thị badget đỏ của Thu 5, sử dụng thêm các màu như vàng, xanh, gray, etc.. cho các status khác nhau." (EN: "For a to-do's status, fold it into the date instead of a separate badge. Use color as the status: overdue shows just a red `Thu 5` badge; use yellow, blue, gray, etc. for the other statuses.")
- "chốt - sử dụng d này." (EN: "Lock it, use this D.")

## Grounding

- **Token source of record:** macOS system semantics, as in the 2026-11-01 brief. Adds `systemYellow` for the date pill (Amendment 4).
- **Kit:** `MenuRowView`, `MenuDivider`, the panel cascade and the status dot ship today. `InboxRowView` and `CapsuleButton` exist only in the dev-only preview (`apps/macos/Navori/Features/Preview/`).

## Signature

**Decide sits on level 0 as plain menu lines whose answers appear under the pointer; everything else waits one hover away, and a to-do's date says by its color how late it is.**

## Surface 1: Level-0 menu (amended)

- **Layout:** banners · `Decide N` header · one row per open Decision · `Needs you` row · separator · `Running` · separator · `Settings… ⌘,` · `About Navori` · `Quit Navori ⌘Q`. Core row rules from the 2026-11-01 brief are unchanged.
  - With no open Decision, the `Decide` header and its rows are absent; `Needs you` follows the banners.
  - With nothing at all to show, `✓ You're all caught up` replaces the Decide block and `Needs you` has no count.
- **Components:**
  - `Decide N` header: `rung 2 — section header` as in the Running submenu (12pt semibold, secondary label, count on the right).
  - Decide row: `new (rung 7) — DecideRowView`. One line, 26pt like every kit row: lead icon in the accent color · title (truncates) · age right-aligned. On hover or keyboard focus the age is replaced by two inline `CapsuleButton`s at a mini size (20pt, 11.5pt medium): primary accent, secondary fill. Reason: `MenuRowView` has no inline action slot, and `InboxRowView` is two lines.
  - `Needs you` row: `rung 2 — MenuRowView`, icon `sparkle`, value = count of To-dos plus Updates, a red status dot when any To-do is overdue, chevron.
- **States:** no Decision · 1–N Decisions · hover/focus on a Decide row (buttons replace the age) · button running (spinner in the button, other button disabled) · action error (the row's tooltip and VoiceOver carry the error; the row shows `Retry` as its primary) · all caught up.
- **Type & color:** as Surface 1 of the 2026-11-01 brief. Decide title 14pt medium; lead icon accent; age 12pt secondary label. Selected row: primary button white with accent text, secondary 22% white.
- **A11y:** ↑↓ reach each Decide row; focus reveals its buttons; Return = primary, ⌘⌫ = secondary. VoiceOver reads "Decision: Review PR #41 in zroute, Minh requested your review, 25 minutes ago, Open, Later". The full reason, hidden on screen, is the tooltip and the accessibility value.

## Surface 2: Needs you submenu (replaces Surface 2 of 2026-11-01)

- **Layout:** header `Needs you N` · segmented filter **All · To-dos n · Updates n** · `Add a to-do…` field with a date chip (`No date`, or the picked day) · separator · the list · separator · `Clear updates` (Updates tab only) · `History ›`. Panel about 350pt wide.
  - List order: To-dos by due date (overdue first, no date last), then Updates newest first.
  - Decisions are not in this submenu. They live only on level 0.
  - The date chip cascades a month calendar to the side with `Today` · `Tomorrow` · `No date` quick picks.
- **Components:**
  - Filter: `rung 1 — NSSegmentedControl` (small).
  - Field: `rung 1 — NSTextField` in a rounded fill; date chip: `CapsuleButton` (fill; accent when a date is set).
  - Calendar: `rung 1 — NSDatePicker` (graphical) in a cascade panel.
  - To-do row: `new (rung 7) — TodoRowView`. One line, 26pt: checkbox (`rung 1 — NSButton` checkbox look) · title (truncates) · date pill. Reason: no kit row has a trailing pill.
  - Date pill: `new (rung 7) — DuePill`. See States for colors.
  - Update row: `InboxRowView` (two lines, `bell` lead, dimmer), as the 2026-11-01 brief.
  - `History`: `rung 2 — MenuRowView` with chevron. Its content (30 days, ticked To-dos with Undo, answered Decisions) is for the spec.
- **States:**
  - Date pill, color is the status:

    | Status | When | Pill | Example |
    |---|---|---|---|
    | Overdue | due date before today | `systemRed` | `Thu 5` |
    | Today | due today | `systemOrange` | `Today` |
    | Soon | due in 1–3 days | `systemYellow` | `Sun` |
    | Later | due in 4 days or more | `systemBlue` | `Fri 13` |
    | None | no due date | fill, secondary label | `No date` |

  - Pill text: `Today`, `Tomorrow`, the weekday within a week, `Wkd D` beyond. Pill fill is the color at 16–22% with the color as text (`systemYellow` text uses a darker ink in light mode).
  - Ticked To-do: strikethrough, then leaves the list after ~0.45 s; Undo lives in History.
  - Empty tab: `✓ You're all caught up`, with the field still shown.
- **Type & color:** row title 14pt; pill 11.5pt semibold, 8pt radius; segmented 12pt medium. On a selected row the pill turns 22% white with white text.
- **A11y:** status is never color alone. VoiceOver reads the pill as "Overdue, Thursday 5 November", "Due today", "Due soon, Sunday 8 November", "Due later, Friday 13 November", "No due date". The checkbox reads "Mark done". The filter is a tab group; the field is in the Tab order before the list.

## Amendments

Decided during review, binding on the spec and design:

1. **Keep awake is not part of Needs you.** No Decision, Update or example uses Awake Leases. This reverses the frame interview answer on issue #35 that ship-day Decisions come from Awake Leases. Which sources raise Decisions on ship day (PR review requests, reminders, …) is left to the spec.
2. **Each item appears in one place.** Decisions only on level 0; To-dos and Updates only in the submenu. The filter drops the `Decisions` tab.
3. **Decide rows are compact.** One line each; answer buttons only on hover or focus; no tinted card, no countdown.
4. **A to-do's status is the color of its date pill**, with no separate badge. This extends the 2026-11-01 rule that semantic color is only the 7pt dot: the date pill may also carry `systemRed`, `systemOrange`, `systemYellow` and `systemBlue`. Blue on a date pill means "due later"; blue on a dot still means review or decision.
5. **The Add a to-do field lives in the submenu**, not on level 0.
