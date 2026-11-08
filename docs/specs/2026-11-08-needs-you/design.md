# Design: Needs you

Feature code: NEEDS
Status: Draft
Date: 2026-11-08
Requirements: ./requirements.md

## Context

Navori's Core runs Modules as supervised actors that meet only through the event bus and dispatched Actions
(ARCH-3, ARCH-10). Today no Module can dispatch: `ModuleCtx` holds the bus, the store, the scheduler and its
config, but no handle to another Module or to the Action catalog (`crates/kernel/src/module.rs:122-131`).
`Kernel::dispatch` (`crates/kernel/src/runtime.rs:330-343`) is called only by the Shell, the CLI over IPC and
Navori AI, and it checks no Risk; the only Risk gate is Navori AI's, which refuses anything not `Read`
(`ai/agent/src/dispatch.rs:33-42`). Level 0 shows banners, `Running`, the Core row when MENU-3.11 allows it and
the footer (`apps/macos/Navori/Features/Panel/PanelView.swift:9-24`); Needs you exists only as `#if DEBUG`
sample rows (`apps/macos/Navori/Features/Preview/PreviewSampleRows.swift`).

The binding constraint is that the user's answer must run the chosen Envelope **inside the Core**, with the
Decision as the one approval for a `Destructive` choice (GOAL-1, ARCH-1, ARCH-5), while any Module can raise a
Decision without depending on the Needs you crate (ARCH-3; `xtask` forbids module-to-module dependencies). That
rules out the Events-only shape — producers subscribing to `decision_answered` and running their own answer —
because a Destructive run would then happen outside the place the user approved it, and no Module subscribes to
the bus today (scan, `.skills/_pending-needs-you/auto/scan.md` §1). It also rules out the Shell dispatching the
chosen Envelope itself (ARCH-1).

The second constraint is Compat External (`docs/agents/project.md`): every Action payload, table, status string and
config key below is permanent once shipped, with forward-only migrations (ARCH-12). The third is the cascade:
submenu panels never become key unless VoiceOver is on (`apps/macos/Navori/StatusBar/CascadePanel.swift:57`), and
the level-0 panel closes the whole menu when it loses key (`StatusPanel.swift:177-188`) — yet brief D puts a text
field and a date calendar in the Needs you submenu (`docs/design/2026-11-08-needs-you-ui-brief.md:41-48`).

Evidence: scan `.skills/_pending-needs-you/auto/scan.md`, design scan `design-scan.md`, code-claim review
`req-review.md`, research `research-nspanel-key.md` (all in that `auto/` directory of the main checkout).
Fresh retrieval (load-subgraph, schema 1.1, fsubr-1.2): owns_coverage 15/20 (0.75); NEEDS has no `tasks.md` yet
(empty OWNS); observations `[]`; nodes NEEDS, MENU, ICON, AWAKE, PSTOP, LOOP, LID, SOAK, KCONTRACT, … — reuse below
comes from the code, not from the envelope. Relies on ARCH-1, ARCH-2, ARCH-3, ARCH-4, ARCH-5, ARCH-6, ARCH-9,
ARCH-10, ARCH-12, ARCH-16.

## Decisions

1. **A kernel-given `ModuleDispatcher`** on every `ModuleCtx`: non-blocking send, its own timeout, a catalog Risk
   lookup. ADR [0011](../../adr/0011-modules-dispatch-through-a-kernel-given-dispatcher.md) (passes the
   three-part gate: hard to reverse, surprising, a real trade-off against Events-only).
2. **Destructive dispatch from a Module needs a grant by ID from the composition root** (`build_kernel`), given only
   to `needs_you` (NEEDS-2.6, NEEDS-4.4). The kernel still knows no Module by name (`crates/kernel/src/tray.rs:1-2`).
3. **Two Module crates**, `modules/needs_you/` (`navori-mod-needs-you`: Decisions, Updates, their History) and
   `modules/todo/` (`navori-mod-todo`: To-dos and theirs), joined in `crates/ffi` into `NeedsYouView` like
   `running.rs` joins Awake and Ports (close-package L-OWNER, F1).
4. **At most once**: `needs_you` writes `answering` to its row before it dispatches the chosen Envelope; a run that
   starts and finds `answering` turns it into `failed` with the restart error and never dispatches it again on its
   own (NEEDS-4.9, NEEDS-4.10; close-package F3, F4).
5. **Risk**: stored at raise in the row for History (NEEDS-1.4); recomputed from the live catalog when `needs_you`
   starts and at each answer — the catalog only changes across a restart, so "now" (NEEDS-3.8) costs nothing per
   snapshot.
6. **History is read on demand** through `Read` Actions (`needs_you.history`, `todo.history`) when `History ›` opens,
   not carried in every snapshot (perf, NEEDS-14.1).
7. **Retention by constructor**: `build_kernel` passes `JournalSweep` (days, or paused) into both Modules, because
   `other_sections` drops the `journal` key from Module config (`crates/config/src/lib.rs:244-254`).
8. **The local day comes from the platform crate**: `navori-macos` reads the UTC offset with `localtime_r`
   (thread-safe; ARCH-9 keeps the `unsafe` there); `time` 0.3 (workspace dep, `Cargo.toml:67`) does date arithmetic.
   Not `time`'s `local-offset` feature, which can return `IndeterminateOffset` and whose tz refresh is unsound in a
   library (research note).
9. **Typing in a submenu**: the Needs you cascade panel accepts key only while it hosts the field or the calendar,
   with `becomesKeyOnlyIfNeeded`; the level-0 panel keeps the menu open when key moves inside the cascade
   (research note; NEEDS-8.2, NEEDS-13.11, NEEDS-13.13). Proven by a throwaway `run-spike` before any view is built.
10. **Docs amended by name**: MENU-1.2, MENU-1.4, AWAKE-11.1, rules 6 and 12 of the 2026-11-01 menubar brief,
    `docs/architecture/data.md:37`, `docs/codebase/modules.md:50`, `docs/codebase/map.md:44`.

## Architecture

Vocabulary for every section below: **module**, **interface**, **implementation**, **seam** (public surface where
behavior is observable and substitutable). Prefer deep modules — an interface much simpler than what it hides.

### A. Module dispatcher (`crates/kernel`)

