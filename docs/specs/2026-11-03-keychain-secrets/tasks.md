# Tasks: Keychain secrets

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: SECRETS
Status: In-progress
Date: 2026-11-03
Approved by the operator ("approve", 2026-11-03 21:01, on the review page at e3b886c, no card selected).
Execution-mode: continuous
Delivered-by: PR #12
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

**Goal:** Give Navori a leaf crate, `navori-secrets`, that keeps its own tokens and API keys in the macOS Keychain behind a `SecretStore` trait and a `Secret` type that never prints its value.

**Architecture:** `crates/secrets/` holds validated names (`SecretKey`, `KeychainService`), a redacting `Secret`, one `SecretError`, the synchronous `SecretStore` trait, a `KeychainStore` over `keyring-core` and `apple-native-keyring-store`, and, behind feature `fake`, a hand-written `MemoryStore`. One contract suite runs against the fake and a `keyring_core::mock` backend by default and against the real Keychain only by hand. Nothing else in the repo changes except the workspace manifest and two doc rows; the catalog row and the ROAD-19 roadmap edit already exist.

**Tech Stack:** Rust 2024 (rust-version 1.90), `keyring-core` 1.0.1, `apple-native-keyring-store` 1.0.2 (feature `keychain`), `thiserror`, `toml` (dev), `cargo nextest`.

**Delivery:** Task 6 writes the real-Keychain test but must not run it in an unattended session (it writes to the login Keychain, a charter hard stop). SECRETS-2.1 and SECRETS-2.2 on the real Keychain, the prompt behavior of `exists` and `set`, and the real error codes stay **unverified** until the author runs that test by hand; the end report lists them as such. Tasks 2 and 3 are prefactor slices (names and value types) that the later slices consume.

## Global Constraints

Source of truth (referenced, not copied — read before every task):

| Source | Content hash (`git hash-object`, first 12) |
|---|---|
| `docs/standards/INDEX.md` | `f83e09733ee4` |
| `docs/standards/testing.md` | `b1a9b42fc1d3` |
| `docs/standards/errors-logging.md` | `f84b3ceeea11` |
| `docs/architecture/INDEX.md` (ARCH-N) | `2e318bd23ff5` |

Compact delta every task inherits:

- **Source classes** — every value in `requirements.md` and `design.md` is a Lock, `agent-decided` or Unverified. A task never promotes one to the operator's choice, and a code comment never states an `agent-decided` value as the operator's decision.
- **Charter hard stops** — no test or step reads or writes a real Keychain item, or a config or data file outside this worktree and a temp directory. Only Task 6 contains such a test; it is `#[ignore]`d and never run here.
- **ARCH-8, ARCH-9** — secrets only in the Keychain; no `unsafe` (the workspace lint forbids it; `[lints] workspace = true` is mandatory, xtask/src/main.rs:699-707).
- **ARCH-4, ARCH-10, ARCH-14** — the crate defines no Action, route or command; release and dev never share an item; callers keep Keychain calls off the actor thread (ARCH-10; their rule, not this crate's).
- **Secrets in output** — `Debug`, `Display` and error text of this crate never contain a value; the backend error is never formatted, logged or `unwrap`ped (design decision 5). Secret-shaped fixtures are fake and split with `concat!` (testing.md).
- **Testing** — test-first. Tests live in `crates/secrets/tests/` and use the public API (testing.md:14); test names are sentences in domain language. Fakes only at the system boundary (the Keychain).
- **Lints** — no `unwrap`/`expect` outside tests. Clippy's `allow-expect-in-tests` covers only `#[test]` functions, so every test file with helpers that call `expect` starts with `#![allow(clippy::unwrap_used, clippy::expect_used)]`; `tests/common/mod.rs` also allows `dead_code` because each test crate uses a different subset. `missing_debug_implementations` is on and CI denies warnings.
- **Formatting and commits** — the code blocks below are content, not hand-aligned layout: `cargo fmt --all` reformats them, and it runs before every gate. Run the clippy gate with `--all-targets` exactly as written (without it the crate-private helpers added in Task 4 are unused until Task 5). Each commit step stages the named paths with `git add` first. Conventional Commits, English, one logical change, every commit builds. In this run commits are local only: no push, no PR, no issue.
- **Commands** — task gate: `cargo fmt --all`, then `cargo nextest run -p navori-secrets` and `cargo clippy -p navori-secrets --all-targets -- -D warnings`. `cargo xtask ci` runs at Tasks 1, 6 and 9 only (it also compiles the Swift shell).
- **Team** — Solo band (`docs/agents/project.md`).

Consulted: `docs/codebase/map.md` and `docs/codebase/dependencies.md` for the crate placement and edges; `docs/standards/` for the constraints above. All Approved or standing; no conflict found.

## File Structure

Create:
- `crates/secrets/Cargo.toml` — manifest: workspace package fields, the two backend crates, `thiserror`, feature `fake`, self dev-dependency, `[lints] workspace = true`.
- `crates/secrets/src/lib.rs` — crate doc, `mod` and `pub use` lines, `#[cfg(feature = "fake")] pub mod fake;`.
- `crates/secrets/src/key.rs` — `KeychainService`, `SecretKey`, `KeyError`, `KeyPart`, `KeyProblem`.
- `crates/secrets/src/secret.rs` — `Secret`.
- `crates/secrets/src/error.rs` — `Operation`, `SecretError`, `OpenError`.
- `crates/secrets/src/store.rs` — `SecretStore`, `Deleted`.
- `crates/secrets/src/keychain.rs` — `KeychainStore`.
- `crates/secrets/src/fake.rs` — `FakeKeychain`, `MemoryStore` (feature `fake`).
- `crates/secrets/tests/common/mod.rs` — the contract suite helpers.
- `crates/secrets/tests/names.rs`, `secret.rs`, `memory_store.rs`, `keychain_mock.rs`, `keychain_real.rs` (Task 6), `crate_rules.rs` (Task 7).

Modify:
- `Cargo.toml` — `members`, internal path dependency, two external dependencies.
- `Cargo.lock` — six new third-party packages plus the workspace package `navori-secrets` (expected, not verified until Task 1's build).
- `docs/codebase/dependencies.md` — a `navori-secrets` row in the "Today" table and the Secrets row of the third-party table.
- `docs/architecture/data.md` — the dev service beside `dev.navori`.

Already done outside this plan: the catalog row (`docs/specs/catalog/app.md`) and the ROAD-19 `After: ROAD-10` edit with `Status: Draft` (commit 167be85) and the `crates/ffi/` Surfaces edit of ROAD-11 and ROAD-19 (commit 11676b4), both labelled in the roadmap.

---

### Task 1: Workspace wiring and the empty crate

**Files:**
- Create: `crates/secrets/Cargo.toml`
- Create: `crates/secrets/src/lib.rs`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`

**Reuse:** rung 2 — the existing members list, `[workspace.dependencies]` block and docs rows

**Interfaces:**
- Consumes: the workspace lint table and `[workspace.dependencies]`.
- Produces: package `navori-secrets` with feature `fake = []`, dependencies `keyring-core`, `apple-native-keyring-store`, `thiserror`, and a self dev-dependency enabling `fake`.

**Depends-on:** none

- [x] **Step 1: Show the crate is missing**

Run: `cargo check -p navori-secrets` — expect: ``package ID specification `navori-secrets` did not match any packages``.

- [x] **Step 2: Add the workspace lines**

In `Cargo.toml`: add `"crates/secrets",` after `"crates/procs",` in `members`; add `navori-secrets = { path = "crates/secrets" }` after `navori-procs`; add `apple-native-keyring-store = { version = "1.0.2", features = ["keychain"] }` alphabetically (after `anyhow`) and `keyring-core = "1.0.1"` (between `futures` and `libc`).

- [x] **Step 3: Create the manifest and an empty library**

`crates/secrets/Cargo.toml`:

```toml
[package]
name = "navori-secrets"
description = "Navori's own secrets in the macOS Keychain behind a redacting Secret and a SecretStore trait"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[features]
fake = []

[dependencies]
apple-native-keyring-store.workspace = true
keyring-core.workspace = true
thiserror.workspace = true

[dev-dependencies]
navori-secrets = { path = ".", features = ["fake"] }
toml.workspace = true

[lints]
workspace = true
```

`crates/secrets/src/lib.rs`:

```rust
//! Navori's own secrets (tokens and API keys), kept only in the macOS Keychain (ARCH-8).
//!
//! A value never appears in `Debug`, `Display` or an error from this crate, and the crate
//! contains no `unsafe` code (ARCH-9).
```

- [x] **Step 4: Build and read the lockfile delta**

Run: `cargo build -p navori-secrets` — expect: success. Run: `git diff --stat Cargo.lock` and `git diff Cargo.lock | grep '^-version'` — expect: only added packages (`navori-secrets`, `keyring-core`, `apple-native-keyring-store`, `security-framework`, `security-framework-sys`, `core-foundation`, `core-foundation-sys`) and no `-version` line. If a locked version moved, add it to `requirements.md` Open Questions (SECRETS-10.2) before going on.

- [x] **Step 5: Gate and commit**

Run: `cargo fmt --all`, then `cargo xtask check-deps` — expect: ok with no xtask change. Run: `cargo xtask ci` — expect: pass.
`git add Cargo.toml Cargo.lock crates/secrets && git commit -m "build(secrets): add the navori-secrets crate to the workspace"`

_Requirements: SECRETS-10.1, SECRETS-10.2, SECRETS-8.1_

---

### Task 2: Name a secret

**Files:**
- Create: `crates/secrets/src/key.rs`
- Modify: `crates/secrets/src/lib.rs`
- Test: `crates/secrets/tests/names.rs`

**Reuse:** none — new code (rung 7); nothing in the repo names or validates a secret

**Interfaces:**
- Consumes: nothing.
- Produces: `SecretKey::new(kind: &str, name: &str) -> Result<SecretKey, KeyError>` (`Debug, Clone, PartialEq, Eq`); `KeyError { pub part: KeyPart, pub problem: KeyProblem }` with `Display`; `KeyPart::{Kind, Name}`, `KeyProblem::{Empty, Slash, Control}`.

**Depends-on:** Task 1

- [x] **Step 1: Write the failing test**

`crates/secrets/tests/names.rs`:

```rust
use navori_secrets::{KeyPart, KeyProblem, SecretKey};

#[test]
fn a_provider_key_is_accepted() {
    assert!(SecretKey::new("provider", "anthropic").is_ok());
}

#[test]
fn a_name_with_a_slash_is_rejected_and_the_error_names_the_name() {
    let err = SecretKey::new("github", "a/b").expect_err("slash");
    assert_eq!((err.part, err.problem), (KeyPart::Name, KeyProblem::Slash));
    assert!(err.to_string().contains("name"));
}

#[test]
fn an_empty_part_a_slash_and_a_control_character_are_each_rejected() {
    let cases = [
        ("", "x", KeyPart::Kind, KeyProblem::Empty),
        ("a/b", "x", KeyPart::Kind, KeyProblem::Slash),
        ("kind", "", KeyPart::Name, KeyProblem::Empty),
        ("kind", "tab\there", KeyPart::Name, KeyProblem::Control),
        ("new\nline", "x", KeyPart::Kind, KeyProblem::Control),
    ];
    for (kind, name, part, problem) in cases {
        let err = SecretKey::new(kind, name).expect_err("must be rejected");
        assert_eq!((err.part, err.problem), (part, problem), "{kind:?}/{name:?}");
    }
}

#[test]
fn each_rejection_says_which_part_and_what_is_wrong() {
    let cases = [
        ("", "x", "kind is empty"),
        ("a/b", "x", "kind contains `/`"),
        ("k", "", "name is empty"),
        ("k", "t\tb", "name contains a control character"),
    ];
    for (kind, name, expected) in cases {
        let shown = SecretKey::new(kind, name).expect_err("rejected").to_string();
        assert!(shown.contains(expected), "{shown}");
    }
}
```

Run: `cargo nextest run -p navori-secrets --test names` — expect: compile error, `SecretKey` not found.

- [x] **Step 2: Implement**

`crates/secrets/src/key.rs`:

```rust
use std::fmt;

/// Which half of a secret key is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPart {
    Kind,
    Name,
}

impl fmt::Display for KeyPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Kind => "kind",
            Self::Name => "name",
        })
    }
}

/// What is wrong with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyProblem {
    Empty,
    Slash,
    Control,
}

impl fmt::Display for KeyProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "is empty",
            Self::Slash => "contains `/`",
            Self::Control => "contains a control character",
        })
    }
}

/// A secret key was refused before any Keychain call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("secret key {part} {problem}; use a non-empty {part} without `/` or control characters")]
pub struct KeyError {
    pub part: KeyPart,
    pub problem: KeyProblem,
}
```

Then, in the same file:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretKey {
    kind: String,
    name: String,
}

impl SecretKey {
    pub fn new(kind: &str, name: &str) -> Result<Self, KeyError> {
        check(KeyPart::Kind, kind)?;
        check(KeyPart::Name, name)?;
        Ok(Self {
            kind: kind.to_owned(),
            name: name.to_owned(),
        })
    }
}

fn check(part: KeyPart, text: &str) -> Result<(), KeyError> {
    let problem = if text.is_empty() {
        KeyProblem::Empty
    } else if text.contains('/') {
        KeyProblem::Slash
    } else if text.chars().any(char::is_control) {
        KeyProblem::Control
    } else {
        return Ok(());
    };
    Err(KeyError { part, problem })
}
```

`lib.rs` gains these two lines:

```rust
mod key;
pub use key::*;
```

The crate-private accessors (`account`, `kind`, `name`) arrive in Task 4 with their first caller, so this task's clippy gate has no unused code (the derived `PartialEq` reads both fields).

