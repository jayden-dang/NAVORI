# Tasks: Harness hooks

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: HOOKS
Status: In-progress
Date: 2026-11-03
Approved by the operator ("approve", 2026-11-03, on the review page at c2da2f5, no card selected).
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

**Goal:** Make the Mac stay awake while Claude Code, Codex or Grok Build works, by adding `navori hook`, an Integrations Module that installs, repairs and removes Navori's hook entries, a `navori doctor` row and a Settings → Integrations tab.

**Architecture:** `navori hook` is a thin command in `apps/cli` that maps one hook event to an existing Awake Action and always exits 0. A new Module crate, `navori-mod-integrations`, owns the pure hook-file model and the file writes, and answers four Actions; `navori-ffi` registers it and `Inner::doctor` calls its pure `inspect`. The Shell tab calls those Actions through `CoreStore.dispatch` and holds no rules.

**Tech Stack:** Rust 2024, clap 4, tokio (current-thread in the hook), serde_json with `preserve_order`, tempfile, schemars, UniFFI 0.32, Swift (SwiftUI). No new third-party dependency: `tempfile` and `serde_json` are already in the workspace.

**Delivery:** If Task 12 stays unverified, Task 13 does not run and the end report lists HOOKS-3.10 and HOOKS-4.1 for Codex as **unmet**, not done. Tasks 1–10 are the build. Tasks 11–12 are research with operator-recorded input and are not run live by an agent. Task 13 waits on Task 12. Task 14 is docs and the final guard check. Task 15 is operator dogfood and is not part of any build wave.

## Global Constraints

Source of truth (referenced, not copied — read before every task):

| Source | Content hash (`git hash-object`, first 12) |
|---|---|
| `docs/standards/INDEX.md` | `f83e09733ee4` |
| `docs/standards/testing.md` | `b1a9b42fc1d3` |
| `docs/standards/errors-logging.md` | `f84b3ceeea11` |
| `docs/standards/performance.md` | `ebf1670ff150` |
| `docs/architecture/INDEX.md` (ARCH-N) | `2e318bd23ff5` |

Compact delta every task inherits:

- **Source classes** — every value in `requirements.md` and `design.md` is a Lock, Weigh, agent-decided or Unverified. A task never promotes one to the operator's choice, and a code comment never states a Weigh or agent-decided value as decided.
- **Charter hard stops** — no test or step may change a real `~/.claude`, `~/.codex`, `~/.grok`, the real dev app's data directory or any file outside this worktree and a temp directory, and no test reads them: tests use a temp home. A research step may read such a file read-only and says so in its note. Do not run `cargo xtask run`, `cargo xtask install` or a live Harness. Every test builds a short temp home (`tempfile::Builder::new().tempdir_in("/tmp")`).
- **ARCH-1, ARCH-2** — logic is Rust; the new Swift files do not import `NavoriCore`.
- **ARCH-3** — `navori-mod-integrations` depends on `navori-kernel` and third-party crates only, never another Module, `navori-ffi` or an `ai` crate. `navori-cli` stays on `navori-config`, `navori-ipc`, `navori-protocol`. `cargo xtask check-deps` is part of `cargo xtask ci`.
- **ARCH-9** — no `unsafe`.
- **ARCH-13, ARCH-14, ARCH-16** — a bad Hook file is a state, never a stop; profiles never share a socket or an entry; no timer and no read while idle.
- **ARCH-6, THR-3** — Events and logs never contain a Hook file's contents, a prompt, an argv or a cwd.
- **THR-6, THR-9, TB-7** — see `docs/security/threat-model.md`.
- **Testing** — test-first. Tests go in each crate's `tests/` and use the public API; the `Navori` Swift app target is verified by building and a manual walk. Fake only at system boundaries. Names are sentences in domain language. No requirement IDs in source or test names.
- **Lints** — no `unwrap`/`expect` outside tests; `missing_debug_implementations` is on and CI denies warnings.
- **Commands** — task gate: `cargo nextest run -p <crate> --test <file>`. Before each commit: `cargo xtask ci` (it also compiles the Swift Shell and runs `swift test`).
- **Commits** — Conventional Commits, English, one logical change, every commit builds. In this run commits are local only: no push, no PR, no issue.
- **Team** — Solo band (`docs/agents/project.md`).

Consulted: `docs/codebase/map.md` for the module layout (`lib.rs`, `action.rs`, `state.rs`, `event.rs`, `domain/`, `infra/`; `actor.rs` is folded into `lib.rs` because the Module is a small request loop) and `docs/codebase/dependencies.md`, which allows every edge below. `design.md` Locality lines match.
_Requirements: HOOKS-10.4, HOOKS-10.5, HOOKS-2.6_

## File Structure

Create:
- `apps/cli/src/hook.rs` — the pure event map (`plan`) and the bounded hook run.
- `modules/integrations/Cargo.toml`, `src/lib.rs` (`Integrations`, `IntegrationsEnv`, `inspect`, the request loop), `src/action.rs` (`IntegrationsAction`), `src/state.rs` (`IntegrationsState`, `HarnessStatus`), `src/event.rs` (kinds and summaries) — the Module.
- `modules/integrations/src/domain/mod.rs`, `hookfile.rs` (desired set, `classify`, `converge`, `strip`), `identity.rs` (command grammar for one profile) — pure, no I/O.
- `modules/integrations/src/infra/mod.rs`, `files.rs` (read, resolve symlink, backup, atomic write, delete) — all disk access.
- `modules/integrations/tests/common/mod.rs`, `status.rs`, `install.rs`, `repair.rs`, `remove.rs`, `fixtures/*.json` — seam tests over a temp home.
- `crates/ffi/tests/hook_session.rs` — a fake-platform Kernel test for a new prompt after Release all.
- `crates/ffi/tests/integrations.rs` — the doctor row through the Core.
- `apps/macos/Navori/Features/Settings/IntegrationsModel.swift`, `IntegrationsTab.swift` — the tab.
- `docs/features/integrations.md` — shipped behavior.

