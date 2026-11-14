# Changelog

All notable changes to Navori. Versions follow the workspace version in `Cargo.toml`.

## v0.1.1 — 2026-11-14

### Needs you (NEEDS)

- "As the user, I want each open Decision as one compact row at the top of the menu, so that I see what needs my decision the moment I open Navori."
- "As the user, I want to answer a Decision with one click or one key, so that the action I chose runs and the Decision leaves the menu."
- A Needs you submenu with To-dos (with a colored due-date pill), Updates, a calendar and a History of the last 30 days.
- "As a Module author, I want to raise a Decision with a title, a reason and its choices by dispatching `needs_you.raise`, so that the user sees in one place what needs their decision." Modules can now dispatch an Action to another Module from inside `run`.

### Awake with the lid closed (LID)

- "As the user with an agent working and the adapter in, I want to close the lid and have the Mac stay awake, so that the session carries on while the lid is shut."
- "As the user on battery, I want `Lid closed` to show `Needs adapter` and let the lid sleep the Mac, so that a closed Mac in a bag never stays awake on battery."
- "As the user, I want to turn `Lid closed` on or off from the Running submenu or from `navori awake lid on|off`, so that I choose when the Mac may stay awake with the lid closed."
- "As the user whose Navori was force-killed while holding the lid, I want the next launch to give lid sleep back, so that closing the lid sleeps the Mac again."
- A `Last sleep` row at the top of the Running submenu, and a lid row in `navori doctor`.

### Navori AI agent loop (LOOP)

- "As the user, I want Navori AI to look things up with Navori's Read Actions during a Turn and answer from what they return, so that asking "which ports are open?" gets an answer from `ports.list` instead of a guess."
- "As the user, I want Navori AI to be unable to run anything but Read Actions before approvals exist, so that no Turn can stop my processes or change my settings."
- "As the user, I want each Turn kept in its Conversation with what started it, its messages, every tool call with its result and the tokens each model call used, and a follow-up Turn to carry that record to the model."
- Every model call goes through the Context Gateway, where it is shielded and audited.

### Soak and hang check (SOAK)

- "As the operator, I want to run `cargo xtask soak` and read one verdict with each signal's start, end and trend, so that I know whether a build holds steady for twenty minutes under load."
- "As the operator, I want `cargo xtask release-check` to run the soak gate after the perf gate and refuse the release when the soak fails, so that no tag is cut on a build that leaks or hangs."

### Fixes and amends

- `core.status` keeps answering while the store is locked (#21).
- Ports: a list asked for before the first scan is answered once that scan lands (PORTS).
- Running menu: port rows show icon actions and a one-line stop confirmation. A project's ports are stopped from its child menu. A session row truncates only its branch (PSTOP).
- Needs you menu polish: a pill filter, one to-do input, no duplicate "all caught up", and a to-do checkbox that no longer stretches across the row.
- Kernel: Module shutdown is cancelled when the kernel is dropped (KCONTRACT).

### Misc

- Codebase untangle refactor, the roadmap closing MILE-2 and MILE-4, and docs and spec upkeep across PERF, PROCS, MENU, AWAKE, ICON, HOOKS, ASK and RELEASE.