Satisfies: NEEDS-2.1, NEEDS-2.2, NEEDS-2.3, NEEDS-2.4, NEEDS-2.5, NEEDS-2.6
Reuse: 2 — extends the existing `SendFn` table (`crates/kernel/src/runtime.rs:60,167-178`, already a non-blocking `try_send` returning a `oneshot::Receiver`) and `Kernel::dispatch`'s timeout and error mapping (`:330-343`)
Respects: ARCH-3, ARCH-4, ARCH-5, ARCH-10
Surface:
- `DispatchError` (`crates/kernel/src/runtime.rs:29`) gains `Refused { module, action }` — readers: `dispatch_to_rpc` (`crates/ffi/src/app.rs:562-570`, **replace**: an exhaustive arm mapping it to `INVALID_PARAMS`, unreachable from `Kernel::dispatch` but required by the match); `CoreError::Dispatch` (`crates/ffi/src/lib.rs`, uses `Display`, needs no edit); every other `match` found by `rg 'DispatchError::' crates ai apps` (**replace**). `Kernel::dispatch` never returns it, so CLI, Shell and Navori AI see no change (NEEDS-13.1).
- `Registered`, `Plumbing`, the `Launcher` type and `start()` (`runtime.rs:60-83,232-248`) — **replace**: the router moves into an `Arc<OnceLock<Router>>` that `start()` fills **before** the first launcher runs, and `Kernel::actions` / `Kernel::dispatch` read the same router.
- `ModuleCtx` (`module.rs:122`) gains a private `dispatcher` field — readers: the launcher closure that builds it (`runtime.rs:205-219`, **replace**); no reader outside `navori-kernel` builds a `ModuleCtx`.
- `KernelBuilder` gains `grant_destructive(id)` — no existing reader. `start()` logs an error and drops a grant naming no registered Module, and a test on `build_kernel` asserts the grant set is exactly `{"needs_you"}`, so a typo cannot pass silently.
Interface:
```
impl<M: Module> ModuleCtx<M> { pub fn dispatcher(&self) -> ModuleDispatcher }
#[derive(Clone)] pub struct ModuleDispatcher            // Send + Sync, cheap clone
impl ModuleDispatcher {
    pub fn send(&self, envelope: Envelope) -> Result<Pending, DispatchError>;   // never awaits
    pub fn risk_of(&self, envelope: &Envelope) -> Option<Risk>;                 // None = not in catalog
}
pub struct Pending;  impl Future for Pending { type Output = Result<Value, DispatchError>; } // own timeout
impl KernelBuilder { pub fn grant_destructive(self, module: &'static str) -> Self }
```
Depth: if it vanished, a Module would have to reach every peer's inbox, the catalog and the timeout itself, and rebuild the grant check — the dispatcher hides the router, the action-name parsing, the timeout and the grant behind one non-blocking send.
Locality: extend `crates/kernel/src/runtime.rs` (router built at `start()`) and `module.rs` (ctx field + accessor); new `crates/kernel/src/dispatcher.rs`. Neighbors: `Kernel::dispatch` reuses the same router (leave its behavior).

The router is the list of `(id, SendFn, Vec<ActionSpec>)` moved into an `Arc<OnceLock<Router>>` in `Plumbing` and
filled once in `start()`, after every Module is registered (a Module registered later is unknown at its peer's
`register`, `runtime.rs:140-248`). `SendFn` holds only an `mpsc::Sender`, so the `Arc` makes no cycle.
`send` looks up the module (`UnknownModule`), names the Action from `envelope.action` — a JSON string or a
single-key object (`crates/protocol/src/lib.rs:33-39`) — and its catalog Risk; if that Risk is `Destructive` and
the calling Module's ID is not in the builder's grant set, it returns `Refused` and sends nothing (NEEDS-2.6).
Otherwise it calls `SendFn` (`InvalidAction`, `Unavailable` as today) and returns `Pending`, which wraps the
`oneshot::Receiver` in `tokio::time::timeout(dispatch_timeout)` and maps the outcome exactly as `Kernel::dispatch`
does (`Timeout`, `Unavailable`, `Action`) (NEEDS-2.3, NEEDS-2.5). Because `send` only `try_send`s, a Module that
dispatches to itself gets its request as a later `ctx.recv()` (NEEDS-2.4); the rule for every caller, written on
the type's doc comment, is to await `Pending` in a `select!` arm or a spawned task, never inline in `run`
(NEEDS-2.2) — the ports actor's `Stops` arm is the precedent (`modules/ports/src/actor.rs:151-166`).

### B. The Needs you Module: raise and its validation (`modules/needs_you`)

Satisfies: NEEDS-1.1, NEEDS-1.2, NEEDS-1.3, NEEDS-1.4, NEEDS-1.5, NEEDS-1.6, NEEDS-1.7, NEEDS-1.8, NEEDS-1.9, NEEDS-1.10, NEEDS-1.11, NEEDS-1.12, NEEDS-2.7
Reuse: 2 — the Module pattern of `modules/awake` (`stored.rs` migrations and `with_conn` idiom, `src/infra/stored.rs:7-124`; externally tagged `snake_case` Action enum with `risk()`/`catalog()`, `src/action.rs:4-86`); `navori_redact::redact_text` (`crates/redact/src/lib.rs:120`)
Respects: ARCH-3, ARCH-4, ARCH-6, ARCH-10, ARCH-12
Security: THR-3, TB-6
Interface:
```
needs_you.raise { key, title, reason, expires_at_ms?, primary: Choice, secondary: Choice }   // Risk Write
Choice { label, envelope?: Envelope }
→ Ok({ id, revision }) | Invalid("…")
NeedsYou::new(retention: JournalSweep) -> NeedsYou        // Module ID "needs_you"
```
Depth: if it vanished, a producer would still need to know only the raise payload and that a repeat `key` replaces — dedupe, validation, the Shield, persistence and Risk lookup stay inside.
Locality: new crate `modules/needs_you/` (`Cargo.toml` with `[lints] workspace = true`, deps `navori-kernel`, `navori-store`, `navori-protocol`, `navori-redact`, `navori-config` (for `JournalSweep`), `rusqlite`, `serde`, `serde_json`, `schemars`, `tokio`, `tracing`, `anyhow`); workspace `members` and `[workspace.dependencies]` (`Cargo.toml:3-21`); leave every other Module.