Modify:
- `Cargo.toml`, `Cargo.lock` — workspace member, path dependency, `serde_json` `preserve_order`.
- `apps/cli/src/main.rs` — `Command::Hook` and the pre-clap branch. `apps/cli/tests/cli.rs` — hook tests, and `start_core` gains `helper_path`.
- `crates/ffi/Cargo.toml`, `crates/ffi/src/lib.rs` (`CoreConfig.helper_path`, `Inner` env), `crates/ffi/src/app.rs` (`build_kernel` argument, doctor row), `crates/ffi/tests/core.rs`, `crates/ffi/tests/config_privacy.rs` (`CoreConfig` literals).
- `apps/macos/Navori/Core/CoreStore.swift` (`helper_path` argument), `apps/macos/Navori/Features/Settings/SettingsView.swift` (fourth tab, Diagnostics hint).
- `docs/codebase/modules.md`, `docs/codebase/dependencies.md`, `docs/codebase/map.md`, `docs/architecture/integrations.md`, `docs/features/awake.md`.

Seam reconcile: `design.md` lists `hook::plan` as a pure unit seam. `apps/cli` is a binary crate and the standards allow tests only through a public API, so Tasks 2–3 observe the same behavior at the binary: `navori hook` against a recording socket server in a temp home, one table row per case. The behavior and the requirement IDs covered are unchanged.

Retrieval (`load-subgraph`, schema 1.1, recipe `fsubr-1.2`, run once on this map at this tree; OWNS coverage 6/11, since CORE, SHELL, CLI and BUILD have no Files blocks): `blast_radius` → AWAKE, KCONTRACT, ICON on `apps/cli/src/main.rs`, `apps/cli/tests/cli.rs`, `crates/ffi/src/lib.rs`, `crates/ffi/src/app.rs`, `crates/ffi/tests/core.rs`; AWAKE and MENU on `SettingsView.swift`; PERF, AWAKE, KCONTRACT, ICON, MENU on `CoreStore.swift`; AWAKE, KCONTRACT, PROCS on `Cargo.toml` and `docs/codebase/modules.md`; AWAKE on `docs/features/awake.md`. `cluster(HOOKS)` → focus only, because HOOKS had no OWNS before this file. Observations: none. All neighbours are finished work. Advisory.

---

### Task 1: Keep the order of JSON object keys (check C4)

**Files:**
- Modify: `Cargo.toml`, `Cargo.lock`
- Test: none new; a throwaway `apps/cli/tests/scratch_order.rs` that is deleted before the commit

**Reuse:** rung 5 — the already-installed `serde_json = "1.0.154"` (`Cargo.toml:49`) with its `preserve_order` feature; `indexmap` is already in `Cargo.lock` through `toml` and `uniffi_bindgen`.

**Interfaces:**
- Consumes: none
- Produces: `serde_json::Map` keeps insertion order workspace-wide; `Map::shift_remove` is available

**Depends-on:** none

- [x] **Step 1: Capture the output before.** In the scratch test, start `running_core()` (`apps/cli/tests/cli.rs` pattern) over a temp home and write the stdout of `navori actions`, `navori dispatch awake '"status"'` and `navori status --json` to `/tmp/hooks-order-before/`. Run: `cargo nextest run -p navori-cli --test scratch_order` — expect: pass, three files written.
- [x] **Step 2: Enable the feature.** Set `serde_json = { version = "1.0.154", features = ["preserve_order"] }` in `[workspace.dependencies]`. Run: `cargo build --workspace` — expect: builds; `git diff Cargo.lock` shows `indexmap` added under `serde_json` and nothing else new.
- [x] **Step 3: Capture after and diff.** Point the scratch test at `/tmp/hooks-order-after/`, run it, then `diff -r /tmp/hooks-order-before /tmp/hooks-order-after`. Normalise the volatile fields first (pid, uptime, the temp home and socket paths in `status --json`), then write down every differing line. Expect: differences only in key order, never in values; check that the `actions` schemas (schemars) did not change in content.
- [x] **Step 4: Check stored JSON.** Run `rg -n "kv_set|to_string|sha|hash" crates/store/src crates/kernel/src` and read each hit that serializes a `serde_json::Value` into the database or into a hash. Confirm that reading rows written before the change still works (object reads do not depend on order) and that no stored text or hash is compared against a re-serialized value. **If any comparison or hash depends on sorted order, stop: revert Step 2 and report; decision 7 falls back and a persisted-format change is a charter hard stop.**
- [x] **Step 5: Run the suite.** Run: `cargo xtask ci` — expect: pass. Update only fixtures that assumed sorted output, and list each in the commit body.
- [x] **Step 6: Delete the scratch test and commit** `git commit -m "build: keep JSON object key order so hook files are edited in place"`

_Requirements: HOOKS-4.8_

---

### Task 2: A prompt starts a Session Lease and a stop ends it (`navori hook`, event map)

**Files:**
- Create: `apps/cli/src/hook.rs`, `crates/ffi/tests/hook_session.rs`
- Modify: `apps/cli/src/main.rs`
- Test: `apps/cli/tests/cli.rs`

**Reuse:** rung 2 — `Client::connect`/`call`, `socket_path`, `method::DISPATCH`, the clap `Command` enum (`apps/cli/src/main.rs:32, 140, 248`); rung 3 — `std::os::unix::process::parent_id`, `std::process::id`; for the new-prompt test, rung 2 — the fake-platform kernel in `crates/ffi/tests/awake.rs` (`start_kernel`).

**Interfaces:**
- Consumes: the Awake Action JSON `{"hold_session":{"session_id","pid","ppid"}}`, `{"stop_session":{"session_id"}}`, `{"end_session":{"session_id"}}` as literals (the CLI cannot import the Module)
- Produces: `hook::plan(stdin: &[u8], own_pid: u32, parent_pid: u32) -> Option<Plan>` (design.md names `plan(stdin)`; the pids are passed in so the function stays pure); `Plan { action: serde_json::Value, deadline: Duration }`; `navori hook`

**Depends-on:** none

