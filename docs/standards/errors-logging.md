# Errors and Logging

Status: Approved
Date: 2026-11-01

## Errors

- Library crates define `thiserror` enums; binaries and composition roots (`navori-ffi`, `apps/cli`, `xtask`) use `anyhow` at the edges.
- Never swallow an error. Either handle it, return it, or log it with `tracing::warn!/error!` and surface it to the user — a silent fallback is a defect. `navori hook` is the one exemption: a Harness runs it on every prompt, so it drops every failure, prints nothing and writes no file.
- An error message says what went wrong and how to fix it, with the path or value involved: `invalid config …/config.toml: journal.retention_days must be at least 1`. No apologies, no vague "something went wrong".
- A missing or invalid config never stops the app (ARCH-13): run with defaults and show the error (panel banner, `navori doctor`).
- Errors crossing FFI become a `CoreError` variant with a readable message; panics are caught by UniFFI and recorded in the session marker (redacted).
- Module action failures return `ActionError` to the caller; a module crash is the supervisor's job (ARCH-10), not the caller's.

## CLI exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | the command ran and failed (action error, failing `doctor` check) |
| 2 | usage error (bad arguments, invalid JSON, unknown profile) |
| 3 | the app is not running. `navori awake run` is the exception: its exit code is the child's, so 3 can mean the child exited 3 while the app is up |

`navori hook` always exits 0, including for usage errors and when the app is not running, because a non-zero exit can block a Harness prompt.

## Logging

- Rust uses `tracing`; the file sink is `~/Library/Logs/Navori[ Dev]/navori.YYYY-MM-DD.log`, rotated daily, 14 files kept. `NAVORI_LOG` overrides the configured level.
- Swift uses `os.Logger` with subsystem `dev.navori`.
- Never log prompt content, model output, secrets or file contents unless debug logging is explicitly on; even then, text passes the Sensitive Data Shield first (ARCH-6).
- Log at the boundary where the decision is made (module, IPC, FFI), with the identifiers needed to correlate (`module`, `kind`, `trace_id` once introduced) — not every line of a happy path.
- Every module gets a `navori doctor` check that explains its health in one line.
