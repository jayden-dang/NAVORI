# Needs you

Decisions a Module raises for you to answer, To-dos you add yourself, and Updates when a Decision expires. Open Decisions sit above the Needs you row on level 0; To-dos and Updates live in the Needs you submenu. Answering a Decision is the one approval that runs a `Destructive` choice from inside the Core.

There is no dedicated CLI verb. Drive it with `navori dispatch`. `navori doctor` still reports `needs_you` and `todo` on the existing `modules` row; there is no new doctor row.

## Commands

```
navori dispatch needs_you '{"raise":{"key":"…","title":"…","reason":"…","expires_at_ms":null,"primary":{"label":"…"},"secondary":{"label":"…"}}}'
navori dispatch needs_you '{"answer":{"id":1,"revision":1,"choice":"primary"}}'
navori dispatch needs_you '{"mark_viewed":{"ids":[1]}}'
navori dispatch needs_you '"clear_updates"'
navori dispatch needs_you '"history"'

navori dispatch todo '{"add":{"title":"…","due_on":"2026-11-09"}}'
navori dispatch todo '{"tick":{"id":1}}'
navori dispatch todo '{"undo":{"id":1}}'
navori dispatch todo '"history"'
```

`raise` is Write; a repeat `key` replaces the open Decision. Each choice may carry an Envelope. `answer` is Destructive and runs the chosen Envelope once accepted. `mark_viewed` and `clear_updates` are Write. `history` is Read.

`todo.add` takes a title and an optional `due_on` (`YYYY-MM-DD`). `tick` moves a To-do to History; `undo` brings it back. `todo.history` is Read.

`navori actions` lists every Action above with its Risk.

## Menu

Open Decisions are the Decide block on level 0: title, reason, age, and two choice capsules. A running answer shows a spinner; a failed answer renames the primary capsule to `Retry` and carries the error in the tooltip and VoiceOver value.

The Needs you row opens a submenu: capsule filter tabs `All · To-dos · Updates` (counts only above zero), `Add a to-do…` with a date chip, the list, `Clear updates` on the Updates tab, and `History ›`. List order is To-dos by due date, then Updates newest first. An empty tab reads `You're all caught up`; level 0 has no such line, and with nothing open the Needs you row just shows no count. Closing the submenu marks drawn Updates viewed. A dispatch failure from answer, add, tick or clear updates stays as one dimmed line in that submenu until the next success of those actions.

`History ›` loads on open: answered Decisions with the label chosen, viewed or cleared Updates, and ticked To-dos with `Undo`, newest first within the journal retention window. An expired Decision is already the Update `Expired: …`, not a second History Decision. Empty History reads `Nothing here yet`.

## What it does not do

- Own a Settings tab or a `[needs_you]` / `[todo]` config section.
- Let Navori AI answer a Decision.
- Add a doctor row; health stays on the modules list.
- Run `cargo xtask perf` for the open budget in this branch — that measurement is once before release.
