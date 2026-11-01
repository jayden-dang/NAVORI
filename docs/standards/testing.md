# Testing

Status: Approved
Date: 2026-11-01

## Test-first

- No production code without a failing test first (the `test-first` skill). Watch every new test fail for the expected reason before making it pass.
- Tests live only at **agreed seams** — public surfaces where behavior is observable. A feature's `design.md` lists its seams under "Seams for testing"; without one, agree the seams with the user before the first test.
- R0 seams, for reference: `navori-redact` (redact_text / redact_json / is_sensitive_path), `navori-config` (paths, load), `navori-store` (open, migrate, KV, journal), `navori-kernel` (Kernel API), `navori-ipc` (real socket round-trip), `navori-ffi` (`NavoriCore` driven from Rust), `navori` CLI (real binary against an in-process core).

## Where and how

- Rust tests live in each crate's `tests/` directory and go through the crate's public API only — never private functions or internal state.
- Use real collaborators: a real in-memory `Store`, real Unix sockets in a temp directory, a real `NavoriCore` for CLI tests (`assert_cmd`).
- Fake only at system boundaries: IOKit / power assertions, process tables, the network (GitHub), LLM providers (`FakeLlm` with scripted text and tool calls), the clock. Inject fakes through traits; never mock code this repo owns.
- Test homes for socket-using tests must be short (e.g. `tempfile::Builder::new().tempdir_in("/tmp")`) so the socket path stays under the 104-byte `sun_path` limit.
- Name a test with a sentence describing behavior in domain language: `a_crashed_module_is_restarted_and_keeps_serving`, `journal_never_persists_secrets`.
- Expected values come from an independent source (a literal, the requirement), never recomputed the way the code computes them.
- Secret-shaped fixtures are fake and split with `concat!` (see `INDEX.md`, Commits).

## Exempt

- `xtask` build tooling, other than its perf logic, and the Swift view layer are verified by building, signing and launching the app (`cargo xtask run`), not by unit tests. `xtask`'s perf logic is unit-tested. `CoreStore` logic that maps snapshots to state is not exempt once it grows beyond direct assignment.

## Gates

- `cargo xtask ci` before every commit. Report the scope you ran: a crate's tests are not "the suite".
- AI behavior: each module ships at least one eval case (does Navori AI call the right action with the right arguments, and ask for approval on Destructive actions?), run across the configured models with `cargo xtask eval` (planned with R2).
