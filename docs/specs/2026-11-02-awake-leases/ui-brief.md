# UI brief — Awake menu

> Locked 2026-11-04 via `draft-ui`. Screenshot of the locked draft: [`ui-brief.png`](ui-brief.png). The draft's Harness marks were sketches; the app draws the logos from `HarnessMarkImage`.

## Decision

**Variant C1, "Mark + word":** direction C (By project), amended, with the Harness mark leading each row. C was chosen over A (Ledger) and B (Control Center); C1 over C2 (tile and badge) and C3 (project cards).

The user's words (the Vietnamese quotes are kept as-is, with an English translation appended):
- "tôi muốn nó dễ nhìn, hiện đại và tối ưu nhất" (EN: "I want it easy to read, modern and as optimized as possible")
- "tôi muốn đi theo hướng C - nhưng di chuyển 4 button lên đầu, và xóa đi thông tin '3 sessions working, 5 in background / Awake since 08:10 · Claude Code' chúng ta ko cần nó" (EN: "I want to go with direction C, but move the 4 buttons to the top, and remove that information, we don't need it")
- "tôi muốn show thêm icons của session đó là claude, codex, grok etc" (EN: "I want to also show the icon of each session: claude, codex, grok etc")
- "C1 ok - chốt version này" (EN: "C1 ok, lock this version")

## Grounding

AppKit semantic colors, as in MENU's design; `docs/standards/design-tokens.md` is absent. Rows, fonts and the glass surface follow `docs/design/2026-11-01-menubar-ui-brief.md`.

## Surface: Awake row (level 0)

- **Layout:** unchanged: icon, `Awake`, dot, one value, chevron.
- **Components:** rung 2 — `MenuRow`, `AwakeRowState`.
- **States:** `Off` · `On · Claude Code ×8, 1 hour` · `Not held` · `Needs attention` · `Unavailable`.
- **Type & color:** as the Core row; color only in the dot.
- **A11y:** title then value as one sentence.

## Surface: Awake Submenu (level 1)

- **Layout:** 360 pt wide. The four commands on top, then a problem line only when something is wrong, then Holding grouped by `Held by you`, `Commands` and project. Rows are 26 pt, on one line. The list scrolls past 344 pt.
- **Components:** rung 2 — `MenuRowView`, gaining `isChecked`; new (rung 7) — `AwakeProblemView`, `HoldingList`, `HoldingLine`, `HoldingMarkView`, reason: no existing row has a mark, a dot and a right-aligned word without selection; new (rung 7) — `HarnessMarkImage`, reason: SF Symbols has no Harness logos.
- **States:** off (commands alone, `Release all` dimmed) · on (no status text) · command's Lease held (check, `until 11:38` on the hour) · not held · display refused · action failed or problem recorded.
- **Type & color:** names 14 pt; words 12 pt; project titles 12 pt semibold in `secondaryLabelColor`; a stopped row entirely in `secondaryLabelColor`; monochrome marks; color only in the dots.
- **A11y:** ↑ and ↓ reach the four commands. VoiceOver reads each row as one sentence, with Harness and pid: `Claude Code, NAVORI/main, working, pid 23719`. The pid is otherwise only in the tooltip.

## Signature

The Harness mark leads each one-line row under its project: which agent, where, and whether it is working, read at a glance.

## Job

- **Question:** what is keeping this Mac awake, and how do I change that?
- **Finding:** the Holding list itself, with `working` rows in full ink and `stopped` rows stepped back. The list states it without a summary sentence, by the user's decision.
- **Primary job:** a command on the whole hold, in the four rows on top.

## Amendments

1. The four commands move above everything else.
2. No status text unless something is wrong. This removes the Lease count, `since`, and `idle sleep blocked` from the normal state (amends AWAKE-11.5).
3. `stopped` replaces `stopped · background work`. A session reads as stopped from `Stop` onward, including the 120 seconds before Navori checks for child processes, so "background" would claim work that may not exist.
4. Harness marks are monochrome, per amendment 3 of the menubar brief.
5. A command shows a check while its Lease exists, and `Display on` keeps its name rather than flipping to `Display off`.
6. A long problem no longer sits on the level-0 row: it reads `Needs attention`, with the text in the Submenu.
