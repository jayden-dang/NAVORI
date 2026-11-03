# Requirements: Keychain secrets

Feature code: SECRETS
Status: Draft
Date: 2026-11-03

<!--
Roadmap item: ROAD-10 keychain-secrets (MILE-3). Claim: GitHub issue #10, assigned.
Written in an unattended run. A value marked `agent-decided` was picked by an agent session
(the interpreting peer), not by the operator; the operator confirms or reverses it on review.
Persisted format: the account `<kind>/<name>` (SECRETS-1.1) and the services `dev.navori` and `dev.navori.dev` (SECRETS-5.1, 5.2) are written to the user's login Keychain and read back by every later build. No item exists until ROAD-11 or ROAD-19 ships, so nothing migrates today; once one exists, renaming either orphans the item (it stays in the Keychain under the old name, unread) and a migration would have to read with the old name first. The charter lists a persisted format as a hard stop; this run went past it knowingly because nothing is written by this change, and the report says so.
Unmarked lines restate a Lock or a repo default (cited in the close package, auto/close-package.md).
"THE SYSTEM" is the `navori-secrets` crate. A "consumer" is a Module, `ai/*` code or `navori-ffi`.
Criteria 1-6 are checked against an in-memory fake by default; the same contract suite runs against
the real Keychain only on request (SECRETS-7.3), and that run is not part of `cargo xtask ci`.
-->

## 1. Name a secret

**Story:** As a consumer author (the provider code of ROAD-11, the GitHub module of ROAD-19), I want to name a secret by kind and name, so that one provider key or one account token has exactly one place and a malformed name is refused before the Keychain is touched.

- **SECRETS-1.1** WHEN a secret key is built from a kind and a name THE SYSTEM SHALL accept it when both are non-empty and contain neither `/` nor a control character, and identify the stored item by the account `<kind>/<name>` (`agent-decided`).
- **SECRETS-1.2** IF the kind or the name is empty, contains `/`, or contains a control character THEN THE SYSTEM SHALL reject the key with an error that says which part is invalid, before any Keychain call (`agent-decided`).

## 2. Keep a secret in the Keychain

**Story:** As a consumer author, I want to store a secret and read it back, so that a token survives a restart without sitting in the database, the config or the logs.

- **SECRETS-2.1** WHEN a consumer sets a value under a secret key THE SYSTEM SHALL store it as a generic password item in the user's regular macOS Keychain, with the service the store was built with (SECRETS-5.1, 5.2) and the key's account (SECRETS-1.1), the value being UTF-8 text (`agent-decided`).
- **SECRETS-2.2** WHEN a consumer sets a value under a key that already holds one THE SYSTEM SHALL replace the value, leaving one item for that key.
- **SECRETS-2.3** WHEN a consumer reads a key that holds a value THE SYSTEM SHALL return it as a Secret.
- **SECRETS-2.4** WHEN a consumer reads a key that holds no item THE SYSTEM SHALL return an outcome that says nothing is stored, distinct from every error (`agent-decided`).
- **SECRETS-2.5** IF the Keychain cannot complete set, get, exists or delete (it is locked, access is denied, the user cancels a prompt, more than one item matches, or the backend fails in any other way) THEN THE SYSTEM SHALL return an error that names the operation, the service, the kind and the name, and that is distinct from the "nothing stored" outcome of SECRETS-2.4. (`agent-decided`: the error names the call, the service, the kind and the name; no backend text)
- **SECRETS-2.6** THE SYSTEM SHALL use the regular Keychain only, with no keychain access group and no data-protection store (ADR 0004).

## 3. Check whether a secret is set

**Story:** As a consumer author (the Settings screen of ROAD-11), I want to ask whether a secret is set, so that the UI can show "set / not set" without the value ever being returned to the caller.

- **SECRETS-3.1** WHEN a consumer asks whether a key holds a secret THE SYSTEM SHALL answer yes or no without returning the value (`agent-decided`).

## 4. Remove a secret

**Story:** As a consumer author (the revoke button of ROAD-19), I want to remove a secret, so that disconnecting leaves nothing in the Keychain and a second attempt is harmless to the caller.

- **SECRETS-4.1** WHEN a consumer deletes a key that holds an item THE SYSTEM SHALL remove the item and report that it was removed (`agent-decided`).
- **SECRETS-4.2** WHEN a consumer deletes a key that holds no item THE SYSTEM SHALL report that it was absent, and report no error (`agent-decided`).

## 5. Keep the release and dev Profiles apart

**Story:** As the author running Navori and Navori Dev side by side, I want each Profile to use its own Keychain service, so that a dev build that is wired to the dev service never reads or overwrites the release token.

- **SECRETS-5.1** THE SYSTEM SHALL offer a release Keychain service named `dev.navori`, and a store SHALL use exactly the service it was built with.
- **SECRETS-5.2** THE SYSTEM SHALL offer a dev Keychain service named `dev.navori.dev` (`agent-decided`), distinct from the release service.
- **SECRETS-5.3** WHEN a value is set through a store built with the release service THE SYSTEM SHALL report nothing stored for the same key read through a store built with the dev service over the same backend, and the reverse. This proves the service mapping against an in-memory backend, not the real Keychain.

