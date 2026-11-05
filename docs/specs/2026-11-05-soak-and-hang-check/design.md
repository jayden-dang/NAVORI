# Design: Soak and hang check

Feature code: SOAK
Status: Draft
Date: 2026-11-05
Requirements: ./requirements.md

<!-- Written unattended under a run charter. `agent-decided` marks a value an agent picked
within the card's Delegated kinds; `Weigh` marks a reviewing agent's pick. Neither is the
operator's own choice. D-n numbers refer to the run's close package. -->

## Context

`cargo xtask perf` (PERF, ROAD-5) is a two-minute gate. It packages release code under the dev
identity into `build/perf/`, runs it in perf mode (`NAVORI_PERF=1`, `NAVORI_PERF_HOME=<run dir>`)
under `/tmp/navori-perf-<uid>/`, and reads the app from outside: a compiled Swift helper calls
`proc_pid_rusage` and `proc_pidinfo` (`xtask/assets/perf-probe.swift`), and a `log stream` reader
picks up the `menuOpened` and `statusItemDrawn` marks the app logs in perf mode
(`xtask/src/perf/logstream.rs`). `cargo xtask release-check` runs preflight, then `ci`, then that
gate (`xtask/src/release.rs:177-180`). Nothing samples for longer than five 12-second idle
windows, nothing counts open files, and nothing detects a hang. The IPC client has no timeout
(`crates/ipc/src/lib.rs:232-259`), so a stalled Core makes `navori status` wait forever.

The binding constraint is the card's Must keep (requirements SOAK-9.2): the release build gains
no probe, timer or wake-up, and the perf gate stays as it is. Everything that soaks therefore
lives in `xtask/` and watches the app from outside, using only what the app already has: perf
mode, the existing CLI Envelopes, the existing SIGUSR1 → `menuOpened` round trip, and the
process's kernel counters. This rules out the obvious alternative, an in-app soak mode with its
own sampler and load timers. `PerfProbe` is compiled into every flavor
(`apps/macos/Navori/Platform/PerfProbe.swift:13-22`), so anything added there ships in release
and adds a timer against ARCH-16. A second constraint is xtask's own: `unsafe` is forbidden and
xtask has no `libc` (`Cargo.toml` lints; `xtask/Cargo.toml`). All process reads go through a
Swift helper, as perf's do, and signals go through `/bin/kill` (`xtask/src/perf/launch.rs:34`).

Fresh retrieval (load-subgraph, schema 1.1, seeds SOAK plus requirement terms plus `xtask/` paths,
owns_coverage 12/17, observations []) gives these path neighbours:
- PERF (`xtask/src/perf/*`, `xtask/assets/perf-probe.swift`, `xtask/src/main.rs`);
- MENU (`xtask/src/perf/logstream.rs`, `xtask/src/perf/mod.rs`);
- PORTS (`xtask/src/perf/home.rs`, `xtask/src/perf/judge.rs`);
- RELEASE (`xtask/src/release.rs`, `xtask/src/main.rs`).

Only RELEASE gains a changed behavior (release-check, below). The others are read, not edited,
apart from the module-visibility change D4.