- [x] **Step 3: Run and gate**

Run: `cargo fmt --all`, then `cargo nextest run -p navori-secrets --test names` — expect: 4 passed. Run: `cargo clippy -p navori-secrets --all-targets -- -D warnings` — expect: clean.

- [x] **Step 4: Commit**

`git add crates/secrets && git commit -m "feat(secrets): validate secret keys by kind and name"`

_Requirements: SECRETS-1.1, SECRETS-1.2_

---

### Task 3: A secret that never prints, and the two services

**Files:**
- Create: `crates/secrets/src/secret.rs`
- Modify: `crates/secrets/src/key.rs`
- Modify: `crates/secrets/src/lib.rs`
- Test: `crates/secrets/tests/secret.rs`

**Reuse:** none — new code (rung 7); nothing in the repo names or validates a secret

**Interfaces:**
- Consumes: nothing.
- Produces: `Secret::new(impl Into<String>)`, `Secret::expose(&self) -> &str`, a hand-written `Debug`, and no `Display`, `Serialize` or `Clone`; `KeychainService::RELEASE`, `KeychainService::DEV`, `as_str(self) -> &'static str` (`Debug, Clone, Copy, PartialEq, Eq`).

**Depends-on:** Task 2

- [x] **Step 1: Write the failing test**

`crates/secrets/tests/secret.rs`:

```rust
use navori_secrets::{KeychainService, Secret};

#[test]
fn debug_output_of_a_secret_never_contains_the_value() {
    let value = concat!("sk-", "ant-FAKEFAKEFAKE-0123456789");
    let shown = format!("{:?}", Secret::new(value));
    assert!(!shown.contains(value));
    assert!(!shown.contains("FAKEFAKE"));
}

#[test]
fn the_text_of_a_secret_comes_only_through_expose() {
    assert_eq!(Secret::new("abc").expose(), "abc");
}

#[test]
fn the_release_and_dev_services_have_their_documented_names_and_differ() {
    assert_eq!(KeychainService::RELEASE.as_str(), "dev.navori");
    assert_eq!(KeychainService::DEV.as_str(), "dev.navori.dev");
    assert_ne!(KeychainService::RELEASE, KeychainService::DEV);
}
```

Run: `cargo nextest run -p navori-secrets --test secret` — expect: compile error, `Secret` not found.

- [x] **Step 2: Implement**

`secret.rs`:

```rust
use std::fmt;

/// A secret value. The text leaves only through [`Secret::expose`]. There is no `Display`,
/// `Serialize` or `Clone`, and `Debug` prints a fixed marker.
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}
```

Append to `key.rs`:

```rust
/// The Keychain service that holds one Profile's items. The composition root maps a Profile to one constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeychainService(&'static str);

impl KeychainService {
    pub const RELEASE: Self = Self("dev.navori");
    pub const DEV: Self = Self("dev.navori.dev");

    pub fn as_str(self) -> &'static str {
        self.0
    }
}
```

`lib.rs` gains:

```rust
mod secret;
pub use secret::*;
```

That `Secret` has no `Display`, `Serialize` or `Clone` is not testable without a compile-fail harness; it is checked by a search in Task 9.

- [x] **Step 3: Run and gate**

Run: `cargo fmt --all`, then `cargo nextest run -p navori-secrets --test secret` — expect: 3 passed. Run: `cargo clippy -p navori-secrets --all-targets -- -D warnings` — expect: clean.

- [x] **Step 4: Commit**

`git add crates/secrets && git commit -m "feat(secrets): add a redacting Secret and the two Keychain services"`

_Requirements: SECRETS-6.1, SECRETS-6.2, SECRETS-5.1, SECRETS-5.2_

---

### Task 4: Keep, check and remove a secret (the store interface and the fake)

**Files:**
- Create: `crates/secrets/src/error.rs`
- Create: `crates/secrets/src/store.rs`
- Create: `crates/secrets/src/fake.rs`
- Modify: `crates/secrets/src/key.rs`
- Modify: `crates/secrets/src/lib.rs`
- Create: `crates/secrets/tests/common/mod.rs`
- Test: `crates/secrets/tests/memory_store.rs`

**Reuse:** none — new code (rung 7); the error style follows `crates/macos/src/error.rs:7,23` (a `thiserror` type whose message names the outcome and never the data)
**Reuse:** rung 2 — the `fake` feature plus self dev-dependency pattern of `crates/macos/Cargo.toml:11,22` and `crates/macos/src/lib.rs:15-16`; the fake itself is rung 7 because `keyring_core::mock` prints secret bytes in `Debug` and logs credentials

**Interfaces:**
- Consumes: `SecretKey`, `Secret`, `KeychainService` (Tasks 2, 3).
- Produces: `trait SecretStore: Send + Sync` with `set(&self, &SecretKey, &Secret) -> Result<(), SecretError>`, `get(&self, &SecretKey) -> Result<Option<Secret>, SecretError>`, `exists(&self, &SecretKey) -> Result<bool, SecretError>`, `delete(&self, &SecretKey) -> Result<Deleted, SecretError>`; `Deleted::{Removed, WasAbsent}`; `Operation::{Set, Get, Exists, Delete}` with `Display`; `SecretError` with `operation()`; crate-private `SecretKey::{account, kind, name}` and `SecretError::new(Operation, KeychainService, &SecretKey)`; `FakeKeychain::{new, fail_next(Operation), item_count() -> usize}` (`item_count` is a feature-`fake` test accessor: a count, never a value); `MemoryStore::new(KeychainService, &FakeKeychain)`; test helpers `contract_set_get`, `contract_exists_delete`, `contract_errors`, `call`.

**Depends-on:** Task 2, Task 3

- [x] **Step 1: Write the failing tests**

`crates/secrets/tests/common/mod.rs`:

```rust
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use navori_secrets::{Deleted, Operation, Secret, SecretError, SecretKey, SecretStore};

pub const FIXTURE: &str = concat!("ghp_", "FAKEFAKEFAKEFAKE0123456789");

pub fn key(kind: &str, name: &str) -> SecretKey {
    SecretKey::new(kind, name).expect("valid key")
}

/// Run one store operation and keep only its error.
pub fn call(store: &dyn SecretStore, op: Operation, key: &SecretKey) -> Result<(), SecretError> {
    match op {
        Operation::Set => store.set(key, &Secret::new(FIXTURE)),
        Operation::Get => store.get(key).map(|_| ()),
        Operation::Exists => store.exists(key).map(|_| ()),
        Operation::Delete => store.delete(key).map(|_| ()),
    }
}

/// SECRETS-2.2 to 2.4.
pub fn contract_set_get(store: &dyn SecretStore, key: &SecretKey) {
    assert!(store.get(key).expect("get never set").is_none());
    store.set(key, &Secret::new(FIXTURE)).expect("set");
    assert_eq!(store.get(key).expect("get").expect("present").expose(), FIXTURE);
    let second = concat!("ghp_", "SECONDSECOND9876543210");
    store.set(key, &Secret::new(second)).expect("replace");
    assert_eq!(store.get(key).expect("get").expect("present").expose(), second);
}

/// SECRETS-3.1, 4.1, 4.2.
pub fn contract_exists_delete(store: &dyn SecretStore, key: &SecretKey) {
    assert!(!store.exists(key).expect("exists before"));
    assert_eq!(store.delete(key).expect("delete nothing"), Deleted::WasAbsent);
    store.set(key, &Secret::new(FIXTURE)).expect("set");
    assert!(store.exists(key).expect("exists after"));
    assert_eq!(store.delete(key).expect("delete"), Deleted::Removed);
    assert!(!store.exists(key).expect("exists gone"));
    assert!(store.get(key).expect("get gone").is_none());
    assert_eq!(store.delete(key).expect("delete twice"), Deleted::WasAbsent);
}

/// SECRETS-2.5 and 6.3: a failure of any of the four calls is an error naming the call and
/// every string in `shown_parts`, and never the value. `inject(op)` makes the next `op` fail.
pub fn contract_errors(
    store: &dyn SecretStore,
    key: &SecretKey,
    shown_parts: &[&str],
    inject: &dyn Fn(Operation),
) {
    store.set(key, &Secret::new(FIXTURE)).expect("set");
    for op in [Operation::Set, Operation::Get, Operation::Exists, Operation::Delete] {
        inject(op);
        let err = call(store, op, key).expect_err("an injected failure must be an error");
        assert_eq!(err.operation(), op);
        for shown in [format!("{err}"), format!("{err:?}")] {
            assert!(!shown.contains(FIXTURE) && !shown.contains("FAKEFAKE"), "{shown}");
            for part in shown_parts {
                assert!(shown.contains(part), "{part} missing from {shown}");
            }
        }
    }
    // a failed delete removed nothing
    assert_eq!(store.get(key).expect("recovers").expect("present").expose(), FIXTURE);
}
```

`crates/secrets/tests/memory_store.rs`:

```rust
mod common;
use common::{contract_errors, contract_exists_delete, contract_set_get, key, FIXTURE};
use navori_secrets::fake::{FakeKeychain, MemoryStore};
use navori_secrets::{KeychainService, Secret, SecretStore};

#[test]
fn a_value_set_through_the_fake_reads_back_and_a_second_set_replaces_it() {
    let vault = FakeKeychain::new();
    let store = MemoryStore::new(KeychainService::RELEASE, &vault);
    contract_set_get(&store, &key("provider", "anthropic"));
}

#[test]
fn asking_whether_a_key_is_set_and_deleting_it_twice_behave_as_documented() {
    let vault = FakeKeychain::new();
    let store = MemoryStore::new(KeychainService::RELEASE, &vault);
    contract_exists_delete(&store, &key("provider", "anthropic"));
}

#[test]
fn an_injected_failure_of_each_call_is_an_error_naming_the_key_and_never_the_value() {
    let vault = FakeKeychain::new();
    let store = MemoryStore::new(KeychainService::DEV, &vault);
    contract_errors(
        &store,
        &key("github", "token"),
        &["dev.navori.dev", "github", "token"],
        &|op| vault.fail_next(op),
    );
}

#[test]
fn a_value_set_through_one_service_is_absent_through_the_other_in_both_directions() {
    let vault = FakeKeychain::new();
    let release = MemoryStore::new(KeychainService::RELEASE, &vault);
    let dev = MemoryStore::new(KeychainService::DEV, &vault);
    let (to_dev, to_release) = (key("github", "token"), key("provider", "openai"));
    release.set(&to_dev, &Secret::new(FIXTURE)).expect("set release");
    assert!(dev.get(&to_dev).expect("dev get").is_none());
    assert!(!dev.exists(&to_dev).expect("dev exists"));
    dev.set(&to_release, &Secret::new(FIXTURE)).expect("set dev");
    assert!(release.get(&to_release).expect("release get").is_none());
    assert!(!release.exists(&to_release).expect("release exists"));
}

#[test]
fn the_debug_output_of_the_fake_and_its_store_never_contains_a_value() {
    let vault = FakeKeychain::new();
    let store = MemoryStore::new(KeychainService::RELEASE, &vault);
    store.set(&key("github", "token"), &Secret::new(FIXTURE)).expect("set");
    for shown in [format!("{vault:?}"), format!("{store:?}")] {
        assert!(!shown.contains(FIXTURE) && !shown.contains("FAKEFAKE"), "{shown}");
    }
}
```

Run: `cargo nextest run -p navori-secrets --test memory_store` — expect: compile error, ``unresolved import `navori_secrets::fake` ``.

- [x] **Step 2: Implement keys, errors and the trait**

Append to `impl SecretKey` in `key.rs`:

```rust
    pub(crate) fn account(&self) -> String {
        format!("{}/{}", self.kind, self.name)
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
```

`crates/secrets/src/error.rs`:

```rust
use std::fmt;

use crate::{KeychainService, SecretKey};

/// Which store call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Set,
    Get,
    Exists,
    Delete,
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Set => "set",
            Self::Get => "get",
            Self::Exists => "exists",
            Self::Delete => "delete",
        })
    }
}

/// A Keychain call failed. It names the call, the service, the kind and the name, and never
/// a value or any text from the backend.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{operation} failed for {kind}/{name} in service {service}; unlock the Keychain, allow Navori access and retry")]
pub struct SecretError {
    operation: Operation,
    service: &'static str,
    kind: String,
    name: String,
}

impl SecretError {
    pub fn operation(&self) -> Operation {
        self.operation
    }

    pub(crate) fn new(operation: Operation, service: KeychainService, key: &SecretKey) -> Self {
        Self {
            operation,
            service: service.as_str(),
            kind: key.kind().to_owned(),
            name: key.name().to_owned(),
        }
    }
}
```

`crates/secrets/src/store.rs`:

```rust
use crate::{Secret, SecretError, SecretKey};

/// What `delete` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deleted {
    Removed,
    WasAbsent,
}

/// Navori's secrets. Synchronous: callers keep these calls off the actor thread and read lazily.
/// "Nothing stored" is an outcome (`None`, `false`, `WasAbsent`), never an error.
pub trait SecretStore: Send + Sync {
    fn set(&self, key: &SecretKey, value: &Secret) -> Result<(), SecretError>;
    fn get(&self, key: &SecretKey) -> Result<Option<Secret>, SecretError>;
    fn exists(&self, key: &SecretKey) -> Result<bool, SecretError>;
    fn delete(&self, key: &SecretKey) -> Result<Deleted, SecretError>;
}
```

- [x] **Step 3: Implement the fake**

`crates/secrets/src/fake.rs`:

