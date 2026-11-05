# ROAD-28 soak-and-hang-check

<!--
The operator's intent for one road, written at /idea while they are at the keyboard.
`/shape --auto` reads every slot as their standing word; agents decide everything else.
At most 30 non-blank lines: a file path, a table name or an API shape belongs in the spec,
not here. Check with `python3 <shape skill>/scripts/check_card.py <this file>`.
Drafted by an agent on 2026-11-05 from the operator's conversation; confirmed by the operator on 2026-11-05.
-->

**Outcome:** Before a release, the user runs one command that keeps a separate Navori running under a fixed load for a set time, and learns whether memory, threads or open files crept up, or whether the menu or the Core stopped answering — what the two-minute perf gate cannot see.

**Examples:**
1. Jayden runs the release check before tagging; its short soak gate stays flat on footprint, threads and open files with nothing stalled, so it passes and prints each signal's start, end and trend.
2. A change leaks one open file per Lease; after the load has started and ended Leases for the whole run, the open-file count keeps rising, so the soak fails and names that signal and its slope.
3. A status request arrives while the store lock is held (the situation of #21) and the Core does not answer within the threshold; the soak reports a hang with when it happened and which side stalled.
4. On his own installed Navori, Jayden reads footprint, threads and open files of the running app over days, and the release build has gained no probe and no extra wake-up.

**Must keep:** The GOAL-11 budgets and the two-minute perf gate unchanged. The release build gains no probe, timer or wake-up for this. A soak run never touches the user's real config, database, socket, logs or Harness hooks.
**Off limits:** Fixing what the soak finds. Absolute budgets for the baseline-only metrics (a GOAL-11 amendment). Recording the perf baseline (#24).
**Decided:** The gate runs on its own bundle with a temporary home and a fixed load (Leases coming and going, ports opening and closing, the store lock held now and then), like the perf gate, and never on the installed release app; the installed app is only observed. Two tiers: a short gate with an accelerated load, part of the release check, whose failure blocks the release; and a long soak of 2 to 3 hours as its own command, report only, that the operator runs before a big release such as 0.1.x to 0.2.0 or to 1.0.0 (operator, 2026-11-05).
**Delegated:** The short gate's duration (stated with its reason), how signals are sampled, the shape of the load, the hang threshold and the leak slope rule (each stated with its source), the report's format, and where the code lives.
**Ask me:** Any change to GOAL-11 or to the perf gate; anything that would make the release check take more than about 30 minutes longer.
**UI:** none