Architecture invariants this feature relies on: **ARCH-16** (no app timer: all timing is in
xtask), **ARCH-14** (dev profile, own temporary home; release data never touched), **ARCH-9** (the
fd and footprint reads stay in a Swift helper; xtask stays `unsafe`-free), **ARCH-11** (Navori
listens on nothing new; the load's TCP listeners are soak-owned processes on loopback),
**ARCH-5** (the soak dispatches no Destructive Action: `release_all` is `Risk::Write`,
`modules/awake/src/action.rs:78-84`), and **ARCH-4** (no measurement-only Action).

## Decisions

1. External soak in xtask, with nothing added to the app, the Core or the `navori` CLI (Lock: card Must keep and Decided).
2. Reuse of perf code (Lock: operator, 2026-11-05, "OK, not a change"; guards Weigh W2–W3; D4). The only edit under `xtask/src/perf/` is `mod` → `pub(crate) mod` for `home`, `launch` and `logstream` (`xtask/src/perf/mod.rs:1-7`; `probe` stays private, since soak has its own helper). Every private helper in `perf/mod.rs` stays private, `judge` included, so soak keeps its own warm-up, bounded command runner and median.
3. Own tree `/tmp/navori-soak-<uid>/navori-perf-<uid>/` (Weigh W2, agent-decided; D5). `PerfHome::prepare(base, uid)` names the leaf (`xtask/src/perf/home.rs:36-37`), the soak creates and guards the outer directory itself, and the Docker link that `fresh_run` makes is removed before launch (D5, D18).
4. The load runs as an ordered plan of timed steps computed up front, a pure function of the tier, and is executed by one driver loop. The schedule is unit-testable without a clock, and the run stays deterministic (SOAK-9.4).
5. The load's helper processes are:
   - `/bin/sleep 3` for command Leases;
   - `cargo xtask`'s own binary re-run with a hidden `soak-listen` subcommand, as the TCP listener (std `TcpListener` on `127.0.0.1:0`, closed after its hold);
   - `/usr/bin/sqlite3` fed `BEGIN IMMEDIATE;` and then `COMMIT;` on stdin, for the store hold. A spike on 2026-11-05 showed a second writer gets `database is locked` during the hold in WAL mode, while a reader still answers.

   Agent-decided: no new dependency, and nothing outside `/usr/bin`, `/bin` and xtask itself.
6. Hang detection is a bounded CLI round trip for the Core (2 s) and a bounded wait for `menuOpened` after SIGUSR1 for the menu (2 s), both agent-decided (D11). A load dispatch is bounded at 10 s, perf's `COMMAND_TIMEOUT` (`xtask/src/perf/mod.rs:32`). A dispatch timeout is counted and printed by SOAK-1.11. The Core probe, not the dispatch, decides whether a hang is recorded (agent-decided).
7. "Cannot drive the load" in SOAK-1.10 means one of these:
   - a helper process cannot be spawned;
   - the store holder does not print `held` within 3 s (Decision 12);
   - the user session is locked when the menu probe is due (Decision 11);
   - the `navori` helper exits 2 (usage error);
   - the helper exits 3 (app not running) while the app process is still alive.

   A dispatch answered with exit 1 (an Action error, for example `release_all` failing while the store is held, `modules/awake/src/actor.rs:518-536`) is load behaving under contention. It is counted and printed by SOAK-1.11, and it is not a tool failure (agent-decided; requirement text synced in SOAK-1.10 and SOAK-1.11).
8. Interrupts: xtask cannot install a signal handler (decision 2's context). The app and every load child are spawned in xtask's process group (`xtask/src/perf/launch.rs:22-24`), so a terminal Ctrl-C reaches all of them. Every app pid is recorded in the soak home, and the next soak's `sweep_stale` stops any survivor (SOAK-1.8, SOAK-7.2).
9. The open-fd count comes from `proc_pidinfo(PROC_PIDLISTFDS)`: a size call, then a call into a buffer with slack, then the count is bytes / `sizeof(proc_fdinfo)`. Same-uid fds are readable without root: `lsof -p <Finder pid>` returned 385 lines on 2026-11-05. That is weak evidence for libproc on a hardened-runtime app, so the first task proves `SoakProbe::read` against the real `Navori Dev.app` before anything builds on it.
10. Store holds are phased against the load (design review F2; Weigh W4, soak-interpret review r3). A hold is requested 1 s after a Lease tick, and opens once confirmed (Decision 12), and its commit closes it 4 s after that. Half a second into the hold a `RenewLease` is sent, so a Core writer is queued behind the lock (every Event is journaled, `crates/kernel/src/bus.rs` `publish`) when the Core probe arrives 1 s into the hold. That is the #21 situation. `status` itself reads no table (`crates/ffi/src/app.rs:46-62`), so without that queued writer the probe would pass falsely. Envelopes are spawned and not waited on: each dispatch is a child process tracked with a 10 s deadline and reaped by the loop, so no dispatch delays a probe. The loop blocks only on a probe (at most 2 s), a sample or a spawn. Steps run in order. A step that comes due while an earlier one is still running starts as soon as that one returns, and its actual start time is what the report records.
11. Menu frames need a drawable display (design review F3). `menuOpened` is logged from the panel's first presented frame (`apps/macos/Navori/StatusBar/StatusPanel.swift:128`), and `show` returns early without a button window (`StatusPanel.swift:97`). The soak therefore holds `/usr/bin/caffeinate -d -i -w <xtask pid>` for the whole run (SOAK-1.12), and starts a fresh `LogStream` just before the menu probe, so a two-hour-old stream cannot void the verdict (F7). A menu hang means "no mark in 2 s" (SOAK-3.4). Before the menu probe, the driver reads `ioreg -n Root -d1` and treats `"CGSSessionScreenIsLocked"=Yes` in `IOConsoleUsers` as a locked session. That is a SOAK-1.10 "cannot drive" failure with `the screen is locked; unlock it and rerun`, not a hang, so a locked screen cannot fail a good release as a hang (conductor final review M2, agent-decided). While unlocked, the key is absent (checked 2026-11-05). Its presence while locked is unverified until Task 7's locked-screen check.
12. The store hold must be confirmed, not assumed (Weigh W5, soak-interpret design review r1). The `sqlite3` shell has no busy timeout of its own, so a `BEGIN IMMEDIATE` that meets a Core write fails at once and the shell carries on (seen on this Mac). The holder is therefore fed `.bail on`, `.timeout 2000`, then `BEGIN IMMEDIATE;`, then `SELECT 'held';`. Without `.bail on`, a `BEGIN` that times out still lets `SELECT 'held'` print (plan review, reproduced 2026-11-05); with it, the shell exits 1 and prints no `held`. The hold opens when `held` is read on its stdout, within 3 s. The renew (+0.5 s), Core probe (+1 s) and commit (+4 s) are timed from that moment. A missing `held` is a SOAK-1.10 failure.
13. No ADR. Every decision is reversible within xtask and follows the perf gate's precedent.

## Architecture

Vocabulary for every section below: **module**, **interface**, **implementation**,
**seam** (public surface where behavior is observable and substitutable). Prefer
deep modules — an interface much simpler than what it hides.

```
cargo xtask soak [--long [--for D]] ─┐            cargo xtask release-check
cargo xtask soak observe [--every D] │                 │ preflight → ci → perf → soak::gate
                                     ▼                 ▼
                        soak::run(app, tier) ── plan(tier) ──► [Step{at, Event}…]
                           │  SoakHome (isolation)          judge(samples, hangs, exit) ──► Report
                           │  Launched + LogStream (perf)                     │ lines(), passed()
                           │  SoakProbe (Swift helper) ─── footprint/threads/fds
                           │  Cli (helper navori, HOME=run, bounded) ── status / dispatch
                           └─ children: /bin/sleep, xtask soak-listen, /usr/bin/sqlite3
```

### Load plan

Satisfies: SOAK-1.2, SOAK-1.3, SOAK-3.1, SOAK-3.5, SOAK-5.1, SOAK-5.3
Reuse: none — new code (rung 7). No scheduler exists in xtask; perf's phases are straight-line code (`xtask/src/perf/mod.rs:119-202`).
Respects: ARCH-16
Interface: `soak::plan::plan(tier: Tier) -> Plan`, where `Tier = Short | Long { load: Duration }` and `Plan { settle: Duration, load: Duration, steps: Vec<Step> }`. A `Step { at: Duration, event: Event }` is measured from launch, and steps are sorted by `at`. `Event = CommandLease | RenewLease | ReleaseAll | OpenListener { hold } | HoldStore { renew_after, probe_after, commit_after } | ProbeCore | Sample | ProbeMenu`. A `HoldStore` carries its own sub-timings (0.5 s, 1 s, 4 s), which the driver runs from the confirmed `held` moment (Decision 12), not from plan offsets.
Depth: if it vanished, callers would still need only "which event happens at which offset for this tier". The interface is one function returning a sorted list; the rates, offsets and the menu-last rule stay hidden.
Locality: new `xtask/src/soak/plan.rs`. No neighbour edit (leave).

Rates, all agent-decided (D7, D8, D9, D10, D11):

| Tier | settle | load | CommandLease and RenewLease | ReleaseAll | OpenListener (hold 45 s) | HoldStore (hold 4 s) | ProbeCore | Sample |
|---|---|---|---|---|---|---|---|---|
| Short | 60 s | 20 min | every 5 s | every 60 s | every 20 s | every 120 s | every 30 s, plus 1 s into each hold | every 10 s |
| Long | 60 s | 2 h, or `--for` | every 25 s | every 300 s | every 100 s | every 600 s | every 30 s, plus 1 s into each hold | every 60 s |

Load and sampling start after the settle. Phases, all agent-decided (Decision 10):
- Lease ticks fall on multiples of the Lease period.
- `HoldStore` is requested at k × hold period + 1 s.
- `ReleaseAll` falls at 30 s (Short) or 150 s (Long) into its period, so it never coincides with a hold's start.
- Every `HoldStore` carries a renew before its probe (`renew_after` < `probe_after` < `commit_after`). Regular ticks that come due inside a hold stay, and run as soon as the loop is free.
- Exactly one `ProbeMenu` sits after the last `Sample` (SOAK-3.5).

### Judge and report

Satisfies: SOAK-1.6, SOAK-1.7, SOAK-1.11, SOAK-1.13, SOAK-2.1, SOAK-2.2, SOAK-2.3, SOAK-2.4, SOAK-2.5, SOAK-3.6, SOAK-3.7, SOAK-5.4, SOAK-9.4
Reuse: rung 2 for the rule. It is the perf regression rule, "baseline median + max(10%, floor)", with the perf floors (`docs/standards/performance.md` Method, Floors), applied within one run. The arithmetic (median, least-squares slope) is rung 7: `perf::judge::median` (`xtask/src/perf/judge.rs:120`) stays private under Decision 2, and a median is three lines.
Interface: `soak::judge::judge(run: &RunFacts) -> Report`, where `RunFacts { settle: Duration, readings: Vec<Reading>, hangs: Vec<Hang>, exited_at: Option<Duration>, elapsed: Duration, load_errors: u32, load_timeouts: u32 }`. `Reading { at, footprint_bytes, threads, fds }`. `Hang { side: Core | Menu, at, waited, load_avg }`. `Report::lines(&self) -> Vec<String>` gives the exact output lines of SOAK-1.6, SOAK-1.7, SOAK-1.9, SOAK-1.11, SOAK-1.13 (elapsed before every verdict), SOAK-2.5 and SOAK-3.6. `Report::passed(&self) -> bool`. `Report::exit_ok(&self, tier: &Tier) -> bool` is `passed()` for Short and always true for Long (SOAK-5.4).
Depth: callers still need only "the facts of one run in, lines and a pass bit out". Thirds, floors, slope and line wording are hidden.
Locality: new `xtask/src/soak/judge.rs`. No neighbour edit (leave).

The floors are 1 MiB for footprint, 2 for threads and 4 for fds (agent-decided, D12). The trend
is the least-squares slope of each signal over the post-settle readings, times 3600 (per hour).
A run with fewer than 9 post-settle readings yields only the too-few-samples line and a fail.

### Soak home (isolation)

Satisfies: SOAK-7.1, SOAK-7.2, SOAK-7.3, SOAK-7.4, SOAK-7.6, SOAK-9.3
Reuse: rung 2: `PerfHome { root }` (pub field), `PerfHome::prepare`, `fresh_run`, `record_pid`, `sweep_stale` and `check_socket_len` (`xtask/src/perf/home.rs:17-148`), reached through Decision 2's visibility change.
Respects: ARCH-14
Security: TB-2, THR-4
Interface: `soak::home::SoakHome::prepare(exes: &[&Path]) -> Result<SoakHome>` and `prepare_in(base: &Path, uid: u32, exes: &[&Path])` for tests. `exes` are the app executables a previous soak may have left running: `build/soak/…/Navori` and `build/perf/…/Navori`. `SoakHome::template(&self) -> PathBuf`. `SoakHome::fresh_run(&self) -> Result<PathBuf>` returns a run directory with no Docker link, after the socket-length check. `SoakHome::record_pid(&self, pid)`. `drop_docker_link(run) -> Result<bool>`, the testable removal step.
Depth: it extends `perf::home::PerfHome` (rung 2) plus a small new guard for the outer directory (rung 7, because perf's `create_dir_mode` is private, `xtask/src/perf/home.rs:173`). If it vanished, callers would still need only "give me a fresh, private run directory with no Docker link".
Locality: new `xtask/src/soak/home.rs` wraps the perf home. `xtask/src/perf/home.rs` is unchanged (leave).

`prepare` runs in this order:
1. Create `/tmp/navori-soak-<uid>/` with mode 0700, or accept an existing directory only if it is a real directory owned by the current user with mode 0700 (SOAK-7.1). Otherwise refuse with a message naming the path.
2. Run `PerfHome { root: <base>/navori-perf-<uid> }.sweep_stale(exe)` once for each `exe` (design review F6: `sweep_stale` matches one executable path, `xtask/src/perf/home.rs:81-130`).
3. Run `PerfHome::prepare(<base>, uid)`.
4. Run `check_socket_len(root/"run-1")`.

`fresh_run` calls `PerfHome::fresh_run(1)`, then removes
`<run>/.docker/run/docker.sock` when it is a symlink, so Ports finds no Docker socket
(`modules/ports/src/lib.rs:58`).

SOAK-7.6 is the observable outcome of this module plus `Cli`'s `HOME`. It is verified by the
manual before/after listing in Seams.

### Soak probe helper

Satisfies: SOAK-1.5
Reuse: rung 7, mirroring rung 2. `xtask/assets/soak-probe.swift` copies the structure of `perf-probe.swift` (argument check, `fail`, `cError`, one JSON line) and adds `PROC_PIDLISTFDS`. `perf-probe.swift` is unchanged (requirements Out of Scope), and `Probe::compile` hard-codes its file name (`xtask/src/perf/probe.rs:58-71`), so soak compiles its own helper the same way (bare `swiftc -O <asset> -o <out_dir>/soak-probe`, as perf runs it).
Respects: ARCH-9
Interface: `soak::probe::SoakProbe::compile(root, out_dir) -> Result<SoakProbe>`. `SoakProbe::read(&self, pid, at) -> Result<Reading>`. The fd count is the second `PROC_PIDLISTFDS` call's returned bytes divided by `MemoryLayout<proc_fdinfo>.stride`; the size call over-estimates. `parse_reading(line) -> Result<Reading>`. The helper prints `{"footprint":…,"threads":…,"fds":…}`.
Depth: callers still need only "pid in, three numbers out". The libproc calls, the buffer-size retry and the JSON stay hidden.
Locality: new `xtask/assets/soak-probe.swift` and `xtask/src/soak/probe.rs` (leave perf's probe).

### Soak driver

Satisfies: SOAK-1.1, SOAK-1.4, SOAK-1.8, SOAK-1.9, SOAK-1.10, SOAK-1.12, SOAK-1.14, SOAK-3.2, SOAK-3.3, SOAK-3.4, SOAK-7.5, SOAK-9.2
Reuse:
- rung 2: `Launched::spawn` and `signal` (`xtask/src/perf/launch.rs:19-38`), `LogStream::start` and `wait_for` (`xtask/src/perf/logstream.rs:94,121`), and `super::bundle(root, Profile::Dev, Flavor::Release, out)` (`xtask/src/main.rs:313`).
- rung 3: std `process::Command` for the bounded runner and the children.
- rung 7: the warm-up and the bounded runner, because perf's (`xtask/src/perf/mod.rs:106,579-662`) are private.
Respects: ARCH-16, ARCH-4, ARCH-5, ARCH-11
Interface:
- `soak::run(root: &Path, tier: Tier) -> Result<bool>`, used by `cargo xtask soak`, builds into `build/soak/`.
- `soak::gate(root: &Path, app: &Path) -> Result<bool>`, used by release-check, takes a built `.app`.

Both print the report and return whether the exit should be 0. Internally, `Cli { helper, home }` exposes only `status(timeout) -> Answer` (awaited) and `dispatch(&Envelope, deadline) -> Pending` (spawned, not awaited; the driver passes 10 s). `Pending::poll(now) -> Result<Option<Answer>>` returns the answer once the child exits, or `TimedOut` after it has been killed at its 10 s deadline. Here `Envelope` is a closed enum `{ CommandLease(pid), RenewLease, ReleaseAll }`, a subset of SOAK-1.4's allow-list. `ports list` is not sent: it answers from the last scan (`modules/ports/src/action.rs:9-10`) and adds no load (design review F11) and `Answer = Ok | Failed(code) | TimedOut`.
Depth: callers still need only "tier or app in, printed report and an exit bit out". Launch, warm-up, the loop, children, probes and teardown are hidden. `Cli` has its own deletion test: its callers know only an awaited `status` and a spawned `dispatch`, each ending in a three-way answer. The `env HOME=` invocation, the helper path, the bounded wait, the kill and the exit-code mapping stay inside it.
Surface:
- `xtask/src/perf/logstream.rs` `LogStream` is a second consumer of the same `log stream` predicate. Disposition: frozen, built around. Soak reads the marks and does not change the predicate or the `Mark` enum.
- `apps/macos/Navori/Platform/PerfProbe.swift` SIGUSR1 handler and `menuOpened` line: frozen, built around. They are used as they are; no app edit.
Locality: new `xtask/src/soak/mod.rs` (driver), `xtask/src/soak/cli.rs` (`Cli`, `Pending`), `xtask/src/soak/hold.rs` (`StoreHold`, the confirmed `sqlite3` holder) and `xtask/src/soak/listen.rs` (listener child). No edit to perf (leave).

The flow:
1. Bundle, unless `gate` was given an app.
2. Create `build/soak/` if it is missing (design review F5), then compile the probe into it.
3. Spawn `/usr/bin/caffeinate -d -i -w <own pid>` (SOAK-1.12).
4. `SoakHome::prepare`.
5. Start a `LogStream`. Warm up the template: launch in perf mode, wait for `statusItemDrawn` (10 s), SIGTERM, wait for exit (15 s).
6. `fresh_run`, then print `docker=not linked` (SOAK-7.4).
7. Launch the app with `NAVORI_PERF=1` and `NAVORI_PERF_HOME=<run>`, record its pid, and wait for its `statusItemDrawn`. Then drop that stream.
8. Walk the plan, sleeping until each step's `at` (Decision 10 for late steps). On each step:
   - reap finished children, and poll pending dispatches into the load error and timeout counts (SOAK-1.11);
   - check whether the app has exited with `Launched::wait_exit(Duration::ZERO)` (design review F9; `Launched` has no `try_wait`). If it has, record `exited_at` and stop the walk.
9. Run the step:
   - `CommandLease`: spawn `/bin/sleep 3`, then dispatch `{"hold_command":{"pid":<it>}}`.
   - `RenewLease`: dispatch `{"hold_for":{"seconds":10}}`.
   - `ReleaseAll`: dispatch `"release_all"`.
   - `OpenListener`: spawn `<current_exe> soak-listen --secs 45`.
   - `HoldStore`: spawn `/usr/bin/sqlite3 <run>/Library/Application Support/Navori Dev/navori.db` and write `.bail on`, `.timeout 2000`, `BEGIN IMMEDIATE;` and `SELECT 'held';`. The path follows `crates/config/src/lib.rs:58-62,74`. Wait up to 3 s for `held` on its stdout (otherwise SOAK-1.10). Then, from that moment, dispatch the renew at +0.5 s (spawned), run the Core probe at +1 s, and at +4 s write `COMMIT;`, close stdin and reap. If the soak dies mid-hold, stdin EOF ends `sqlite3` and the transaction rolls back.
   - `ProbeCore`: `status(2 s)`; `TimedOut` records a `Hang { side: Core }` with the 1-minute load average from `sysctl -n vm.loadavg`.
   - `Sample`: `SoakProbe::read(pid)`.
   - `ProbeMenu`: check the session lock (Decision 11), then start a fresh `LogStream`, then `signal("USR1")` (`Launched::signal` adds the dash itself, `xtask/src/perf/launch.rs:70-71`; perf passes bare names), then `wait_for(pid, MenuOpened, 2 s)`; `None` records a `Hang { side: Menu }`.
10. Teardown, always, including on error. Every child, pending dispatches included, sits in a `Children` guard whose `Drop` kills and reaps it, so none outlives the run, because std children have no kill-on-drop (design review F8). `Launched`'s own `Drop` sends SIGKILL. The orderly path sends SIGTERM, waits 5 s, sends SIGKILL (SOAK-1.8), drops the guard, then judges and prints. On a fail verdict it prints `soak evidence <run dir> pid <pid>` and leaves the run directory, which lives under the soak's own `/tmp` tree and is replaced by the next soak (SOAK-1.14).

Each CLI call runs as `/usr/bin/env HOME=<run> <app>/Contents/Helpers/navori --profile dev …`, the same shape as perf (`xtask/src/perf/mod.rs:204-224`) (SOAK-7.5). The bounded runner kills the child at its timeout and reaps it. Progress lines (`soak +mm:ss …`) are flushed as they happen, so a 20-minute run is never silent.

### Observe

Satisfies: SOAK-6.1, SOAK-6.2, SOAK-6.3, SOAK-6.4
Reuse: rung 2 (`SoakProbe::read`); rung 3 (`/bin/ps -axo pid=,comm=` through std `Command`).
Interface: `soak::observe::observe(root, every: Option<Duration>) -> Result<()>` and `find_installed_pid(ps_output: &str) -> Option<u32>` (pure). It matches `comm` equal to `/Applications/Navori.app/Contents/MacOS/Navori` exactly, so `Navori Dev.app` never matches.
Depth: callers still need only "print the installed app's three numbers, once or every D". The process lookup and the probe stay hidden.
Locality: new `xtask/src/soak/observe.rs` (leave).

`observe` compiles the probe into `build/soak/` before observing starts. During observation it reads only: no signal, no Envelope, no file written (SOAK-6.4). Each line reads `observe <HH:MM:SS> pid <n> footprint <MiB> threads <n> fds <n>`.

### Commands

Satisfies: SOAK-5.2, SOAK-8.3
Reuse: rung 2, the clap `Cmd` enum and dispatch (`xtask/src/main.rs:27-76,139-163`).
Surface: the readers of `cargo xtask --help` and of the existing subcommands get additive changes only (a new `soak` subcommand and a hidden `soak-listen`). Disposition: replace. The help text is regenerated by clap, and no existing variant or argument changes. The `ReleaseCheck` doc comment at `xtask/src/main.rs:73` ("ci, perf") becomes "ci, perf, soak gate". Disposition: replace.
Interface:
- `Cmd::Soak { long: bool, for_: Option<Duration> (requires --long), command: Option<SoakCmd> }`, with `SoakCmd::Observe { every: Option<Duration> }`.
- Hidden `Cmd::SoakListen { secs: u64 }`.
- `parse_duration("90s" | "20m" | "2h")`, a pure function.

`Cmd::Soak` with no subcommand calls `soak::run`; `observe` calls `soak::observe`; a false result bails with `soak gate failed`.
Depth: n/a — extends the `Cmd` enum.
Locality: `xtask/src/main.rs` gains `mod soak;` and two variants (extend).

### Release-check step

Satisfies: SOAK-4.1, SOAK-4.2, SOAK-4.3, SOAK-4.4, SOAK-5.5, SOAK-9.1
Reuse: rung 2, `release::check` (`xtask/src/release.rs:156-184`).
Surface: these readers describe release-check as "ci + perf" and must learn about the soak step.

| Reader | Location | Change | Disposition |
|---|---|---|---|
| `release::check` | `xtask/src/release.rs:180-182` | one line after perf: `ensure!(super::soak::gate(root, &root.join("build/perf/Navori Dev.app"))?, "soak gate failed")`; the tag lines stay after it | replace |
| RELEASE-2.1, RELEASE-2.2 | `docs/specs/2026-11-04-public-release/requirements.md:65-66` | an amendment note in that file's own style ("*(Amended 2026-11-05 by SOAK-4.1/4.2: after perf, release-check runs the soak gate; the tag prints only when it passes too.)*"). The IDs and their wording are not renumbered | replace |
| RELEASE-7.1 | same file, line 130 | unaffected: it speaks of `perf` inside the check | leave |
| RELEASE-2.8 (guard on `ci` and `perf` run on their own) | same file, line 72 | unaffected: the only edit under `xtask/src/perf/` is visibility, and `perf` alone runs every check it ran before | leave |
| release steps | `docs/agents/project.md:141-143` | step 1 names the soak gate and its ~23-minute cost | replace |
| standards index | `docs/standards/INDEX.md:26-27` | "(ci + perf)" becomes "(ci + perf + soak gate)"; "Before every release" adds the soak gate | replace |
| comment | `crates/macos/tests/exits_latency.rs:42` | names release-check as a runner of `ci`, which stays true | leave |
| RELEASE story, design and tasks | `docs/specs/2026-11-04-public-release/requirements.md:63`, `design.md:24,104`, `tasks.md:17,190,197`, `acceptance.md:44` | a historical record of what that feature built; the RELEASE-2.1/2.2 amendment note carries the change | leave |
| ADR 0007 | `docs/adr/0007-self-signed-public-releases-from-ci.md:5` | a decision record of its date; "perf never runs on CI" stays true, and the soak does not run on CI either | leave |

Depth: n/a — extends `release::check`.
Locality: `xtask/src/release.rs` gains one line; docs edits as listed (extend).

The bundle path is the one `perf::run` builds (`xtask/src/perf/mod.rs:83-89`: `build/perf`, app
name `Navori Dev.app`). `gate` fails with a clear message if it is missing. The long tier has no
entry point from `release::check` (SOAK-5.5).

### Perf visibility

Satisfies: SOAK-8.1, SOAK-8.2
Reuse: rung 2, the existing perf modules.
Surface: `xtask/src/perf/mod.rs:1-7`, the module declarations for `home`, `launch` and `logstream`. They become readable by `crate::soak`. Disposition: replace. The edit is visibility only, with no body edit, so perf's own call sites, tests and output are unchanged (Lock: operator, 2026-11-05; guards Weigh W2–W3).
Depth: n/a — extends `xtask/src/perf`.
Locality: three words in `xtask/src/perf/mod.rs` (extend). Any body edit under `perf/` is out of this design and goes to the operator (Ask me).

### Documentation

Satisfies: none (infrastructure: it records the method next to perf's).
Reuse: rung 2, `docs/standards/performance.md`.
Interface: a new `## Soak` section between `## Method` and `## Design rules` in `docs/standards/performance.md`. It states the tiers, rates, threshold, floors and leak rule, each labelled `agent-decided` with its source, and the #21 consequence.
Depth: n/a — extends `docs/standards/performance.md`.
Locality: docs only (extend).

## Seams for testing

xtask's soak logic is perf logic in the sense of `docs/standards/testing.md` (Exempt): its pure parts are unit-tested in place (`#[cfg(test)]`, as `xtask/src/perf/*` does), and the rest is verified by running it.

| Seam | Kind | Covers |
|---|---|---|
| `soak::plan::plan(Tier)` | unit | SOAK-1.2, SOAK-1.3, SOAK-3.1, SOAK-3.5, SOAK-5.1, SOAK-5.3 |
| `soak::judge::judge` → `Report::lines / passed / exit_ok` | unit (synthetic run facts) | SOAK-1.6, SOAK-1.7, SOAK-1.13, SOAK-1.9 (lines), SOAK-1.11, SOAK-2.1, SOAK-2.2, SOAK-2.3, SOAK-2.5, SOAK-3.6, SOAK-3.7, SOAK-5.4 |
| `SoakProbe::read` on a real fixture child that opens one fd per tick (`perl`, as `xtask/src/perf/mod.rs:832` uses a real child), then `judge`; plus one read of a running `Navori Dev.app` (Decision 9) | integration | SOAK-1.5, SOAK-2.4 |
| `SoakHome::prepare_in` / `fresh_run` in a tempdir (`/tmp`) | unit | SOAK-7.1, SOAK-7.2, SOAK-7.3, SOAK-7.4, SOAK-9.3 |
| bounded runner (`Cli::status` and `Pending::poll` against a real `sleep`, plus a fake helper script that exits 1, 2 or 3) and the store holder against a temp-dir db already locked by another `sqlite3` (no `held` within 3 s is a failure; `held` once the lock frees) | unit | SOAK-3.2, SOAK-1.10, SOAK-1.8 (kill on timeout) |
| `find_installed_pid(ps text)` and `parse_duration` | unit | SOAK-6.1, SOAK-6.3, SOAK-5.2 |
| `cargo xtask soak` on this Mac, with `lsof` on the soak app, `find -newer` on the dev paths, hook-file hashes before and after, and `pmset -g assertions` during the run | e2e (manual, printed elapsed time) | SOAK-1.1, SOAK-1.4, SOAK-1.8, SOAK-1.9, SOAK-1.12, SOAK-3.3, SOAK-3.4, SOAK-7.5, SOAK-7.6, SOAK-9.1, SOAK-9.2, SOAK-9.4 (3 runs) |
| `cargo xtask soak observe [--every 10s]` against the installed app | e2e (manual) | SOAK-6.2, SOAK-6.4 |
| `cargo xtask release-check` on a clean tree, plus a `--help` listing | e2e (manual) | SOAK-4.1, SOAK-4.2, SOAK-4.3, SOAK-4.4, SOAK-5.5, SOAK-8.3 |
| `cargo xtask perf` before and after, plus `cargo test -p xtask` | e2e and unit (existing) | SOAK-8.1, SOAK-8.2 |

New seams: the soak module's own public functions. No existing seam changes.

## Coverage check

Every requirement ID in requirements.md appears in exactly one `Satisfies:` line:
- SOAK-1.1 to 1.14;
- SOAK-2.1 to 2.5;
- SOAK-3.1 to 3.7;
- SOAK-4.1 to 4.4;
- SOAK-5.1 to 5.5;
- SOAK-6.1 to 6.4;
- SOAK-7.1 to 7.6;
- SOAK-8.1 to 8.3;
- SOAK-9.1 to 9.4.

`Documentation` satisfies none and is labelled infrastructure. No ID is deliberately unmapped.

Known consequence, carried from requirements Out of Scope: while issue #21 is open, the short
gate's store holds are expected to produce `soak hang core` and fail release-check. A passing gate
on `main` is first demonstrable once #21 is fixed.
