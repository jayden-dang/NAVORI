# UI brief: Awake with the lid closed, and the last sleep

> Locked 2026-11-06 via `draft-ui`. Screenshot: [`2026-11-06-awake-lid-and-last-sleep-ui-brief.png`](2026-11-06-awake-lid-and-last-sleep-ui-brief.png). It shows real local data from the lock date.
> Amends Surface 2 of [`2026-11-04-running-menu-ui-brief.md`](2026-11-04-running-menu-ui-brief.md). Leaves its Surface 1 (the `Running` row) and Surface 3 (the project child menu) as they are. Waits for the spec that adopts it. Nothing here is built yet.

## Decision

**Direction C, reduced in review: a `Lid closed` switch beside `Keep awake` and `Tracking agents`, and one quiet `Last sleep` row at the top of the Running submenu. Nothing about sleep inside project menus.** Chosen over A (a grey line and a three-choice `Lid closed` submenu) and B (a sleep log submenu with a 24-hour strip).

The user's words, in order (Vietnamese verbatim, with an English translation):
- "Tôi muốn upgrade phần awake 1 tí - thứ nhất là cho phép 1 tính tăng máy vẫn hoạt động khi gập máy, thứ 2 là show trạng thái nghỉ gần nhất của máy - và của từng project trong phần awake" (EN: "I want to upgrade the awake part a bit: first, a feature that keeps the Mac working when the lid is closed; second, show the Mac's most recent rest, and each project's, in the awake part")
- "Tôi thấy C đang ổn nhất- nhưng cần upgrade phần này gọn hơn, hiện đại hơn" (EN: "C looks best, but this part [the project header's 'Paused 7m' line] needs to be tidier and more modern")
- "Nếu thế hiện tại các dự án đều show, nó sẽ tạo ra duplicated thông tin - bỏ phần đó ở trong từng dự án đi, show nó bên ngoài - thay cho phần warning, không cần nó - chỉ show thời gian cuối cùng máy sleep là khi nào là được." (EN: "Then every project shows it, which duplicates information. Remove it from each project and show it outside, in place of the warning, which isn't needed. Just show when the Mac last slept.")
- "lock it"

## Grounding

- **Token source of record:** macOS system semantics, as in the 2026-11-01 brief. No new color.
- **Kit:** `MenuRowView`, `MenuDivider`, the panel cascade, the status dot.

## Signature

**One quiet line says when the Mac last slept; one switch stops the lid from making it sleep.**

## Surface 2: Running submenu (amended)

- **Layout:** `Last sleep` row · separator · `Keep awake` (opens its third level) · `Tracking agents` · `Lid closed` · a problem line only when something is wrong · separator · project rows · separator · `Stop all ports…`. Rows stay 26pt in a panel about 272–300pt wide.
- **Components:**
  - `Last sleep`: `rung 2 — MenuRowView`, icon `moon`, value is the time. Not selectable: no hover pill, no action.
  - `Lid closed`: icon `laptopcomputer`. Drawn as a trailing switch: `new (rung 7) — switch on a menu row`, because the kit has no switch. Fallback if the design prefers the kit: `rung 2 — MenuRowView` with an `On`/`Off` value and green dot, the same as `Tracking agents`.
  - The orange warning card in direction C was dropped. The existing problem line (`AwakeProblemView`) is unchanged.
- **States:**
  - `Last sleep` value: `09:20` today · `Yesterday 12:03` · `Wed 11:43` within a week · a date beyond that. No row before Navori has seen a sleep.
  - The value is when the Mac went to sleep, not how long it slept. Dark wakes and maintenance sleeps do not count; a lid close, idle sleep, Apple menu sleep, or low-battery sleep does.
  - `Lid closed` off: closing the lid sleeps the Mac (macOS default).
  - `Lid closed` on: closing the lid does not sleep the Mac while a Lease holds it. With no Lease held, the lid sleeps the Mac as usual.
  - `Lid closed` refused by macOS: the problem line says so, the switch returns to off.
