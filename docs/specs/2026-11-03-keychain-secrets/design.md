# Design: Keychain secrets

Feature code: SECRETS
Status: Draft
Date: 2026-11-03
Requirements: ./requirements.md

<!--
Written in an unattended run. `agent-decided` marks a pick made by an agent session, not by the operator
(close package: auto/close-package.md; backend facts: auto/design-scan.md, read from downloaded crate sources and never built;
independent review: auto/design-review.md, findings F1-F17 applied below).
-->

## Context

Nothing stores Navori's own credentials today. `crates/secrets/` does not exist, and the repo has no Keychain code (the only Keychain touch is `xtask` importing the signing identity, xtask/src/main.rs:583-593). The docs already plan the crate (docs/codebase/map.md:38, docs/architecture/data.md:33) and name two consumers, the provider adapters of ROAD-11 and the GitHub module of ROAD-19. The surrounding pattern is known: a plain leaf crate like `crates/redact` (workspace-inherited package fields, `[lints] workspace = true`, `mod x; pub use x::*;`), and a `fake` feature with a self dev-dependency as in `crates/macos/Cargo.toml:11,22` and `crates/macos/src/lib.rs:15-16`.

The binding constraint is ARCH-9: `unsafe` lives only in `navori-macos` and `navori-ffi`, and `navori-macos` may not be named by this crate (xtask/src/main.rs:684-689). The Keychain is a C API, so the crate reaches it through a third-party crate that carries the `unsafe`. The docs name `keyring` (docs/codebase/dependencies.md:69), which has since been split: `keyring-core` is the trait layer and `apple-native-keyring-store` (feature `keychain`) is the macOS store on the regular Keychain that ADR 0004 requires. Writing our own `SecItem` FFI in `navori-macos` is ruled out by that constraint: it needs `unsafe` and a change to the leaf rule.

A second constraint shapes the error path. `keyring_core::Error` is `#[non_exhaustive]`, and `BadEncoding(Vec<u8>)` carries the stored bytes, which its derived `Debug` prints (design-scan.md B). So no backend error is ever formatted: each is mapped by variant to one of our own errors and its payload is dropped. The crate's own fake is hand-written, because `keyring_core::mock` prints secret bytes in `Debug` and `keyring-core` logs credentials at `debug!`. The real store's credential `Debug` prints only domain, service and account, no value (apple keychain.rs:62-67). Process logging is installed by `navori-ffi` (`crates/ffi/src/logging.rs:30`, `try_init`), and `tracing-subscriber` bridges `log` records, so `keyring-core`'s debug lines (service and account, never a value) can reach the log file; the claim "no value in any output" rests on that `Debug`, not on the absence of a logger.

## Decisions