```rust
//! A test double for the Keychain, enabled by feature `fake` and never linked into the app.
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::{Deleted, KeychainService, Operation, Secret, SecretError, SecretKey, SecretStore};

#[derive(Default)]
struct State {
    items: HashMap<(String, String), Secret>,
    failures: Vec<Operation>,
}

/// A shared in-memory vault keyed by (service, account). Clones share the vault.
#[derive(Clone, Default)]
pub struct FakeKeychain {
    state: Arc<Mutex<State>>,
}

impl FakeKeychain {
    pub fn new() -> Self {
        Self::default()
    }

    /// Make the next call of `operation` on any store over this vault fail.
    pub fn fail_next(&self, operation: Operation) {
        self.lock().failures.push(operation);
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for FakeKeychain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeKeychain")
            .field("items", &self.lock().items.len())
            .finish()
    }
}

/// A [`SecretStore`] over a [`FakeKeychain`] for one service.
pub struct MemoryStore {
    service: KeychainService,
    vault: FakeKeychain,
}

impl MemoryStore {
    pub fn new(service: KeychainService, vault: &FakeKeychain) -> Self {
        Self {
            service,
            vault: vault.clone(),
        }
    }

    fn slot(&self, key: &SecretKey) -> (String, String) {
        (self.service.as_str().to_owned(), key.account())
    }

    fn check(&self, state: &mut State, operation: Operation, key: &SecretKey) -> Result<(), SecretError> {
        match state.failures.iter().position(|queued| *queued == operation) {
            Some(index) => {
                state.failures.remove(index);
                Err(SecretError::new(operation, self.service, key))
            }
            None => Ok(()),
        }
    }
}

impl fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryStore")
            .field("service", &self.service)
            .finish()
    }
}

impl SecretStore for MemoryStore {
    fn set(&self, key: &SecretKey, value: &Secret) -> Result<(), SecretError> {
        let mut state = self.vault.lock();
        self.check(&mut state, Operation::Set, key)?;
        state.items.insert(self.slot(key), Secret::new(value.expose()));
        Ok(())
    }

    fn get(&self, key: &SecretKey) -> Result<Option<Secret>, SecretError> {
        let mut state = self.vault.lock();
        self.check(&mut state, Operation::Get, key)?;
        Ok(state.items.get(&self.slot(key)).map(|found| Secret::new(found.expose())))
    }

    fn exists(&self, key: &SecretKey) -> Result<bool, SecretError> {
        let mut state = self.vault.lock();
        self.check(&mut state, Operation::Exists, key)?;
        Ok(state.items.contains_key(&self.slot(key)))
    }

    fn delete(&self, key: &SecretKey) -> Result<Deleted, SecretError> {
        let mut state = self.vault.lock();
        self.check(&mut state, Operation::Delete, key)?;
        Ok(if state.items.remove(&self.slot(key)).is_some() {
            Deleted::Removed
        } else {
            Deleted::WasAbsent
        })
    }
}
```

`lib.rs` gains:

```rust
mod error;
mod store;
#[cfg(feature = "fake")]
pub mod fake;
pub use error::*;
pub use store::*;
```

- [x] **Step 4: Run and gate**

Run: `cargo fmt --all`, then `cargo nextest run -p navori-secrets` — expect: all pass (3 + 3 + 5 tests). Run: `cargo clippy -p navori-secrets --all-targets -- -D warnings` — expect: clean.

- [x] **Step 5: Commit**

`git add crates/secrets && git commit -m "feat(secrets): add the SecretStore interface and an in-memory fake"`

_Requirements: SECRETS-2.2, SECRETS-2.3, SECRETS-2.4, SECRETS-2.5, SECRETS-3.1, SECRETS-4.1, SECRETS-4.2, SECRETS-5.3, SECRETS-6.3, SECRETS-7.1, SECRETS-7.2_

---

### Task 5: The Keychain-backed store, run against the mock backend

**Files:**
- Create: `crates/secrets/src/keychain.rs`
- Modify: `crates/secrets/src/error.rs`
- Modify: `crates/secrets/src/lib.rs`
- Test: `crates/secrets/tests/keychain_mock.rs`

**Reuse:** none — new code (rung 7) over a new dependency (decision 1); no installed dependency covers the Keychain

**Interfaces:**
- Consumes: `SecretStore`, `Deleted`, `SecretError`, `Operation`, `KeychainService`, `SecretKey::account`, the contract helpers (Tasks 2 to 4).
- Produces: `OpenError`; `KeychainStore::open(service: KeychainService) -> Result<KeychainStore, OpenError>` (`#[cfg(target_os = "macos")]`); `KeychainStore::with_backend(service, Arc<dyn keyring_core::api::CredentialStoreApi + Send + Sync>) -> KeychainStore` (`#[cfg(feature = "fake")]`); `impl SecretStore for KeychainStore`; a manual `Debug` printing the service only.

**Depends-on:** Task 4

- [x] **Step 1: Write the failing tests**

`crates/secrets/tests/keychain_mock.rs`:

```rust
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use std::sync::Arc;

use common::{call, contract_errors, contract_exists_delete, contract_set_get, key, FIXTURE};
use keyring_core::api::CredentialStoreApi;
use keyring_core::mock::{Cred, Store};
use keyring_core::Error;
use navori_secrets::{KeychainService, KeychainStore, Operation, Secret, SecretStore};

fn mock_store(service: KeychainService) -> (Arc<Store>, KeychainStore) {
    let mock = Store::new().expect("mock store");
    let store = KeychainStore::with_backend(service, mock.clone());
    (mock, store)
}

/// Make the next call on the credential for (service, account) fail with `err`.
fn inject(mock: &Arc<Store>, service: &str, account: &str, err: Error) {
    let entry = mock.build(service, account, None).expect("entry");
    entry
        .as_any()
        .downcast_ref::<Cred>()
        .expect("mock credential")
        .set_error(err);
}

#[test]
fn the_keychain_store_keeps_replaces_checks_and_removes_a_secret() {
    let (_mock, store) = mock_store(KeychainService::RELEASE);
    contract_set_get(&store, &key("provider", "anthropic"));
    contract_exists_delete(&store, &key("provider", "openai"));
}

#[test]
fn a_secret_is_stored_under_the_service_and_account_the_store_was_built_with() {
    for (service, name) in [
        (KeychainService::RELEASE, "dev.navori"),
        (KeychainService::DEV, "dev.navori.dev"),
    ] {
        let (mock, store) = mock_store(service);
        store.set(&key("provider", "anthropic"), &Secret::new(FIXTURE)).expect("set");
        let entry = mock.build(name, "provider/anthropic", None).expect("entry");
        assert_eq!(entry.get_password().expect("read back"), FIXTURE);
    }
}

#[test]
fn a_secret_set_for_one_service_is_absent_for_the_other_in_both_directions() {
    let mock = Store::new().expect("mock store");
    let release = KeychainStore::with_backend(KeychainService::RELEASE, mock.clone());
    let dev = KeychainStore::with_backend(KeychainService::DEV, mock.clone());
    let (to_dev, to_release) = (key("github", "token"), key("provider", "openai"));
    release.set(&to_dev, &Secret::new(FIXTURE)).expect("set release");
    assert!(dev.get(&to_dev).expect("dev get").is_none());
    assert!(!dev.exists(&to_dev).expect("dev exists"));
    dev.set(&to_release, &Secret::new(FIXTURE)).expect("set dev");
    assert!(release.get(&to_release).expect("release get").is_none());
    assert!(!release.exists(&to_release).expect("release exists"));
}

#[test]
fn a_backend_failure_of_each_call_is_an_error_naming_the_key_and_never_the_value() {
    let (mock, store) = mock_store(KeychainService::DEV);
    contract_errors(
        &store,
        &key("github", "token"),
        &["dev.navori.dev", "github", "token"],
        &|_op| {
            inject(&mock, "dev.navori.dev", "github/token", Error::PlatformFailure("locked".into()))
        },
    );
}

#[test]
fn every_kind_of_backend_error_is_an_error_never_an_absent_or_false_outcome() {
    let (mock, store) = mock_store(KeychainService::RELEASE);
    let k = key("provider", "anthropic");
    store.set(&k, &Secret::new(FIXTURE)).expect("set");
    let kinds: [fn() -> Error; 3] = [
        || Error::PlatformFailure("boom".into()),
        || Error::NoStorageAccess("denied".into()),
        || Error::Ambiguous(Vec::new()),
    ];
    for make in kinds {
        for op in [Operation::Set, Operation::Get, Operation::Exists, Operation::Delete] {
            inject(&mock, "dev.navori", "provider/anthropic", make());
            let err = call(&store, op, &k).expect_err("a backend error must stay an error");
            assert_eq!(err.operation(), op);
        }
    }
}

#[test]
fn a_stored_value_carried_inside_a_backend_error_never_reaches_our_error() {
    let (mock, store) = mock_store(KeychainService::RELEASE);
    let k = key("provider", "anthropic");
    store.set(&k, &Secret::new(FIXTURE)).expect("set");
    inject(&mock, "dev.navori", "provider/anthropic", Error::BadEncoding(FIXTURE.as_bytes().to_vec()));
    let err = store.get(&k).expect_err("bad encoding");
    assert_eq!(err.operation(), Operation::Get);
    for shown in [format!("{err}"), format!("{err:?}")] {
        assert!(!shown.contains(FIXTURE) && !shown.contains("FAKEFAKE"), "{shown}");
    }
}

#[test]
fn an_item_that_exists_but_is_not_text_still_counts_as_set() {
    let (mock, store) = mock_store(KeychainService::RELEASE);
    let entry = mock.build("dev.navori", "provider/odd", None).expect("entry");
    entry.set_secret(&[0xff, 0xfe, 0x00]).expect("raw bytes");
    assert!(store.exists(&key("provider", "odd")).expect("exists"));
}

#[test]
fn the_debug_output_of_the_store_names_the_service_and_no_value() {
    let (_mock, store) = mock_store(KeychainService::DEV);
    store.set(&key("github", "token"), &Secret::new(FIXTURE)).expect("set");
    let shown = format!("{store:?}");
    assert!(shown.contains("dev.navori.dev"));
    assert!(!shown.contains(FIXTURE) && !shown.contains("FAKEFAKE"), "{shown}");
}
```

Run: `cargo nextest run -p navori-secrets --test keychain_mock` — expect: compile error, `KeychainStore` not found.

- [x] **Step 2: Implement**

Append to `error.rs`:

```rust
/// The Keychain store could not be built.
#[derive(Debug, thiserror::Error)]
#[error("the Keychain store could not be opened; check that this is macOS and retry")]
pub struct OpenError;
```

`crates/secrets/src/keychain.rs`:

```rust
use std::fmt;
use std::sync::Arc;

use keyring_core::api::CredentialStoreApi;
use keyring_core::{Entry, Error};

#[cfg(target_os = "macos")]
use crate::OpenError;
use crate::{Deleted, KeychainService, Operation, Secret, SecretError, SecretKey, SecretStore};

/// The Keychain-backed store. A backend error is matched by variant and dropped unformatted:
/// `keyring_core::Error::BadEncoding` carries the stored bytes and prints them in `Debug`.
pub struct KeychainStore {
    service: KeychainService,
    backend: Arc<dyn CredentialStoreApi + Send + Sync>,
}

impl KeychainStore {
    /// Open the user's regular (login) Keychain for `service`.
    #[cfg(target_os = "macos")]
    pub fn open(service: KeychainService) -> Result<Self, OpenError> {
        let backend = apple_native_keyring_store::keychain::Store::new().map_err(|_| OpenError)?;
        Ok(Self { service, backend })
    }

    /// Build a store over any `keyring-core` backend. Test seam, available with feature `fake`.
    #[cfg(feature = "fake")]
    pub fn with_backend(
        service: KeychainService,
        backend: Arc<dyn CredentialStoreApi + Send + Sync>,
    ) -> Self {
        Self { service, backend }
    }

    fn entry(&self, operation: Operation, key: &SecretKey) -> Result<Entry, SecretError> {
        self.backend
            .build(self.service.as_str(), &key.account(), None)
            .map_err(|_| SecretError::new(operation, self.service, key))
    }
}

impl fmt::Debug for KeychainStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeychainStore")
            .field("service", &self.service)
            .finish()
    }
}

impl SecretStore for KeychainStore {
    fn set(&self, key: &SecretKey, value: &Secret) -> Result<(), SecretError> {
        self.entry(Operation::Set, key)?
            .set_password(value.expose())
            .map_err(|_| SecretError::new(Operation::Set, self.service, key))
    }

    fn get(&self, key: &SecretKey) -> Result<Option<Secret>, SecretError> {
        match self.entry(Operation::Get, key)?.get_password() {
            Ok(text) => Ok(Some(Secret::new(text))),
            Err(Error::NoEntry) => Ok(None),
            Err(_) => Err(SecretError::new(Operation::Get, self.service, key)),
        }
    }

    fn exists(&self, key: &SecretKey) -> Result<bool, SecretError> {
        match self.entry(Operation::Exists, key)?.get_credential() {
            Ok(_) => Ok(true),
            Err(Error::NoEntry) => Ok(false),
            Err(_) => Err(SecretError::new(Operation::Exists, self.service, key)),
        }
    }

    fn delete(&self, key: &SecretKey) -> Result<Deleted, SecretError> {
        match self.entry(Operation::Delete, key)?.delete_credential() {
            Ok(()) => Ok(Deleted::Removed),
            Err(Error::NoEntry) => Ok(Deleted::WasAbsent),
            Err(_) => Err(SecretError::new(Operation::Delete, self.service, key)),
        }
    }
}
```

`lib.rs` gains:

```rust
mod keychain;
pub use keychain::*;
```

The crate declares no `log` or `tracing` dependency, so nothing here emits a log line.

- [x] **Step 3: Run and gate**

Run: `cargo fmt --all`, then `cargo nextest run -p navori-secrets` — expect: all pass. Run: `cargo clippy -p navori-secrets --all-targets -- -D warnings` — expect: clean.