## 6. Never reveal a secret

**Story:** As the author, I want a secret to stay unreadable in everything this crate prints or reports, so that a stray log line or error message cannot leak a token.

- **SECRETS-6.1** THE SYSTEM SHALL give the Secret type a `Debug` output that never contains the value, and neither `Display` nor `Serialize` (`agent-decided`).
- **SECRETS-6.2** WHEN a consumer needs the text of a Secret THE SYSTEM SHALL provide it through one explicit accessor, so that every read of a value can be found by search (`agent-decided`).
- **SECRETS-6.3** THE SYSTEM SHALL NOT put a secret value in any error it constructs; an error may name the operation, the service, the kind and the name. Text passed through from the real backend is covered only by the contract run of SECRETS-7.3. (`agent-decided`)
- **SECRETS-6.4** THE SYSTEM SHALL emit no tracing event and no log line that carries a secret value. No automated test covers this; it is checked in the whole-branch review.

## 7. Test without the real Keychain

**Story:** As the author writing a consumer's tests, I want an in-memory store with the same behavior, so that tests never touch my login Keychain and never prompt.

- **SECRETS-7.1** THE SYSTEM SHALL define one store interface that the Keychain-backed store and an in-memory fake both implement, the Keychain being a system boundary (`agent-decided`; docs/standards/testing.md lists boundaries without naming it).
- **SECRETS-7.2** THE SYSTEM SHALL make the fake available to other crates' tests without linking it into the shipped app, and the fake SHALL behave as SECRETS-1 to SECRETS-6 describe; one shared contract suite runs against it in the default tests (`agent-decided`).
- **SECRETS-7.3** WHERE the author sets the opt-in variable for the contract run THE SYSTEM SHALL run the same contract suite against the real Keychain; otherwise that test is ignored (`agent-decided`).
- **SECRETS-7.4** WHEN `cargo test --workspace` or `cargo xtask ci` runs THE SYSTEM SHALL NOT read or write any real Keychain item; checked as: no test file outside the manual contract run opens the real Keychain. (`agent-decided`)

## 8. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** None — no latency or throughput surface (docs/product/metrics.md and docs/ops/reliability.md are absent, so there is no standing target). The calls are synchronous; keeping them off the actor thread and reading lazily (ARCH-17) is the consumers' rule, not this crate's feature.
- **Security:**
  - **SECRETS-8.1** THE SYSTEM SHALL contain no `unsafe` code (ARCH-9) — verified by the workspace `unsafe_code = "forbid"` lint compiling clean and by a search of `crates/secrets/` finding none.
  - **SECRETS-8.2** THE SYSTEM SHALL define no ActionSpec, IPC route or CLI command and depend on no other `navori-*` crate (ARCH-4, THR-7; `agent-decided`: the close package forbids only `navori-macos` and `navori-config`) — verified by a test that reads the crate manifest. A consumer wrapping the crate in an Action is out of scope and is checked by review.
  - **SECRETS-8.3** THE SYSTEM SHALL write a secret value nowhere but the Keychain: no file, database row, config entry, Event or Journal record (ARCH-8, ARCH-6, THR-3, THR-7) — verified by a manifest test asserting the crate depends on no `navori-*` crate and neither `rusqlite` nor `navori-store`, and by a search of `crates/secrets/src` for file and database use in the whole-branch review (the search half is review-only, no automated test).
- **Reliability:** None — durability and availability belong to the macOS Keychain, and failures are reported by SECRETS-2.5; there is no standing target (docs/ops/reliability.md is absent).
- **Accessibility:** None — no UI.

## 9. The docs and the roadmap say what lands

**Story:** As the author, I want the dependency table, the data doc and the roadmap to name what this change adds, so that a reader finds the crate, the dev service and the order without reading code.

- **SECRETS-9.1** WHEN the crate lands THE SYSTEM SHALL list `navori-secrets` in the crate table of `docs/codebase/dependencies.md` with its allowed consumers, and SHALL replace the planned `keyring` entry of the Secrets row with the backend crates actually used (`agent-decided`).
- **SECRETS-9.2** WHEN the crate lands THE SYSTEM SHALL name the dev service `dev.navori.dev` beside `dev.navori` in `docs/architecture/data.md` (`agent-decided`).
- **SECRETS-9.3** THE SYSTEM SHALL record in `docs/roadmap/INDEX.md` that ROAD-19 comes After ROAD-10, because it stores its token through `crates/secrets/`, and that ROAD-11 and ROAD-19 list `crates/ffi/` among their Surfaces (the owner of the Swift-to-Rust save call and the Profile mapping), and the roadmap SHALL stay Draft (`agent-decided`; applied in this change, labelled in the roadmap and listed in the run report).

## 10. Existing behavior stays intact

**Story:** As the author, I want the existing gates, edges and documents to keep working after the crate lands, so that `cargo xtask ci` still passes.

