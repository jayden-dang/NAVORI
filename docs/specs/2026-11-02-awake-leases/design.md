# Design: Awake leases

Feature code: AWAKE
Status: Approved
Date: 2026-11-02
Requirements: ./requirements.md

## Context

No Awake code exists. PROCS built everything below the Lease: `PowerAssertions`, `ProcWatcher`, `harness_of`, `Snapshot`, and fakes for each. KCONTRACT built the wait: `WaitHandle::wait_until` with absolute `SystemTime` deadlines, a 30-second closed-menu gap and wake pulses. `build_kernel` registers no Module today (`crates/ffi/src/app.rs:233`), `CoreSnapshot` carries lifecycle only (`crates/ffi/src/lib.rs:89`), and `Store` has one Owner (`core`). MENU, merged at `030774d`, owns the cascade that story 11 sits on.

The binding constraint is that a Lease's truth must survive three different deaths at zero idle cost: a panic inside `run`, an app quit, and a process that exits with no message to anyone. Idle means 0% CPU, ≤ 1 wakeup/s and no timer (GOAL-11, ARCH-16). The design is therefore one actor whose whole state lives in `self`, one wait on one computed deadline, kqueue for exits, and rows on disk only for Leases a process owns.

That rules out a reconciling tick, which is what a simple "every N seconds compare the table to the assertions" would be, and shelling out to `caffeinate` as KeepingYouAwake does. A `caffeinate` child gives no typed refusal, adds a process per hold and bypasses the PROCS wrapper that lock 1 of PROCS built for this.

Fresh retrieval (`fsubr-1.2`, OWNS coverage 4/9; CORE, SHELL, CLI and BUILD have no `tasks.md`, so this is not exhaustive): KCONTRACT `via both` on `crates/ffi/src/app.rs`, `crates/ffi/src/lib.rs`, `apps/macos/Navori/Core/CoreStore.swift`; MENU `via both` on `CoreStore.swift`, `PanelView.swift`, `StatusBarController.swift`, `StatusPanel.swift`; PERF `via both` on `CoreStore.swift`, `StatusBarController.swift`, `StatusPanel.swift`; PROCS `via both` on `crates/macos/src/fake/power.rs`, `fake/processes.rs`. Observations: none.

Delivery: Wave 1 is everything except story 11. Wave 2 is story 11 and the Swift half of its guards. MENU changed no file under `crates/`, `modules/` or `apps/cli/`, so nothing here had to change for Wave 1.

## Decisions

Locks 1–19 in `.skills/AWAKE/close-package.md` are inputs and are not repeated. These are new, each **proposed for approval** with the requirement Open Question it closes:

