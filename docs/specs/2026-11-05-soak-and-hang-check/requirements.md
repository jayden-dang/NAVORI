# Requirements: Soak and hang check

Feature code: SOAK
Status: Approved
Approval: the operator's /shape --go, before reading the spec (operator command: `/shape #27 --auto --go --video --work=opus --interpret=opus --build=sonnet`, relayed by the conductor on 2026-11-05)
Date: 2026-11-05

<!--
Roadmap item ROAD-28 (docs/roadmap/roads/ROAD-28.md), issue #27.
Written unattended under a run charter. Every value marked `agent-decided` was decided by an
agent within the card's Delegated kinds (duration, sampling, load shape, hang threshold, leak
slope rule, report format, where the code lives); `Weigh` marks a reviewing agent's pick. Neither
is the operator's own choice. Decision numbers (D-n) refer to the run's close package.
IDs are immutable once Status is Approved. Retire a requirement by striking it through.
-->

## 1. Run the soak gate on a build

**Story:** As the operator, I want to run `cargo xtask soak` and read one verdict with each signal's start, end and trend, so that I know whether a build holds steady for twenty minutes under load.

- **SOAK-1.1** WHEN the operator runs `cargo xtask soak` THE SYSTEM SHALL build release-flavor code packaged as the dev identity into `build/soak/` and launch it in perf mode (`NAVORI_PERF=1`, `NAVORI_PERF_HOME` set to a fresh run directory under the soak's own temporary home). The `build/soak/` output directory is agent-decided (D6), so that a standalone soak never overwrites perf's `build/perf/`.
- **SOAK-1.2** WHILE the soak runs THE SYSTEM SHALL let the app settle for 1 minute and then drive the load for 20 minutes (agent-decided, D7: inside the operator's ~30-minute Ask-me bound on release-check, with room for settle and teardown).
- **SOAK-1.3** WHILE the load runs THE SYSTEM SHALL repeat this schedule (agent-decided, D9):
  - every 5 s, spawn a new 3-second child process and take a Lease bound to it through `awake` `{"hold_command":{"pid":<child>}}`. That Lease ends when the child exits, so Leases come and go;
  - every 5 s, renew the duration Lease through `awake` `{"hold_for":{"seconds":10}}`. It replaces the current duration Lease (`modules/awake/src/actor.rs:294`);
  - every 60 s, call `awake release_all`;
  - every 20 s, open a TCP listener on `127.0.0.1` in a soak-owned child process and close it 45 s later. 45 s is longer than the Ports Module's 30 s scan cadence with the menu closed (`modules/ports/src/actor.rs:28`), and `ports list` answers from the last scan without rescanning (`modules/ports/src/action.rs:9-10`);
  - every 2 minutes, starting 1 s after a Lease tick, have a second process hold a write transaction on the run's `navori.db` for 4 s. The hold counts as open only once that process confirms it holds the lock, and the timings below run from that moment (agent-decided; Weigh W5, soak-interpret design review r1). Half a second into each hold, one extra `{"hold_for":{"seconds":10}}` renew is sent, so that a Core writer is queued behind the lock before the Core probe 1 s into the hold (SOAK-3.1). Envelopes are sent without waiting for their answer, so no dispatch delays a probe (agent-decided; Weigh W4, soak-interpret review r3: every Event is journaled, `crates/kernel/src/bus.rs` `publish`, while `status` reads no table, `crates/ffi/src/app.rs:46-62`).
- **SOAK-1.4** THE SYSTEM SHALL send the app no Envelope other than `status`, `awake hold_for`, `awake hold_command`, `awake release_all` and `ports list` (agent-decided, D9: the smallest set that drives the three load kinds in the card).
- **SOAK-1.5** WHILE the load runs THE SYSTEM SHALL sample, from outside the app process, its footprint (`phys_footprint`, MiB), thread count and open file-descriptor count every 10 s (agent-decided, D10).
- **SOAK-1.6** WHEN the load ends THE SYSTEM SHALL print one line per signal in the form `soak <footprint|threads|fds> start <v> end <v> trend <±v>/h <ok|LEAK>`, where start and end are the medians of the first and last third of the post-settle samples and trend is their least-squares slope per hour (agent-decided, D12, D14).
- **SOAK-1.7** WHEN no signal is LEAK and no hang was recorded THE SYSTEM SHALL print `soak verdict pass` and exit 0 (output wording agent-decided, D14).
- **SOAK-1.8** WHEN the run ends, fails or is interrupted THE SYSTEM SHALL stop the app (SIGTERM, then SIGKILL after 5 s) and every child process the soak started (5 s agent-decided: the same grace perf gives, `xtask/src/perf/mod.rs:443-458`).
- **SOAK-1.9** IF the app process exits before the load ends THEN THE SYSTEM SHALL print `soak app exited at +<mm:ss>`, print `soak verdict fail` and exit non-zero (agent-decided: a crash is the strongest stall and must not pass as "no hang"; output wording D14).
- **SOAK-1.10** IF the soak cannot build, launch or sample the app, or cannot drive the load (a helper process cannot be spawned; the `navori` helper exits 2 for a usage error, or exits 3 while the app process is still alive; or the store-holding process does not confirm its lock within 3 s; or the user session is locked when the menu probe is due, read from `ioreg -n Root -d1` `IOConsoleUsers` `CGSSessionScreenIsLocked`, reported as `the screen is locked; unlock it and rerun` instead of a menu hang) THEN THE SYSTEM SHALL print what failed and exit non-zero without printing a verdict (the definition of "cannot drive" is agent-decided, design Decision 7).
- **SOAK-1.11** WHEN the load ends THE SYSTEM SHALL print `soak load errors <n> timeouts <n>`, counting Envelopes answered with an Action error (exit 1) and Envelopes not answered within 10 s. Neither count changes the verdict (agent-decided, design Decisions 6 and 7: an Action error under store contention is load behaving, and a stall is judged by the Core probe).
- **SOAK-1.13** WHEN a soak run prints a verdict, pass or fail, THE SYSTEM SHALL print `soak elapsed <mm:ss>` on the line before it (agent-decided, D14; soak-interpret tasks review r1: the gate is expected to fail while #21 is open, and SOAK-9.1 is verified by this time).
- **SOAK-1.14** WHEN a soak run ends with `soak verdict fail` THE SYSTEM SHALL leave its run directory in place (its `navori.db` and the Core's logs under `Library/Logs/Navori Dev`) and print `soak evidence <run directory> pid <app pid>`, so the cause can be read afterwards with that path and `log show --predicate 'processID == <pid>'`. The next soak replaces it (SOAK-7.2). Nothing is kept in the real home (agent-decided; conductor final review M1).
- **SOAK-1.12** WHILE the soak runs THE SYSTEM SHALL hold a display and idle-sleep assertion bound to its own process (`/usr/bin/caffeinate -d -i -w <pid>`), so that the menu can draw the frame its `menuOpened` mark waits for (agent-decided, design review: `menuOpened` is logged from the panel's first presented frame, `apps/macos/Navori/StatusBar/StatusPanel.swift:128`).

## 2. A creeping signal fails the soak and is named

**Story:** As the operator, I want a signal that keeps rising to fail the soak with its name and slope, so that a slow leak cannot reach a release unseen.

- **SOAK-2.1** THE SYSTEM SHALL mark a signal LEAK when, over the post-settle samples split into thirds, median(last third) − median(first third) exceeds max(10% of median(first third), floor) and median(middle third) ≥ median(first third) (agent-decided, D12; source: the perf regression rule "baseline median + max(10%, floor)", `docs/standards/performance.md` Method).
- **SOAK-2.2** THE SYSTEM SHALL use the floors 1 MiB for footprint, 2 for threads and 4 for fds (agent-decided, D12; footprint and threads are the perf floors in `docs/standards/performance.md`; fds has no repo precedent).
- **SOAK-2.3** WHEN any signal is LEAK THE SYSTEM SHALL print `soak verdict fail` and exit non-zero.
- **SOAK-2.4** WHEN the sampled process opens one more file descriptor on every load cycle and never closes it THE SYSTEM SHALL print `soak fds … LEAK` with a positive trend.
- **SOAK-2.5** IF fewer than 9 post-settle samples were taken THEN THE SYSTEM SHALL print `soak too few samples (<n>)`, print `soak verdict fail` and exit non-zero instead of judging any signal (agent-decided: 9 is the fewest samples that give three per third, so each median in SOAK-2.1 rests on at least 3 points; output wording D14).

## 3. A Core or menu stall is reported as a hang

**Story:** As the operator, I want a Core or menu that does not answer within 2 s reported as a hang, with when it happened and which side stalled, so that a stall like issue #21 shows up before a release.

- **SOAK-3.1** WHILE the load runs THE SYSTEM SHALL probe the Core every 30 s, and once 1 s into each store write hold (SOAK-1.3), by running `navori --profile dev status` against the run's socket (agent-decided, D11).
- **SOAK-3.2** IF a Core probe has not answered within 2 s THEN THE SYSTEM SHALL end that probe and record a hang on side `core` (agent-decided, D11: four times Apple's 500 ms "proper hang" (WWDC23 session 10248) as margin for a busy machine, and below the store's 5 s `busy_timeout` (`crates/store/src/lib.rs:100`) so that a status waiting behind a store lock counts).
- **SOAK-3.3** WHEN the last trend sample has been taken THE SYSTEM SHALL probe the menu once, by sending the app SIGUSR1 and waiting for its `menuOpened` log line (agent-decided, D11).
- **SOAK-3.4** IF the `menuOpened` line has not arrived within 2 s of the SIGUSR1 THEN THE SYSTEM SHALL record a hang on side `menu` (agent-decided, D11). A menu hang means "no `menuOpened` mark within 2 s". With a locked session caught before the probe (SOAK-1.10), the remaining cause is a main thread that does not draw the menu.
- **SOAK-3.5** THE SYSTEM SHALL send the app no SIGUSR1, SIGUSR2 or SIGINFO before the last trend sample, so that the menu stays closed through the trend window (agent-decided, D11).
- **SOAK-3.6** WHEN a hang is recorded THE SYSTEM SHALL print `soak hang <core|menu> at +<mm:ss> waited <ms> load <1-minute load average>` (output wording agent-decided, D14).
- **SOAK-3.7** WHEN any hang was recorded THE SYSTEM SHALL print `soak verdict fail` and exit non-zero.

## 4. The release check runs the soak gate

**Story:** As the operator, I want `cargo xtask release-check` to run the soak gate after the perf gate and refuse the release when the soak fails, so that no tag is cut on a build that leaks or hangs.

- **SOAK-4.1** WHEN `cargo xtask release-check` has passed the perf gate THE SYSTEM SHALL run the short soak (SOAK-1.2 to SOAK-3.7) on the bundle the perf gate just built (`build/perf/Navori Dev.app`), without building another (agent-decided, D6).
- **SOAK-4.2** IF the short soak fails THEN THE SYSTEM SHALL end `cargo xtask release-check` with `soak gate failed`, exit non-zero and print no tag command (output wording agent-decided, D14).
- **SOAK-4.3** (guard) WHEN `cargo xtask release-check` runs THE SYSTEM SHALL CONTINUE TO run preflight, then `cargo xtask ci`, then the perf gate, in that order, stopping at the first failure (`xtask/src/release.rs:177-180`).
- **SOAK-4.4** (guard) WHEN every step of `cargo xtask release-check` passes THE SYSTEM SHALL CONTINUE TO print the tag and the `git tag … && git push origin …` line (`xtask/src/release.rs:181-182`).

## 5. A long soak before a big release

**Story:** As the operator, I want `cargo xtask soak --long` to run the same checks for two hours and report without failing, so that before a big release (0.1.x to 0.2.0, or to 1.0.0) I can read how a build behaves over hours.

- **SOAK-5.1** WHEN the operator runs `cargo xtask soak --long` THE SYSTEM SHALL drive the SOAK-1.3 schedule at one fifth of its rate for 2 hours after the settle (agent-decided, D8, D9).
- **SOAK-5.2** WHERE `--long --for <duration>` is given THE SYSTEM SHALL drive the load for that duration instead of 2 hours (agent-decided, D8).
- **SOAK-5.3** WHILE a long soak runs THE SYSTEM SHALL sample every 60 s, and probe the Core as SOAK-3.1 says (agent-decided, D10).
- **SOAK-5.4** WHEN a long soak ends THE SYSTEM SHALL print the same signal, hang and verdict lines as the short soak, and exit 0 whatever the verdict.
- **SOAK-5.5** THE SYSTEM SHALL never run the long soak from `cargo xtask release-check`.

## 6. Observe the installed app

**Story:** As the operator, I want `cargo xtask soak observe` to print my installed Navori's footprint, threads and open files, so that I can watch them over days without the app gaining anything.

- **SOAK-6.1** WHEN the operator runs `cargo xtask soak observe` THE SYSTEM SHALL print one line with the local time, pid, footprint (MiB), thread count and open file-descriptor count of the running `/Applications/Navori.app/Contents/MacOS/Navori` process (agent-decided, D13).
- **SOAK-6.2** WHERE `--every <duration>` is given THE SYSTEM SHALL print one such line per interval until interrupted (agent-decided, D13).
- **SOAK-6.3** IF no process from `/Applications/Navori.app` is running THEN THE SYSTEM SHALL print `Navori is not running` and exit non-zero (output wording agent-decided, D14).
- **SOAK-6.4** WHILE observing THE SYSTEM SHALL send the observed process no signal, send it no Envelope and write no file. The probe helper is compiled into `build/soak/` before observing starts (D13, D15).

## 7. A soak leaves my own Navori alone

**Story:** As the operator, I want a soak run to leave my real config, database, socket, logs, Harness hooks and Docker untouched, so that I can soak on the Mac I work on.

- **SOAK-7.1** WHEN a soak starts THE SYSTEM SHALL create `/tmp/navori-soak-<uid>/` (its run home is the nested `/tmp/navori-soak-<uid>/navori-perf-<uid>/`, because `PerfHome::prepare` names that leaf, `xtask/src/perf/home.rs:37`) with mode 0700 owned by the current user, and refuse to start if that path is a symlink, belongs to another user or has a mode other than 0700. An existing directory that passes these checks is reused (D5, Weigh mechanism).
- **SOAK-7.2** WHEN a soak starts THE SYSTEM SHALL stop the app processes recorded by a previous soak under its own home before replacing that home, whether that soak ran from `build/soak/` or from the gate's `build/perf/` bundle, and never touch perf's `/tmp/navori-perf-<uid>/` (D5, Weigh mechanism).
- **SOAK-7.3** IF the run's socket path would exceed 103 bytes THEN THE SYSTEM SHALL refuse to start, so that the Core never falls back to the shared `/tmp/navori-<uid>/dev.sock` (`crates/config/src/lib.rs:64-68`).
- **SOAK-7.4** WHEN the soak prepares a run directory THE SYSTEM SHALL remove the Docker socket link from it before launching the app and print `docker=not linked` (agent-decided, D18).
- **SOAK-7.5** THE SYSTEM SHALL run every `navori` CLI call with `--profile dev` and `HOME` set to the run directory.
- **SOAK-7.6** WHEN a soak run ends THE SYSTEM SHALL have created, changed or removed nothing in the real home's `Library/Application Support/Navori`, `Library/Application Support/Navori Dev`, `Library/Logs/Navori` and `Library/Logs/Navori Dev`, in `/tmp/navori-<uid>/`, or among the hook files `~/.claude/settings.json`, `~/.codex/hooks.json` and `~/.grok/hooks/navori.json` and their `*.navori-bak-*` backups (`modules/integrations/src/infra/files.rs:29-47`). Verified during a run by `lsof` on the soak's app showing no open path in those places, by `find … -newer <start marker>` on the dev paths, and by comparing hashes of the hook files (the `navori` lines of `~/.claude/settings.json`) before and after. Modification times alone cannot prove it, because the installed Navori and other sessions write there.

## 8. The perf gate and the other commands stay as they are

**Story:** As the operator, I want `cargo xtask perf` and every existing `cargo xtask` command to behave exactly as before, so that adding the soak costs nothing I already rely on.

- **SOAK-8.1** (guard) WHEN `cargo xtask perf` runs THE SYSTEM SHALL CONTINUE TO measure, judge and print the same metrics, budgets, floors and lines, in the same phases, from the same `/tmp/navori-perf-<uid>/` home (`xtask/src/perf/mod.rs:34-81,92-102`; `xtask/src/perf/home.rs:37`; `docs/standards/performance.md`).
- **SOAK-8.2** (guard) WHEN the xtask test suite runs THE SYSTEM SHALL CONTINUE TO pass every existing perf test unchanged (Lock: operator, 2026-11-05, "perf output and tests unchanged"; guard first raised as Weigh W3; D4).
- **SOAK-8.3** (guard) WHEN any existing `cargo xtask` subcommand is invoked THE SYSTEM SHALL CONTINUE TO accept the same arguments and do the same work (`xtask/src/main.rs:140-163`).

Files this change touches:
- `xtask/src/perf/mod.rs`: guarded by SOAK-8.1 and SOAK-8.2. The only edit is `mod` → `pub(crate) mod` for `home`, `launch` and `logstream` (Lock: operator, 2026-11-05, "OK, not a change"; D4; narrowed from four modules to three by the plan review, because soak does not use `perf::probe`).
- `xtask/src/release.rs`: guarded by SOAK-4.3 and SOAK-4.4.
- `xtask/src/main.rs`: guarded by SOAK-8.3.
- `xtask/src/soak/` (new): no behavior to guard.
- `xtask/assets/soak-probe.swift` (new): no behavior to guard.
- `docs/agents/project.md` (release step 1 text), `docs/standards/performance.md` or a new standards note: documentation only, no behavior to guard.

## 9. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** **SOAK-9.1** WHEN `cargo xtask release-check` runs the short soak THE SYSTEM SHALL add no more than 23 minutes to it (agent-decided, D7: 1 minute settle + 20 minutes load + teardown; the operator's Ask-me bound is about 30 minutes). Verified by the elapsed time SOAK-1.13 prints. **SOAK-9.2** THE SYSTEM SHALL add no timer, polling loop, signal handler, Envelope, IPC method, FFI function or CLI command to the app, the Core or the `navori` CLI for the soak (card Must keep; ARCH-16). Verified by the diff touching only `xtask/` and docs, and by `cargo xtask perf` staying within its budgets.
- **Security:** **SOAK-9.3** WHEN the soak creates its temporary home THE SYSTEM SHALL keep the run socket inside a 0700 tree owned by the current user, so that no other local user can drive the soak's Core (THR-4 at TB-2, `docs/security/threat-model.md`, Approved). Verified by a test on the directory's mode and owner, and by SOAK-7.1 and SOAK-7.3.
- **Reliability:** **SOAK-9.4** WHEN `cargo xtask soak` runs 3 times in a row on the same revision with no code change, on a machine with no other build running, THE SYSTEM SHALL give the same verdict and the same set of LEAK signals and hang sides in all 3 runs. Verified manually once built (same shape as PERF-6.4). No standing reliability doc applies (`docs/ops/reliability.md` absent).
- **Accessibility:** None. This is a developer command-line tool with no UI.

## Out of Scope

- Fixing anything the soak finds, issue #21 included. While #21 is open, the short gate is expected to record `soak hang core` during store write holds and fail `cargo xtask release-check` until #21 is fixed. This is the case the card's Example 3 names; the stall length is unmeasured.
- Absolute budgets for the baseline-only metrics (a GOAL-11 amendment), and any change to GOAL-11.
- Recording the perf baseline (#24).
- Any change to what `cargo xtask perf` measures, judges, prints or how long it takes, and any edit to `xtask/assets/perf-probe.swift`.
- A history, baseline or result file for the soak or for `observe`: output is printed only.
- Any probe, timer, wake-up, signal handler, Action, IPC method or CLI command in the app, the Core or the `navori` CLI for the soak.
- Sending signals to, or driving load on, the installed `/Applications/Navori.app`.
- Menu probes inside the trend window, synthetic clicks through Accessibility, `powermetrics` and `sudo`.
- Reading the user's real Docker daemon during a soak.
- Isolating the dev preferences domain. The soak bundle shares `dev.navori.app.debug` preferences with `cargo xtask run`, as the perf gate already does (`docs/standards/performance.md` Method). The app code writes no preferences of its own: there is no `UserDefaults` or `@AppStorage` in `apps/macos/Navori`, and `LoginItem` (SMAppService) and `CLIInstaller` (`~/.local/bin/navori`) run only from Settings buttons (`apps/macos/Navori/Features/Settings/SettingsView.swift:55,85`), which the soak never presses. What remains is AppKit's own window state in that domain, accepted as perf accepts it. SOAK-7.6 does not cover that domain.
- Running the soak on any machine other than the author's Mac, or in GitHub Actions.

## Open Questions

- ~~Whether releases stay blocked while issue #21 is open.~~ **Resolved by the operator, 2026-11-05 (Lock):** "Keep it, fix #21 first". Build as specified, with no skip and no override; releases wait until #21 is fixed under its own ticket. Under this spec's Locks, the short gate's store holds are expected to record `soak hang core` and fail `cargo xtask release-check` until then.

- ~~Whether "the perf gate unchanged" covers source as well as behavior.~~ **Resolved by the operator, 2026-11-05 (Lock):** "OK, not a change". Behavior is what counts, so the 3-line `mod` → `pub(crate) mod` edit for `home`, `launch` and `logstream` in `xtask/src/perf/mod.rs:1-7` stays, with perf's output and tests unchanged.