1. **Backend (`agent-decided`, F2):** `keyring-core` 1.0.1 and `apple-native-keyring-store` 1.0.2 with `features = ["keychain"]` only; `protected` (and keyring-core's `sample`) stay off (SECRETS-2.6). New to `Cargo.lock`: those two plus `security-framework`, `security-framework-sys`, `core-foundation`, `core-foundation-sys`; the locked `log`, `bitflags` and `libc` satisfy them, to be confirmed by the first build. Adopting a new dependency was decided in F2 and is not asked twice. Cost, checked on crates.io on 2026-11-03: both crates have the same single owner (`brotskydotcom`) and `keyring-core` 1.0.1 was published on 2026-10-13, so every Navori secret passes through two young crates with one maintainer. Mitigation: the workspace declares `1.0.1` and `1.0.2`, which Cargo reads as caret ranges, and `Cargo.lock` locks the exact versions (dependencies.md:54); no `=` requirement is added, `cargo xtask outdated` reports drift, and each bump of these two crates is reviewed by hand (`agent-decided`).
2. **No global default store.** Entries are built through `CredentialStoreApi::build` on a store this crate holds (keyring-core api.rs:201), never `Entry::new`.
3. **The service is a value, not a Profile (`agent-decided`).** `KeychainService` has two constants, `RELEASE` = `dev.navori` and `DEV` = `dev.navori.dev` (F1). The crate neither imports `navori-config` nor repeats its `Profile` enum (crates/config/src/lib.rs:16, CONTEXT.md **Profile**); the composition root maps Profile to a constant.
4. **`SecretStore` is a synchronous trait** with four operations; `list` is out (`agent-decided`, F3). Consumers keep calls off the actor thread and read lazily (ARCH-17); that is their rule.
5. **One error type, no reason enum, no backend text (`agent-decided`).** `SecretError` carries the operation, service, kind and name only. The real store never produces `Ambiguous` (it keys by service and account), and `NoStorageAccess` covers only six status codes while a locked or cancelled Keychain arrives as `PlatformFailure` (apple keychain.rs:367-377), so a reason enum would claim distinctions the backend does not make. The cost: a failure cannot be told apart by cause from the error; the backend's status code is dropped to stay inside SECRETS-6.3. If a failure proves undebuggable, adding the status text for the two boxed variants is the follow-up.
6. **`exists` is `Entry::get_credential` mapped `NoEntry` to false (`agent-decided`).** It reads the item without decoding it as UTF-8, so an item written by another tool as non-UTF-8 still answers true, and no value is copied into a `String`. The API has no attribute-only lookup except `Store::search`, whose matching rules and prompt behavior are unverified (design-scan.md B), so `exists` may prompt like `get`. If the contract run shows prompts, `search` is the follow-up.
7. **Hand-written `MemoryStore` fake** (`agent-decided`) under feature `fake` over a shared `FakeKeychain` vault, with failure injection (`fail_next`), so consumers' tests can also exercise "the Keychain failed".
8. **`KeychainStore::with_backend` exists only under feature `fake`** (`agent-decided`), so the shipped interface does not expose a `keyring-core` type and a mock backend cannot reach release builds.
9. **One contract suite, three runs (`agent-decided`):** the same function runs against `MemoryStore`, against `KeychainStore` over `keyring_core::mock` (default tests; its `set_error` injects failures through a second entry on the same credential), and against the real Keychain only by hand (SECRETS-7.3). The mock proves interface semantics and the error mapping; it proves nothing about SECRETS-2.2 on the real store, because it overwrites trivially.
10. **The real contract run uses the login Keychain.** `apple-native-keyring-store` accepts only `User`, `System`, `Common`, `Dynamic` for its `keychain` setting (keychain.rs:336-347), so a temporary keychain file is impossible. The run writes under service `dev.navori.dev`, kind `contract-test`, unique names, and deletes them. It is `#[ignore]`, and when run with `-- --ignored` it panics with a message naming `NAVORI_KEYCHAIN_CONTRACT=1` unless that variable is set, so it can never pass vacuously. An unattended session never runs it.
11. **Persisted format (`agent-decided`).** The account `<kind>/<name>` and the services `dev.navori` and `dev.navori.dev` are written to the login Keychain and read by every later build. Nothing is written by this change and no item exists until ROAD-11 or ROAD-19 ships, so nothing migrates today; renaming either later orphans existing items and needs a read-with-the-old-name migration. The charter's hard stop on a persisted format was passed knowingly for that reason (end report).
12. **No ADR.** The service split and the backend are easy to reverse behind the trait and are not surprising given ARCH-14 and ADR 0004.

Spine invariants this feature relies on: ARCH-4, ARCH-6, ARCH-8, ARCH-9, ARCH-14, ARCH-17 (cited per section).

## Architecture

All new code lives in `crates/secrets/`. Layout: `lib.rs`, `key.rs`, `secret.rs`, `error.rs`, `store.rs`, `keychain.rs`, `fake.rs` (feature `fake`).

### Names and values: `KeychainService`, `SecretKey`, `Secret`

Satisfies: SECRETS-1.1, SECRETS-1.2, SECRETS-5.1, SECRETS-5.2, SECRETS-6.1, SECRETS-6.2
Reuse: none — new code (rung 7); nothing in the repo names or validates a secret
Respects: ARCH-14 (makes the split possible; the composition root must pick the service)
Security: THR-7, THR-3
Interface: `KeychainService::RELEASE`, `KeychainService::DEV`, `as_str()`; `SecretKey::new(kind: &str, name: &str) -> Result<SecretKey, KeyError>` (the account `<kind>/<name>` and the parts stay crate-private); `KeyError { part: KeyPart, problem: KeyProblem }` (`Kind|Name`, `Empty|Slash|Control`); `Secret::new(impl Into<String>)`, `expose(&self) -> &str` (`agent-decided`, F5), a hand-written `Debug` printing a fixed marker, and no `Display`, `Serialize` or `Clone`.
Depth: if this module vanished, callers would still have to know the account format, the two service strings, the key grammar and how to keep a value out of `Debug` — about four rules, not the code.
Locality: leave — new files only; `navori-config` is untouched.

`SecretKey::new` checks kind then name; for each it rejects empty, then `/`, then any `char::is_control`. A name that is too long for the Keychain is not caught here: the backend refuses it at the call and it surfaces as `SecretError` (requirement 1.2 lists only the three cases). An empty `Secret` is accepted and passed through; the real Keychain's handling of an empty value is unverified and no requirement addresses it.

### Store interface and errors: `SecretStore`, `SecretError`, `Deleted`

Satisfies: SECRETS-2.2, SECRETS-2.3, SECRETS-2.4, SECRETS-2.5, SECRETS-3.1, SECRETS-4.1, SECRETS-4.2, SECRETS-7.1
Reuse: none — new code (rung 7); the error style follows `crates/macos/src/error.rs:7,23` (a `thiserror` type whose message names the outcome and never the data)
Respects: ARCH-4, ARCH-8
Security: THR-7
Interface: `trait SecretStore: Send + Sync { set(&self, &SecretKey, &Secret) -> Result<(), SecretError>; get(&self, &SecretKey) -> Result<Option<Secret>, SecretError>; exists(&self, &SecretKey) -> Result<bool, SecretError>; delete(&self, &SecretKey) -> Result<Deleted, SecretError> }`; `Deleted::{Removed, WasAbsent}`; `SecretError` with private fields, a `Display` naming operation, service, kind and name plus one fixed fix-hint sentence, and `operation() -> Operation`; `Operation::{Set, Get, Exists, Delete}`; `OpenError` for building the real store.
Depth: if this module vanished, callers would have to re-derive that "absent" is not an error, that deleting nothing is not an error, and that no error carries a value — the contract consumers rely on and the contract suite pins.
Locality: leave — new files; the trait is the one seam consumers (ROAD-11, ROAD-19) and `navori-ffi` depend on.

`get` returns `Ok(None)` for nothing stored (SECRETS-2.4), `exists` answers `Ok(false)`, `delete` returns `Ok(Deleted::WasAbsent)`. Every other failure is a `SecretError`, distinct from those outcomes (SECRETS-2.5). This crate defines no `ActionSpec`, route or command (SECRETS-8.2).

### Keychain-backed store: `KeychainStore`

Satisfies: SECRETS-2.1, SECRETS-2.6, SECRETS-6.3, SECRETS-6.4
Reuse: none — new code (rung 7) over a new dependency (decision 1); no installed dependency covers the Keychain
Respects: ARCH-9, ARCH-8, ARCH-14
Security: THR-7, THR-3
Interface: `KeychainStore::open(service: KeychainService) -> Result<KeychainStore, OpenError>` (macOS only, the User/login keychain through `apple_native_keyring_store::keychain::Store::new()`); under feature `fake`, `KeychainStore::with_backend(service, Arc<dyn CredentialStoreApi + Send + Sync>) -> KeychainStore`; implements `SecretStore`; a manual `Debug` printing the service only.
Depth: if this module vanished, callers would still need the service/account mapping and the rule that every backend failure becomes one error without a payload — but not the `keyring-core` API, which stays out of `SecretStore`.
Locality: leave — new file.

Each operation builds an `Entry` with `service = service.as_str()` and `user = key account`, and calls `set_password`, `get_password`, `get_credential` (`exists`) or `delete_credential`. The mapping function takes the `keyring_core::Error` by value and matches by variant with a `_` arm: `NoEntry` is the absent outcome for get, exists and delete; every other variant, including `BadEncoding`, `Ambiguous`, `NoStorageAccess` and `PlatformFailure`, becomes a `SecretError`. The error value is never formatted, logged, stored or `unwrap`ped. The crate has no `tracing` or `log` dependency of its own, so nothing in this crate emits a log line (SECRETS-6.4, reviewed, no automated test). `set` is `set_generic_password` underneath: it finds the item first and falls back to add on any find error (security-framework `src/os/macos/passwords.rs:275-278`), so `set` can also prompt, a denied find on an existing item surfaces as a duplicate-item failure, and two first `set`s of one key can race; all unverified on the real Keychain and covered only by the manual run.

### In-memory fake and the contract suite: `FakeKeychain`, `MemoryStore`

Satisfies: SECRETS-5.3, SECRETS-7.2, SECRETS-7.3, SECRETS-7.4
Reuse: rung 2 — the `fake` feature plus self dev-dependency pattern of `crates/macos/Cargo.toml:11,22` and `crates/macos/src/lib.rs:15-16`; the fake itself is rung 7 because `keyring_core::mock` prints secret bytes in `Debug` and logs credentials
Respects: ARCH-8
Interface: under feature `fake`: `FakeKeychain::new()`, `fail_next(Operation)`, `MemoryStore::new(service: KeychainService, &FakeKeychain)` implementing `SecretStore`, both with a manual `Debug` that omits values. Tests: `tests/common/mod.rs` holds `contract(...)` and `contract_errors(...)`; `tests/memory_store.rs` and `tests/keychain_mock.rs` run them by default; `tests/keychain_real.rs` is the manual run (decision 10).
Depth: if this module vanished, a consumer's tests would build their own store and drift from the real behavior; the fake and its one shared suite pin it.
Locality: leave — new files; consumers enable `fake` as a dev-dependency only, so it is not linked into the shipped app.

`FakeKeychain` is a shared map keyed by `(service, account)` holding `Secret`s; `fail_next(op)` makes the next call of that operation return a `SecretError`. The suite checks 2.2-2.4, 3.1, 4.1, 4.2, and that `Debug` and error text omit a multi-character fixture value split with `concat!`; the error half runs on both fakes. Dedicated tests in `tests/keychain_mock.rs` keep the concrete `Arc<keyring_core::mock::Store>`, hand a clone to `with_backend`, and read the value back by building a second entry on the same mock store with `("dev.navori", "<kind>/<name>")` for the release service and `("dev.navori.dev", ...)` for the dev one (SECRETS-2.1, 5.1, 5.2). The cross-service test (SECRETS-5.3) runs on two `KeychainStore`s sharing one mock store, so it depends on the real service mapping, and again on two `MemoryStore`s over one vault. A further test injects `Error::BadEncoding(<fixture bytes>)` through the mock and asserts the resulting `SecretError`'s `Debug` and `Display` lack the fixture (SECRETS-6.3).

### Workspace wiring and docs

Satisfies: SECRETS-9.1, SECRETS-9.2, SECRETS-9.3, SECRETS-10.1, SECRETS-10.2, SECRETS-10.3, SECRETS-10.4, SECRETS-10.5
Reuse: rung 2 — the existing members list, `[workspace.dependencies]` block and docs rows
Respects: ARCH-9
Surface: reader `docs/codebase/dependencies.md:69` (Secrets row naming `keyring`) — replace; reader `docs/architecture/data.md:33` (service `dev.navori` only) — replace with both services; reader `docs/roadmap/INDEX.md:112` (ROAD-19 `After: none`) — replace, already applied in commit 167be85; readers of the `Cargo.toml` members and dependency lists (`xtask check-deps` through `cargo metadata --no-deps`, xtask/src/main.rs:619-624) — replace, no rule edit; no other reader of the word `keyring` exists (search over `*.md`, `*.toml`, `*.rs` outside the specs found only dependencies.md:69).
Interface: `"crates/secrets"` after `"crates/procs"` in `members` (Cargo.toml:11); `navori-secrets = { path = "crates/secrets" }` after `navori-procs` (Cargo.toml:33); `apple-native-keyring-store = { version = "1.0.2", features = ["keychain"] }` and `keyring-core = "1.0.1"` alphabetically in the external block; the new `crates/secrets/Cargo.toml` with `[lints] workspace = true` (required for every package but `navori-ffi` and `navori-macos`, xtask/src/main.rs:699-707), the two backend dependencies, `thiserror`, feature `fake = []` and the self dev-dependency; a `navori-secrets` row in the crate table of dependencies.md and the backend crates in its Secrets row; the dev service in data.md:33.
Depth: n/a — extends the workspace manifest and existing docs
Locality: extend — `Cargo.toml` gains four lines and `Cargo.lock` six crates (expected, not verified until the first build), `dependencies.md` and `data.md` one line each; the catalog row (`docs/specs/catalog/app.md`) and the roadmap edit already exist; `xtask/` is not edited.

`Cargo.lock` is expected to gain the six crates of decision 1 with no locked version moving; this is expected, not verified until the first build, and a bump would be recorded (SECRETS-10.2).

### Crate checks: unsafe, dependencies, test isolation

Satisfies: SECRETS-8.1, SECRETS-8.2, SECRETS-8.3
Reuse: none — new code (rung 7); the workspace `toml` dependency (Cargo.toml:55) and `env!("CARGO_MANIFEST_DIR")` do the reading, but no crate has a manifest-reading test yet (design-scan.md A)
Respects: ARCH-9, ARCH-4, ARCH-8
Security: THR-7, THR-3
Interface: `tests/crate_rules.rs` reads `crates/secrets/Cargo.toml` and the workspace `Cargo.toml` and asserts: every `*dependencies*` table, including `[target.*.dependencies]`, of the crate names no `navori-*` crate other than itself and neither `rusqlite` nor `navori-store`; the crate's own entry for `apple-native-keyring-store` is `workspace = true` with no `features` key, and the workspace entry lists exactly `keychain`; no test file under any `tests/` directory in the workspace except `crates/secrets/tests/keychain_real.rs` contains the call text, which the test builds with `concat!` so the file does not match itself, and `keychain_real.rs` carries `#[ignore]`.
Depth: if this module vanished, nothing in CI would notice a new `navori-*` dependency or an enabled `protected` feature; the test is the pin and holds only literals.
Locality: leave — one new test file.

`unsafe` is excluded by the inherited `unsafe_code = "forbid"`, so a clean compile is the check, and a search of `crates/secrets/` for `unsafe` in the whole-branch review is the second. SECRETS-8.2's "no ActionSpec, route or command" half holds by construction: those types live in crates this one cannot depend on. SECRETS-7.4 is checked only as "no test file outside the manual run opens the real Keychain": `#[cfg(test)]` code in another crate's `src`, and a consumer's `src` calling `open` at start-up (the composition root's business), are outside it. SECRETS-2.6 is checked against the manifests; feature unification by another member is covered by the same scan of the workspace.

