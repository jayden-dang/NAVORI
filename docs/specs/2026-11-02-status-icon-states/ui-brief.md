# UI brief: Status icon states

Feature code: ICON
Status: Locked
Date: 2026-11-03

Screenshot: `./ui-brief.png`, the locked page with real status item captures. Spike notes: `.skills/ICON/spike.md` (local).

## Decision

**G, Fold refined.** The Navori N made of three folded planes, where each state has its own shape as well as its own light. Chosen over the SF Symbol sets (A, B, C), Bearing (E), Tile (F) and the original Fold (D).

The user's words, in order:
- "icosn và logo hiện tại chưa đủ đẹp … hơi đơn điệu - tôi cần nó hiện đại và signature hơn" (EN: "the current icons and logo aren't good enough … a bit monotonous; I need them more modern and signature")
- "Tôi nghỉ D sẽ tối ưu hơn" (EN: "I think D is better")
- "Chốt G" (EN: "Lock G")

## Grounding

Token source of record: macOS system semantics. The menu bar icon is a template image, and the system tints it. No color is chosen for the icon. `docs/design/2026-11-01-menubar-ui-brief.md` amendment 3 (monochrome icons) still holds.

## Surface: status item icon

**Layout:** one 18 × 18 pt template image centered in the status item's 38 × 33 pt cell. Coordinates below are on that 18 pt grid with y going down.

The three planes:

| Plane | Points |
|---|---|
| Left | (2.6, 6.5) (6.1, 11.6) (6.1, 15.4) (2.6, 15.4) |
| Diagonal | (2.6, 2.6) (6.7, 2.6) (15.4, 15.4) (11.3, 15.4) |
| Right | (11.9, 2.6) (15.4, 2.6) (15.4, 11.5) (11.9, 6.4) |

**Components:**
- `rung 2 — NSStatusItem` button image, as `StatusBarController.render` sets today.
- `new (rung 7) — hand-drawn glyph set`: SF Symbols have no folded N.
- `new (rung 7) — attention mark`: drawn into the same template image.

**States:**

| State | Drawing | Description source |
|---|---|---|
| Normal | Left and Right at 50% alpha, Diagonal solid | `summary` |
| Keeping awake | Solid rounded square at (1.3, 1.3), 15.4 × 15.4, radius 4.4, with the three planes cut out at 68% scale around (9, 9) | `summary` |
| AI running | Left and Right solid, Diagonal cut into pieces over t = 0–0.26, 0.37–0.63 and 0.74–1 of its length | `summary` |
| Problem | Solid triangle (9, 2.2) (16.4, 15.4) (1.6, 15.4), with `!` cut out: 1.8 wide, round caps, (9, 6.8)–(9, 10.8) and (9, 13)–(9, 13.4) | `summary` |
| Core not running | 3.1-wide lines: (3.2, 3.2)–(14.8, 14.8) solid; (14.8, 3.2)–(10.4, 7.6) and (7.6, 10.4)–(3.2, 14.8) at 50% alpha | `Core not running` |
| Starting | The three planes as 0.9-wide outlines, round joins, dimmed with `appearsDisabled` | `Starting` |

**Attention mark:** clear a circle of radius 3.7 at (14.5, 3.5), then fill a circle of radius 2.5 there. It applies to Normal, Keeping awake, AI running and Problem. Core not running and Starting have no mark, because no icon state exists then.

**Motion:** the Loop is opacity 1 to 0.35 over 1 s each way, on the AI running glyph only. The Beat is opacity 1 to 0.2 and back over 0.5 s in total. Neither runs under Reduce Motion.

**Type & color:** no type. Partial alpha is used only for the fold tone; a capture on a real status item showed it survives in a template image in a dark menu bar.

**A11y:** every state differs in shape, so each stays readable with motion off and without color (ICON-11.6). VoiceOver reads the description above.

## Surface: logo

**Layout:** app icon on a 100-unit square with corner radius 23, on a vertical gradient from `#fbfaf7` to `#e3e0d8`. The N is the three planes translated by (19, 19) and scaled by 3.45.

**Components:** `rung 2 — xtask/assets/make-icon.swift`, which draws the body on Apple's 824 px grid inside a 1024 px canvas today; the 100-unit square above maps onto that body. `new (rung 7)` — the artwork.

**States:** none in the app icon.

**Type & color:** Left and Right `#8b8f99`, Diagonal `#1a1d24`. As a one-color mark: Left and Right at 50% alpha, Diagonal solid.

**A11y:** not applicable beyond the app name.

## Signature

The same folded N in every state where Navori is healthy, with the fold itself saying what is happening.

## Amendments

Decided during review:
1. Drop the sparkle logo. It is a generic AI symbol and says nothing about Navori.
2. Each state needs its own shape. Brightness alone is not enough (ICON-11.6).
3. The Problem glyph is one solid triangle, because the `!` was lost on the two-tone version.

## Open

- The mark leaves a small piece of the N's right stem on Normal. The user locked G after seeing this.
- A light menu bar and the highlighted state with the menu open have not been seen on a real status item.
- The Netflix mark is a nearby N ribbon. This has been flagged and accepted for now.
- Light moving piece by piece across the broken fold is an idea only. It needs separate layers and has not been measured, so it is not part of this lock.
