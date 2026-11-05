# Tasks: Soak and hang check

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: SOAK
Status: Approved
Approval: the operator's /shape --go, before reading the spec (operator command: `/shape #27 --auto --go --video --work=opus --interpret=opus --build=sonnet`, relayed by the conductor on 2026-11-05)
Date: 2026-11-05
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

**Goal:** `cargo xtask soak` runs an isolated Navori Dev under a fixed load and reports footprint, thread and open-file creep and Core or menu hangs. A 20-minute tier gates `cargo xtask release-check`, a long tier only reports, and `soak observe` reads the installed app.

**Architecture:** A new `xtask/src/soak/` module watches the app from outside. It has a pure load plan, a pure judge, a soak home that wraps perf's `PerfHome`, a Swift helper for footprint, threads and fds, and a CLI runner and store holder. A driver walks the plan against a perf-mode bundle. The app, the Core and the `navori` CLI are not edited. The only edit under `xtask/src/perf/` is module visibility.

**Tech Stack:** Rust 2024 (xtask: `anyhow`, `clap`, `serde_json`, dev-dep `tempfile`; no new dependency), Swift (`swiftc -O`, libproc), `/usr/bin/sqlite3`, `/usr/bin/caffeinate`, `/bin/ps`, `/bin/sleep`, `/usr/bin/log`. Tests: `cargo test -p xtask` (inline `#[cfg(test)]` modules, as `xtask/src/perf/*` does).

## Global Constraints

Source: `docs/standards/` (SSOT) and `docs/architecture/INDEX.md`. Hashes are sha256 prefixes at plan time:
- `docs/standards/INDEX.md` `ecbda86ee6332064`
- `docs/standards/testing.md` `3f8c4bb0ff37b706`
- `docs/standards/errors-logging.md` `8db5edd4e78b157f`
- `docs/standards/performance.md` `cc6e08b051f68c55`
- `docs/architecture/INDEX.md` `621c667c346d8740`

Every task inherits these:
- **Gate before every commit:** `cargo xtask ci`. Task gate: `cargo test -p xtask` (`docs/agents/project.md` verify commands).
- **Lints:** `unsafe_code = "forbid"` in xtask (ARCH-9). `unwrap`/`expect` only in tests. Derive `Debug` on every type. `clippy -D warnings`, so no dead code.
- **Errors:** `anyhow` with messages that name the path or value and say how to fix it. Never swallow an error (`errors-logging.md`).
- **Tests:** test-first at the seams in `design.md` "Seams for testing". Real collaborators: real child processes and temp dirs under `/tmp`, as `xtask/src/perf/launch.rs` tests do. Test names are behavior sentences. No requirement IDs in test or production source. Expected values are literals.
- **Invariants:**
  - ARCH-16: nothing in the app; all timing lives in xtask.
  - ARCH-14: dev profile, temporary home only.
  - ARCH-4: no measurement-only Action.
  - ARCH-5: no Destructive Action dispatched.
  - ARCH-11: the soak's listeners bind `127.0.0.1` only.
- **Do not edit:**
  - any body under `xtask/src/perf/` (only the 3 `mod` lines in Task 1);
  - `xtask/assets/perf-probe.swift`;
  - anything under `apps/`, `crates/` or `modules/`.

  An edit there is an operator Ask-me item.
- **Commits:** Conventional Commits, English, one logical change each, every commit builds.
- **Team:** Solo (`docs/agents/project.md` `## Team`). No assignees.

## File Structure

| File | New / changed | Responsibility |
|---|---|---|
| `xtask/src/perf/mod.rs` | changed (lines 1-7 only) | `mod home/launch/logstream` → `pub(crate) mod` |
| `xtask/src/soak/mod.rs` | new | `run(root, tier)`, `gate(root, app)`, the driver loop, the `Children` guard, warm-up, teardown, progress output |
| `xtask/src/soak/plan.rs` | new | `Tier`, `Event`, `Step`, `Plan`, `plan(tier)` (pure) |
| `xtask/src/soak/judge.rs` | new | `Reading`, `Hang`, `Side`, `RunFacts`, `Report`, `judge(&RunFacts)` (pure) |
| `xtask/src/soak/probe.rs` | new | `SoakProbe::compile/read`, `parse_reading` |
| `xtask/assets/soak-probe.swift` | new | prints `{"footprint":…,"threads":…,"fds":…}` for one pid |
| `xtask/src/soak/home.rs` | new | `SoakHome::prepare/prepare_in/template/fresh_run/record_pid` |
| `xtask/src/soak/cli.rs` | new | `Cli { helper, home }`, `Envelope`, `Answer`, `Pending`, `Cli::status`, `Cli::dispatch` |
| `xtask/src/soak/hold.rs` | new | `StoreHold::open(db, confirm_within)`, `StoreHold::commit()` (the `sqlite3` holder) |
| `xtask/src/soak/listen.rs` | new | `listen(secs)`: binds `127.0.0.1:0`, holds, exits (body of the hidden `soak-listen` command) |
| `xtask/src/soak/observe.rs` | new | `observe(root, every)`, `find_installed_pid(ps_output)` |
| `xtask/src/main.rs` | changed | `mod soak;`, `Cmd::Soak`, `SoakCmd::Observe`, hidden `Cmd::SoakListen`, `parse_duration`, the ReleaseCheck doc comment |
| `xtask/src/release.rs` | changed | one `soak::gate` line after the perf gate in `check` (around line 180) |
| `docs/standards/performance.md` | changed | new `## Soak` section between `## Method` and `## Design rules` |
| `docs/standards/INDEX.md` | changed | Gates: release-check is ci + perf + soak gate |
| `docs/agents/project.md` | changed | release step 1 names the soak gate and its cost |
| `docs/specs/2026-11-04-public-release/requirements.md` | changed | amendment note on RELEASE-2.1 and RELEASE-2.2 |