Validation order on `raise`, refusing with `ActionError::Invalid` and writing nothing: empty or whitespace `key`,
`title` or a `label` (NEEDS-1.7); `expires_at_ms` not after `wait.now()` (NEEDS-5.3); a choice Envelope whose
`module` is `needs_you` (NEEDS-1.6); a choice Envelope for which `ctx.dispatcher().risk_of` is `None` (NEEDS-1.5);
an open Decision with this `key` whose state is `answering` (NEEDS-1.11); a choice Envelope in which
`navori_redact::redact_json` on a copy finds anything (NEEDS-1.12 — the stored Envelope is never altered, so it
still runs as raised). Then `redact_text(..).text` on title,
reason and both labels (NEEDS-1.8), and in one transaction either insert a new `open` row (NEEDS-1.2) or, for an
open/failed row with this `key`, replace its text, expiry and choices, bump `revision`, clear `chosen` and `error`, and set it back to `open`
(NEEDS-1.3). Each choice's Risk is stored as the lowercase Risk name or NULL (NEEDS-1.4). The CLI reaches the same
code through IPC (`navori dispatch needs_you '{"raise":{…}}'`, `apps/cli/src/main.rs:58-63`; NEEDS-1.10), and a
Module through `ModuleDispatcher::send` (NEEDS-2.7). At the top of every `run` the Module migrates (no-op when
current) and reloads State from its tables (NEEDS-1.9; precedent `modules/awake/src/actor/mod.rs:81-84`).

Tables (designed here; the migration is written in the build, never in this run), Owner `needs_you`:
```
needs_you_decisions(
  id INTEGER PRIMARY KEY, key TEXT NOT NULL, revision INTEGER NOT NULL,
  state TEXT NOT NULL,                 -- 'open' | 'answering' | 'failed' | 'answered' | 'expired'
  title TEXT NOT NULL, reason TEXT NOT NULL, raised_at_ms INTEGER NOT NULL, expires_at_ms INTEGER,
  primary_label TEXT NOT NULL, primary_envelope TEXT, primary_risk TEXT,
  secondary_label TEXT NOT NULL, secondary_envelope TEXT, secondary_risk TEXT,
  chosen TEXT,                         -- 'primary' | 'secondary'
  error TEXT, resolved_at_ms INTEGER)
UNIQUE INDEX needs_you_decisions_open_key ON needs_you_decisions(key) WHERE state IN ('open','answering','failed')
needs_you_updates(
  id INTEGER PRIMARY KEY, kind TEXT NOT NULL,   -- 'decision_expired' (ROAD-23 adds kinds)
  title TEXT NOT NULL, decision_id INTEGER,      -- soft link, no FOREIGN KEY
  created_at_ms INTEGER NOT NULL, resolved_at_ms INTEGER,
  resolution TEXT)                               -- 'viewed' | 'cleared'
```
`failed` counts as open for the `key` index and for attention; `answered` and `expired` rows with a
`resolved_at_ms` are History. Envelopes are stored as their JSON text. The string columns carry no `CHECK`: the
values are permanent (Compat External), but a later kind or state (ROAD-23, a snooze) is then an added value, not
a table rebuild; the Rust enums validate on read and write. `decision_id` has no `FOREIGN KEY` because the store
runs with `foreign_keys=ON` (`crates/store/src/lib.rs:99`) and History prunes Decisions and Updates on their own
clocks.

### C. Answering a Decision (`modules/needs_you`)

Satisfies: NEEDS-4.3, NEEDS-4.4, NEEDS-4.5, NEEDS-4.7, NEEDS-4.8, NEEDS-4.9, NEEDS-4.10, NEEDS-4.11, NEEDS-4.12, NEEDS-3.8, NEEDS-3.4, NEEDS-11.9
Reuse: 2 — section A's `ModuleDispatcher`; Navori AI's existing gate (`ai/agent/src/dispatch.rs:33-42`) for NEEDS-4.11 with no edit; the ports actor's in-flight arm (`modules/ports/src/actor.rs:35-44,151-166`); `Module::tray` (`crates/kernel/src/module.rs:75`)
Respects: ARCH-1, ARCH-5, ARCH-10
Security: THR-2, TB-3
Interface:
```
needs_you.answer { id, revision, choice: "primary" | "secondary" }   // Risk Destructive → Ok(()) once accepted
NeedsYouState { decisions: Vec<OpenDecision>, updates: Vec<UnviewedUpdate>, … }  // read through Kernel::state
OpenDecision.state: Open | Answering { choice } | Failed { choice, error }
```
Depth: callers know one Action and the three states a Decision row can show; the persisted marker, the dispatch, the reply arm, the Risk recheck and the restart rule stay inside.
Locality: `modules/needs_you/src/actor.rs` (run loop), `answer.rs`; `crates/ffi/src/app.rs` `build_kernel` adds `.grant_destructive("needs_you")` (one line, extend).

`answer` is catalogued `Destructive` (NEEDS-4.3), so Navori AI's gate refuses it (NEEDS-4.11). In the run loop:
refuse unless the row is `open` or `failed` and `revision` matches (NEEDS-4.9, NEEDS-4.12); re-read the chosen
Envelope's Risk with `risk_of` — `None` → `failed` with "this action no longer exists" (NEEDS-4.7). Write
`state='answering', chosen=…` and `update_state` **before** dispatching (decision 4), reply `Ok(())` to the
caller, then `send` the Envelope and push its `Pending` into a `FuturesUnordered` arm keyed by row id (NEEDS-4.4,
NEEDS-2.2). The `Ok(())` reply goes only after the `answering` write has committed. On `Ok` → `state='answered',
resolved_at_ms=now` (History, NEEDS-4.5). Errors split by whether the Envelope may have run: `send` failing at once
(`UnknownModule`, `InvalidAction`, `Refused`, or `Unavailable` because the inbox is full) and `Action(_)` are
definite — nothing ran, or the target said it failed — and give `failed` with the error; `Timeout`, or
`Unavailable` after the request was queued, are uncertain and give `failed` with `<error> — it may already have
run`, so `Retry` is never a blind second run (NEEDS-4.7). Error text passes `redact_text` before it is stored
(ARCH-6). An `answer` arriving at or after the Decision's `expires_at_ms` is refused and the Decision expires at
once (NEEDS-4.9, NEEDS-5.2). A choice with no Envelope goes straight to `answered` (NEEDS-4.5). `Retry` is
an ordinary `answer` with the failed choice (NEEDS-4.8). At the top of `run`, every `answering` row becomes
`failed` with `Navori stopped before this finished — it may already have run` (NEEDS-4.10), and every open row's
choice Risk is recomputed from the live catalog into State (NEEDS-3.8). `tray()` returns
`attention: Some("N decisions waiting")` exactly while State holds an open, answering or failed Decision, and
nothing for Updates (NEEDS-3.4, NEEDS-11.9); it reads only State (`module.rs:63-77`).