1. **`Display on` is its own Lease.** A Manual Lease of kind `display`, no deadline, not stored, created and ended by one `display { on }` Action. Rejected: a `display` flag on every Lease (most Leases never need it, and hooks would have to choose), and a global flag that is not a Lease (contradicts lock 5's "a Lease asks for the display"). Consequence: pressing `Display on` with no other Lease also holds System, because a Lease exists. **This changes AWAKE-2.5; see Upstream changes.**
2. **Session Leases and Command Leases are always System.** `hold_session` and `hold_command` take no mode.
3. **One hold-kind Manual Lease at a time.** A new duration or until-off Lease replaces the current one. Pressing `Keep awake for 1 hour` again restarts the hour. The `display` Lease is separate. Rejected: stacking, which gives two rows that mean the same thing.
4. **`hold_session` tries `pid`, then `ppid`.** Both not running: `ActionError::Invalid`, no Lease. `NotAHarness`: `ActionError::Invalid`, no Lease. A Lease with no Harness process could end only by TTL, and this keeps a forged hold from an unrelated process from getting one (THR-6). Consequence: the story 4 demo must run from inside a Harness, because a plain shell gets `NotAHarness`.
5. **`Snapshot::take` runs in `spawn_blocking`.** It costs a few ms of sysctl (2.56 ms median, `.skills/PROCS/spike.md`) on a two-worker runtime. Store calls stay inline, following `System::prune`; one row write is unmeasured and on the Must-measure list.
6. **`awake run` exit codes follow `env(1)`.** 127 when `<cmd>` is not found, 126 when it cannot be executed, 128+N when it dies by signal N. Each start failure writes one stderr line. No Lease is asked for.
7. **`hold_command` on a gone or zombie pid answers Ok with no Lease.** The command finished first, so a warning would be noise (AWAKE-3.9).
8. **The platform is built lazily.** `MacProcesses::new()` and `KqueueExits::start` run at the first process-owned Lease or restored row, not at start. So an idle app has no `navori-exits` thread and test cores are unaffected. A build failure never fails the Module: a process-owned hold answers `Failed`, and stored rows stay on disk, recorded as a problem and retried on the 60-second timer.
9. **Migration runs inside `run`, on every entry.** It is idempotent. An error goes to the supervisor and ends in `failed` after 5 restarts, which the `modules` and `awake` doctor rows show. The app still starts.
10. **Release all deletes first.** A single atomic `DELETE FROM awake_leases` runs before anything changes in memory; if it fails, nothing changes and the Action answers `Failed`, so AWAKE-2.3 and AWAKE-2.4 hold whenever Release all succeeds. A failed row write keeps the Lease in memory, marks its key dirty, records the problem, and retries on the 60-second timer.
11. **Problems are tracked per source.** `system`, `display`, `release`, `store`, `platform`, `exits`. A recovery clears only its own. `AwakeState.problem` carries the first by that order, and the doctor row is ok only when all are empty.
12. **`session_id` is limited to `[A-Za-z0-9._:-]`, 1 to 128 bytes.** Anything else answers `Invalid`. It is caller text that reaches the database through raw `with_conn`, which does no redaction, so limiting it is the protection (ARCH-6, THR-3), not a Shield pass. Whether all three Harnesses' real ids fit is on the Must-measure list. `pid` must be positive and `ppid` non-negative.
13. **A dead exit source is a problem, not a loop.** When `ProcWatcher::next` returns `None`, the Actor drops the watcher, records `exits`, and rebuilds it and re-watches every process-owned Lease on the 60-second timer. Until then, Command Leases have no end signal, which the problem states.
14. **`Reconciler` tries System and Display independently.** If System is refused and Display is wanted, Display is still tried, and `system_held` stays false.

Added 2026-11-04 by stories 14 and 15, each chosen by the user:

15. **`Indefinitely` is the display Lease.** It holds the Display assertion as well as System, in the user's words "display on chính là infinity" (EN: "display on is the infinity"). The cost, stated to the user: the screen stays on through a night of music. The lengths hold System only.
16. **Keep awake choices are one Action each, not a sequence.** `keep_for`, `keep_indefinitely` and `keep_off` change the table in one step before the Reconciler runs, so a change of choice has no moment without System. Rejected: two menu dispatches (`hold_for` then `display off`), which could stop half done. `hold_for`, `hold_until_off` and `display` stay for the CLI and agents, unchanged. `keep_off` is needed because Release all also ends Command and Session Leases.
17. **Tracking off ends nothing.** The table keeps following sessions, and only `wants()` and `snapshot()` leave them out. So turning it on holds at once for a session mid-turn, which a delete could not do: the hook only sends `hold_session` on `UserPromptSubmit`, and the Mac could sleep mid-run. Rejected: deleting rows at off and waiting for the next prompt. Every published reader (Icon, doctor, Holding list, status) is correct with no change of its own.
18. **Tracking off touches only Session Leases.** Manual and Command Leases stay, so turning it off does not end a night of music.
19. **The setting is one `kv` row, `awake/tracking`, written before the table changes.** A failed write answers `Failed` and changes nothing, as Decision 10 does. Nothing stored, or a value that is not a bool, reads as on. A failed read keeps the current value, which is on at first entry, leaving the Mac awake rather than asleep mid-run.
20. **While off, a session's lease Events are not emitted.** "Keeping this Mac awake: Claude Code" would be false. One `awake.tracking_changed` Event marks each change.
21. **Reported work is state on the Lease, not a hold.** A subagent start does not clear the Stop mark, because a subagent can outlive the main turn and then no later `Stop` would ever come, leaving the Lease to the 8-hour TTL. Rejected: treating `SubagentStart` as another `hold_session`, for that reason; and `PreToolUse`, which would start a process on every tool call. A real run on Claude Code 2.1.318 showed `SubagentStop` fires per subagent turn, while the same payload's `background_tasks` still lists that subagent as running, so the set alone would release too early. `stop_session` and `subagent_stop` therefore carry a `background` flag from that list, and the latest one wins. While either says work runs, the Lease has no `next_check`, so it costs no timer and no process read. The Cap stays as the bound for a report that never clears.
23. **A title is held in memory on the Lease and comes from the hook.** The order, chosen by the user, is the name the user gave, then Claude Code's own title, then the branch. `session_title` is documented, but a real run showed it is empty until `/rename` or `--name`, so an unnamed session needs its AI title, which only the transcript holds. The hook reads the last 128 KB of the file and parses only whole `custom-title` and `ai-title` records: in 44 recent transcripts these were never more than 31 KB from the end. This format is not documented, so anything else reads as no title and the row keeps its branch. Rejected: reading transcripts in the Module, which would put file reads and a path from a hook payload inside the Core; and `~/.claude/sessions/<pid>.json`, whose `name` is a derived slug such as `klynt-bd`.
22. **The `working` label needs evidence after grace.** `stopped` stays true through grace and turns false only on a `Reading::NewDescendant`. `Reading::Unknown` keeps the Lease but keeps the label, because nothing was found. So a session can read `working`, `stopped` for up to 120 seconds, then `working` again.

No ADR: none of these is hard to reverse, and `docs/architecture/data.md:33` already leaves the row to this design.

## Upstream changes

Two Approved requirements change. Both are edited in `requirements.md` and need approval with this file.

- **AWAKE-2.5.** It said Release all is the only user command for ending Leases, while story 7 ends a `display` Lease by turning `Display on` off. New text: "THE SYSTEM SHALL offer Release all as its only user command for ending Leases, apart from turning `Display on` off." If Decision 1 is rejected, this edit is dropped.
- **AWAKE-13.10.** It said every row and button is reachable by keyboard. MENU-5.1 moves the keyboard only among rows with an action, so Holding rows cannot be reached that way without changing MENU. New text: every command is reachable by keyboard, and every row and button is read as one sentence by VoiceOver.

## Architecture

Crate `navori-mod-awake` at `modules/awake/`, added to workspace `members` and `[workspace.dependencies]`. It depends on `navori-kernel`, `navori-macos`, `navori-procs`, `navori-store`, `navori-protocol`, `rusqlite` (`navori-store` does not re-export it), `serde`, `serde_json`, `schemars`, `tokio`, `tracing`, `anyhow`, with `navori-macos`'s `fake` feature and tokio `test-util` for tests. check-deps needs no change (`xtask/src/main.rs:670,688-704`).

### Lease table

Satisfies: AWAKE-1.1, AWAKE-1.3, AWAKE-1.5, AWAKE-2.1, AWAKE-2.2, AWAKE-2.6, AWAKE-3.4, AWAKE-4.3, AWAKE-4.4, AWAKE-4.5, AWAKE-4.6, AWAKE-4.7, AWAKE-4.9, AWAKE-5.1, AWAKE-5.2, AWAKE-5.3, AWAKE-5.5, AWAKE-5.6, AWAKE-5.7, AWAKE-5.8, AWAKE-5.9, AWAKE-5.11, AWAKE-5.12, AWAKE-5.14
Reuse: none — new code (rung 7). No existing type models a Lease.
Respects: ARCH-1, ARCH-10
Interface: `LeaseTable::new(Timing)` with `hold_for`, `hold_until_off`, `set_display`, `hold_command`, `hold_session`, `stop_session`, `end_session`, `release_all`, `on_exit(ProcId) -> Vec<Ended>`, `next_deadline() -> Option<SystemTime>`, `settle(now) -> Settled { ended: Vec<Ended>, to_check: Vec<(LeaseKey, ProcId, SystemTime)> }`, `resolve_check(key, Reading, now)`, `leases_on(ProcId) -> usize`, `wants() -> Wants { system, display }`. `Ended { lease, reason: EndReason }`. `Timing { grace, recheck, cap, session_ttl }` with `Timing::FIXED`.
Depth: Deleting this puts the deadline arithmetic, Stop-mark rules, replacement rules and Cap checks into the Actor's handlers; callers otherwise know only "tell it what happened, ask what is due".
Locality: `modules/awake/src/domain/table.rs`, new. leave: every other crate. Pure: takes `now`, never reads a clock or does I/O.

```rust
pub enum LeaseSource {
    Duration, UntilOff, Display,
    Command { process: ProcId },
    Session { session_id: String, harness: Harness, process: ProcId, first_hold: SystemTime, stop_mark: Option<SystemTime> },
}
pub struct Lease { key: LeaseKey, source: LeaseSource, deadline: Option<SystemTime>, label: Option<String> }
pub enum EndReason { Deadline, ProcessExited, GraceClean, Cap, EndSession, ReleaseAll, Replaced }
pub enum Reading { NoNewDescendant, NewDescendant, Unknown }
```

`next_deadline` is the minimum of every Lease's deadline, every Stop-marked session's next check and Cap, and is `None` with no Lease. The Actor calls `settle(now)` at the top of every loop turn, so a late timer, an Action or a swallowed wake pulse all settle everything due. First hold is stamped by the Actor from `wait.now()` on the Action's arrival, before the snapshot, truncated to microseconds so memory and disk agree. `Reading::Unknown` counts as busy. Keys: `duration`/`until_off` share one Manual key (Decision 3), `display`, `session:<session_id>`, `command:<pid>:<start_us>`. `on_exit` ends every Lease with that `ProcId`, because a `/clear` can put two sessions on one Harness process. `ProcWatcher` keeps one entry per pid, so the Actor calls `unwatch` only when `leases_on(process)` reaches zero.

### Reconciler

Satisfies: AWAKE-1.2, AWAKE-1.7, AWAKE-7.1, AWAKE-7.2, AWAKE-7.3, AWAKE-7.4, AWAKE-7.5, AWAKE-8.1, AWAKE-8.2, AWAKE-8.3, AWAKE-8.4, AWAKE-8.5, AWAKE-8.8
Reuse: rung 2 — `PowerAssertions`, `AssertionGuard`, `AssertionKind` (`crates/macos/src/power.rs:29`), for the holding. The policy on top is new.
Respects: ARCH-9
Security: THR-3
Interface: `Reconciler::new(power, name)`, `apply(Wants) -> Held { system, display, refused: Vec<AssertionKind>, release_failed: Option<i32> }`, computed from its guard fields each call, so it repairs any disagreement after a restart.
Depth: Deleting this puts hold order, explicit release and refusal handling into the Actor; callers otherwise know only `Wants`.
Locality: `modules/awake/src/domain/reconciler.rs`, new. leave: `navori-macos`.

Holds System before Display; releases Display before System; never drops a guard implicitly on a normal path, so a failed `release()` reaches the problems. The Actor keeps one retry time, 60 seconds on, while anything wanted is not held or any row is dirty, and it joins the deadline. With no Lease and nothing dirty it is `None`. The assertion name is a fixed string from the composition root, `Navori: keeping this Mac awake` or `Navori Dev: keeping this Mac awake`, so `pmset` tells Profiles apart and nothing user-supplied reaches IOKit.

### Stored Leases

Satisfies: AWAKE-2.3, AWAKE-2.4, AWAKE-6.1, AWAKE-6.2, AWAKE-6.3, AWAKE-6.4, AWAKE-6.5, AWAKE-6.6, AWAKE-6.7, AWAKE-6.8, AWAKE-6.9, AWAKE-13.6
Reuse: rung 2 — `Store::migrate`, `Store::with_conn` (`crates/store/src/lib.rs:119,128`).
Respects: ARCH-6, ARCH-12, ARCH-14
Security: TB-6, THR-3
Interface: `migrate(&Store)`, `load(&Store) -> Vec<StoredLease>`, `upsert(&Store, &StoredLease)`, `delete(&Store, key)`, `clear(&Store)`.
Depth: Deleting this puts SQL and microsecond conversion into the Actor. It is deliberately shallow: its value is that no other file knows the schema.
Locality: `modules/awake/src/infra/stored.rs`, new. Owner `awake`. leave: `crates/store`.

```sql
CREATE TABLE awake_leases (
  key               TEXT PRIMARY KEY,
  kind              TEXT NOT NULL CHECK (kind IN ('session','command')),
  session_id        TEXT,
  harness           TEXT,
  pid               INTEGER NOT NULL,
  process_start_us  INTEGER NOT NULL,
  first_hold_us     INTEGER,
  stop_mark_us      INTEGER,
  deadline_us       INTEGER
);
```

Every time is Unix microseconds, the unit of `StartTime`. A Command row has NULL `first_hold_us`, `stop_mark_us` and `deadline_us`. No argv, environment, cwd or label is stored; the label is recomputed. Restore: a `restored` flag on `self`, set only after the last row, so a panic mid-restore re-runs it, and rows insert by key so re-running is harmless. Load rows; for each, `ProcessSource::entry(pid)`; a gone or zombie process or another start time deletes the row without ever holding; an expired deadline or passed Cap ends it (the Cap with its Event); the rest are watched, added and reconciled. `ProcWatcher::watch` registers before it compares, so an exit in that gap still gives one `Exited`. If the platform cannot be built, rows stay, the problem is recorded and restore retries on the 60-second timer. One `Snapshot::take` recomputes labels only when a row survives, so an empty table costs one `SELECT`. The Stop mark is stored, so grace and the Cap count from it.

### Process reader

Satisfies: AWAKE-3.2, AWAKE-3.3, AWAKE-3.9, AWAKE-4.1, AWAKE-4.2, AWAKE-4.8, AWAKE-5.4, AWAKE-6.10
Reuse: rung 2 — `Snapshot`, `harness_of`, `resolve_location`, `ProcWatcher`, `KqueueExits`, `MacProcesses` (`crates/procs/src/`, `crates/macos/src/`). The real `ProcWatcher` plus `KqueueExits` pairing has never run together in the repo, so Task 6 proves it.
Interface: `Platform` with `Platform::mac()` and a test constructor. `identify(pid) -> Option<ProcId>`, `resolve_session(pid, ppid) -> Result<(Harness, ProcId, Option<String>), Rejected>`, `read_descendants(process, first_hold) -> Reading`.
Depth: Deleting this puts lazy platform construction, the uid and zombie filter and `spawn_blocking` into the Actor; callers otherwise know only "this pid is that process" and "is there new work".
Locality: `modules/awake/src/infra/platform.rs`, new. leave: `crates/procs`, `crates/macos` except the fakes.

`identify` is one `entry` with the uid and zombie filter `harness_of` applies privately. `resolve_session` and `has_new_descendant` take a `Snapshot` in `spawn_blocking`. `read_descendants` uses `Snapshot::descendants`, not `activity`, so CPU is never read. A snapshot that cannot be taken gives `Reading::Unknown`: the Lease stays and is bounded by the Cap. If `ProcWatcher::watch` is rejected by kqueue, a process-owned hold answers `Failed` with no Lease, because a Command Lease with no exit signal and no TTL could never end.

### Actor

Satisfies: AWAKE-1.4, AWAKE-1.6, AWAKE-2.7, AWAKE-4.10, AWAKE-5.10, AWAKE-5.13, AWAKE-9.1, AWAKE-9.2, AWAKE-9.3, AWAKE-9.4, AWAKE-9.5, AWAKE-13.2, AWAKE-13.3, AWAKE-13.4, AWAKE-13.5, AWAKE-13.7, AWAKE-13.8, AWAKE-13.9, AWAKE-13.11, AWAKE-14.2, AWAKE-14.3, AWAKE-14.6, AWAKE-14.9, AWAKE-14.10, AWAKE-15.3, AWAKE-15.4, AWAKE-15.5, AWAKE-15.6, AWAKE-15.7, AWAKE-15.8, AWAKE-15.9, AWAKE-15.10, AWAKE-15.11, AWAKE-15.14, AWAKE-15.15, AWAKE-15.16, AWAKE-16.1, AWAKE-16.2, AWAKE-16.3, AWAKE-16.4, AWAKE-16.5, AWAKE-16.6, AWAKE-16.7, AWAKE-16.8, AWAKE-16.9, AWAKE-16.10, AWAKE-16.11, AWAKE-17.1, AWAKE-17.2, AWAKE-17.3
Reuse: rung 2 — `Module` and the `select!` shape of `System` (`crates/kernel/src/system.rs:107-138`), `WaitHandle`, `ctx.emit`.
Respects: ARCH-3, ARCH-4, ARCH-5, ARCH-10, ARCH-16
Security: THR-6
Interface: `Awake::new(Platform, name, Timing)`; `AwakeAction`; `AwakeState`. Everything else is private.
Depth: n/a — extends `Module`.
Locality: `modules/awake/src/lib.rs`, `actor.rs`, `action.rs`, `state.rs`, `event.rs`, laid out per `docs/codebase/map.md`. extend: nothing in the Kernel. `Config = NoConfig`.

```rust
#[serde(rename_all = "snake_case")]
pub enum AwakeAction {
    Status,                                         // read
    HoldFor { seconds: u64 },                       // write, from here down
    HoldUntilOff,
    Display { on: bool },
    HoldCommand { pid: i32 },
    HoldSession { session_id: String, pid: i32, ppid: i32, title: Option<String> },
    StopSession { session_id: String, background: bool, title: Option<String> },   // both optional
    EndSession { session_id: String },
    ReleaseAll,
    KeepFor { seconds: u64 },                       // the menu's choices, Decision 16
    KeepIndefinitely,
    KeepOff,
    Tracking { on: bool },
    SubagentStart { session_id: String, agent_id: String, pid: i32, ppid: i32 },
    SubagentStop { session_id: String, agent_id: String, background: bool },
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct AwakeState {
    pub leases: Vec<LeaseState>,
    pub system_held: bool,
    pub display_held: bool,
    pub since_ms: Option<i64>,
    pub problem: Option<String>,
    pub tracking: bool,
}
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct LeaseState {
    pub key: String,
    pub kind: String,        // duration | until_off | display | command | session
    pub harness: Option<String>,
    pub label: Option<String>,
    pub pid: Option<i32>,
    pub stopped: bool,
    pub ends_at_ms: Option<i64>,
    pub title: Option<String>,   // sessions only; never stored
}
```

Every Action answers `AwakeState`, so a caller sees that a Lease exists with `system_held: false`. `seconds` must be positive and at most 7 days; `session_id`, `pid` and `ppid` follow Decision 12; otherwise `Invalid`, since these cross TB-2.

```text
run: migrate
     restore unless restored
     settle(now) → reconcile → flush dirty rows → publish      // every entry, so a restart repairs itself
     let mut wait = ctx.wait_handle();                         // one handle per run entry
     loop {
       settle(now) → reconcile → flush → publish if changed
       let deadline = self.deadline();                         // computed before select!: E0502 otherwise
       select! {
         req  = ctx.recv()                 → apply, persist, reconcile, publish, respond
         exit = watcher_next(&mut self.watcher) → Some: on_exit | None: Decision 13
         wake = wait.wait_until(deadline)  → Due: loop | PanelChanged: loop
       }
     }
```

The Lease table, Reconciler and watcher are fields of `self`, so a restart keeps them, and re-entry reconciles before anything else (AWAKE-13.8). A panic between a table change and `apply` is therefore repaired on re-entry. Permanent failure drops `self`, and each guard's `Drop` releases (AWAKE-13.9). `update_state` runs only on a change, because each call pushes a snapshot. Time comes only from `wait.now()`, so tests can drive it.

Events: kind `awake.lease_started` or `awake.lease_ended`, severity Info, Cap release Warn; `awake.tracking_changed`, severity Info. `<source>` is only the Harness name, `1 hour`, `until off`, `display`, or the command's executable name, never a project or branch name.

| Case | Summary |
|---|---|
| Started | `Keeping this Mac awake: <source>.` |
| Ended by duration or TTL | `Stopped keeping this Mac awake: <source> reached its end time.` |
| Ended by process exit | `Stopped keeping this Mac awake: <source> exited.` |
| Ended by grace | `Stopped keeping this Mac awake: <source> stopped.` |
| Ended by Cap | `Stopped keeping this Mac awake: <source> was still busy 2 hours after it stopped.` |
| Release all | `Stopped keeping this Mac awake: Release all.` |
| Session end, display off, or a replaced manual lease | `Stopped keeping this Mac awake: <source>.` |
| Tracking agents on | `Tracking agents is on: agent sessions keep this Mac awake.` |
| Tracking agents off | `Tracking agents is off: agent sessions no longer keep this Mac awake.` |

### Fakes

Satisfies: AWAKE-12.12
Reuse: rung 2 — extends `FakePower` and `FakeProcesses`.
Surface:
- `FakePower::refuse` callers in `crates/macos/tests/power.rs` — `frozen`: keeps refusing every kind (AWAKE-12.12); new behavior is new methods.
- `FakeProcesses::reads` callers in `crates/procs/tests/` — `frozen`: unchanged; `list_calls()` is a new method.
Interface: `FakePower::refuse_kind(kind, code)`, `allow()`, `fail_release(code)`, `panic_on_hold(times)`. `FakeProcesses::list_calls()`.
Depth: n/a — extends `crates/macos/src/fake`.
Locality: `crates/macos/src/fake/power.rs`, `fake/processes.rs`. extend.

### FFI record and doctor

Satisfies: AWAKE-8.6, AWAKE-8.7, AWAKE-8.9, AWAKE-10.1, AWAKE-10.2, AWAKE-10.3, AWAKE-10.4, AWAKE-10.5, AWAKE-10.6, AWAKE-12.1, AWAKE-12.2, AWAKE-12.3, AWAKE-12.4, AWAKE-12.10, AWAKE-12.11, AWAKE-13.1, AWAKE-15.12
Reuse: rung 2 — `Kernel::state`, `Kernel::modules`, `snapshot()`, `Inner::doctor`.
Respects: ARCH-1, ARCH-2, ARCH-14
Surface:
- `build_kernel` caller `crates/ffi/src/lib.rs:187` — `replace`: passes the Profile; every test core now registers Awake. Holds nothing and starts no thread until a Lease (Decision 8).
- `CoreSnapshot` readers in `crates/ffi/tests/core.rs`, `crates/ffi/tests/config_privacy.rs`, `apps/cli/tests/cli.rs`, `apps/macos/Navori/Core/CoreStore.swift`, `SettingsView.swift` — `frozen`: built around; the field is additive and these keep passing unedited.
- `core.doctor` over IPC, and `doctor` readers `crates/ffi/tests/core.rs:407-421` and `:568`, `apps/cli/tests/cli.rs:150-187`, `crates/ffi/tests/config_privacy.rs:58` — `frozen`: built around; the five rows stay, one row is added and is ok when idle. The `modules` row text becomes `2 running`; nothing asserts it.
- `core.status` over IPC, `apps/cli/src/main.rs:256-274`, `crates/ffi/tests/core.rs:84-89` — `frozen`: built around; a second module row, and tests look for `system`.
- `navori actions` output, `apps/cli/tests/cli.rs:101-108` — `frozen`: built around; it only asserts `system.ping`. `--json` grows by nine copies of the enum schema.
- `SettingsView.swift:129` help text, `docs/agents/verify.md`, `README.md:27`, which name the doctor rows — `replace`: add `awake`. `docs/standards/errors-logging.md:30` already says every Module has a doctor check, so it stays. The Swift line waits for Wave 2.
- `CoreStore.moduleCount`, `PerfProbe`'s `modules=`, `modules_running` in `xtask/src/perf/mod.rs:248` and `judge.rs:163` — `frozen`: built around; informational only, no gate.
- `CoreStore.health` — `frozen`: built around; a `failed` or `restarting` `awake` turns the icon to the warning state, which is correct, since the Core has a failing Module.
Interface: `CoreSnapshot.awake: Option<AwakeView>`, `AwakeView`, `LeaseView`. `pub fn awake_view(&Kernel) -> Option<AwakeView>`, used by both `snapshot()` and `Inner::doctor`.
Depth: n/a — extends `navori-ffi`.
Locality: `crates/ffi/src/lib.rs`, `app.rs`, `Cargo.toml`. extend. `navori-mod-awake` has no `uniffi` dependency.

`awake_view` deserializes `Kernel::state("awake")` into `AwakeState`, then `From<AwakeState> for AwakeView`. The mapper destructures `AwakeState` and `LeaseState` with no `..`, so a field added or removed in the Module type breaks the build. That is stricter than the limit recorded with lock 6, and costs one mapper edit per field. While the lifecycle is `failed` or `stopped`, it reports nothing held, with the problem `The Awake module stopped.` (AWAKE-8.9), because the last published State outlives the Module (`crates/kernel/src/runtime.rs:175-176`). State that will not deserialize gives `None`. Doctor adds one fixed row after `modules`: ok with `no problem` or the Lease count; failing with the problem text. No seam.

### CLI

Satisfies: AWAKE-3.1, AWAKE-3.5, AWAKE-3.6, AWAKE-3.7, AWAKE-3.8, AWAKE-9.6, AWAKE-12.5, AWAKE-12.6, AWAKE-15.13
Reuse: rung 2 — `Client`, `Failure`, the clap `Command` enum (`apps/cli/src/main.rs:28`); rung 3 — `std::process::Command`, so no new tokio feature.
Respects: ARCH-11
Surface:
- `main`'s exit mapping (`apps/cli/src/main.rs:86-100`) — `replace`: `awake run` returns its own `ExitCode` before `Client::connect` at `:127`. Other commands keep the mapping.
Interface:
```text
navori awake hold [--for <1h|30m|90s>]    no flag means until turned off
navori awake release-all
navori awake status [--json]
navori awake run -- <cmd> [args...]
```
`Display on` has no command: `navori dispatch awake '{"display":{"on":true}}'` covers Wave 1.
Depth: n/a — extends `apps/cli`.
Locality: `apps/cli/src/main.rs`. extend. Deps stay `config`, `ipc`, `protocol`.

`awake run`: spawn `<cmd>` with inherited stdio, connect, dispatch `hold_command { pid }` under a 5-second timeout, then wait and exit with its code. No handler is installed, so Ctrl-C reaches the shared process group as with any wrapper. A failed connect writes `navori: no Lease held because Navori is not running`; any other failure writes `navori: no Lease held: <reason>`. Stderr text is one line, and it does not change the exit code.

## UI design

Grounding: the user's locks, then `docs/design/2026-11-01-menubar-ui-brief.md` Surfaces 1 and 3, then `feat/menu`'s glass and `MenuRow` system. Story 11 is a native surface, not a browser one; the section is filled because MENU's design set that precedent. Tokens are the AppKit semantic colors MENU uses. The Awake sample in `Features/Preview/PreviewMenu.swift`, which the user judged in the Preview, sets the row's icon and the Submenu's shape.

Respects: ARCH-1, ARCH-2
Satisfies: AWAKE-11.1, AWAKE-11.2, AWAKE-11.3, AWAKE-11.4, AWAKE-11.6, AWAKE-11.7, AWAKE-11.8, AWAKE-11.9, AWAKE-11.10, AWAKE-11.11, AWAKE-11.12, AWAKE-11.13, AWAKE-11.14, AWAKE-11.15, AWAKE-11.17, AWAKE-11.18, AWAKE-11.21, AWAKE-12.7, AWAKE-12.8, AWAKE-12.9, AWAKE-13.10, AWAKE-14.1, AWAKE-14.4, AWAKE-14.5, AWAKE-14.7, AWAKE-14.8, AWAKE-14.11, AWAKE-15.1, AWAKE-15.2, AWAKE-17.4, AWAKE-17.5, AWAKE-17.6, AWAKE-17.7
Reuse: rung 2 — `MenuRow`, `CascadeNode`, `CascadeContent`, `CoreRowState` pattern, `CoreStore.dispatch`; new (rung 7): `AwakeRowState` in `NavoriPanel`, `AwakeSubmenuView`, `HoldingList`, `HarnessMarkImage`.
Surface:
- MENU-1.2 and MENU-1.4 in MENU's `requirements.md`, and MENU's "Comes back if" line — `replace`: amended as story 11 states.
- MENU Task 18's grep for `"Awake"` (`tasks.md:527`) — `frozen`: built around; a closed task's record of what was true at delivery, not a live gate.
- `Features/Preview/` sample rows, including `Let my Mac sleep now` — `frozen`: built around; dev-only sample data marked `NAVORI-PREVIEW-SAMPLE`, not the Awake Submenu.
- `menuTree()` and `PanelView`, which must agree on ids — `replace`: both gain `awake`.
- The `submenu` closure in `StatusBarController.swift:19-22`, which guards on `id == "core"` — `replace`: it switches on `core` and `awake`.
- `MenuRow` memberwise init, at every call site in `PanelView.swift`, `PreviewMenu.swift` and `CoreSubmenuView.swift` — `frozen`: built around; `isDimmed` defaults to `false`.
- The perf probe's `open("core")` — `frozen`: built around, the Core row keeps its id.
Interface: `CoreStore.awakeStatus: AwakeStatus`, `CoreStore.awakeSubmenu: AwakeSubmenuState`, `CoreStore.perform(AwakeCommand)`, and plain `AwakeLeaseInput` for the Submenu, so no generated types pass `CoreStore`.
Depth: If `AwakeRowState` vanished, callers would still need only a status enum; the label, dot and submenu rules are hidden and tested.
Locality: wave 2 only. new: `NavoriPanel/AwakeRowState.swift`, `NavoriPanel/AwakeSubmenuState.swift`, `NavoriPanel/KeepAwakeChoice.swift`, `Features/Panel/AwakeSubmenuView.swift`, `Features/Panel/HarnessMarkImage.swift`. extend: `PanelView.swift`, `MenuRowView.swift`, `StatusBarController.swift`, `CoreStore.swift`. leave: `CascadeState`, `CascadeController`, `Features/Preview/`.

### Awake row (level 0)

Layout: one `MenuRow` directly above the Core row, with icon `cup.and.saucer`, title `Awake`, a dot when held, and a chevron. No text value.
Components: rung 2 — `MenuRow`, gaining `spokenValue`; new — `AwakeRowState`, tested like `CoreRowState`.
States: a Lease has the System assertion → green dot · anything else (no Lease, not held, or no Lease with a recorded problem) → no dot · a failed menu action leaves the dot as the assertion has it · Awake record missing or Module failed → no dot, no chevron, no Submenu. A problem is stated in the Submenu and by the tray icon, per ICON-4.3.
Type & color: as the Core row; the dot is the row's only state.
A11y: `MenuRowView` reads title then `spokenValue` as one sentence: `Awake, On · Claude Code ×8`, `Awake, Off`, `Awake, Not held`, `Awake, Needs attention` or `Awake, Unavailable`.

### Awake Submenu (level 1)

Lifted from `ui-brief.md` (locked 2026-11-04, variant C1, with its amendment 7).

Layout: the two commands on top: `Keep awake`, which opens the Keep awake Submenu, and `Tracking agents`. Then a separator and a problem line, only when something is wrong. Then a separator and the Holding list, grouped: `Held by you`, `Commands`, then projects in case-insensitive name order. Within a project, working sessions first, then by name, then by pid, so a group does not move when a session's state changes. The list is 26 pt per title and per row, and scrolls past 344 pt. The commands are `MenuRowView` rows, because the cascade's selection and keyboard work on rows.
Components: rung 2 — `MenuRowView` for commands, gaining `MenuRow.isChecked` beside `isDimmed`; new (rung 7) — `AwakeProblemView`, `HoldingList`, `HoldingLine`, `HoldingMarkView`, reason: no existing row has a mark, a dot and a right-aligned word without selection; new (rung 7) — `HarnessMarkImage`, reason: SF Symbols has no harness logos, and SVG template images need no resource bundle. The tree holds `awake.keep`, with one child per `KeepAwakeChoice`, and `awake.tracking`, and is static, so a changing Holding list needs no `replaceTree`. `awake` has children whenever `store.hasSnapshot`, as `core` does, and the row's `hasSubmenu` flipping calls `replaceTree`.
States: off → the commands alone · on → the commands and the list, with no status text · a Keep awake lease exists → `until 11:38` or `Indefinitely` on the `Keep awake` row · Tracking agents → a green dot and `On`, or `Off` with no dot · not held → `Can't hold this Mac awake` / `macOS refused the request. Navori keeps trying.` · display refused → `Display not held` / `2 Leases · idle sleep blocked` · a failed action or a recorded problem → its text, under the refusal title if there is one.
Holding rows: session → Harness mark, its title or else its branch, green dot and `working`, or dimmed with `stopped` · Command → `terminal`, `<exe name>`, `running` · Manual → `clock` `1 hour` `until 11:38`, `infinity` `Until off`, `display` `Display`, dimmed while not held. The Harness, branch and pid stay in the tooltip and VoiceOver: `Road-128 ticket shaping · Claude Code · klynt/dev · pid 51865`, without the title when there is none.
Type & color: as MENU; names 14 pt, words 12 pt, project titles 12 pt semibold in `secondaryLabelColor`. A stopped row is `secondaryLabelColor` throughout. Marks are monochrome template images; color only in the dots.
A11y: ↑ and ↓ reach every command, per MENU-5.1, and → opens `Keep awake`; the problem line and each Holding row are read by VoiceOver as one sentence, for example `Claude Code, NAVORI/main, working, pid 23719`, and carry no keyboard stop; focus follows MENU's rules.

### Keep awake Submenu (level 2)

Layout: 180 pt wide, so `CascadePanel` now takes its width from the content instead of a fixed 360; every other surface still sets 360 itself. It opens on the side away from level 0. One `MenuRowView` per `KeepAwakeChoice.all`: `Indefinitely`, `5 minutes`, `10 minutes`, `15 minutes`, `30 minutes`, `1 hour`, `2 hours`, `5 hours`. No icons, as in Caffeine.
Components: rung 2 — `MenuRowView`; new — `KeepAwakeSubmenuView`, `KeepAwakeChoice`, `AwakeCommand`.
States: a check on the choice its Lease came from: a display or until-off Lease checks `Indefinitely`; a duration Lease checks the row whose title is its published label, so a length from the CLI the menu does not offer checks nothing. Pressing a checked row sends `keep_off`; any other sends that choice. The panel stays open so the check is seen, as the commands did before.
A11y: a checked row adds the selected trait.

## Seams for testing

New seams: one, the `Platform` test constructor.

| Seam | Kind | Covers |
|---|---|---|
| `Kernel::builder_with_clock` + `register(Awake::new(fakes))` + `Kernel::dispatch`, on paused tokio time (which does not auto-advance during `spawn_blocking`, tokio 1.54.1 `src/time/clock.rs:127-155`) | integration | stories 1, 2, 4, 5, 6, 7, 8 except AWAKE-8.7 and AWAKE-8.9; AWAKE-3.2, 3.3, 3.4, 3.9; AWAKE-9.1–9.5; AWAKE-13.2–13.6; AWAKE-13.8, 13.9 through `FakePower::panic_on_hold` |
| `Timing` passed to `Awake::new` | integration | AWAKE-5.14 |
| `NavoriCore` in `crates/ffi/tests/core.rs` | integration | AWAKE-15.12, AWAKE-5.13, AWAKE-10.1–10.4, 10.6, 12.1–12.4, 12.10, 12.11, and the idle `awake` row of AWAKE-8.7 |
| `awake_view` and `Inner::doctor` on an `Inner` around a Kernel with a fake Awake, in `crates/ffi/tests/` | integration | AWAKE-8.7, 8.9 |
| The real `navori` binary in `apps/cli/tests/cli.rs` | e2e | AWAKE-15.13, AWAKE-3.1, 3.5–3.8, 9.6, 12.5 |
| Real OS test in `modules/awake/tests/real_os.rs`, reading `pmset` | integration | AWAKE-1.2, 1.7, 7.1, 7.3; real `ProcWatcher` + `KqueueExits`, real restore |
| `cargo xtask check-deps`, `cargo xtask ci` | gate | AWAKE-10.4, 12.6, 13.7 |
| Fakes' own tests in `crates/macos` | unit | AWAKE-12.12 |
| `cargo xtask perf` | gate | AWAKE-13.1 |
| Diff review | manual | AWAKE-2.6, 2.7, 10.5, 13.11 |
| `modules/awake/tests/keep.rs`, `tracking.rs`, `subagent.rs`, `title.rs`, through the same Kernel seam | integration | AWAKE-14.2, 14.3, 14.6, 14.9, 14.10, 15.3–15.11, 15.14–15.16, 16.1–16.11, 17.1–17.3 |
| `AwakeRowState`, `AwakeSubmenuState`, `KeepAwakeChoice`, `CascadeState` in `NavoriPanelTests` | unit | AWAKE-14.1, 14.4, 14.5, 14.11, 15.2, 17.4–17.7, 11.2, 11.3, 11.4, 11.7, 11.8, 11.9, 11.11, 11.12, 11.13, 11.14, 11.15, 11.17, 11.18 |
| Hand walk with VoiceOver, per `docs/agents/verify.md` | manual | AWAKE-14.7, 14.8, 15.1, 11.1, 11.6, 11.10, 11.21, 12.7–12.9, 13.10 |

## Coverage check

Every ID in `requirements.md` appears in exactly one `Satisfies:` line above. Deliberately unmapped: none.

## Still open

- The Must-measure list in `requirements.md` stays open, plus whether all three Harnesses' real session ids fit Decision 12. Nothing here assumes those results.
- `wait` is created per `run` entry, so the 30-second gap restarts after a crash. KCONTRACT Decision 1 places the handle inside `run`; this design keeps that.
- The exact English in this file is proposed, for approval with the file.