- [x] **Step 4: Commit**

`git add crates/secrets && git commit -m "feat(secrets): store secrets through keyring-core and map its errors without payloads"`

_Requirements: SECRETS-2.1, SECRETS-2.2, SECRETS-2.3, SECRETS-2.4, SECRETS-2.5, SECRETS-3.1, SECRETS-4.1, SECRETS-4.2, SECRETS-5.1, SECRETS-5.2, SECRETS-5.3, SECRETS-6.3, SECRETS-6.4, SECRETS-7.1_

---

### Task 6: The manual real-Keychain contract run (written, not run)

**Files:**
- Test: `crates/secrets/tests/keychain_real.rs`

**Reuse:** rung 2 — the `fake` feature plus self dev-dependency pattern of `crates/macos/Cargo.toml:11,22` and `crates/macos/src/lib.rs:15-16`; the fake itself is rung 7 because `keyring_core::mock` prints secret bytes in `Debug` and logs credentials

**Interfaces:**
- Consumes: `KeychainStore::open`, `KeychainService::DEV`, the contract helpers (Tasks 4, 5).
- Produces: one `#[ignore]`d test.

**Depends-on:** Task 5

- [x] **Step 1: Write the test**

`crates/secrets/tests/keychain_real.rs`:

```rust
mod common;
use std::time::{SystemTime, UNIX_EPOCH};

use common::{contract_exists_delete, contract_set_get};
use navori_secrets::{KeychainService, KeychainStore, SecretKey, SecretStore};

/// Manual only. Writes to the login Keychain under service `dev.navori.dev`, kind `contract-test`,
/// and deletes what it wrote. A run that panics midway can leave an item named `run-<pid>-<nanos>`
/// there; remove it with Keychain Access.
/// Run: NAVORI_KEYCHAIN_CONTRACT=1 cargo test -p navori-secrets --test keychain_real -- --ignored
#[test]
#[ignore = "writes to the login Keychain; run by hand with NAVORI_KEYCHAIN_CONTRACT=1 and --ignored"]
fn the_real_keychain_honours_the_store_contract() {
    assert_eq!(
        std::env::var("NAVORI_KEYCHAIN_CONTRACT").as_deref(),
        Ok("1"),
        "set NAVORI_KEYCHAIN_CONTRACT=1 to run this test against the login Keychain"
    );
    let store = KeychainStore::open(KeychainService::DEV).expect("open the login keychain store");
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
    let name = format!("run-{}-{stamp}", std::process::id());
    // get of a never-set key, replace in place, then exists, delete and a second delete
    let first = SecretKey::new("contract-test", &name).expect("key");
    contract_set_get(&store, &first);
    let second = SecretKey::new("contract-test", &format!("{name}-b")).expect("key");
    contract_exists_delete(&store, &second);
    store.delete(&first).expect("clean up the first key");
}
```

- [x] **Step 2: Prove it compiles and is skipped by default; do not run it**

Run: `cargo fmt --all`, then `cargo nextest run -p navori-secrets --test keychain_real` — expect: 0 tests run, 1 skipped. **Do not** pass `--ignored` or `--run-ignored` in an unattended session. Run: `cargo clippy -p navori-secrets --all-targets -- -D warnings` — expect: clean; this also proves `KeychainStore::open` compiles against the real Apple store, including the `Send + Sync` coercion.

- [x] **Step 3: Gate and commit**

Run: `cargo xtask ci` — expect: pass.
`git add crates/secrets && git commit -m "test(secrets): add the manual real-Keychain contract run"`

_Requirements: SECRETS-7.3, SECRETS-7.4_

---

### Task 7: Crate rules as tests

**Files:**
- Test: `crates/secrets/tests/crate_rules.rs`

**Reuse:** none — new code (rung 7); the workspace `toml` dependency (Cargo.toml:55) and `env!("CARGO_MANIFEST_DIR")` do the reading, but no crate has a manifest-reading test yet (design-scan.md A)

**Interfaces:**
- Consumes: the manifests from Task 1 and `tests/keychain_real.rs` from Task 6.
- Produces: three guard tests.

**Depends-on:** Task 6

- [x] **Step 1: Write the tests**

`crates/secrets/tests/crate_rules.rs`:

```rust
#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::fs;
use std::path::{Path, PathBuf};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    crate_dir().join("../..")
}

fn read(path: &Path) -> toml::Table {
    fs::read_to_string(path).expect("read").parse().expect("toml")
}

/// Every `*dependencies*` table of a manifest, including `[target.*.dependencies]`.
fn dependency_tables(manifest: &toml::Table) -> Vec<(String, &toml::Table)> {
    let mut found = Vec::new();
    for (key, value) in manifest {
        if key.ends_with("dependencies")
            && let Some(table) = value.as_table()
        {
            found.push((key.clone(), table));
        }
        if key == "target" {
            for (cfg, body) in value.as_table().into_iter().flatten() {
                for (inner, table) in body.as_table().into_iter().flatten() {
                    if inner.ends_with("dependencies")
                        && let Some(table) = table.as_table()
                    {
                        found.push((format!("target.{cfg}.{inner}"), table));
                    }
                }
            }
        }
    }
    found
}

#[test]
fn the_crate_depends_on_no_other_navori_crate_and_no_database_crate() {
    let manifest = read(&crate_dir().join("Cargo.toml"));
    for (table_name, table) in dependency_tables(&manifest) {
        for name in table.keys() {
            assert!(name != "rusqlite" && name != "navori-store", "{table_name} names {name}");
            let own_dev_entry = table_name == "dev-dependencies" && name == "navori-secrets";
            assert!(!name.starts_with("navori-") || own_dev_entry, "{table_name} names {name}");
        }
    }
}

#[test]
fn the_apple_store_is_the_regular_keychain_only() {
    let manifest = read(&crate_dir().join("Cargo.toml"));
    let own = manifest["dependencies"]["apple-native-keyring-store"].as_table().expect("table");
    assert_eq!(own.get("workspace").and_then(toml::Value::as_bool), Some(true));
    assert!(own.get("features").is_none(), "the crate must not add features of its own");
    let root = read(&workspace_root().join("Cargo.toml"));
    let shared = root["workspace"]["dependencies"]["apple-native-keyring-store"]
        .as_table()
        .expect("table");
    let features: Vec<&str> = shared["features"]
        .as_array()
        .expect("features")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    assert_eq!(features, ["keychain"]);
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if matches!(entry.file_name().to_str(), Some("target" | "build" | ".git" | ".worktrees" | "node_modules")) {
            continue;
        }
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn only_the_manual_run_opens_the_real_keychain_and_it_is_ignored_by_default() {
    let needle = concat!("Keychain", "Store::open(");
    let manual = crate_dir().join("tests/keychain_real.rs").canonicalize().expect("manual run exists");
    let mut files = Vec::new();
    rust_files(&workspace_root(), &mut files);
    assert!(!files.is_empty(), "the scan found no Rust files; the workspace root is wrong");
    for file in files {
        let text = fs::read_to_string(&file).expect("read");
        if file.canonicalize().ok().as_deref() == Some(manual.as_path()) {
            assert!(
                text.lines().any(|line| line.trim_start().starts_with("#[ignore")),
                "the manual run must stay #[ignore]d"
            );
        } else if file.components().any(|part| part.as_os_str() == "tests") {
            assert!(!text.contains(needle), "{} opens the real Keychain", file.display());
        }
    }
}
```