Known limit: the store runs WAL with `synchronous=NORMAL` (`crates/store/src/lib.rs:89,98`), so a committed
`answering` row survives a process crash but may be lost on power loss; after a power cut mid-answer the Decision
can come back `open` although its Envelope ran. Accepted here, named in the report; `FULL` for this one write would
cost every Module's store latency.

### D. Expiry, Updates and their History (`modules/needs_you`)

Satisfies: NEEDS-5.1, NEEDS-5.2, NEEDS-5.3, NEEDS-5.4, NEEDS-5.5, NEEDS-6.3, NEEDS-6.4, NEEDS-7.2, NEEDS-12.2, NEEDS-12.3, NEEDS-12.4
Reuse: 2 — the Scheduler's `WaitHandle::wait_until(Option<SystemTime>) -> Wake` (`crates/kernel/src/scheduler.rs:112`), wake- and panel-aware with the closed-menu 30 s gap (KCONTRACT-4.9, 5.1, 6.1–6.3; `scheduler.rs:91,118-171`); `System`'s loop as template (`crates/kernel/src/system.rs:107-137`)
Respects: ARCH-16, ARCH-12
Interface:
```
needs_you.mark_viewed { ids: [i64] }   // Risk Write
needs_you.clear_updates                // Risk Write
needs_you.history                      // Risk Read → [HistoryEntry { kind, title, chosen_label?, resolved_at_ms }]
```
Depth: callers know "mark these seen", "clear all", "read History"; deadlines, expiry, the Update rows and pruning stay inside.
Locality: `modules/needs_you/src/expiry.rs`, `history.rs`; leave `crates/kernel/src/system.rs` (the Journal sweep is unchanged, NEEDS-13.4).