Files touched: `Cargo.toml`, `Cargo.lock`, `crates/secrets/` (new), `docs/codebase/dependencies.md`, `docs/architecture/data.md`, `docs/roadmap/INDEX.md`, `docs/specs/catalog/app.md`. `crates/secrets/` has no behavior to guard. `xtask/` and every existing crate are not touched.

- **SECRETS-10.1** (guard) WHEN `cargo xtask ci` runs THE SYSTEM SHALL CONTINUE TO pass formatting, `clippy -D warnings`, the workspace tests and `check-deps`, with no change to the `check-deps` rules and with the workspace lint table unchanged.
- **SECRETS-10.2** (guard) WHEN a dependency is added THE SYSTEM SHALL CONTINUE TO keep every already locked dependency version in `Cargo.lock` unchanged, unless a backend crate forces a bump, in which case the bump is recorded (Open Questions).
- **SECRETS-10.3** (guard) WHEN `docs/codebase/dependencies.md` gains the `navori-secrets` row THE SYSTEM SHALL CONTINUE TO allow `modules/*` and `ai/*` to depend on secrets, and to limit the `navori` CLI to `config`, `ipc` and `protocol`.
- **SECRETS-10.4** (guard) WHEN `docs/architecture/data.md` names the dev service THE SYSTEM SHALL CONTINUE TO say that Keychain items hold the GitHub token and the provider API keys and never sit in the database, the config or the logs.
- **SECRETS-10.5** (guard) WHEN the roadmap and the catalog are edited THE SYSTEM SHALL CONTINUE TO keep every existing MILE, ROAD and feature code and every other row unchanged, except the ROAD-19 `After` field and the ROAD-11 and ROAD-19 Surfaces of SECRETS-9.3.

## Out of Scope

- Provider configuration, model roles and any screen for entering a key: ROAD-11.
- GitHub Device Flow, the `TokenProvider` choice and the revoke button: ROAD-19.
- Detecting and masking secrets (Sensitive Data Shield): ROAD-12.
- Connected accounts beyond GitHub, and several GitHub accounts: deferred (docs/product/vision.md:47-48).
- Listing or searching the stored items (`agent-decided`: no consumer needs it; one key per provider is known by name).
- Zeroing the value in memory and a `zeroize` dependency (`agent-decided`: no threat in docs/security/threat-model.md covers memory scraping; revisit if one does).
- The data-protection store, iCloud sync, biometrics and keychain access groups (ADR 0004).
- Reading or writing secrets from the `navori` CLI, over IPC, as an Action, or by the AI (ARCH-4; docs/ARCHITECTURE.md:243).
- The shape of a stored JSON blob for a refresh token or expiry: the value is opaque text here, ROAD-19 owns its content.
- Caching "set / not set" in Settings (ROAD-11 must not call `exists` on every Settings open, because the backend reads the item to answer and may prompt).
- Choosing the service from a Profile: the composition root maps the Profile to the release or dev service, so this crate does not duplicate the Profile enum or depend on `navori-config` (`agent-decided`).
- The call from Swift into Rust that saves a pasted key or token, opens `KeychainStore` and maps the Profile belongs to ROAD-11 (provider keys) and ROAD-19 (the GitHub token) through `crates/ffi/`, which their roadmap Surfaces now list (`agent-decided`, edit labelled in `docs/roadmap/INDEX.md`). Each must test its Profile-to-service mapping.
- Showing a failure to the person: a `SecretError` (locked Keychain, denied prompt, any backend failure) must reach the person as a clear message and never as "no key set"; this crate keeps the two apart (SECRETS-2.4, 2.5) and ROAD-11 and ROAD-19 own the display (`agent-decided`).
- Migrating Keychain items when the account format or a service name changes: none is planned, see the persisted-format note in the header (`agent-decided`).
- Re-granting access after a later switch to a Developer ID signature (ADR 0004).

## Open Questions

- Whether the first build resolves `keyring-core` 1.0.1 and `apple-native-keyring-store` 1.0.2 with the locked `log`, `bitflags` and `libc` — plan-tasks, first build task — 2026-11-03 — forbid-guess: the API was read from the crate sources (design.md decisions 1 and 2) but never built; assume no more than that.
- Whether asking "is it set" can avoid a Keychain prompt — design-solution — 2026-11-03 — forbid-guess: no requirement or test may say it never prompts. The design reads the item through `get_credential` and discards it (design.md decision 6), so it may prompt like a read.
- Whether the backend crates force a newer version of an already locked dependency — plan-tasks, first build task — 2026-11-03 — forbid-guess: assume neither that the locked set stays unchanged nor that it moves; if a bump happens, record it (SECRETS-10.2).

Resolved: the contract run cannot target a temporary keychain file, because the Apple store accepts only the User, System, Common and Dynamic keychains (keychain.rs:336-347); the run is manual, writes to the login Keychain under the `dev.navori.dev` service and kind `contract-test`, and an unattended session never runs it (design.md decision 10).
