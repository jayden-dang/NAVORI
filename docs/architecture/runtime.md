# Architecture domain: Runtime

Status: Approved
Date: 2026-11-01

## Purpose and boundary

How Navori runs: which processes and threads exist, how the Core starts, serves and stops, the timing limits it enforces, and the environments it runs in. Values are taken from the code at the stated revision; when a number here and the code disagree, the code is right and this file is stale.

Not covered here: what each component is for (`system.md`), what is stored where (`data.md`), external APIs (`integrations.md`), performance budgets (`docs/standards/performance.md`), binding rules (`INDEX.md`).

## Topology

| Process / service | Role |
|---|---|
| `Navori.app` — main thread | AppKit + SwiftUI Shell: menubar item, menu/panel, Settings. Every Core callback is hopped here in `CoreStore` before it touches UI state. |
| `Navori.app` — `navori-core` threads | The Core's own tokio runtime, 2 worker threads (`crates/ffi/src/lib.rs`). Swift never sees tokio. |
| ↳ module supervisor tasks | One task per module running its actor loop; restarts it on crash with the same inbox (ARCH-10). |
| ↳ IPC accept loop | Accepts connections on the user-private socket; one task per connection; rejects peers with another UID (ARCH-11). |
| ↳ snapshot pump | Wakes on module-state revisions and Events, coalesces (33 ms while the menu is open, 250 ms while closed) and calls the Swift `SnapshotListener`. |
| ↳ log writer | `tracing-appender` non-blocking writer thread for the daily log file. |
| `navori` CLI | Short-lived process (current-thread runtime), one socket connection per command; exits 0/1/2/3. Lives in `Navori.app/Contents/Helpers/`. |
| Hook invocations (planned, R1) | `navori hook` processes spawned by Claude Code, Codex and Grok Build hooks; they call the running app over the socket and exit. |
| `navori mcp` (planned, R6) | Long-lived stdio process spawned by an MCP client (Claude Code, Codex, Grok); proxies tool calls to the running app over the socket. |
| Observed processes (planned, R1/R3) | Harness sessions and dev servers Navori watches (kqueue process-exit) or stops; never owned by Navori. |

## Lifecycle

**Start** (`NavoriCore::start`, called from `applicationDidFinishLaunching`):
1. Resolve the Profile and paths; create the data and logs directories.
2. Load `config.toml` — an invalid file falls back to defaults and is reported (ARCH-13); start file logging.
3. Build the tokio runtime; open the Store (applying migrations, with a backup before upgrading existing data).
4. Start the Kernel and its modules.
5. Bind the IPC socket — it doubles as the single-instance lock: a live socket for the same Profile aborts start-up with "Navori (<profile>) is already running".
6. Write the session marker (a leftover marker means the previous session ended uncleanly; it is reported once), emit `core.started`, start the snapshot pump.

**Serve:** commands arrive as Envelopes (Shell, CLI, later AI and MCP), are routed to a module inbox and answered or time out; state changes and Events flow back through the snapshot pump.

**Stop** (`shutdown()`, called from `applicationWillTerminate`; idempotent): emit `core.stopped`, cancel the pump, cancel every module and wait up to 5 s (then abort), close the socket, give the runtime 1 s to finish, remove the session marker.

**Crash:** a module panic or error is caught by its supervisor and the module restarts with backoff; a process crash leaves the session marker behind (with redacted panic text) for the next start to report.

## Timing and limits

| Parameter | Value | Source |
|---|---|---|
| Dispatch timeout | 30 s | `crates/kernel/src/runtime.rs` |
| Module inbox capacity | 256 requests | `crates/kernel/src/runtime.rs` |
| Restart policy | 5 restarts, backoff from 500 ms (×2, capped at 30 s); a run of ≥ 60 s restores the budget | `crates/kernel/src/runtime.rs` |
| Module shutdown grace | 5 s, then abort | `crates/kernel/src/runtime.rs` |
| Runtime shutdown timeout | 1 s | `crates/ffi/src/lib.rs` |
| Snapshot coalescing | 33 ms menu open · 250 ms menu closed | `crates/ffi/src/app.rs` |
| IPC request line limit | 1 MiB (responses up to 16 MiB) | `crates/ipc/src/lib.rs` |
| Journal retention sweep | at start, then every 6 h (retention from config, default 30 days) | `crates/kernel/src/system.rs` |

## Environments

| Environment | Notes |
|---|---|
| `release` | `/Applications/Navori.app`, bundle id `dev.navori.app`; data `~/Library/Application Support/Navori/`, logs `~/Library/Logs/Navori/`, socket `~/Library/Application Support/Navori/run/navori.sock`. Installed with `cargo xtask install`; the daily-use copy. |
| `dev` | `build/dev/Navori Dev.app`, bundle id `dev.navori.app.debug`; data, logs and socket under `Navori Dev` — never shared with `release` (ARCH-14). Launched with `cargo xtask run`; the CLI needs `--profile dev`. |
| Tests | `cargo test`: in-memory Store, temporary home under `/tmp` (keeps socket paths under the 104-byte limit), real sockets and a real in-process Core; no app bundle, no file logging. |

Both app profiles run only on macOS 26+ on Apple Silicon, signed with the stable self-signed identity, outside the App Sandbox. If a socket path would exceed 103 bytes, the socket moves to a private `/tmp/navori-<uid>/<profile>.sock`.

## Not the invariant spine

Runtime narrative does **not** redefine ARCH-N invariants. It shows where the runtime honors them (ARCH-10, ARCH-11, ARCH-13, ARCH-14); when this file and `docs/architecture/INDEX.md` disagree, the spine wins.