- [x] **Step 1: Write failing tests** in `apps/cli/tests/cli.rs`, each starting a recording server (a `UnixListener` at `navori_config::Paths::new(Profile::Dev, home, uid).socket`, answering with the request's own `id` as `{"jsonrpc":"2.0","id":<id>,"result":{}}` and storing the request; it creates the socket's parent `run/` directory first, and the error reply shape is `{"jsonrpc":"2.0","id":<id>,"error":{"code":-32000,"message":"…"}}`): `a_prompt_submit_sends_hold_session_with_its_own_pids`, `a_stop_without_a_reason_sends_stop_session`, `a_stop_with_reason_end_turn_sends_stop_session`, `a_stop_with_another_reason_sends_nothing` (the `reason` rule is the Grok-verified one, `10-hooks.md:677, 703`; applying it to Claude Code and Codex is provisional until Task 11 and is labelled so in a code comment), `a_stop_cancelled_sends_stop_session`, `a_stop_failure_sends_stop_session`, `a_session_end_sends_end_session_with_no_pid`, `an_unknown_event_sends_nothing`, `a_payload_sends_nothing_but_the_session_id_and_pids` (a prompt text field in the payload never appears in the recorded request), `a_grok_payload_with_both_key_spellings_is_read_by_the_snake_case_keys`. Run the binary with `HOME` set to the temp home, `--profile dev hook` and the payload on stdin; every row also asserts exit 0 and empty stdout and stderr.
Run: `cargo nextest run -p navori-cli --test cli` — expect: unrecognized subcommand `hook`.
- [x] **Step 2: Implement** `hook.rs`: parse only `hook_event_name`, `session_id`, `reason` with serde (unknown fields ignored); map per `design.md` ("`navori hook` command", step 2); `i32::try_from` on both pids, nothing sent if either fails; deadline 0.7 s for `SessionEnd`, 1.5 s otherwise. Add `Command::Hook` to the clap enum with no arguments (and a `Command::Hook => unreachable!()` arm in the exhaustive `run()` match, since `main()` handles it first), and a minimal `run_hook` that reads all of stdin, builds the plan and makes one `core.dispatch` call with `module: "awake"`. Bounds and failure handling come in Task 3.
Run: same command — expect: pass.
- [x] **Step 3: Write a characterisation test** in `crates/ffi/tests/hook_session.rs`: `a_new_prompt_after_release_all_makes_a_new_lease` — `hold_session`, `release_all`, `hold_session` again for the same id; expect one Lease and two `awake.lease_started` Events (`modules/awake/src/event.rs`). It describes shipped Awake behavior, so it is expected to pass at once: there is no red step. Copy the private `start_kernel` helper from `crates/ffi/tests/awake.rs:22` (imports `navori_macos::fake::{FakeExitsHandle, FakePower, FakeProcesses}`, `navori_mod_awake::{Awake, ExitsFactory, Platform, Timing}`, `navori_store::Store`; the ffi dev-dependencies already cover them). Run: `cargo nextest run -p navori-ffi --test hook_session` — expect: pass. If it fails, that is an Awake behavior gap: stop and report, do not edit `modules/awake/`.
- [x] **Step 4: Confirm the Awake tests that carry the duplicate and exit cases** by running `cargo nextest run -p navori-mod-awake --test session` — expect: `a_repeated_hold_keeps_one_lease_moves_the_deadline_and_keeps_the_first_hold`, `the_harness_exiting_ends_the_lease` and `the_session_ttl_ends_the_lease_8_hours_after_the_latest_hold` pass.
- [x] **Step 5: Run** `cargo xtask check-deps` — expect: pass, the CLI still on `navori-config`, `navori-ipc`, `navori-protocol`. Run `cargo xtask ci` — expect: pass.
- [x] **Step 6: Commit** `git commit -m "feat(cli): add navori hook to turn Harness events into session Lease Actions"`

_Requirements: HOOKS-1.1, HOOKS-1.2, HOOKS-1.3, HOOKS-1.4, HOOKS-1.5, HOOKS-1.6, HOOKS-1.7, HOOKS-1.8, HOOKS-1.9, HOOKS-1.10, HOOKS-2.5, HOOKS-2.6, HOOKS-10.3_

---

### Task 3: A hook never blocks a Harness

**Files:**
- Modify: `apps/cli/src/hook.rs`, `apps/cli/src/main.rs`
- Test: `apps/cli/tests/cli.rs`

**Reuse:** rung 2 — the fail-open `ask_for_command_lease` pattern and its `tokio::time::timeout` (`apps/cli/src/main.rs:283-318`); rung 3 — `std::thread`, `std::sync::mpsc::recv_timeout`, `std::panic::catch_unwind`, `std::process::exit`.

**Interfaces:**
- Consumes: `hook::plan`, `Client`, `socket_path` from Task 2
- Produces: the bounded run — a raw-argument branch before `Cli::parse`; exit 0 in every case; profile from `--profile dev` only

**Depends-on:** Task 2