The loop's deadline is the earliest of: the soonest `expires_at_ms` of an `open` Decision (an `answering` one
does not expire, NEEDS-5.2), and the next History prune. On `Wake::Due` (also delivered at once after a wake
pulse past the deadline, KCONTRACT-6.1, and at `run` start for anything already past, NEEDS-5.4) each due row
becomes `expired` and a `needs_you_updates` row of kind `decision_expired`, title `Expired: <title>`, is added
(NEEDS-5.2). A `failed` Decision is unanswered with no answer running, so it expires too, as NEEDS-5.2 reads. No deadline while nothing expires and no prune is due → `wait_until(None)` parks (NEEDS-5.1;
KCONTRACT-4.2). No Action posts an Update (NEEDS-5.5). `mark_viewed` sets `resolution='viewed',
resolved_at_ms=now` for exactly the ids given (the Shell decides which, section H, NEEDS-6.2); State then omits
them from the list and every count and History keeps them (NEEDS-6.3). An Update has no click target, so a click
dispatches nothing (NEEDS-6.4). `clear_updates`
resolves every unresolved Update as `cleared` (NEEDS-7.2). `history` reads rows resolved within the retention
window (NEEDS-12.2's data). Prune runs at `run` start and then every 24 h: `DELETE … WHERE resolved_at_ms < now -
days` in small statements (the store is one blocking connection, `crates/store/src/lib.rs:77,119`) (NEEDS-12.3);
`JournalSweep::Paused` → no prune deadline at all (NEEDS-12.4).

### E. The To-do Module (`modules/todo`) and the local day (`crates/macos`)

Satisfies: NEEDS-8.4, NEEDS-8.6, NEEDS-8.7, NEEDS-8.8, NEEDS-9.1, NEEDS-9.2, NEEDS-9.4, NEEDS-10.2, NEEDS-12.5, NEEDS-12.6
Reuse: 2 — the same Module pattern as B; env injection like `Ports::new(PortsEnv)` (`crates/ffi/src/app.rs:271-273`); `time` 0.3 (rung 5, workspace dep) for `Date` arithmetic; `navori-macos` (rung 2, existing platform crate) gains one function
Respects: ARCH-3, ARCH-6, ARCH-9, ARCH-10, ARCH-12, ARCH-16
Security: THR-3, TB-6
Surface: `navori-macos` gains `pub fn local_utc_offset(at: SystemTime) -> i32` — new, no reader. Nothing else changes in that crate.
Interface:
```
todo.add { title, due_on?: "YYYY-MM-DD" }  // Write → Ok({ id })
todo.tick { id }  todo.undo { id }         // Write
todo.history                               // Read → [{ id, title, due_on?, done_at_ms }]
Todo::new(env: TodoEnv { calendar: Arc<dyn LocalCalendar> }, retention: JournalSweep)   // Module ID "todo"
trait LocalCalendar: Send + Sync { fn utc_offset(&self, at: SystemTime) -> i32 }       // fake in tests
TodoState { today: "YYYY-MM-DD", open: Vec<OpenTodo { id, title, due_on?, status: Overdue|Today|Soon|Later|None }> }
```
Depth: callers know four Actions and a status per To-do; storage, ordering inputs, midnight rollover and pruning stay inside.
Locality: new crate `modules/todo/` (same deps as B plus `time` with its `parsing` feature for `due_on`; `crates/ffi` takes `navori-macos` as a normal dependency for the real `LocalCalendar`); one function in `crates/macos/src/` beside the existing `unsafe` FFI; `build_kernel` registers it (extend).

`add` trims the title, refuses empty (NEEDS-8.5, enforced here as well as in the field), redacts it (NEEDS-8.6) and
inserts (NEEDS-8.4 data). Status per To-do is computed in the Module from `due_on` against today's local date,
where today = `wait.now()` shifted by `calendar.utc_offset(now)` re-read at each computation: before today Overdue,
today Today, 1–3 days Soon, ≥ 4 days Later, no date None (NEEDS-9.2). The loop's deadline is the next local
midnight (and the next prune); on `Wake::Due`, a wake pulse or `Wake::PanelChanged` it recomputes `today` and the
statuses and calls `update_state` only when they changed, so a snapshot is pushed (`snapshot_pump` reacts to
revision bumps, `crates/ffi/src/app.rs:576-`) (NEEDS-9.4). `tick` sets `done_at_ms` (NEEDS-10.1's data), `undo`
clears it (NEEDS-12.5). Restart reloads from `todo_items` (NEEDS-8.8). Prune and pause as in D (NEEDS-12.3/12.4
for To-dos).

Table, Owner `todo`:
```
todo_items(id INTEGER PRIMARY KEY, title TEXT NOT NULL, due_on TEXT, created_at_ms INTEGER NOT NULL, done_at_ms INTEGER)
```
`due_on` is a local calendar date with no time or zone, so a To-do due "Fri 13" stays Friday when the zone changes.
It is the field a later full TODO feature (ROAD-42) extends.

### F. The Needs you view in the Core (`crates/ffi`)

Satisfies: NEEDS-3.1, NEEDS-11.2, NEEDS-11.3, NEEDS-11.4, NEEDS-11.6, NEEDS-13.8
Reuse: 2 — the `running.rs` pure-function join (`crates/ffi/src/running.rs:96`) and `awake_view`'s state read with `module_is_down` overlay (`crates/ffi/src/app.rs:344-400`); pre-built request strings as `RunningPort.stop_request` (`running.rs:43,349`)
Respects: ARCH-1, ARCH-2
Surface:
- `CoreSnapshot` (`crates/ffi/src/lib.rs:187-209`) gains `needs_you: Option<NeedsYouView>` — readers: `snapshot()` (`lib.rs:575-623`, **replace**), the generated UniFFI Swift bindings (**replace**, regenerated by the build), `CoreStore.apply` and its computed properties (`apps/macos/Navori/Core/CoreStore.swift:456`, **replace**), the test `SnapshotListener`s and `status()` readers (`crates/ffi/tests/{agent,ask,integrations,config_privacy,actions_risk,status_lock,live_agent,core}.rs`, `apps/cli/tests/cli.rs`) — **replace**, compile unchanged (none builds a `CoreSnapshot` literal).
- `build_kernel` (`crates/ffi/src/app.rs:257-275`) registers `NeedsYou` and `Todo`, passes `JournalSweep` and the `TodoEnv`, and grants `needs_you` — reader: `NavoriCore::start_with` only (**replace**).
- `crates/ffi/tests/actions_risk.rs`: `SAMPLED` (`:63`), the `sample()` match (`:65-107`) and the per-module match (`:125-132`) — **replace**: add every `needs_you` and `todo` Action and two arms (NEEDS-13.8).
- A new `NavoriCore::needs_you_history() -> HistoryView` export — new, no reader.
Interface:
```
NeedsYouView { decisions: Vec<DecideRow>, todos: Vec<TodoRow>, updates: Vec<UpdateRow>, today: String,
               needs_count: u32, any_overdue: bool, all_caught_up: bool, down: bool }
DecideRow { id, title, reason, raised_at_ms, state: DecideState, primary: ChoiceView, secondary: ChoiceView }
ChoiceView { label, risk: Option<String>, answer_request: String }   // JSON for needs_you.answer
TodoRow { id, title, due_on: Option<String>, status: TodoStatus, tick_request: String }
UpdateRow { id, kind, title, created_at_ms }
fn build(needs: Option<&NeedsYouState>, todo: Option<&TodoState>) -> Option<NeedsYouView>   // pure
```
Depth: n/a — extends the `running.rs` join pattern.
Locality: new `crates/ffi/src/needs_you.rs`; extend `lib.rs` (`CoreSnapshot` field, `snapshot()`), `app.rs` (`build_kernel`, `needs_you_view`); `crates/ffi/Cargo.toml` adds the two module deps (the only crate allowed to, `xtask` check-deps).

`build` orders Decisions by earliest `expires_at_ms` (none last) then oldest `raised_at_ms` (NEEDS-3.1), To-dos by
`due_on` with Overdue first and no date last (NEEDS-9.1's order), Updates newest first (NEEDS-6.1's order);
`needs_count` = open To-dos + unviewed Updates (NEEDS-11.2); `any_overdue` (NEEDS-11.3); `all_caught_up` = no
Decision, no open To-do, no unviewed Update (NEEDS-11.4); the `All` list is To-dos then Updates (NEEDS-11.6). It
reads only Module State through `kernel.state(..)` — no store read in `snapshot()` for this view (NEEDS-14.1).
When either Module is down, `down` is set and the Shell shows the rows it last had greyed, like Awake's problem
overlay. `needs_you_history()` dispatches `needs_you.history` and `todo.history` through `Kernel::dispatch` and
merges them newest first (NEEDS-12.1's data). Dispatch errors from the Shell keep `Kernel::dispatch`'s behavior
(NEEDS-13.1).

### G. Level 0 in the Shell (`apps/macos`)

Satisfies: NEEDS-3.2, NEEDS-3.3, NEEDS-3.5, NEEDS-3.6, NEEDS-3.9, NEEDS-3.10, NEEDS-4.1, NEEDS-4.2, NEEDS-4.6, NEEDS-11.1, NEEDS-11.5, NEEDS-13.7
Reuse: 2 — `MenuRowView`/`MenuRow` (`apps/macos/Navori/Features/Panel/MenuRowView.swift:5-133`) for the `Needs you` row; `CapsuleButton` moved out of `#if DEBUG` (`Features/Preview/PreviewSampleRows.swift:40-73`) into `Features/Panel/`; `LevelZeroTree.rows` (`apps/macos/NavoriPanel/CoreRowState.swift:84-102`); `CoreStore.perform` pattern (`CoreStore.swift:441-454`); new `DecideRowView` (rung 7 — `MenuRowView` has no inline action slot, brief :33)
Respects: ARCH-1, ARCH-2
Surface:
- `PanelView` body (`PanelView.swift:9-24`) — **replace**: Decide block and `Needs you` row before `Running`, a separator after (NEEDS-11.1).
- `LevelZeroTree.rows(running:core:status:inDevProfile:)` (`CoreRowState.swift:84-102`) and `StatusBarController.menuTree()` / `onSnapshotChange` / `observeSnapshot` (`StatusBar/StatusBarController.swift:253-299`) — **replace**: Decide row ids, the `needs` node and its children join the tree and the change comparison.
- `MenuHitMap.rowId` (`apps/macos/NavoriPanel/MenuMetrics.swift:19-35`) and `StatusPanel.activateRow(at:)` (`StatusPanel.swift:299`) — **replace**: first-mouse hit-testing covers rows above `running`, and a Decide row reports its two button frames so a first mouse-up on a button answers that choice (a SwiftUI `Button` misses the first mouse-up in this panel); `CapsuleButton` gains a real action instead of `{}`.
- `CapsuleButton` readers: `PreviewSampleRows.swift`, `PreviewMenu.swift` — **replace**: import the moved component; Preview stays `#if DEBUG` (NEEDS-13.12).
- `StatusPanel.routeKey` (`StatusBar/StatusPanel.swift:283-297`) — **replace**: ⌘⌫ on a focused Decide row answers the secondary choice (NEEDS-4.2); every other ⌘ chord passes through as today.
Interface: `CoreStore.needsYou: NeedsYouView?`, `CoreStore.answer(_ request: String)`; views `DecideBlockView`, `DecideRowView`.
Depth: rung 7 for `DecideRowView` — callers know a `DecideRow` and a cascade; the hover/focus swap, spinner, Retry and VoiceOver string stay inside.
Tooltip and accessibility hint of a `Destructive` choice: `Runs <module>.<action> — changes things and cannot be undone` (NEEDS-3.10), from `ChoiceView.risk` and the Envelope.
Locality: new `Features/Panel/DecideRowView.swift`, `NeedsYouRow` in `PanelView.swift`; pure helpers (age text, VoiceOver string) in `apps/macos/NavoriPanel/NeedsYouText.swift`; extend `CoreStore.swift`, `StatusBarController.swift`, `CoreRowState.swift`, `MenuMetrics.swift`.

### H. The Needs you submenu, the calendar and History (`apps/macos`)

Satisfies: NEEDS-3.7, NEEDS-6.1, NEEDS-6.2, NEEDS-6.5, NEEDS-7.1, NEEDS-8.1, NEEDS-8.3, NEEDS-8.5, NEEDS-9.3, NEEDS-9.5, NEEDS-10.1, NEEDS-10.3, NEEDS-11.7, NEEDS-11.8, NEEDS-12.1
Reuse: 2 — `RunningSubmenuView` as template (`Features/Panel/RunningSubmenuView.swift:5-23`, `CappedScroll`, `MenuDivider`); the `awake.keep` depth-2 child precedent for the calendar and History (`StatusBarController.swift:301-309`); a new `UpdateRowView` drawn on the shape of the preview's `InboxRowView` (`PreviewSampleRows.swift:75-143`) — `InboxRowView` itself stays in the preview, which still reads it (`PreviewMenu.swift:126`), and keeps `PreviewSampleRows.marker` (`:142`), which must never reach Release code; AppKit `NSSegmentedControl`, `NSTextField`, `NSDatePicker` (rung 4) through `NSViewRepresentable`; new `TodoRowView`, `DuePill` (rung 7 — no kit row has a trailing pill, brief :49-50)
Respects: ARCH-1, ARCH-2
Surface:
- `StatusBarController.panel` submenu switch and `activate` switch (`StatusBarController.swift:21-32,311-330`) — **replace**: `needs`, `needs.date`, `needs.history` cases.
- `CascadeController.closeSubmenus` / `hideFrom` (`StatusBar/CascadeController.swift:103-111,246-250`) — **replace**: they call a new `onSubmenuClosed(id)` before hiding, so the store marks drawn Updates viewed; no other reader changes.
Interface: `NeedsYouSubmenuView(store:)`, `HistorySubmenuView(store:)`, `CoreStore.addTodo(title:dueOn:)`, `tick(_:)`, `undo(_:)`, `clearUpdates()`, `markViewed(_ ids:)`, `loadHistory()`.
Depth: rung 7 for `TodoRowView` / `DuePill` — callers know a `TodoRow` and `today`; the pill text, color mapping, strike-through hold and VoiceOver words stay inside.
Locality: new `Features/Panel/NeedsYouSubmenuView.swift`, `TodoRowView.swift`, `UpdateRowView.swift`, `AddTodoField.swift`, `DueCalendarView.swift`, `HistorySubmenuView.swift`; pure helpers `apps/macos/NavoriPanel/NeedsYouText.swift` (pill text, VoiceOver words) and `NeedsYouList.swift` (filter, drawn-row set); extend `CoreStore.swift`, `CascadeController.swift`.

The list, not the header, field or footer, sits in `CappedScroll`. The field's draft text and picked date live in
`CoreStore`, so closing and reopening the submenu keeps them until Navori quits. The submenu records, for each Update row, whether its frame intersects the scroll viewport (`onGeometryChange`),
keeping the set of drawn Update ids for the tab shown; `onSubmenuClosed("needs")` sends exactly that set through
`markViewed` (NEEDS-6.2). Nothing is sent if Navori stops first (NEEDS-6.5 by construction). Ticking strikes the
row through at once and dispatches `todo.tick`; the view keeps a transient copy for ~0.45 s after the snapshot
drops it (NEEDS-10.1). The date chip is a depth-2 cascade node `needs.date` hosting a graphical `NSDatePicker` with
`Today`, `Tomorrow`, `No date` buttons (NEEDS-8.3); `History ›` is the depth-2 node `needs.history`, which calls
`CoreStore.loadHistory()` on open (NEEDS-12.1) and offers `Undo` on To-do entries only (NEEDS-12.2, NEEDS-12.5).

### I. Key window and keys in the cascade (`apps/macos/Navori/StatusBar`)

Satisfies: NEEDS-8.2, NEEDS-13.11, NEEDS-13.13
Reuse: 4 — AppKit `NSPanel.becomesKeyOnlyIfNeeded` and `NSView.needsPanelToBecomeKey` (research note); `StatusPanel.routeKey`'s text-field pass-through (`StatusPanel.swift:290-293`)
Surface:
- `CascadePanel.canBecomeKey` (`CascadePanel.swift:57`) — **replace**: `isVoiceOverEnabled || acceptsTyping`, with `acceptsTyping` set only on the panel hosting `needs` or `needs.date`, and `becomesKeyOnlyIfNeeded = true` so a click elsewhere in it does not take key (NEEDS-13.11).
- `CascadePanel.routeKey` (`:77-86`) — **replace**: pass keys through while the first responder is an `NSText` (Esc still closes), so typing and Return-to-add reach the field (NEEDS-8.2).
- `StatusPanel.resignKey` non-VoiceOver branch (`:177-188`) — **replace**: defer the check to the next main-queue turn and close only when key left the cascade (`cascade.keyIsInside` false), exactly as the VoiceOver branch does; a window outside the menu becoming key still closes it (NEEDS-13.13).
- `CascadeController.hideFrom` / `closeSubmenus` (`CascadeController.swift:103-111,246-250`) — **replace**: when the panel being hidden is key, make the level-0 `StatusPanel` key again before `orderOut`, so arrows and Esc keep working on level 0.
Interface: `CascadePanel.acceptsTyping: Bool`.
Depth: n/a — extends `CascadePanel` and `StatusPanel`.
Locality: extend `CascadePanel.swift`, `StatusPanel.swift`; leave every other submenu (they never set `acceptsTyping`).

The first build task is a throwaway `run-spike` that proves, in a scratch target, three unverified facts: an
editable `NSTextField` hosted through SwiftUI makes a `becomesKeyOnlyIfNeeded` panel key; moving key into the
cascade keeps the level-0 panel open with the change above; and a graphical `NSDatePicker` in a depth-2 cascade
takes clicks and arrow keys. A failure there reopens decision 9 before any view is built.

### J. Documents and amendments

Satisfies: NEEDS-13.10, NEEDS-13.12 (infrastructure: records the amendments other requirements make)
Reuse: 2 — the "(amended by …)" convention (`docs/specs/2026-11-02-menubar-visual-foundation/requirements.md:33-35`)
Surface (docs with readers — each **replace**):
- MENU-1.2 and AWAKE-11.1: append "(amended by NEEDS-11.1)" — level 0 order.
- MENU-1.4: append "(amended by NEEDS-3.9)" — Needs you and `You're all caught up` ship in Release; the Ask field, Ports and Pull Requests rows stay hidden (NEEDS-13.10).
- `docs/design/2026-11-01-menubar-ui-brief.md` rule 6 (`:116`, "at most 3 inbox items") and rule 12 (`:122`) — amendment notes naming NEEDS-3.1 and NEEDS-3.9.
- `docs/architecture/data.md:37` `inbox_*` → rows `needs_you_decisions`, `needs_you_updates` (Owner `needs_you`) and `todo_items` (Owner `todo`).
- `docs/codebase/modules.md:50` `navori-mod-inbox` → `navori-mod-needs-you`, `navori-mod-todo`; `docs/codebase/map.md:44` `inbox/` → `needs_you/`, `todo/`.
Interface: none (documents).
Depth: n/a — documents.
Locality: the files above only.

### K. The test producer

Satisfies: none — infrastructure (test code; it proves section B's raise through a dispatcher)
Reuse: 2 — the counter test Module of `crates/kernel/tests/kernel.rs:1-60` and the kernel harness `Kernel::builder(Store::open_in_memory()).pause_journal_sweep()` (`crates/ffi/tests/awake.rs:23-42`); the test-local `FakeClock` pattern (`crates/kernel/tests/scheduler.rs:38-72`)
Interface: a `Producer` test Module that raises through `ctx.dispatcher()` and a `Target` test Module that records each Envelope it receives, registered with `NeedsYou` in `modules/needs_you/tests/`.
Depth: n/a — test code.
Locality: `modules/needs_you/tests/producer.rs`; no production code.

## UI design

Grounding: lifts `docs/design/2026-11-08-needs-you-ui-brief.md` (brief D, locked 2026-11-08) Surfaces 1 and 2 one to
one; tokens are macOS system semantics as in `docs/design/2026-11-01-menubar-ui-brief.md` plus `systemYellow`
(brief D Amendment 4). Nothing here re-decides the brief; ordering and History content come from the requirements.

### Level 0 — Decide block and `Needs you` row

Layout: banners · `Decide N` header · one Decide row per open Decision · `Needs you` row · separator · `Running` · Core row (MENU-3.11) · separator · `Settings… ⌘,` · `About Navori` · `Quit Navori ⌘Q`; level-0 width 360 pt (`MenuMetrics.level0Width`), padding `MenuMetrics.pad`.
Components: `Decide N` header — rung 2, the Running submenu's section header (12 pt semibold, secondary label, count right); Decide row — new (rung 7) `DecideRowView`, one line, 26 pt (`MenuMetrics.rowHeight`): accent lead icon · title (truncates) · age right; on hover or focus two mini `CapsuleButton`s (20 pt, 11.5 pt medium; primary accent, secondary fill) replace the age; `Needs you` row — rung 2 `MenuRowView`, icon `sparkle`, value = `needs_count`, red `StatusDot` when `any_overdue`, chevron.
States: no Decision (header and rows absent) · 1–N Decisions (no cap) · hover/focus (buttons replace the age) · answer running (spinner in the chosen button, other disabled) · failed (`Retry` as primary; error in tooltip and accessibility value) · restarted mid-answer (failed, with `Navori stopped before this finished — it may already have run`) · all caught up (`✓ You're all caught up` replaces the Decide block; `Needs you` shows no count) · Module down (rows as last shown, dimmed).
Type & color: Decide title 14 pt medium; lead icon `controlAccentColor`; age 12 pt `secondaryLabelColor`; selected row `selectedContentBackgroundColor`, primary button white with accent text, secondary 22% white.
A11y: ↑↓ reach each Decide row; focus reveals its buttons; Return = primary, ⌘⌫ = secondary; VoiceOver reads `Decision: <title>, <reason>, <age>, <primary>, <secondary>`; the full reason is the tooltip and accessibility value.

### Needs you submenu

Layout: about 350 pt wide: header `Needs you N` · segmented `All · To-dos n · Updates n` · `Add a to-do…` field with date chip · separator · list · separator · `Clear updates` (Updates tab only) · `History ›`. List order: To-dos by due date (Overdue first, no date last), then Updates newest first. No Decision appears here.
Components: filter — rung 4 `NSSegmentedControl` (small); field — rung 4 `NSTextField` in a rounded fill; date chip — rung 2 `CapsuleButton` (fill; accent when a date is set); calendar — rung 4 `NSDatePicker` (graphical) in the depth-2 cascade `needs.date` with `Today` · `Tomorrow` · `No date`; To-do row — new (rung 7) `TodoRowView`, one line, 26 pt: checkbox (`NSButton` checkbox look) · title · `DuePill`; date pill — new (rung 7) `DuePill`; Update row — rung 2 `UpdateRowView` (the former `InboxRowView`, two lines, `bell` lead, dimmer); `History ›` — rung 2 `MenuRowView` with chevron.
States: date pill — Overdue `systemRed` · Today `systemOrange` · Soon (1–3 days) `systemYellow` · Later (≥ 4 days) `systemBlue` · None fill with `secondaryLabelColor`; pill text `Today`, `Tomorrow`, weekday for 2–6 days ahead, `Wkd D` otherwise, `No date`; ticked To-do (strikethrough, leaves after ~0.45 s); empty tab (`✓ You're all caught up`, field still shown); field focused (typing; Return adds; Esc closes); Module down (rows dimmed).
Type & color: row title 14 pt; pill 11.5 pt semibold, 8 pt radius, fill = the color at 16–22% with the color as text (`systemYellow` text darker in light mode); segmented 12 pt medium; on a selected row the pill is 22% white with white text.
A11y: status never by color alone; the pill reads `Overdue, Thursday 5 November` · `Due today` · `Due soon, Sunday 8 November` · `Due later, Friday 13 November` · `No due date`; the checkbox reads `Mark done`; the filter is a tab group; the field precedes the list in the Tab order.

### History submenu

Layout: depth-2 cascade from `History ›`, same width as the Needs you submenu; newest first; one line per entry: ticked To-dos with `Undo`, answered Decisions with the label chosen, viewed or cleared Updates.
Components: rung 2 `MenuRowView` rows; `Undo` — rung 2 `CapsuleButton` (fill).
States: loading (the rows the last load returned, or `Loading…` on the first open) · empty (`Nothing in the last N days`, N = retention days) · error (the error as one dimmed line).
Type & color: as the Needs you submenu; resolved entries in `secondaryLabelColor`.
A11y: ↑↓ through entries; `Undo` in the row's Tab order; VoiceOver reads kind, title and outcome.

## Seams for testing

| Seam | Kind | Covers |
|---|---|---|
| `ModuleDispatcher` through a `Kernel` built with test Modules (`crates/kernel/tests/dispatcher.rs`) | integration | NEEDS-2.1, 2.2, 2.3, 2.4, 2.5, 2.6 |
| `needs_you` through `Kernel::dispatch` + `Kernel::state` + a `FakeClock`, with the `Producer`/`Target` test Modules (`modules/needs_you/tests/`) | integration | NEEDS-1.1–1.12, 2.7, 3.4, 3.8, 11.9 (tray), 4.3, 4.4, 4.5, 4.7, 4.8, 4.9, 4.10, 4.12, 5.1–5.5, 6.3, 6.4, 6.5, 7.2, 12.2, 12.3, 12.4 (Decisions/Updates), 14.3 (Shield), 14.4 (restart, migration) |
| `todo` through `Kernel::dispatch` + `Kernel::state` + `FakeClock` + a fake `LocalCalendar` (`modules/todo/tests/`) | integration | NEEDS-8.4 (data), 8.5, 8.6, 8.7, 8.8, 9.2, 9.4, 10.1 (data), 10.2, 12.3, 12.4 (To-dos), 12.5, 12.6, 14.3, 14.4 |
| `navori-macos` `local_utc_offset` against the system zone (`crates/macos/tests/`) | integration | NEEDS-9.2 (offset input) |
| `ffi::needs_you::build` pure join (`crates/ffi/tests/needs_you_view.rs` or unit) | unit | NEEDS-3.1, 3.7 (no Decision in the submenu lists), 11.2, 11.3, 11.4, 11.6, 9.1 (order), 6.1 (order) |
| `NavoriCore` with the real `build_kernel` (`crates/ffi/tests/actions_risk.rs`, `core.rs`) | integration | NEEDS-13.8, 12.1 (history merge via `needs_you_history`), 13.1, 13.2 (kept green) |
| Navori AI's tool gate with `needs_you.answer` in the toolset (`ai/agent/tests/`) | integration | NEEDS-4.11, 13.5 |
| `NavoriPanel` pure Swift (`apps/macos/Tests/NavoriPanelTests/`) | unit | NEEDS-3.6 (VoiceOver string), 3.10 (Destructive hint text), 9.3 (pill text), 9.5 (pill words), 10.3, 6.2 (drawn-row set), 11.6 (filter), 11.1 (`LevelZeroTree` order), 4.2 (key map) |
| Existing suites kept green: `crates/kernel/tests/{kernel,scheduler,tray}.rs`, `modules/awake/tests/`, `crates/ffi/tests/core.rs`, `running.rs` | integration | NEEDS-13.1, 13.2, 13.3, 13.4, 13.6, 13.9 |
| Release-build check: `strings` on the Release binary has no `PreviewSampleRows.marker` | integration (build script step) | NEEDS-13.10, 13.12 |
| Throwaway `run-spike` (scratch target, deleted) | spike | decision 9 facts behind NEEDS-8.2, 8.3, 13.11, 13.13 |
| Dogfood walk in the running dev app (`docs/agents/verify.md` Launch / Drive / Evidence) | e2e (manual) | NEEDS-3.2, 3.3, 3.5, 3.9, 4.1, 4.2, 4.6, 6.1, 6.2, 6.6, 7.1, 8.1, 8.2, 8.3, 9.1, 10.1, 10.3, 11.1, 11.5, 11.7, 11.8, 12.1, 13.7, 13.11, 13.13, 14.5 |
| Release perf run, once before release (`cargo xtask perf`) — not per feature | perf (release) | NEEDS-14.1, 14.2 |

## Upstream sync-back

Changed in `requirements.md` while designing (to be re-approved with this design):
- NEEDS-13.11 widened to "neither the `Add a to-do…` field nor the date calendar has focus" — the calendar cascade
  must take key for keyboard use of `NSDatePicker`.
- NEEDS-13.13 added (guard): a window outside the menu becoming key still closes it — section I changes
  `StatusPanel.resignKey`, which had no guard; file list gains `StatusPanel.swift`.
- `expires_at` renamed `expires_at_ms` (Unix ms) in NEEDS-1.1, 3.1, 5.1–5.4 — the payload is permanent under Compat
  External and the repo names times `*_ms`.
- NEEDS-4.12 added *(agent-decided)*: an answer naming a Decision version a re-raise replaced is refused — the
  `revision` in section C.
- NEEDS-1.12 added *(agent-decided)*, from the design review: a raise whose Envelope holds text the Shield would
  redact is refused — ARCH-6 covers the stored Envelope, and redacting it would change what runs.
- NEEDS-3.10 added *(agent-decided)*, from the design review: a `Destructive` choice's tooltip and accessibility
  hint name the Action it runs — the button label is the raising Module's text.

## Coverage check

Checked by script: each of the 102 IDs in `requirements.md` appears in exactly one `Satisfies:` line, except guards
NEEDS-13.1–13.6 and 13.9 and NFRs NEEDS-14.1–14.5, which no section changes — they are kept by leaving the guarded
code alone or measured as a whole, and map to rows of the seams table. Where a second section carries part of a
behavior, its text says so: NEEDS-6.2 (Shell picks the set, H; Core records it, D), NEEDS-10.1 (Core data, E; the
0.45 s hold, H), NEEDS-12.1 (Core merge, F; list, H), NEEDS-8.4 (Core add, E; field, H), NEEDS-9.1 (order computed
in F). Deliberately unmapped: none.