## Seams for testing

| Seam | Kind | Covers |
|---|---|---|
| `SecretKey::new`, `Secret` (public API) | unit | SECRETS-1.1, SECRETS-1.2, SECRETS-6.1, SECRETS-6.2 |
| `SecretStore` contract suite over `MemoryStore` (including `fail_next` and the cross-service case) | integration | SECRETS-2.2, SECRETS-2.3, SECRETS-2.4, SECRETS-2.5, SECRETS-3.1, SECRETS-4.1, SECRETS-4.2, SECRETS-5.3, SECRETS-7.1, SECRETS-7.2 |
| `KeychainStore::with_backend` over `keyring_core::mock`: same suite, service-string tests, injected failures, injected `BadEncoding` | integration | SECRETS-2.1, SECRETS-2.5, SECRETS-5.1, SECRETS-5.2, SECRETS-5.3, SECRETS-6.3, SECRETS-7.1 |
| `tests/crate_rules.rs` (manifest and test-layout literals) | integration | SECRETS-2.6, SECRETS-7.4, SECRETS-8.2, SECRETS-8.3 |
| `tests/keychain_real.rs`, `#[ignore]`, `NAVORI_KEYCHAIN_CONTRACT=1`, manual; must include get-of-a-never-set key and delete-twice | integration | SECRETS-7.3, and SECRETS-2.1, 2.2, 2.4, 2.5, 3.1, 4.1, 4.2 against the real Keychain |
| `cargo xtask ci` | e2e | SECRETS-8.1, SECRETS-10.1, SECRETS-10.2 |
| grep of docs and roadmap (a task step) | review | SECRETS-9.1, SECRETS-9.2, SECRETS-9.3, SECRETS-10.3, SECRETS-10.4, SECRETS-10.5 |
| whole-branch review | review | SECRETS-6.4, SECRETS-8.3 (search half) |

The only seam beyond the trait is `with_backend`, available under `fake`. SECRETS-2.2 on the real Keychain (replace in place, one item) is shown only by the manual run.

## Coverage check

Every requirement ID appears in exactly one `Satisfies:` line: 1.1, 1.2, 5.1, 5.2, 6.1, 6.2 names and values; 2.2 to 2.5, 3.1, 4.1, 4.2, 7.1 interface and errors; 2.1, 2.6, 6.3, 6.4 keychain store; 5.3, 7.2, 7.3, 7.4 fake and suite; 8.1 to 8.3 crate checks; 9.1 to 9.3 and 10.1 to 10.5 wiring. No ID is unmapped. Story 8 (NFR) has no section of its own: its three criteria live in the crate-checks section.

Open items carried: whether `exists` and `set` prompt, and what `set` does when the find is denied (unverified, manual run); the first build confirming the dependency delta; the empty-value and over-long-name behavior on the real Keychain; the dropped backend status code (decision 5).