- [x] **Step 1: Write failing tests** in `apps/cli/tests/cli.rs`: `a_core_that_is_not_running_gives_exit_0_and_no_output`; `a_core_that_never_answers_gives_exit_0_within_two_seconds` (a listener that accepts and never replies); `a_session_end_gives_up_within_one_second_when_the_core_never_answers`; `stdin_that_never_closes_gives_exit_0_within_two_seconds` (piped stdin kept open); `invalid_json_gives_exit_0_and_no_output`; `a_missing_session_id_gives_exit_0_and_no_request`; `a_reply_of_invalid_for_a_non_harness_pid_gives_exit_0` (recording server replies with a JSON-RPC error); `a_bad_profile_variable_gives_exit_0_and_no_output` (`NAVORI_PROFILE=bogus`); `a_profile_variable_of_dev_does_not_send_a_release_hook_to_the_dev_core` (two recording servers, release and dev; no `--profile` flag, env `dev`: only the release server gets the request); `a_profile_flag_dev_reaches_only_the_dev_core`; `awake_run_with_an_argument_named_hook_still_runs_its_command` (`navori awake run -- echo hook` prints `hook` and returns the command's exit code); `other_commands_keep_their_exit_codes` (`status` with the app down exits 3, a bad flag exits 2).
Run: `cargo nextest run -p navori-cli --test cli` — expect: failures on exit codes, timing and the env cases.
- [x] **Step 2: Implement the bounded run.** In `main()`, before `Cli::parse`, look at the raw arguments: skip an optional `--profile <v>` or `--profile=<v>`; if the next token is `hook`, take the hook path and never call clap. On that path: take `start = Instant::now()`; the profile is `dev` only when the flag says `dev`, else `release`, and `NAVORI_PROFILE` is not read; install a panic hook that prints nothing; wrap the body in `catch_unwind`; read stdin on a `std::thread` (capped at 16 MiB with `take`) and wait `recv_timeout(0.7 s)`; build the plan; build a current-thread runtime and run `timeout(start + plan.deadline − now, connect + call)`; drop every error; end with `std::process::exit(0)`. Keep `Command::Hook` in the enum so `--help` lists it.
Run: same command — expect: pass.
- [x] **Step 3: Write a failing test and make it pass** for `a_hook_dispatch_is_a_single_awake_call_per_event` (exactly one request recorded per run).
- [x] **Step 4: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(cli): make navori hook fail silent, bounded and profile-pinned"`

_Requirements: HOOKS-2.1, HOOKS-2.2, HOOKS-2.3, HOOKS-2.4, HOOKS-8.4, HOOKS-9.1_

---

### Task 4: Measure the hook's start cost (check C3)

**Files:**
- Create: `/Users/jayden/Developer/experiments/NAVORI/.skills/_pending-harness-hooks/auto/measure-hook.md` (the run directory; not tracked)
- Test: a throwaway `apps/cli/tests/scratch_cost.rs`, deleted before finishing

**Reuse:** rung 2 — the recording-server helper from Task 2 in `apps/cli/tests/cli.rs`

**Interfaces:**
- Consumes: the release-built `navori`, the recording server
- Produces: measured median and p95 of one `navori hook` run, recorded in the run directory's `measure-hook.md`

**Depends-on:** Task 3

- [x] **Step 1: Build** `cargo build --release -p navori-cli` — expect: builds.
- [x] **Step 2: Measure.** In the scratch test, over a temp home, run `target/release/navori` explicitly (`assert_cmd::cargo_bin` returns the debug binary) 50 times with a `UserPromptSubmit` payload against a recording server that replies at once, and 50 times with the server absent. Record median and p95 wall time of each set. Never touch the real dev app or its data directory.
- [x] **Step 3: Compare.** The deadline for `SessionEnd` is 0.7 s (HOOKS-2.3, agent-decided). **If the p95 against an answering server exceeds 350 ms (half of that deadline, agent-decided), stop and report: HOOKS-2.3 needs a decision.** Otherwise write the numbers and the machine model into `measure-hook.md` in the run directory.
- [x] **Step 4: Delete the scratch test.** No commit is needed when nothing tracked changed.

_Requirements: HOOKS-2.3, HOOKS-10.2_

---

### Task 5: The Integrations Module reports each Harness's state

**Files:**
- Create: `modules/integrations/Cargo.toml`, `modules/integrations/src/{lib,action,state,event}.rs`, `modules/integrations/src/domain/{mod,hookfile,identity}.rs`, `modules/integrations/src/infra/{mod,files}.rs`, `modules/integrations/tests/common/mod.rs`, `modules/integrations/tests/status.rs`, `modules/integrations/tests/fixtures/`
- Modify: `Cargo.toml`, `crates/ffi/Cargo.toml`, `crates/ffi/src/lib.rs`, `crates/ffi/src/app.rs`, `crates/ffi/tests/core.rs`, `crates/ffi/tests/config_privacy.rs`, `apps/cli/tests/cli.rs` (`start_core`), `apps/macos/Navori/Core/CoreStore.swift`

**Reuse:** rung 2 — the Awake Module as the template: `Module` impl in `lib.rs`, `ModuleAction` and `catalog()` in `action.rs`, `ModuleCtx::emit`, `Request::respond`, `NoConfig` (`modules/awake/src/{lib,action,event}.rs`, `crates/kernel/src/module.rs:47-80`); registration at `crates/ffi/src/app.rs:241-254`. Rung 5 — `serde_json` (Task 1), `schemars`, `serde`, `tokio` (`spawn_blocking`). New code (rung 7) for the domain and the file read.

**Interfaces:**
- Consumes: `Module`, `ModuleAction`, `ActionDoc`, `Risk`, `ModuleCtx` from `navori-kernel`
- Produces: `Integrations::new(IntegrationsEnv)`; `IntegrationsEnv { home: PathBuf, dev: bool, helper: PathBuf }`; `inspect(&IntegrationsEnv) -> IntegrationsState`; `IntegrationsAction::{Status, Install{harness}, Repair{harness}, Remove{harness}}` (only `Status` is answered in this task; the others reply `Invalid` until Tasks 6–8); `IntegrationsState { harnesses: Vec<HarnessStatus> }`; `HarnessStatus { harness: Harness, path: String, state: State, reason: Option<String>, offers: Vec<Offer> }`; `CoreConfig.helper_path: String`; `domain::classify(&Value, Harness, dev, helper) -> Classification`

**Depends-on:** Task 1

- [x] **Step 1: Write failing tests** in `modules/integrations/tests/status.rs` over a temp home (an `IntegrationsEnv` with `home` = the temp dir), through the Kernel like `modules/awake/tests/`: `a_harness_without_a_config_directory_is_not_found_with_a_reason`; `a_config_directory_with_no_hook_file_is_not_installed`; `a_hook_file_with_only_foreign_hooks_is_not_installed`; `a_complete_correct_set_is_installed` (fixture written by hand for Claude and for Grok); `a_set_missing_an_event_needs_repair`; `a_stale_helper_path_needs_repair`; `an_async_entry_needs_repair`; `a_handler_sharing_a_group_with_a_matcher_needs_repair`; `a_file_that_is_not_json_is_unreadable_with_the_parse_failure`; `a_root_that_is_not_an_object_is_unreadable`; `a_dev_entry_does_not_make_the_release_profile_installed` and the reverse; `the_three_harnesses_come_back_in_a_fixed_order`; `status_changes_no_file_and_creates_no_directory`. Test dir uses `~`-free temp paths only.
Run: `cargo nextest run -p navori-mod-integrations --test status` — expect: crate not found.
- [x] **Step 2: Implement the crate.** `domain/identity.rs`: parse a `command` string — one POSIX-single-quoted program path whose last three components are `Contents/Helpers/navori`, then exactly `hook` (release) or `--profile dev hook` (dev); anything else is foreign. `domain/hookfile.rs`: the required events per Harness, `desired()`, `classify()` per `design.md` ("Hook-file model"). `infra/files.rs`: `read()` returning `NoDir | NoFile | Doc | Unreadable`, resolving a symlink first (a dangling link is `Unreadable`), reading no file for `NoDir`. `lib.rs`: the request loop answering `Status` in `spawn_blocking`; the published state stays the empty initial one; `inspect` is the same scan. Codex rows report `not_installed` or `installed` as found but offer nothing, with the reason "the Codex timeout unit is not verified yet" until Task 13.
Run: same command — expect: pass.
- [x] **Step 3: Wire it into the Core.** Add the `modules/integrations` entry to the explicit `members` list and `navori-mod-integrations = { path = "modules/integrations" }` under `[workspace.dependencies]` in `Cargo.toml`; the new crate's `Cargo.toml` takes `tempfile`, `serde`, `serde_json`, `schemars`, `anyhow`, `tokio`, `tracing`, `navori-kernel` as regular dependencies, `navori-store` and `tokio` with `test-util` as dev-dependencies, and `[lints] workspace = true` (check-deps requires it). Also add the dependency to `crates/ffi/Cargo.toml`, `helper_path: String` to `CoreConfig` (`crates/ffi/src/lib.rs:36-44`), an `IntegrationsEnv` built once from `CoreConfig` and held in `Inner`, and a new argument on `build_kernel` registering `.register(Integrations::new(env))`. Update the three Rust `CoreConfig` literals (`crates/ffi/tests/core.rs:35`, `crates/ffi/tests/config_privacy.rs:25`, `apps/cli/tests/cli.rs:52`) with a temp-dir helper path, and `CoreStore.swift:26` with `helperPath: AppEnvironment.cliURL.path` (UniFFI camel-cases the field).
Run: `cargo build --workspace` — expect: builds.
- [x] **Step 4: Write and pass a Core test** in `crates/ffi/tests/core.rs`: `dispatching_integrations_status_lists_three_harnesses_over_a_temp_home` (`dispatch_json(&core, "integrations", "\"status\"")`), The idle budget (no timer, no read while idle) is checked by the perf run in Task 14.
- [x] **Step 5: Run** `cargo xtask check-deps` — expect: pass (`navori-mod-integrations` on `navori-kernel` only). Run `cargo xtask ci` — expect: pass.
- [x] **Step 6: Commit** `git commit -m "feat(integrations): add a Module that reports each Harness's hook state"`

_Requirements: HOOKS-3.3, HOOKS-3.4, HOOKS-3.5, HOOKS-3.6, HOOKS-3.7, HOOKS-3.8, HOOKS-3.11, HOOKS-3.12, HOOKS-4.6, HOOKS-8.2, HOOKS-9.5, HOOKS-10.1, HOOKS-10.5_

---

### Task 6: Install a hook for Claude Code and Grok

**Files:**
- Modify: `modules/integrations/src/{lib,action,event}.rs`, `modules/integrations/src/domain/hookfile.rs`, `modules/integrations/src/infra/files.rs`
- Create: `modules/integrations/tests/install.rs`, more `fixtures/`
- Test: `modules/integrations/tests/install.rs`

**Reuse:** rung 5 — `tempfile::NamedTempFile::new_in` + `persist` for the atomic write (a regular dependency of this crate, already in the workspace); rung 3 — `std::fs` for read, copy, metadata, `canonicalize`, `set_permissions`; rung 2 — the Awake `event.rs` kinds-and-sentences pattern and `ModuleCtx::emit`.

**Interfaces:**
- Consumes: `classify`, `read`, `IntegrationsEnv` from Task 5
- Produces: `domain::converge(&mut Value, Harness, dev, helper) -> Changed`; `infra::apply(&IntegrationsEnv, Harness, Plan) -> Result<Outcome, ApplyError>`; Action `Install{harness}`; Events `integrations.hook_installed` (Info) and `integrations.hook_failed` (Error)

**Depends-on:** Task 5

- [x] **Step 1: Write failing tests** in `modules/integrations/tests/install.rs` over a temp home with a hand-written Hook file per case: `installing_into_a_claude_file_adds_three_entries_and_keeps_foreign_hooks_and_key_order` (includes a foreign key in the middle); `installing_for_grok_creates_navori_json_with_five_events`; `installing_for_grok_creates_the_hooks_directory_when_grok_exists`; `installing_never_creates_a_harness_config_directory`; `a_backup_is_written_beside_the_file_before_the_change`; `installing_when_the_file_is_already_correct_changes_nothing_and_writes_no_backup`; `installing_into_an_unparseable_file_changes_nothing_writes_no_backup_and_names_the_file`; `a_failed_write_leaves_the_original_and_a_complete_backup` (read-only directory); `a_symlinked_hook_file_is_written_through_and_the_link_stays`; `the_written_file_keeps_the_original_mode`; `the_release_command_is_the_single_quoted_helper_path_then_hook`; `a_helper_path_with_a_space_and_a_quote_is_quoted_safely`; `the_dev_command_carries_profile_dev_before_hook`; `no_entry_uses_async_and_timeouts_are_2_and_1_seconds`; `dev_install_leaves_a_release_entry_byte_identical`; `an_install_emits_one_event_naming_the_harness_and_a_failure_emits_an_error_event`; `only_the_three_hook_file_paths_and_their_backups_are_written` (walk the temp home before and after); `foreign_hooks_still_present_after_install`.
Run: `cargo nextest run -p navori-mod-integrations --test install` — expect: the Action replies `Invalid`.
- [x] **Step 2: Implement** `converge` per `design.md` (canonical group, in-place replace, append, remove ours elsewhere, `shift_remove` for object keys) and `infra::apply`: resolve target → read and parse (a failure returns before any write) → plan → `Nothing` when unchanged → backup `<name>.navori-bak-<unix seconds>` with `create_new` and a numeric suffix on collision → temp file in the same directory → explicit `set_permissions` to the original's mode (0600 for a new file) → `sync_all` → `persist`. Quote the command with single quotes and `'\''`. Codex stays unavailable here.
Run: same command — expect: pass.
- [x] **Step 3: Check the dev/release rows** by running `cargo nextest run -p navori-mod-integrations --test status --test install` — expect: pass.
- [x] **Step 4: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 5: Commit** `git commit -m "feat(integrations): install Navori's hooks into Claude Code and Grok with a backup"`

_Requirements: HOOKS-4.1, HOOKS-4.2, HOOKS-4.3, HOOKS-4.4, HOOKS-4.5, HOOKS-4.7, HOOKS-4.8, HOOKS-4.9, HOOKS-4.10, HOOKS-4.11, HOOKS-4.12, HOOKS-4.13, HOOKS-4.14, HOOKS-4.15, HOOKS-8.1, HOOKS-8.3, HOOKS-9.6, HOOKS-10.4, HOOKS-10.6_

---

### Task 7: Repair a hook that drifted

**Files:**
- Modify: `modules/integrations/src/lib.rs`, `modules/integrations/src/event.rs`
- Create: `modules/integrations/tests/repair.rs`
- Test: `modules/integrations/tests/repair.rs`

**Reuse:** rung 2 — `domain::converge` and `infra::apply` from Task 6 (Repair is the same algorithm)

**Interfaces:**
- Consumes: `converge`, `apply`, `classify`
- Produces: Action `Repair{harness}`; Event `integrations.hook_repaired` (Info)

**Depends-on:** Task 6

- [x] **Step 1: Write failing tests** in `tests/repair.rs`: `repair_replaces_an_entry_whose_helper_moved_and_leaves_one_entry_per_event`; `repair_adds_a_missing_event`; `repair_removes_an_async_flag`; `repair_removes_an_entry_for_an_event_install_would_not_write`; `repair_moves_a_handler_out_of_a_group_with_a_matcher_and_keeps_the_matcher_group`; `repair_on_an_installed_harness_changes_nothing_and_writes_no_backup`; `repair_writes_a_backup_and_refuses_an_unparseable_file`; `repair_touches_only_the_running_profiles_entry`; `a_repair_emits_one_event`; `repair_leaves_a_double_quoted_hand_edited_entry_alone_as_foreign`.
Run: `cargo nextest run -p navori-mod-integrations --test repair` — expect: the Action replies `Invalid`.
- [x] **Step 2: Implement** the `Repair` branch as `converge` plus the shared `apply`, and its Event sentence.
Run: same command — expect: pass.
- [x] **Step 3: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 4: Commit** `git commit -m "feat(integrations): repair a drifted hook in place"`

_Requirements: HOOKS-5.1, HOOKS-5.2, HOOKS-5.3, HOOKS-5.4, HOOKS-5.5_

---

### Task 8: Remove a Harness's hook

**Files:**
- Modify: `modules/integrations/src/lib.rs`, `modules/integrations/src/event.rs`, `modules/integrations/src/domain/hookfile.rs`, `modules/integrations/src/infra/files.rs`
- Create: `modules/integrations/tests/remove.rs`
- Test: `modules/integrations/tests/remove.rs`

**Reuse:** rung 2 — `infra::apply` (extended with a `Delete` plan) and the `shift_remove` rule from Task 6

**Interfaces:**
- Consumes: `classify`, `apply`
- Produces: `domain::strip(&mut Value, Harness, dev, helper) -> Changed`; `Plan::Delete`; Action `Remove{harness}`; Event `integrations.hook_removed` (Info)

**Depends-on:** Task 7

- [x] **Step 1: Write failing tests** in `tests/remove.rs`: `removing_from_claude_drops_only_navoris_entries_and_keeps_every_foreign_hook_and_key_order` (with a foreign key between two of Navori's keys); `an_event_array_emptied_by_removal_is_dropped_and_a_foreign_one_stays`; `a_hooks_object_emptied_by_removal_is_dropped_but_the_file_stays`; `removing_from_grok_deletes_navori_json_after_a_backup_when_it_holds_only_navori_entries`; `removing_from_grok_keeps_the_file_when_it_holds_anything_else`; `removing_when_nothing_of_the_profile_is_there_changes_nothing_and_writes_no_backup`; `remove_refuses_an_unparseable_file`; `remove_writes_through_a_symlink`; `dev_remove_leaves_a_release_entry_alone`; `a_removal_emits_one_event`.
Run: `cargo nextest run -p navori-mod-integrations --test remove` — expect: the Action replies `Invalid`.
- [x] **Step 2: Implement** `strip` and the `Remove` branch; a Grok document that is empty after `strip` becomes `Plan::Delete` (backup first), Claude and Codex files are never deleted.
Run: same command — expect: pass.
- [x] **Step 3: Run** `cargo xtask ci` — expect: pass.
- [x] **Step 4: Commit** `git commit -m "feat(integrations): remove Navori's hooks and nothing else"`

_Requirements: HOOKS-6.1, HOOKS-6.2, HOOKS-6.3, HOOKS-6.4, HOOKS-6.5_

---

### Task 9: `navori doctor` reports hooks

**Files:**
- Modify: `crates/ffi/src/app.rs`, `crates/ffi/tests/core.rs` (the doctor row-list assertion near line 759, which becomes `config, database, ipc, modules, awake, integrations, logs`)
- Create: `crates/ffi/tests/integrations.rs`
- Test: `crates/ffi/tests/integrations.rs`, `crates/ffi/tests/core.rs`, `apps/cli/tests/cli.rs`

**Reuse:** rung 2 — the `awake_check` row pattern and `check(name, ok, detail)` (`crates/ffi/src/app.rs:123, 141, 330`); `navori_mod_integrations::inspect` from Task 5

**Interfaces:**
- Consumes: `inspect`, `IntegrationsEnv` held in `Inner` (Task 5)
- Produces: one `integrations` row in `Inner::doctor`, after `awake` and before `logs`

**Depends-on:** Task 5

- [ ] **Step 1: Write failing tests** in `crates/ffi/tests/integrations.rs` over a temp home: `doctor_has_an_integrations_row_between_awake_and_logs`; `the_row_is_ok_when_every_harness_is_not_found`; `the_row_is_ok_when_a_harness_is_not_installed`; `the_row_fails_when_a_harness_needs_repair_and_names_it`; `the_row_fails_when_a_hook_file_is_unreadable_and_names_it`; `the_detail_names_all_three_harnesses_with_their_states`; `doctor_and_the_status_action_agree_on_each_harness`; `the_existing_rows_keep_their_order` (config, database, ipc, modules, awake, then the new row, then logs; the old exact-list assertion in `crates/ffi/tests/core.rs` fails first and is updated in this task). In `apps/cli/tests/cli.rs`: `doctor_prints_the_integrations_row_and_exits_1_when_it_fails` and `doctor_with_the_app_down_still_prints_only_the_app_row_and_exits_3` (a characterisation test: it already holds today and guards the behavior).
Run: `cargo nextest run -p navori-ffi --test integrations` — expect: no `integrations` row.
- [ ] **Step 2: Implement** `integrations_check(&IntegrationsState) -> Check` and the push in `Inner::doctor`.
Run: same command, then `cargo nextest run -p navori-cli --test cli` — expect: pass.
- [ ] **Step 3: Run** `cargo xtask ci` — expect: pass.
- [ ] **Step 4: Commit** `git commit -m "feat(doctor): report each Harness's hook state"`

_Requirements: HOOKS-7.1, HOOKS-7.2, HOOKS-7.3, HOOKS-7.4, HOOKS-7.5, HOOKS-9.2_

---

### Task 10: Settings → Integrations tab

**Files:**
- Create: `apps/macos/Navori/Features/Settings/IntegrationsModel.swift`, `apps/macos/Navori/Features/Settings/IntegrationsTab.swift`
- Modify: `apps/macos/Navori/Features/Settings/SettingsView.swift`

**Reuse:** rung 2 — `SettingsTab` and the `TabView` (`SettingsView.swift:5-29`), `Form{Section}.formStyle(.grouped)` as in `GeneralTab`, `CoreStore.dispatch(module:actionJSON:)` and the `performAwake` call-log-banner pattern (`CoreStore.swift:218`)

**Interfaces:**
- Consumes: the Module reply `{"harnesses":[{"harness","path","state","reason","offers"}]}`; Action strings `"status"`, `{"install":{"harness":"claude"}}`, `{"repair":…}`, `{"remove":…}`
- Produces: `IntegrationsModel` (`@MainActor @Observable`: `rows`, `refresh()`, `perform(_:harness:)`); `SettingsTab.integrations`

**Depends-on:** Task 7, Task 8

- [ ] **Step 1: Add the tab.** `SettingsTab` gains `integrations`; the `TabView` gets a fourth `tabItem` and tag; the Diagnostics hint text names the new `integrations` row. Run: `swift build` through `cargo xtask ci` later in Step 4 — expect: compile error for the missing view.
- [ ] **Step 2: Write the model and view.** `IntegrationsModel` decodes the reply with `JSONDecoder` (`convertFromSnakeCase`), refreshes on appear and after every Action, logs and shows an error line on a dispatch failure. The tab lists Claude Code, Codex, Grok Build in that order; each row shows a text label and an SF Symbol per state, the `reason` caption, and Install / Repair / Remove (Install visible for `not_found` and `not_installed` and disabled unless `offers` has it; Repair for `needs_repair`; Remove when offered; nothing for `unreadable`); every button has an accessibility label naming its Harness. No `NavoriCore` import.
- [ ] **Step 3: Note the manual check.** Whether three rows fit the 560×420 window needs the built app, which an agent does not launch against real data in this run; it is left to the operator in Task 15 Step 3.
- [ ] **Step 4: Run** `cargo xtask ci` — expect: pass, with the Swift Shell compiled and `swift test` green.
- [ ] **Step 5: Commit** `git commit -m "feat(settings): add an Integrations tab for Harness hooks"`

_Requirements: HOOKS-3.1, HOOKS-3.2, HOOKS-3.9, HOOKS-3.10, HOOKS-9.4, HOOKS-10.7_

---

### Task 11: Confirm the Claude Code and Codex payloads against recorded ones (check C1) — operator-recorded

**Files:**
- Create: `/Users/jayden/Developer/experiments/NAVORI/.skills/_pending-harness-hooks/auto/payloads-claude-codex.md` (the operator's recorded payloads; not tracked)

**Reuse:** the documentation answers already in `research-hooks-claude-codex.md` (run directory): Claude Code `session_id` and `hook_event_name` are common fields (raw); Claude Code and Codex `Stop` carry no `reason` (docs summaries)

**Interfaces:**
- Consumes: `research-hooks-claude-codex.md`
- Produces: a recorded `UserPromptSubmit`, `Stop` and `SessionEnd` payload per Harness, compared with the documentation

**Depends-on:** none

- [ ] **Step 1: Documentation is done.** The research step is complete; do not repeat it. Only a recorded payload is open.
- [ ] **Step 2: Ask the operator for the recorded payloads.** A live Harness run touches that Harness's real session data outside the worktree, a charter hard stop for this work session, so an agent does not run Claude Code or Codex here.
- [ ] **Step 3: Compare.** If a key differs from `hook_event_name` / `session_id`, or `Stop` carries a `reason`, **stop and report**: HOOKS-1.3 and 1.6 need a decision, and Task 2's `reason` rule goes back for one.

_Requirements: HOOKS-1.3, HOOKS-1.6_

---

### Task 12: Confirm the Codex `timeout` unit and shell behavior on 0.166 (check C2) — operator-recorded

**Files:**
- Create: `/Users/jayden/Developer/experiments/NAVORI/.skills/_pending-harness-hooks/auto/codex-timeout-run.md` (the operator's recorded run; not tracked)

**Reuse:** `research-hooks-claude-codex.md` (run directory): the key `timeout` is confirmed locally; the unit is documented as seconds (raw) without a version; shell versus exec is not stated

**Interfaces:**
- Consumes: `research-hooks-claude-codex.md`
- Produces: the confirmed `timeout` unit and whether `command` runs through a shell on Codex 0.166

**Depends-on:** none

- [ ] **Step 1: Documentation is done.** Nothing more to read here.
- [ ] **Step 2: Ask the operator** for one recorded run on 0.166: a hook that sleeps longer than its `timeout`, and a hook whose command is a single-quoted path containing a space. An agent does not run Codex here.
- [ ] **Step 3: If the unit or the shell behavior stays unconfirmed,** leave Task 13 blocked and say so in the report.

_Requirements: HOOKS-4.5_

---

### Task 13: Install a hook for Codex

**Files:**
- Modify: `modules/integrations/src/domain/hookfile.rs`, `modules/integrations/src/lib.rs`
- Test: `modules/integrations/tests/install.rs`, `modules/integrations/tests/status.rs`

**Reuse:** rung 2 — `converge`, `apply`, the Claude path from Task 6; the facts recorded by Task 12

**Interfaces:**
- Consumes: `.skills/research/2026-11-03-codex-hook-timeout.md`
- Produces: `Harness::Codex` offers Install, Repair and Remove with the verified timeout key and unit

**Depends-on:** Task 6, Task 7, Task 8, Task 12

- [ ] **Step 1: Write failing tests:** `installing_for_codex_adds_three_entries_with_the_verified_timeout`; `codex_repair_and_remove_behave_like_claude`; `codex_no_longer_reports_the_unverified_reason`. Run: `cargo nextest run -p navori-mod-integrations` — expect: Codex still unavailable.
- [ ] **Step 2: Implement** the Codex branch with the key and unit from Task 12; drop the interim reason.
- [ ] **Step 3: Run** `cargo xtask ci` — expect: pass.
- [ ] **Step 4: Commit** `git commit -m "feat(integrations): install Navori's hooks into Codex"`

_Requirements: HOOKS-3.10, HOOKS-4.1, HOOKS-4.5_

---

### Task 14: Docs and the final guard check

**Files:**
- Create: `docs/features/integrations.md`
- Modify: `docs/codebase/modules.md`, `docs/codebase/dependencies.md`, `docs/codebase/map.md`, `docs/architecture/integrations.md`, `docs/features/awake.md`

**Reuse:** rung 2 — `docs/features/awake.md` as the template for the shipped-behavior page; the table rows for `navori-mod-awake` in the codebase docs

**Interfaces:**
- Consumes: the behavior shipped in Tasks 2–10 and 13
- Produces: documentation only

**Depends-on:** Task 10

- [ ] **Step 1: Write `docs/features/integrations.md`** with commands, Actions, Events, the doctor row, files touched, and a "Not verified" list copied from `requirements.md` Open Questions.
- [ ] **Step 2: Update** `docs/codebase/modules.md` (new module row, `navori-ffi` and CLI rows), `dependencies.md` (the new module row), `map.md:29`, `docs/architecture/integrations.md` (move the Harness-hooks row from Planned to Built, no more than the contract that shipped), and the lines of `docs/features/awake.md` that say hook install is not there and there is no Settings tab.
- [ ] **Step 3: Check the guards.** Run `git diff main --stat -- modules/awake/` — expect: empty (Awake untouched). Run `cargo nextest run -p navori-mod-awake` — expect: pass. Run `cargo xtask perf` (it redirects the Core home with `NAVORI_PERF_HOME`, `AppEnvironment.swift:25-27`; confirm the new `helperPath` does not make it read the real home) — expect: pass inside the GOAL-11 budget; if it fails on machine noise, report the run and do not record a baseline.
- [ ] **Step 4: Run** `cargo xtask ci` — expect: pass.
- [ ] **Step 5: Commit** `git commit -m "docs(hooks): describe the Integrations Module and navori hook"`

_Requirements: HOOKS-3.11, HOOKS-7.1, HOOKS-9.3, HOOKS-9.7, HOOKS-10.1_

---

### Task 15: Operator dogfood (check C5) — not an agent task

**Files:**
- none

**Reuse:** `docs/agents/verify.md` for the cold-start drive; `write-dogfood` can turn this list into a guide

**Interfaces:**
- Consumes: a signed build of the dev app
- Produces: the operator's pass/fail on each Unverified item

**Depends-on:** Task 14

- [ ] **Step 1: Operator, with real Claude Code, Codex and Grok sessions and a signed dev build:** Install each Harness's hook from Settings → Integrations; confirm a session Lease appears on a prompt and ends after the turn; confirm Remove restores the config; confirm a dev install leaves a release entry alone.
- [ ] **Step 2: Operator records:** whether macOS asks for permission to write `~/.claude`, `~/.codex` and `~/.grok`; whether Claude Code or Codex show a hook's standard error on exit 0; whether Grok's daemon route leaves a live Harness ancestor; how long after process start a `UserPromptSubmit` payload reaches stdin (a payload later than 0.7 s starts no Lease); Claude Code and Codex payload keys and `Stop` `reason`.
- [ ] **Step 3: Operator checks the two documented gaps** (see the Known risks in `design.md`): whether Esc or an API error in a real Claude Code session leaves the Lease open until the next turn, `SessionEnd`, exit or the TTL; and whether a freshly installed Codex hook waits for trust in `/hooks` before it runs. Neither result is decided by this plan.
- [ ] **Step 4: Operator runs the keyboard and VoiceOver walk** of the Integrations tab and confirms its rows fit the 560×420 window.

_Requirements: HOOKS-1.1, HOOKS-2.2, HOOKS-10.7_
