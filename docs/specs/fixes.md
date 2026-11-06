# Fixes

Fix requirements for features that have no spec folder of their own. Each fix adds one requirement
for the corrected behavior and one `SHALL CONTINUE TO` guard. IDs are permanent.

## CORE — `core.status` answers while the store is locked (issue #21)

Cause, as proposed and accepted by the operator on 2026-11-05 (`confirmed_for_scope`: two-worker Core
runtime, another process holding the database write lock, a Core write queued behind it): the Event
bus wrote each Event to the Journal synchronously on a Core runtime worker. While SQLite waited in its
5 s busy timeout, the IPC task serving `core.status` stayed queued on that same worker and was not
served. Support: `crates/ffi/tests/status_lock.rs` red 3/3 before the fix; a thread sample of the
stall (one worker in `journal_append` → SQLite busy sleep, the other parked); a `block_in_place`
experiment green 3/3.

- **CORE-21.1** WHILE another process holds the store's write lock and a Core write is waiting behind it, WHEN a client calls `core.status` over the local socket THE SYSTEM SHALL answer within 1 s. Verified by `crates/ffi/tests/status_lock.rs`.
- **CORE-21.2** (guard) WHEN a Module emits an Event THE SYSTEM SHALL CONTINUE TO redact it, write it to the Journal and only then broadcast it, on any runtime flavor and outside a runtime (`crates/kernel/src/bus.rs`). Verified by the existing kernel and Module test suites, which run on current-thread runtimes.
