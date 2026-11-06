# ROAD-41 awake-lid-and-last-sleep

<!--
The operator's intent for one road, written at /idea while they are at the keyboard.
`/shape --auto` reads every slot as their standing word; agents decide everything else.
At most 30 non-blank lines: a file path, a table name or an API shape belongs in the spec,
not here. Check with `python3 <shape skill>/scripts/check_card.py <this file>`.
Drafted by an agent on 2026-11-06 from the operator's conversation (ticket #34); confirmed by the operator on 2026-11-06.
-->

**Outcome:** While an agent is working and the Mac is on its power adapter, the user closes the lid and the work carries on. The Running menu also tells them when the Mac last slept.

**Examples:**
1. Jayden has a Claude Code session working in klynt, the adapter plugged in and `Lid closed` on; he closes the lid at 09:20 for an hour. The Mac never sleeps, and the session keeps going.
2. Same Jayden, but on battery at 80%: the `Lid closed` row still shows on, with a grey `Needs adapter`. He closes the lid and the Mac sleeps, as macOS does today.
3. Nothing holds the Mac awake, the adapter is plugged in and `Lid closed` is on. Jayden closes the lid and the Mac sleeps within seconds.
4. The Mac slept at 09:20 and woke at 09:27. Jayden opens the Running menu: its first row reads `Last sleep 09:20`; yesterday's would read `Yesterday 12:03`.
5. Navori is force-killed while it holds the lid. The next time Navori starts it gives lid sleep back, so closing the lid sleeps the Mac again; a setting another app made is left alone.

**Must keep:** Leases, the power assertion and Tracking agents behave as today. The Running menu layout of 2026-11-04 (project rows are a dot and a name). The GOAL-11 budgets; the menu never reads system logs when it opens.
**Off limits:** Holding the lid on battery. Per-project sleep information. A sleep log or history view. A watchdog or helper process. Root or a privileged helper.
**Decided:** "adepter only". Variant C as amended: one quiet `Last sleep` row at the top of the Running submenu, a `Lid closed` switch under Tracking agents, nothing about sleep in project menus. The lid hold applies only while a Lease is held. On battery the switch stays on and shows `Needs adapter`. After a crash, Navori gives lid sleep back at its next launch only when its own record says it was holding it (operator, 2026-11-06).
**Delegated:** The Journal event's name and payload, which sleeps count as a sleep (dark wakes and maintenance sleeps do not), how the power source is watched, the switch's exact wording, the Action and CLI names, where the code lives.
**Ask me:** Any change to holding on battery; showing sleep anywhere other than the one row; adding a process.
**UI:** new
**UI brief:** docs/design/2026-11-06-awake-lid-and-last-sleep-ui-brief.md