- **Type & color:** as Surface 1 of the 2026-11-01 brief. The `Last sleep` value is secondary label color. The switch's on color is the system green.
- **A11y:** `Last sleep` reads as static text, for example `Last sleep, today at 09:20`. `Lid closed` reads as a switch with its state: `Lid closed, stay awake, on`.

## Surface 3: Project child menu

Unchanged from the 2026-11-04 brief. **No sleep or rest line in any project menu** (Amendment 3).

## Amendments (decided in review)

1. Direction C over A and B.
2. The project header's `Paused 7m · Mac slept 09:20–09:27 while working` sentence was too long. Three compact takes were drawn (a pill beside the name, a pill and time, a gap bar).
3. Then removed entirely: per-project sleep info duplicates the same fact on every project. The user's ask for "each project's rest" is dropped.
4. The warning card (`Lid sleep paused 3 projects` with a `Stay awake with lid closed` button) is removed. Only the last sleep time remains, at the top.
5. The time shown is when the Mac last slept. No duration, reason or project list.

## What the draft showed about the data

Read from this Mac on 2026-11-06. Inputs for the spec, not decisions.

- `PreventUserIdleSystemSleep`, the assertion Navori holds today, never prevents lid-close sleep. Apple's `IOPMLib.h` says the system "may still sleep for lid close".
- `pmset -g log` shows 5 lid-close sleeps between 2026-11-03 and 2026-11-06. At 4 of them, Navori journal Leases for klynt, NAVORI, zroute or skills sessions were held. 3 of those 4 were on battery, one at 21%.
- `IOConnectCallScalarMethod` on the `IOPMrootDomain` user client with `kPMSetClamshellSleepState` (public header `IOPMLibDefs.h`, undocumented use) returned success as uid 501, no root. No registry property shows the effect while the lid is open; three lid-close runs on 2026-11-06 (a 5 s heartbeat plus `pmset -g log`) settled it:
  - **Held:** setting on, process alive, lid closed 09:52:39–09:53:09 on AC. Heartbeat unbroken, no sleep entry. The call works without root.
  - **Process exited without restoring:** lid closed 09:54:59–09:57:10. Heartbeat unbroken, no sleep entry. **The setting outlives the process**: a Navori crash would leave the Mac unable to sleep on lid close until something sets it back.
  - **Control, after restoring to 0:** lid closed 09:58:04; `Clamshell Sleep` at 09:58:08 on battery, woke 09:58:56, heartbeat gap 09:58:09–09:58:59. Restoring works, and the two runs above were the setting's doing.
- `IOPMrootDomain` publishes `Last Sleep Reason` and `Wake Reason` (which names `lid`). A sleep notification can journal the reason at the time it happens; the menu then reads the journal, never `pmset` (AWAKE-11.10, GOAL-11).
- The journal's `awake.lease_started` / `awake.lease_ended` payloads carry only the Lease key, not the project.

## Open questions for `design-solution`

Agent proposals, not reviewed by the user:

- **Battery:** the user did not decide. Proposal: `Lid closed` keeps the Mac awake on battery down to 20%, then lets it sleep. Alternative: adapter only, which would have covered 1 of the 4 interrupted lid closes above.
- **Scope of the hold:** proposal: only while a Lease is held, never on its own, because a closed Mac in a bag can overheat. Thermal pressure should also release it.
- **Crash safety (confirmed risk, see the second lid run):** if Navori exits while the clamshell setting is on, the Mac does not sleep on lid close. Needs a guard released on drop, like `AssertionGuard`, and an unconditional restore at launch. A crash, `kill -9` or power loss still leaves it on until the next Navori launch; whether a reboot clears it is untested.
- **Placement:** the draft puts `Last sleep` at the top. Under `Lid closed` was offered and not chosen.
- **Spec conflicts to amend:** AWAKE-11.7 (menu text about Leases only), AWAKE-11.10 (the menu must not read system state on open).