- [x] **Step 2: Run, then prove each test can fail**

Run: `cargo fmt --all`, then `cargo nextest run -p navori-secrets --test crate_rules` — expect: 3 passed. Then apply each mutation, expect the named test to fail, and revert it:
1. add `navori-store = { workspace = true }` under `[dependencies]` of `crates/secrets/Cargo.toml` — test 1 fails;
2. add `[target.'cfg(unix)'.dependencies]` with `rusqlite = { workspace = true }` — test 1 fails (the target branch);
3. change the workspace entry to `features = ["keychain", "protected"]` — test 2 fails;
4. delete the `#[ignore = ...]` line of `keychain_real.rs` — test 3 fails;
5. create `crates/secrets/tests/probe.rs` holding the single comment line `// KeychainStore::open(x)` — test 3 fails; delete the file.
Run `git status --short` — expect: nothing but this task's file.

- [x] **Step 3: Gate and commit**

Run: `cargo clippy -p navori-secrets --all-targets -- -D warnings` — expect: clean.
`git add crates/secrets && git commit -m "test(secrets): pin the crate's dependency and test-layout rules"`

_Requirements: SECRETS-2.6, SECRETS-7.4, SECRETS-8.2, SECRETS-8.3_

---

### Task 8: The docs say what lands

**Files:**
- Modify: `docs/codebase/dependencies.md`
- Modify: `docs/architecture/data.md`

**Reuse:** rung 2 — the existing members list, `[workspace.dependencies]` block and docs rows

**Interfaces:**
- Consumes: the crate and the service names (Tasks 1, 3).
- Produces: a `navori-secrets` row in the "Today" table, the backend crates in the Secrets row of the third-party table, the dev service in `data.md`.

**Depends-on:** Task 3

**Collisions (load-subgraph blast_radius):** `docs/codebase/dependencies.md` is also in the Files of the PROCS, AWAKE and HOOKS plans, and `docs/architecture/data.md` in the AWAKE and KCONTRACT plans. Those plans add or edit other rows of the same tables; this task adds only the `navori-secrets` row, the Secrets row and line 33. On a merge conflict keep both sides' rows; never reorder or reformat the table.

- [x] **Step 1: Show what is missing**

Run: `rg -n 'navori-secrets' docs/codebase/dependencies.md` — expect: only the `ai/*` row (line 39) and no row of its own in the "Today" table. Run: `rg -n 'dev.navori.dev' docs/architecture/data.md` — expect: no match. Run: `rg -n 'keyring' docs/codebase/dependencies.md` — expect: the Secrets row naming `keyring` (apple-native) under Planned.

- [x] **Step 2: Edit**

In `docs/codebase/dependencies.md`: add to the "Today" table, after the `navori-procs` row, `| \`navori-secrets\` | — | allowed: none — leaf; consumed by \`modules/*\` and \`ai/*\` (rows below) and may be opened by the composition root \`navori-ffi\` (it may depend on any foundation crate) |`; replace the Secrets row of the third-party table with `| Secrets | \`keyring-core\`, \`apple-native-keyring-store\` (feature \`keychain\`) | — |`. In `docs/architecture/data.md` line 33: write `Keychain items, service \`dev.navori\` for the release Profile and \`dev.navori.dev\` for the dev Profile (R2, R4)`, keeping the second cell unchanged (GitHub token, AI provider API keys; never in the database, config or logs).

- [x] **Step 3: Verify**

Run the three `rg` commands again — expect: the new row, both service names, the new crate names, and no remaining `keyring` (apple-native). Run: `git diff -U0 docs/codebase/dependencies.md docs/architecture/data.md` — expect: only those lines changed. Run: `git diff $(git merge-base HEAD main) -- docs/roadmap/INDEX.md docs/specs/catalog/app.md` — expect: only `Status: Approved` to `Status: Draft`, ROAD-19 `After: none` to `After: ROAD-10`, `crates/ffi/` added to the Surfaces of ROAD-11 and ROAD-19 (each with its `agent-decided` marker comment), and one added `SECRETS` row (SECRETS-9.3, 10.5). Run: `head -4 docs/roadmap/INDEX.md` — expect: `Status: Draft`.

- [x] **Step 4: Commit**

`git add docs/codebase/dependencies.md docs/architecture/data.md && git commit -m "docs(secrets): list navori-secrets, its backend crates and the dev Keychain service"`

_Requirements: SECRETS-9.1, SECRETS-9.2, SECRETS-9.3, SECRETS-10.3, SECRETS-10.4, SECRETS-10.5_

---

### Task 9: Whole-branch gate

**Files:**
- Modify: `crates/secrets/` (fixes only, if a check finds something)

**Reuse:** rung 2 — the existing members list, `[workspace.dependencies]` block and docs rows

**Interfaces:**
- Consumes: everything above.
- Produces: the verified branch.

**Depends-on:** Task 7, Task 8

- [x] **Step 1: Full gate**

Run: `cargo fmt --all --check`, then `cargo xtask ci` — expect: pass (fmt, clippy `-D warnings`, workspace tests, `check-deps`, `build-core`, Swift build).

- [x] **Step 2: Review searches (SECRETS-6.1, 6.4, 8.1, 8.3)**

Run: `rg -n '\bunsafe\s*(\{|fn|impl|trait|extern)' crates/secrets/` — expect: no match (the crate doc line that says the crate has no `unsafe` is prose and does not match). Run: `rg -n 'tracing|log::|println!|eprintln!|dbg!' crates/secrets/src` — expect: no match. Run: `rg -n 'std::fs|File::|rusqlite|OpenOptions' crates/secrets/src` — expect: no match. Run: `rg -n 'unwrap\(|expect\(' crates/secrets/src` — expect: no match. Run: `rg -n 'impl.*(Display|Serialize|Clone).*for Secret|derive.*(Serialize|Clone)' crates/secrets/src/secret.rs` — expect: no match.

- [x] **Step 3: Lock delta**

Run: `git diff $(git merge-base HEAD main) -- Cargo.lock | grep '^-version'` — expect: no match. If one appears, add it to `requirements.md` Open Questions (SECRETS-10.2).

- [x] **Step 4: Commit only if something changed**

If a check above failed and was fixed, stage the changed paths and commit with a conventional subject that names the finding. Otherwise make no commit.

_Requirements: SECRETS-8.1, SECRETS-8.3, SECRETS-6.1, SECRETS-6.4, SECRETS-10.1, SECRETS-10.2_
