# Architecture domain: Data

Status: Approved
Date: 2026-11-01

## Purpose and boundary

What Navori keeps, where it lives, and the rules that keep it consistent and private. All data is local to the user's Mac (local-first, no Navori server); each Profile (`release`, `dev`) has its own copy of everything below (ARCH-14).

Paths are shown for `release`; `dev` uses `Navori Dev` in place of `Navori`. Not covered here: process and timing behaviour (`runtime.md`), external APIs that produce the data (`integrations.md`), per-feature table schemas (each feature's `design.md`).

## Entities and stores

**Built (R0)**

| Entity / store | Notes |
|---|---|
| Database `~/Library/Application Support/Navori/navori.db` | SQLite, WAL mode. One file shared by every Owner; each Owner prefixes its own tables (for example `github_*` and `ai_session_*`). Opened by `navori-store`. |
| `schema_migrations` (owner, version) | Records every applied migration per Owner, with its name and time. The source of an Owner's schema version. |
| `kv` (ns, key → JSON value) | Small namespaced settings and cursors; a namespace per module. |
| `events` — the Journal (id, ts_ms, module, kind, severity, summary, payload) | ULID ids, indexed by time. Redacted before insert. Pruned by retention (`journal.retention_days`, default 30). A bad `journal` value, or a file that cannot be read or is not TOML, pauses the sweep so it deletes nothing. |
| Backups `~/Library/Application Support/Navori/backups/` | Full-database copies taken before an Owner's existing data is migrated: `<epoch-ms>-<owner>-v<from-version>.db`; the 5 newest are kept. |
| Config `~/Library/Application Support/Navori/config.toml` | User-editable TOML, optional; sections per module/AI. Read at start-up. A bad value falls back alone and the rest of the file is kept. |
| Session marker `~/Library/Application Support/Navori/session.marker` | Exists while the Core runs (pid, start time, redacted panic text); a leftover one means the last session ended uncleanly. |
| Logs `~/Library/Logs/Navori/navori.YYYY-MM-DD.log` | `tracing` output, daily rotation, 14 files kept; no prompt content or secrets unless debug is on. |
| Socket `~/Library/Application Support/Navori/run/navori.sock` | Not data at rest — the IPC endpoint and single-instance lock, in a `0700` directory. |
| `awake_leases` | Process-owned (session and command) Leases: pid and process start time, plus session fields. Manual duration, until-off and display Leases are not stored; they end when the app closes. On reopen, if the process is still the same, the assertion returns. |

**Planned** (schemas are decided in each feature's design, not here)

| Entity / store | Notes |
|---|---|
| Keychain items, service `dev.navori` for the release Profile and `dev.navori.dev` for the dev Profile (R2, R4) | GitHub token, AI provider API keys. Never in the database, config or logs (ARCH-8). |
| `inbox_*` (R2) | Needs you: Decisions, To-dos, Updates and their outcomes. |
| `ai_session_*`, usage (R2) | Conversations in the canonical message format, tool calls, token usage and estimated cost per turn. |
| `memory_*` (R2) | Facts and preferences, episodic summaries, embeddings. Written only after redaction. |
| Context audit log (R2) | Per LLM request: the kinds of data sent, the provider, the number of redacted items — never the content. |
| `github_*` (R4) | Cached PR inbox, checks, notification cursor, so the menu shows data without a network call. |
| On-disk cache (R4) | Network-sourced files such as avatars (ARCH-18); location set in the R4 design. |

## Consistency rules

- Every write that leaves the process or lands on disk passes the Sensitive Data Shield first: Events, the Journal, memory, the audit log, panic text in the session marker (ARCH-6).
- Each Owner owns its tables and versions them only through `Store::migrate`; one transaction per migration; a failed migration rolls back and leaves the version unchanged (ARCH-12).
- Before an Owner with existing data is upgraded, the whole database is copied to `backups/` (`VACUUM INTO`); an Owner's first migration takes no backup.
- A module reads and writes only its own Owner's tables and KV namespace; it learns about other modules' data through Events and Actions, never by querying their tables (ARCH-3).
- Journal ids are ULIDs (time-sortable); re-inserting the same id replaces the row, so replays are idempotent.
- SQLite runs with `synchronous = NORMAL`, `foreign_keys = ON` and a 5 s busy timeout; all access goes through one connection guarded by the Store.
- A missing file means defaults; a bad value falls back alone and the rest of the file is kept; a bad `journal` value, or a file that cannot be read or is not TOML, pauses the journal sweep and is reported (ARCH-13). Unknown keys stay ignored so older builds can read newer configs.
- `release` and `dev` never read each other's files (ARCH-14).

## Not the invariant spine

Data narrative does **not** redefine ARCH-N invariants. It cites the ones the data layer honors (ARCH-3, ARCH-6, ARCH-8, ARCH-12, ARCH-13, ARCH-14, ARCH-18); when this file and `docs/architecture/INDEX.md` disagree, the spine wins.