Retrieval (load-subgraph, schema 1.1): blast_radius on these paths is advisory, not a gate.
- `perf/mod.rs` → MENU, PERF, PORTS.
- `main.rs` → MENU, PERF, PROCS, RELEASE.
- `release.rs` → RELEASE.
- `performance.md` → KCONTRACT, MENU, PERF, PORTS.
- `docs/standards/INDEX.md` → MENU.
- `docs/agents/project.md` → PERF, RELEASE.
- the new soak paths → none.

cluster(SOAK) = [SOAK], owns_coverage 12/17. Those features' behavior is guarded by SOAK-8.1 to 8.3, SOAK-4.3 and SOAK-4.4.

---

### Task 1: Open perf's home, launch and log-stream modules to the crate

**Files:**
- Modify: `xtask/src/perf/mod.rs` (lines 1-7: `mod home;`, `mod launch;`, `mod logstream;` become `pub(crate) mod …`; nothing else)
- Test: `xtask/src/perf/mod.rs` (existing tests, unchanged)

**Reuse:** rung 2, the existing perf modules.

**Interfaces:**
- Consumes: none
- Produces: `crate::perf::home::{PerfHome, current_uid, check_socket_len}`, `crate::perf::launch::Launched`, `crate::perf::logstream::{LogStream, Mark}` (now reachable from `crate::soak`)

**Depends-on:** none

- [x] **Step 1: Record perf's current output shape.** Run `cargo test -p xtask perf::` and save the list of test names it prints: `cargo test -p xtask perf:: -- --list > /tmp/soak-perf-tests-before.txt`. Expect: pass.
- [x] **Step 2: Change the three declarations** to `pub(crate) mod`. Leave `baseline`, `docker`, `judge` and `probe` private.
- [x] **Step 3: Run** `cargo test -p xtask perf:: -- --list | diff /tmp/soak-perf-tests-before.txt -`. Expect: no diff. Then run `cargo test -p xtask` and `cargo clippy -p xtask --all-targets -- -D warnings`. Expect: pass, with no new warning.
- [x] **Step 4: Commit** `git commit -m "refactor(xtask): let the crate reach perf's home, launch and log-stream modules"`

_Requirements: SOAK-8.1, SOAK-8.2_

---

### Task 2: Load plan for the short and long tiers

**Files:**
- Create: `xtask/src/soak/plan.rs`
- Create: `xtask/src/soak/mod.rs` (only `pub(crate) mod plan;` for now)
- Modify: `xtask/src/main.rs` (add `#[cfg_attr(not(test), allow(dead_code))] mod soak;` after `mod release;` at line 15; Task 9 removes the attribute once `release::check` calls `soak::gate`)
- Test: `xtask/src/soak/plan.rs` (`#[cfg(test)] mod tests`)

**Reuse:** none — new code (rung 7). No scheduler exists in xtask; perf's phases are straight-line code (`xtask/src/perf/mod.rs:119-202`).

**Interfaces:**
- Consumes: none
- Produces:
  - `pub(crate) enum Tier { Short, Long { load: Duration } }`
  - `pub(crate) enum Event { CommandLease, RenewLease, ReleaseAll, OpenListener { hold: Duration }, HoldStore { renew_after: Duration, probe_after: Duration, commit_after: Duration }, ProbeCore, Sample, ProbeMenu }`
  - `pub(crate) struct Step { pub at: Duration, pub event: Event }`
  - `pub(crate) struct Plan { pub settle: Duration, pub load: Duration, pub steps: Vec<Step> }`
  - `pub(crate) fn plan(tier: &Tier) -> Plan` (steps sorted by `at`, measured from launch)

  All derive `Debug, Clone, PartialEq`.

**Depends-on:** none

- [x] **Step 1: Write the failing tests.** Each is a behavior sentence over `plan(&Tier::Short)` and `plan(&Tier::Long { load: 2h })`:
  - `the_short_tier_settles_for_a_minute_then_loads_for_twenty`: settle is 60 s, load is 1200 s, and every step's `at` is ≥ 60 s.
  - `the_short_tier_takes_a_command_lease_every_five_seconds`: the `CommandLease` count is 240, and consecutive gaps are 5 s.
  - `the_short_tier_releases_all_every_minute_at_half_past`: the `ReleaseAll` `at` values, minus 60 s, are 30, 90, 150 and so on.
  - `the_short_tier_opens_a_listener_every_twenty_seconds_for_forty_five`.
  - `the_short_tier_holds_the_store_every_two_minutes_one_second_after_a_lease_tick`: `HoldStore` `at` = 60 + 120k + 1, with sub-timings 0.5 s, 1 s and 4 s.
  - `every_store_hold_renews_before_its_probe`: `renew_after < probe_after < commit_after`.
  - `the_core_is_probed_every_thirty_seconds`.
  - `the_short_tier_samples_every_ten_seconds`.
  - `the_menu_is_probed_once_and_only_after_the_last_sample`: exactly one `ProbeMenu`, whose `at` is greater than every `Sample` `at`.
  - `the_long_tier_runs_the_same_steps_at_its_own_rates`: lease 25 s, release_all 300 s (offset 150), listener 100 s, store 600 s, sample 60 s, core probe 30 s, and load equal to the given duration.

  Run: `cargo test -p xtask soak::plan` — expect: compile error, no `plan` yet.
- [x] **Step 2: Implement** `plan()`. One rate table per tier (the design's "Load plan" table). Generate each event series from `settle + phase + k × period` while it stays below `settle + load`. Append one `ProbeMenu` at `settle + load`. Sort stably by `at`.

  Run: `cargo test -p xtask soak::plan` — expect: pass.
- [x] **Step 3: Run** `cargo clippy -p xtask --all-targets -- -D warnings` — expect: pass. The module-level attribute on `mod soak;` covers items unused until Tasks 7 to 9.
- [x] **Step 4: Commit** `git commit -m "feat(xtask): plan the soak load for the short and long tiers"`

_Requirements: SOAK-1.2, SOAK-1.3, SOAK-3.1, SOAK-3.5, SOAK-5.1, SOAK-5.3_

---

### Task 3: Judge a run: leak rule, hangs, verdict lines

**Files:**
- Create: `xtask/src/soak/judge.rs`
- Modify: `xtask/src/soak/mod.rs` (add `pub(crate) mod judge;`)
- Test: `xtask/src/soak/judge.rs` (`#[cfg(test)] mod tests`)

**Reuse:** rung 2 for the rule. It is the perf regression rule, "baseline median + max(10%, floor)", with the perf floors (`docs/standards/performance.md` Method, Floors), applied within one run. The arithmetic (median, least-squares slope) is rung 7: `perf::judge::median` (`xtask/src/perf/judge.rs:120`) stays private under Decision 2.

**Interfaces:**
- Consumes: none
- Produces:
  - `pub(crate) struct Reading { pub at: Duration, pub footprint_bytes: u64, pub threads: u32, pub fds: u32 }`
  - `pub(crate) enum Side { Core, Menu }`
  - `pub(crate) struct Hang { pub side: Side, pub at: Duration, pub waited: Duration, pub load_avg: f64 }`
  - `pub(crate) struct RunFacts { pub settle: Duration, pub readings: Vec<Reading>, pub hangs: Vec<Hang>, pub exited_at: Option<Duration>, pub elapsed: Duration, pub load_errors: u32, pub load_timeouts: u32 }`
  - `pub(crate) struct Report` with `lines(&self) -> Vec<String>`, `passed(&self) -> bool` and `exit_ok(&self, tier: &Tier) -> bool`
  - `pub(crate) fn judge(run: &RunFacts) -> Report`

**Depends-on:** Task 2 (`Tier`)

- [x] **Step 1: Write the failing tests.** Readings are taken every 10 s after a 60 s settle, 12 per series, so each third holds 4. All expected values are literals:
  - `flat_signals_pass_and_print_start_end_and_trend`: footprint 30 MiB constant, threads 9, fds 40. Lines include `soak footprint start 30.0 end 30.0 trend +0.0/h ok`, `soak threads start 9 end 9 trend +0/h ok`, `soak fds start 40 end 40 trend +0/h ok` and `soak verdict pass`, and `passed()` is true.
  - `one_more_fd_every_reading_is_a_leak_named_with_its_slope`: fds = 40, 41, …, 51. The first-third median is 41.5 and the last-third median 49.5. The rise of 8 exceeds max(4.15, 4), so the line is `soak fds start 41.5 end 49.5 trend +360/h LEAK`, then `soak verdict fail`.
  - `a_rise_within_the_floor_is_not_a_leak`: threads 9, 9, 10, 10, 10, 10, 10, 10, 11, 11, 11, 11. The rise of 1.5 is under the floor of 2, so `ok`.
  - `a_rise_that_falls_back_in_the_middle_is_not_a_leak`: fds 40 ×4, 30 ×4, 50 ×4. The middle median is below the first, so `ok`.
  - `ten_percent_of_a_large_footprint_outweighs_its_floor`: thirds of 40, 41.5 and 43 MiB are `ok` (a rise of 3 is under max(4, 1) MiB); thirds of 40, 42 and 45 MiB are `LEAK`.
  - `fewer_than_nine_readings_fail_without_judging`: 8 readings give `soak too few samples (8)`, then `soak verdict fail`, and no signal line.
  - `a_core_hang_fails_and_says_when_and_how_long`: a hang at 754 s, waited 2 s, load 3.2, gives `soak hang core at +12:34 waited 2000 load 3.20`, then `soak verdict fail`.
  - `an_app_that_exited_fails`: `exited_at` 300 s gives `soak app exited at +05:00`, then `soak verdict fail`.
  - `load_errors_and_timeouts_are_printed_but_do_not_fail`: `soak load errors 3 timeouts 1`, and `passed()` stays true.
  - `a_pass_prints_the_elapsed_time`: elapsed 1265 s gives the line `soak elapsed 21:05` right before `soak verdict pass`.
  - `a_fail_still_prints_the_elapsed_time`: a run with a core hang and elapsed 1265 s prints `soak elapsed 21:05` right before `soak verdict fail`.
  - `the_long_tier_exits_ok_even_on_a_leak`: `exit_ok(&Tier::Long{..})` is true when `passed()` is false. `exit_ok(&Tier::Short)` equals `passed()`.

  Run: `cargo test -p xtask soak::judge` — expect: compile error.
- [x] **Step 2: Implement** `judge`:
  - median of a slice;
  - split the post-settle readings into thirds; with a remainder, the extra readings go to the middle third;
  - the leak rule with floors 1 MiB, 2 threads and 4 fds;
  - the least-squares slope × 3600, rounded to whole units (footprint to 0.1 MiB);
  - the line formats above, with times as `mm:ss`. Footprint is printed in MiB with one decimal. Threads and fds start and end are printed as integers when whole and with one decimal otherwise (`41.5`). Trends are whole numbers per hour with a sign.

  Run: `cargo test -p xtask soak::judge` — expect: pass.
- [x] **Step 3: Commit** `git commit -m "feat(xtask): judge a soak run for creeping signals and hangs"`

_Requirements: SOAK-1.6, SOAK-1.7, SOAK-1.11, SOAK-1.13, SOAK-2.1, SOAK-2.2, SOAK-2.3, SOAK-2.5, SOAK-3.6, SOAK-3.7, SOAK-5.4_

---

### Task 4: Soak probe helper reads footprint, threads and open files

**Files:**
- Create: `xtask/assets/soak-probe.swift`
- Create: `xtask/src/soak/probe.rs`
- Modify: `xtask/src/soak/mod.rs` (add `pub(crate) mod probe;`)
- Test: `xtask/src/soak/probe.rs` (`#[cfg(test)] mod tests`)

**Reuse:** rung 7, mirroring rung 2. `xtask/assets/soak-probe.swift` copies the structure of `perf-probe.swift` (argument check, `fail`, `cError`, one JSON line) and adds `PROC_PIDLISTFDS`. `perf-probe.swift` is unchanged, and `Probe::compile` hard-codes its file name (`xtask/src/perf/probe.rs:58-71`), so soak compiles its own helper the same way (`swiftc -O <asset> -o <out_dir>/soak-probe`).

**Interfaces:**
- Consumes: `judge::{Reading, RunFacts, judge}` (Task 3)
- Produces:
  - `pub(crate) struct SoakProbe { pub bin: PathBuf }`
  - `SoakProbe::compile(root: &Path, out_dir: &Path) -> Result<SoakProbe>`, which creates `out_dir` if it is missing
  - `SoakProbe::read(&self, pid: u32, at: Duration) -> Result<Reading>`
  - `pub(crate) fn parse_reading(line: &str, at: Duration) -> Result<Reading>`

**Depends-on:** Task 3

- [ ] **Step 1: Write the failing tests:**
  - `a_probe_line_parses_into_a_reading`: `{"footprint":31457280,"threads":9,"fds":40}` gives footprint 31457280, threads 9 and fds 40.
  - `a_probe_line_missing_fds_is_an_error_naming_the_field`.
  - (integration) `a_process_that_opens_a_file_every_tick_is_judged_a_leak`:
    1. compile the probe into a tempdir;
    2. spawn `/usr/bin/perl -e 'my @f; for (1..40) { open(my $h, "<", "/dev/null"); push @f, $h; sleep 1 } sleep 5'`;
    3. read the child 12 times, one per second, after a 2 s settle;
    4. assert fds strictly increase overall, then build a `RunFacts` (settle 0) and assert `judge` prints `soak fds … LEAK`.

  Run: `cargo test -p xtask soak::probe` — expect: compile error.
- [ ] **Step 2: Write `soak-probe.swift`:**
  - `proc_pid_rusage(RUSAGE_INFO_V4)` for `ri_phys_footprint`;
  - `proc_pidinfo(PROC_PIDTASKINFO)` for `pti_threadnum`;
  - `proc_pidinfo(pid, PROC_PIDLISTFDS, 0, nil, 0)` for the size, then a buffer of size + 32 × `MemoryLayout<proc_fdinfo>.stride`; the count is the **second** call's returned bytes / stride (the size call over-estimates);
  - print one JSON line.

  Write `probe.rs`, which compiles the helper with bare `swiftc -O`, the same way perf does (`xtask/src/perf/probe.rs:58-71`).

  Run: `cargo test -p xtask soak::probe` — expect: pass.
- [ ] **Step 3: Prove it on the real app (design Decision 9).** Call the compiled helper, read-only, on the running installed app's pid (`pgrep -x -f /Applications/Navori.app/Contents/MacOS/Navori`), a hardened-runtime app. Do not start `cargo xtask run`, which would write the real `Navori Dev` data that Task 7 checks the soak leaves alone. The plan review already did this once (17 fds, no EPERM; `.skills/_pending-soak/auto/plan-review.md`). Expect: one JSON line with fds > 0. If it fails with EPERM, stop. That is a design blocker, and it goes back to the operator rather than being worked around.
- [ ] **Step 4: Commit** `git commit -m "feat(xtask): add a soak probe for footprint, threads and open files"`

_Requirements: SOAK-1.5, SOAK-2.4_

---

### Task 5: Soak home stays private and leaves the real home alone

**Files:**
- Create: `xtask/src/soak/home.rs`
- Modify: `xtask/src/soak/mod.rs` (add `pub(crate) mod home;`)
- Test: `xtask/src/soak/home.rs` (`#[cfg(test)] mod tests`, tempdirs under `/tmp`)

**Reuse:** rung 2: `PerfHome { root }` (pub field), `PerfHome::prepare`, `fresh_run`, `record_pid`, `sweep_stale` and `check_socket_len` (`xtask/src/perf/home.rs:17-148`), reached through Decision 2's visibility change.

**Interfaces:**
- Consumes: `crate::perf::home::{PerfHome, current_uid, check_socket_len}` (Task 1)
- Produces:
  - `pub(crate) struct SoakHome { perf: PerfHome }`
  - `SoakHome::prepare(exes: &[&Path]) -> Result<SoakHome>`, with base `/tmp/navori-soak-<uid>`
  - `SoakHome::prepare_in(base: &Path, uid: u32, exes: &[&Path]) -> Result<SoakHome>`
  - `SoakHome::template(&self) -> PathBuf`
  - `SoakHome::fresh_run(&self) -> Result<PathBuf>`, which returns `<root>/run-1` with no `.docker/run/docker.sock`
  - `pub(crate) fn drop_docker_link(run: &Path) -> Result<bool>`, which removes `<run>/.docker/run/docker.sock` when it is a symlink and returns whether it did
  - `SoakHome::record_pid(&self, pid: u32) -> Result<()>`

**Depends-on:** Task 1, Task 2

- [ ] **Step 1: Write the failing tests:**
  - `a_new_soak_base_is_created_private_to_the_user`: mode 0700 and owner = uid.
  - `a_soak_base_that_is_a_symlink_is_refused`: the error names the path.
  - `a_soak_base_with_a_wider_mode_is_refused`: 0755.
  - `an_existing_private_soak_base_is_reused`.
  - `the_run_home_is_nested_under_the_soak_base`: `<base>/navori-perf-<uid>/run-1`.
  - `a_docker_link_in_a_run_directory_is_removed`: create `<run>/.docker/run/docker.sock` as a symlink to a temp file, call `drop_docker_link`, and assert `true` and that the link is gone while its target remains.
  - `a_fresh_run_directory_has_no_docker_link`: `fresh_run()` then `symlink_metadata(<run>/.docker/run/docker.sock)` is NotFound.
  - `a_stale_app_recorded_by_an_earlier_soak_is_stopped_before_the_home_is_replaced`: write a `pids` file with a running `/bin/sleep 60` and pass its exe path, then assert the process is gone.
  - `a_base_that_would_make_the_socket_path_too_long_is_refused`.

  Run: `cargo test -p xtask soak::home` — expect: compile error.
- [ ] **Step 2: Implement:**
  - the outer-directory guard: `DirBuilder` mode 0700, then `symlink_metadata` checks for a real directory, owner and mode;
  - `sweep_stale` for each exe on `PerfHome { root: base.join(format!("navori-perf-{uid}")) }`;
  - `PerfHome::prepare(base, uid)`, then `check_socket_len(root.join("run-1"))`;
  - `fresh_run`: `PerfHome::fresh_run(1)`, then `remove_file` on the docker link when `symlink_metadata` says it is a symlink.

  Run: `cargo test -p xtask soak::home` — expect: pass.
- [ ] **Step 3: Commit** `git commit -m "feat(xtask): give the soak its own private home with no Docker link"`

_Requirements: SOAK-7.1, SOAK-7.2, SOAK-7.3, SOAK-7.4, SOAK-9.3_

---

### Task 6: Bounded CLI calls and a confirmed store hold

**Files:**
- Create: `xtask/src/soak/cli.rs`
- Create: `xtask/src/soak/hold.rs`
- Modify: `xtask/src/soak/mod.rs` (add `pub(crate) mod cli; pub(crate) mod hold;`)
- Test: `xtask/src/soak/cli.rs` and `xtask/src/soak/hold.rs` (`#[cfg(test)] mod tests`)

**Reuse:** rung 3, std `process::Command` for the bounded runner and the children. Rung 7 for the bounded runner, because perf's (`xtask/src/perf/mod.rs:106,579-662`) are private.

**Interfaces:**
- Consumes: none
- Produces:
  - `pub(crate) struct Cli { helper: PathBuf, home: PathBuf }` and `Cli::new(app: &Path, home: &Path)`. The helper is `<app>/Contents/Helpers/navori`, run as `/usr/bin/env HOME=<home> <helper> --profile dev …`.
  - `pub(crate) enum Envelope { CommandLease(u32), RenewLease, ReleaseAll }`. The JSON is `awake` `{"hold_command":{"pid":N}}`, `{"hold_for":{"seconds":10}}` and `"release_all"`.
  - `pub(crate) enum Answer { Ok, Failed(i32), TimedOut }`
  - `Cli::status(&self, timeout: Duration) -> Result<Answer>`, which is awaited and kills the child at the timeout
  - `Cli::dispatch(&self, e: &Envelope, deadline: Duration) -> Result<Pending>`, which is spawned and not awaited (the driver passes 10 s)
  - `pub(crate) struct Pending` and `Pending::poll(&mut self, now: Instant) -> Result<Option<Answer>>`, which returns `TimedOut` after killing the child at its deadline
  - `pub(crate) struct StoreHold`:
    - `StoreHold::open(db: &Path, confirm_within: Duration) -> Result<StoreHold>` spawns `/usr/bin/sqlite3 <db>`, writes `.bail on`, `.timeout 2000`, `BEGIN IMMEDIATE;` and `SELECT 'held';`, and fails if `held` is not read within `confirm_within` (without `.bail on`, a `BEGIN` that times out still prints `held`);
    - `StoreHold::commit(self) -> Result<()>` writes `COMMIT;`, closes stdin and reaps.

**Depends-on:** Task 2

- [ ] **Step 1: Write the failing tests.** Use a fake helper: a tempdir `<app>/Contents/Helpers/navori` shell script that reads `$HOME/mode` and then does `exec sleep 5`, `exit 1`, `exit 2`, `exit 3` or `exit 0`.
  - `a_status_that_does_not_answer_in_two_seconds_times_out_and_the_child_is_killed`.
  - `a_status_that_exits_zero_is_ok`.
  - `a_dispatch_returns_at_once_and_is_answered_later`.
  - `a_dispatch_still_running_at_its_deadline_is_killed_and_timed_out`: `dispatch(&Envelope::RenewLease, Duration::from_millis(300))` with mode `sleep`, then poll after 500 ms → `TimedOut`, and the child is gone.
  - `exit_codes_one_two_and_three_are_reported_as_failed_with_the_code`.
  - `a_store_hold_is_confirmed_and_blocks_another_writer`: in a temp WAL db, after `open`, a second `sqlite3 -cmd '.timeout 0'` insert fails with `database is locked`; after `commit`, it succeeds.
  - `a_store_hold_that_cannot_take_the_lock_in_time_fails`: another holder keeps the lock, so `open` with `confirm_within` 3 s returns an error naming the db.

  Run: `cargo test -p xtask soak::` — expect: compile error.
- [ ] **Step 2: Implement** the bounded wait (a `try_wait` loop, then `kill` and `wait`), the spawned `Pending`, and `StoreHold`, which reads stdout on a thread that sends lines to a channel and uses `recv_timeout` for `held`.

  Run: `cargo test -p xtask soak::` — expect: pass.
- [ ] **Step 3: Commit** `git commit -m "feat(xtask): add bounded CLI calls and a confirmed store hold for the soak"`

_Requirements: SOAK-1.10, SOAK-3.2_

---

### Task 7: `cargo xtask soak` drives the load end to end

**Files:**
- Modify: `xtask/src/soak/mod.rs` (`run`, `gate`, the driver loop, `Children`, warm-up, teardown, progress lines)
- Create: `xtask/src/soak/listen.rs`
- Modify: `xtask/src/main.rs`:
  - `Cmd::Soak { #[arg(long)] long: bool, #[arg(long = "for", requires = "long", value_parser = parse_duration)] for_: Option<Duration>, #[command(subcommand)] command: Option<SoakCmd> }`;
  - `enum SoakCmd {}` for now (Observe arrives in Task 8);
  - hidden `#[command(hide = true)] SoakListen { #[arg(long)] secs: u64 }`;
  - `fn parse_duration(s: &str) -> Result<Duration, String>`;
  - the dispatch arms.
- Test: `xtask/src/main.rs` (`parse_duration` unit tests in the existing `mod tests`), and the manual e2e run below

**Reuse:**
- rung 2: `Launched::spawn` and `signal` (`xtask/src/perf/launch.rs:19-38`), `LogStream::start` and `wait_for` (`xtask/src/perf/logstream.rs:94,121`), and `super::bundle(root, Profile::Dev, Flavor::Release, out)` (`xtask/src/main.rs:313`).
- rung 3: std `process::Command` for the bounded runner and the children.
- rung 7: the warm-up and the bounded runner, because perf's (`xtask/src/perf/mod.rs:106,579-662`) are private.

**Interfaces:**
- Consumes:
  - `plan::{plan, Tier, Event, Step}` (Task 2)
  - `judge::{judge, RunFacts, Reading, Hang, Side, Report}` (Task 3)
  - `probe::SoakProbe` (Task 4)
  - `home::SoakHome` (Task 5)
  - `cli::{Cli, Envelope, Answer, Pending}` and `hold::StoreHold` (Task 6)
  - `crate::perf::launch::Launched`, `crate::perf::logstream::{LogStream, Mark}` (Task 1)
- Produces:
  - `pub(crate) fn run(root: &Path, tier: Tier) -> Result<bool>`
  - `pub(crate) fn gate(root: &Path, app: &Path) -> Result<bool>`
  - `pub(crate) fn listen(secs: u64) -> Result<()>`
  - `fn parse_duration(s: &str) -> Result<Duration, String>`, which accepts `<n>s`, `<n>m` and `<n>h`

**Depends-on:** Task 1, Task 2, Task 3, Task 4, Task 5, Task 6

- [ ] **Step 1: Write the failing tests** for `parse_duration`:
  - `durations_read_seconds_minutes_and_hours`: `"90s"` → 90 s, `"20m"` → 1200 s, `"2h"` → 7200 s.
  - `a_duration_without_a_unit_is_refused`: `"90"` → Err naming `90`.

  Run: `cargo test -p xtask duration` — expect: compile error.
- [ ] **Step 2: Implement the driver** in the order of design "Soak driver" steps 1 to 10:
  1. bundle into `build/soak/` (`run` only);
  2. compile the probe into `build/soak/`;
  3. spawn `caffeinate -d -i -w <std::process::id()>`;
  4. `SoakHome::prepare(&[build/soak exe, build/perf exe])`;
  5. start a `LogStream`, then warm up the template: launch in perf mode, record the pid, wait for `statusItemDrawn` (10 s), SIGTERM, and require exit within 15 s;
  6. `fresh_run`, then print `docker=not linked`;
  7. launch the app, `SoakHome::record_pid` it (Decision 8), and wait for `statusItemDrawn`, then drop the stream;
  8. walk the plan, sleeping until each step's `at`; a step that comes due while an earlier one runs starts as soon as that one returns, and the actual start is what is recorded (design Decision 10):
     - `RenewLease`/`CommandLease`/`ReleaseAll` → `Cli::dispatch(…, 10 s)` into a pending list;
     - `OpenListener` → spawn `current_exe() soak-listen --secs 45`;
     - `HoldStore` → `StoreHold::open(<run>/Library/Application Support/Navori Dev/navori.db, 3 s)`, then the renew at +0.5 s, the probe at +1 s and the commit at +4 s;
     - `ProbeCore` → `Cli::status(2 s)`; `TimedOut` records a `Hang` with the load average from `/usr/sbin/sysctl -n vm.loadavg`;
     - `Sample` → `SoakProbe::read`;
     - `ProbeMenu` → if `ioreg -n Root -d1` shows `"CGSSessionScreenIsLocked"=Yes`, return the SOAK-1.10 error `the screen is locked; unlock it and rerun`; otherwise a fresh `LogStream::start()`, then `signal("USR1")`, then `wait_for(pid, |m| matches!(m, Mark::MenuOpened{..}), 2 s)`;
  9. on each step, poll pending dispatches into the error and timeout counts, reap children, and check `wait_exit(Duration::ZERO)`;
  10. teardown in a `Children` guard: SIGTERM, 5 s, then SIGKILL; on a fail verdict print `soak evidence <run dir> pid <pid>` and leave the run directory.

  Print `soak +mm:ss <event>` progress lines as they happen. Then print `judge(&facts).lines()` and return `report.exit_ok(&tier)`. A SOAK-1.10 condition returns `Err` without printing a verdict.
- [ ] **Step 3: Wire the commands** in `main.rs`:
  - `Cmd::Soak { long: false, .. }` calls `soak::run(&root, Tier::Short)`;
  - `long: true` calls `Tier::Long { load: for_.unwrap_or(2h) }`;
  - a false result bails with `soak gate failed`;
  - `Cmd::SoakListen { secs }` calls `soak::listen(secs)`;
  - the `command: Option<SoakCmd>` field is matched: `None` runs the soak (Task 8 adds `Some(SoakCmd::Observe { .. })`).

  Run: `cargo test -p xtask` and `cargo clippy -p xtask --all-targets -- -D warnings` — expect: pass.
- [ ] **Step 4: Run it end to end on this Mac.**
  1. Before the run, save `shasum -a 256` of each existing file among `~/.codex/hooks.json`, `~/.grok/hooks/navori.json` and the `navori` lines of `~/.claude/settings.json` (`grep -n navori`), into `/tmp/soak-hooks-before.txt`. Then `touch /tmp/soak-start`. The installed Navori and other Claude sessions may legitimately write to the real home during the run, so modification times alone cannot prove SOAK-7.6.
  2. Run `cargo xtask soak`.
  3. During the run:
     - `pmset -g assertions` shows a `caffeinate` display assertion;
     - `lsof -p <soak app pid>` lists no path under `~/Library/Application Support/Navori`, `~/Library/Application Support/Navori Dev`, `~/Library/Logs/Navori`, `~/Library/Logs/Navori Dev` or `/tmp/navori-$(id -u)/`; its socket and db are under `/tmp/navori-soak-$(id -u)/`;
     - `find ~/Library/Application\ Support/Navori\ Dev ~/Library/Logs/Navori\ Dev /tmp/navori-$(id -u) -newer /tmp/soak-start 2>/dev/null` prints nothing written by the soak. No `cargo xtask run` dev app may be running.

  Expect: the output includes `docker=not linked`, progress lines, the three signal lines, `soak load errors … timeouts …`, any hang lines and a verdict. While #21 is open, expect `soak hang core` lines and `soak verdict fail`, which is the documented consequence. The exit status is non-zero on fail. The run takes ≤ 23 minutes.

  Then:
  - redo the hook-file hashes into `/tmp/soak-hooks-after.txt` and `diff` them: expect no difference;
  - run a soak and `kill -9` its app after 3 minutes: expect `soak app exited at +…`, `soak verdict fail` and a `soak evidence <dir> pid <pid>` line whose directory still exists;
  - run `cargo xtask soak --long --for 3m` and lock the screen (Ctrl-Cmd-Q) before it ends: expect `the screen is locked; unlock it and rerun`, no verdict and a non-zero exit, not a menu hang;
  - `cargo xtask soak --long --for 3m`: expect the same line kinds and exit 0 whatever the verdict;
  - `pgrep -f "build/soak/Navori Dev.app"` prints nothing;
  - Ctrl-C a second run after 2 minutes, and expect no `Navori Dev` from `build/soak` left running;
  - run `cargo xtask soak` 3 times in a row on an idle machine: expect the same verdict, the same LEAK set and the same hang sides;
  - `git diff --stat main -- apps crates modules` is empty.
- [ ] **Step 5: Commit** `git commit -m "feat(xtask): add cargo xtask soak, a 20-minute and a long soak of an isolated Navori"`

_Requirements: SOAK-1.1, SOAK-1.4, SOAK-1.8, SOAK-1.9, SOAK-1.10, SOAK-1.12, SOAK-1.14, SOAK-3.3, SOAK-3.4, SOAK-5.2, SOAK-7.5, SOAK-7.6, SOAK-8.3, SOAK-9.2, SOAK-9.4_

---

### Task 8: `cargo xtask soak observe` reads the installed app

**Files:**
- Create: `xtask/src/soak/observe.rs`
- Modify: `xtask/src/soak/mod.rs` (add `pub(crate) mod observe;`)
- Modify: `xtask/src/main.rs` (`SoakCmd::Observe { #[arg(long, value_parser = parse_duration)] every: Option<Duration> }` and its dispatch arm)
- Test: `xtask/src/soak/observe.rs` (`#[cfg(test)] mod tests`)

**Reuse:** rung 2 (`SoakProbe::read`); rung 3 (`/bin/ps -axo pid=,comm=` through std `Command`).

**Interfaces:**
- Consumes: `probe::SoakProbe` (Task 4), `parse_duration` and `Cmd::Soak` (Task 7)
- Produces:
  - `pub(crate) fn observe(root: &Path, every: Option<Duration>) -> Result<()>`
  - `pub(crate) fn find_installed_pid(ps_output: &str) -> Option<u32>`

**Depends-on:** Task 4, Task 7

- [ ] **Step 1: Write the failing tests:**
  - `the_installed_app_is_found_by_its_exact_executable_path`: a ps text with `  812 /Applications/Navori.app/Contents/MacOS/Navori` → `Some(812)`.
  - `the_dev_app_is_never_taken_for_the_installed_app`: only `/…/Navori Dev.app/Contents/MacOS/Navori` → `None`.
  - `no_navori_process_is_none`.

  Run: `cargo test -p xtask soak::observe` — expect: compile error.
- [ ] **Step 2: Implement** `observe`:
  - compile the probe into `build/soak/`;
  - find the pid with `ps`; with no pid, bail with `Navori is not running`;
  - print `observe <HH:MM:SS> pid <n> footprint <MiB> threads <n> fds <n>` once, or every `every` until interrupted;
  - send no signal, dispatch nothing and write no file.

  Run: `cargo test -p xtask soak::observe` — expect: pass.
- [ ] **Step 3: Run** `cargo xtask soak observe` and `cargo xtask soak observe --every 10s` (Ctrl-C after 3 lines) against `/Applications/Navori.app`. Expect: one line, then three lines. `git status --short` shows no new file outside `build/`. With Navori quit, the command prints `Navori is not running` and exits non-zero.
- [ ] **Step 4: Commit** `git commit -m "feat(xtask): add cargo xtask soak observe for the installed app"`

_Requirements: SOAK-6.1, SOAK-6.2, SOAK-6.3, SOAK-6.4_

---

### Task 9: Release-check runs the soak gate after perf

**Files:**
- Modify: `xtask/src/release.rs` (after the perf line in `check`, around line 180: `ensure!(super::soak::gate(root, &root.join("build/perf/Navori Dev.app"))?, "soak gate failed");`)
- Modify: `xtask/src/main.rs` (the `ReleaseCheck` doc comment at line 73: "tag free, tree clean, ci, perf, soak gate"; remove Task 2's `#[cfg_attr(not(test), allow(dead_code))]` on `mod soak;`)
- Modify: `xtask/src/soak/mod.rs` (`gate` checks that the app exists)
- Modify: `docs/standards/performance.md` (new `## Soak` section between `## Method` and `## Design rules`: the tiers, rates, 2 s threshold, floors and leak rule, each labelled agent-decided with its source; the #21 consequence)
- Modify: `docs/standards/INDEX.md` (Gates, lines 26-27: "Before every release" adds the soak gate; "(ci + perf)" becomes "(ci + perf + soak gate)")
- Modify: `docs/agents/project.md` (release step 1, lines 141-143: add "then the 20-minute soak gate (`cargo xtask soak`, about 23 minutes)")
- Modify: `docs/specs/2026-11-04-public-release/requirements.md` (after RELEASE-2.1 and RELEASE-2.2: "*(Amended 2026-11-05 by SOAK-4.1/4.2: after perf, release-check runs the soak gate; the tag prints only when it passes too.)*")
- Test: manual run of `cargo xtask release-check`; `cargo xtask --help`

**Reuse:** rung 2, `release::check` (`xtask/src/release.rs:156-184`).

**Interfaces:**
- Consumes: `soak::gate(root, app) -> Result<bool>` (Task 7)
- Produces: none

**Depends-on:** Task 7, Task 8

- [ ] **Step 1: Add the gate line and the doc comment.** `gate` checks that the app exists and fails with `build/perf/Navori Dev.app is missing; run cargo xtask perf first` if not, which is SOAK-1.10's "cannot launch" with no verdict printed. A direct `cargo xtask soak` builds its own bundle, so it never meets this case. Remove the `allow(dead_code)` on `mod soak;`. Run `cargo test -p xtask` and `cargo clippy -p xtask --all-targets -- -D warnings` — expect: pass.
- [ ] **Step 2: Update the four docs** listed under Files.
- [ ] **Step 3: Run `cargo xtask release-check`** on a clean tree whose version tag is free. Expect: preflight, then ci, then perf, then the soak gate, in that order.
  - On a soak fail (expected while #21 is open): it ends `soak gate failed`, exits non-zero and prints no `git tag` line.
  - On a soak pass: it prints the tag and the `git tag … && git push origin …` line.
  - The soak adds ≤ 23 minutes: read the `soak elapsed` line, which is printed on fail as well as pass.
  - `cargo xtask --help` lists `soak` but not `soak-listen`. No release-check path reaches `--long`; `rg -n 'Tier::Long' xtask/src/release.rs` prints nothing.
- [ ] **Step 4: Commit** `git commit -m "feat(xtask): run the soak gate in release-check after perf"`

_Requirements: SOAK-1.10, SOAK-4.1, SOAK-4.2, SOAK-4.3, SOAK-4.4, SOAK-5.5, SOAK-9.1_
