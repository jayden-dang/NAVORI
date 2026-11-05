# Project configuration (agent-facing)

Written by `configure-repo`. Skills read this file for repo-specific **machine config** —
commands, globs, paths — plus **posture** and **team** (below). The project-docs layer
(`/define-project`) is enabled: product vision and goals in `docs/product/vision.md`
(GOAL-N), architecture invariants in `docs/architecture/INDEX.md` (ARCH-N), and engineering
standards — including the Definition of Done — in `docs/standards/` (the standards SSOT;
`plan-tasks` sources guidelines from there). `docs/ARCHITECTURE.md` is the system design
narrative and rationale.

## Project posture

The project's standing intent, lifecycle phase, and compatibility obligation. Skills read
this instead of re-asking: `frame-change` and `clarify-decisions` right-size migration /
compat / deprecation against **Compat obligation**; `interpret-session` reuses all three.
`clarify-decisions` production coverage needs Delivery **Production** + Lifecycle
**Cut Released / Scaling / Maintenance** + operate/launch surface (or an ops ask); MVP,
absent, and Early/Active stay off. Edit these lines when the project moves phase.

**SSOT:** the **Compat obligation** values and what each one means live only here. Skills
carry a one-clause copy of the derivation because a repo configured before the field existed
has no Compat obligation text in this file for them to read — change the meanings here, and
the derivation in `frame-change`, `clarify-decisions`, and `interpret-session` with it.

Compat obligation values (the line below states it; when it is absent the value is **derived**): **None:** nothing
outside this repo depends on any current schema, endpoint, or file format. **Internal:** other
services or teams in the same org read them. **External:** users, customers, or third-party
clients depend on them. Lifecycle Idea / Early development / Active development → **None**;
Cut Released / Scaling / Maintenance → **External**. Add a `- **Compat obligation:**` line only
to override that default (e.g. once the installed app's data in
`~/Library/Application Support/Navori/` must survive upgrades).

- **Delivery intent:** `Production` — the quality bar the output must meet. A bar, not a release state: **Production** says how well the code must be built, never that the project has shipped.
- **Lifecycle stage:** `Active development` — where the project is in its life.
- **Compat obligation:** External — from v0.1.0 people install Navori with Homebrew and keep their data across upgrades: `navori.db` (forward-only migrations), `config.toml` keys (a rename is a migration), hook entries, Keychain service names and the signing identity stay stable.
- **Default PR base:** `main` — `land-branch` reads it as the third rung of its base-resolution ladder.
- **Default landing action:** `merge`

These are distinct from product goals (what success looks like): posture is *how carefully
to build right now*, not *what to build*.

## Team

Who works on this repo and how skills should package collaboration.
Skills that plan, review, or hand off read this section when present and
right-size **packaging** only (Solo / Small / Multi) — Iron Law gates never
change. Edit freely; re-run `/configure-repo` to re-draft from git/CODEOWNERS.
If this section is absent, skills do not invent a team.

**SSOT:** **band** derivation and the **packaging** matrix live only here.
Consumers **read** this section; they do not re-copy these rules into skill bodies.

### Roster

- Contributor — Jayden Đặng

Suggested roles (freeform allowed): Tech Lead, Backend Engineer, Frontend Engineer, Full-stack Engineer, Designer, Product Manager, QA, DevOps/SRE, Docs.

### Ownership notes (optional)

- none

### Workflow band

- **Override (optional):** blank — skills derive the band (currently **Solo**).
- **Derive (when override blank):**
  1. Headcount from **Roster only**: each `Role — Name` = 1; each `N × Role` / `N Role(s)` adds N.
     Ignore Ownership notes and placeholders (`<…>`).
  2. Buckets: empty roster → **no band** (same packaging as Team absent); 1 → **Solo**;
     2–4 → **Small**; ≥5 → **Multi**.
  3. Specialty upgrade only: if **Small** and ≥3 distinct role titles (case-insensitive, trimmed),
     upgrade to **Multi**. Never downgrade Multi→Small.

### Packaging matrix

| Band | Packaging |
|---|---|
| **Solo** | Lean multi-person ritual language; no invented peer reviewers/assignees; agent-as-pair; full gates |
| **Small** | Design-review checkpoints; ownership boundaries via optional freeform notes; name people when roster has names |
| **Multi** | CODEOWNERS-aware review language when ownership notes exist; explicit review responsibilities as prose; write-handoff/docs emphasis |
| **(no band)** | Team absent, or empty roster with blank override — pre-feature default; do not invent a team; do not hard-fail |

## verify commands

Run in this order; all must pass before any completion claim.

| Check | Command |
|---|---|
| Typecheck | `cargo check --workspace --all-targets` |
| Lint | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings` |
| Unit tests | `cargo nextest run --workspace` |
| E2E / smoke | none as a command — the boot-and-drive recipe is `docs/agents/verify.md` |

Single test file: `cargo nextest run -p <crate> --test <file>` — e.g.
`cargo nextest run -p navori-redact --test redact_json`. Builds only that test binary.

Task gate (changed units + their dependents; `none` when the whole suite is fast):
`none` — the whole suite is fast; the suite is the gate.

Wave gate (compile + lint every target after a parallel merge; `none` = typecheck + lint):
`cargo xtask ci` — fmt check, clippy `-D warnings`, `cargo test --workspace`, `check-deps`
(crate dependency rules), `build-core` (UniFFI bindings + XCFramework), Swift build. It is
the only command that compiles the Swift shell (`apps/macos`).

The whole suite runs once, at close. Each task runs its task gate instead, escalating
to the whole suite when the change reaches what the build graph cannot see (schema,
migrations, shared fixtures, manifests, generated contracts, a root package).

The traceability check is not a command here — the `audit-trace` skill runs it as
`grep`/`git` over `docs/specs/` (and optional architecture). It is **docs-only**
and does not grep application tests for requirement IDs.

Legacy annotations in consumer code (if any) are ignored; do not require new ones.

Specs directory override: (blank — `docs/specs/`)

## Run locally (dev)

How to start the app for user-facing acceptance checks (read by `validate-api`,
`validate-ui`, `validate-feature`). Full recipe: `docs/agents/verify.md`.

| Surface | Start command | Ready signal |
|---|---|---|
| Core over IPC (Unix socket) — the API the CLI, UI and AI call | `cargo xtask run` | `"build/dev/Navori Dev.app/Contents/Helpers/navori" --profile dev doctor` exits 0 with every check ✓ |
| Menubar UI (native SwiftUI/AppKit — not a browser) | same launch | the Navori Dev icon appears in the menu bar |

Native macOS app: there is no web frontend, so kimi-webbridge does not apply. Drive the
core through the CLI. Do not add a Playwright suite for acceptance.

## Remote environments

Read by `debug-remote` and `assess-observability`. Never put tokens here.

None — not deployed

## release steps

Run in order; `cut-release` executes exactly these and does not improvise. `<version>` is
`[workspace.package] version` in `Cargo.toml`.

1. `cargo xtask release-check` — the local gate: tag free, tree clean, `cargo xtask ci`,
   `cargo xtask perf` (GOAL-11 budget; `--record` is not part of a release). On success it
   prints the tag to push.
2. `git tag v<version>`
3. `git push origin v<version>` — pushing a `v*` tag runs `.github/workflows/release.yml`
   (it runs only once that file is on the default branch, see Release setup), which builds
   and signs the zip and always creates the Release as a draft first. While the repository
   variable `TAP_REPOSITORY` is set, the workflow also audits the cask in that tap
   (`brew style`, `brew audit`) and stops before publishing if the audit fails. The Release
   is published and the cask pushed to the tap only when the repository variable
   `PUBLISH_RELEASE` is exactly `true`; otherwise the Release stays a draft and nothing is
   pushed. Pre-release tags behave the same way, but their cask is `navori-rc`
   (`brew install --cask navori-rc`); final tags use `navori`. Build the artifacts by hand
   with `cargo xtask release --tag v<version> --out <dir>` (`--check` only validates the
   tag).
4. Clean-account checks (RELEASE-6.3), once the Release is published and the cask updated.
   Use a macOS 26 user account that has never run Navori. For a pre-release check, use the
   cask `navori-rc` in every command below; for a final release, use `navori`:
   1. RELEASE-3.7: `brew tap jayden-dang/navori`,
      `brew trust --cask jayden-dang/navori/<cask>`, `brew install --cask <cask>`, then open
      Navori. Expected: it opens with no Gatekeeper dialog.
   2. RELEASE-3.8: in Settings → Integrations install the Claude Code hooks, work in a
      Claude Code session, then run `pmset -g assertions`. Expected: it lists
      "Navori: keeping this Mac awake".
   3. RELEASE-4.4: with two releases of the same kind published (two pre-releases, then
      `brew upgrade --cask navori-rc`; or two finals, then `brew upgrade --cask navori`),
      install the older, set a setting, keep the hooks and save an AI key, then upgrade.
      Expected: settings, hooks and the saved AI key survive with no Keychain prompt and no
      Gatekeeper dialog.

   The 2026-11-04/05 run of these checks against the test tap is recorded in
   `docs/specs/2026-11-04-public-release/acceptance.md`.

Optional local check, not part of the release order: `cargo xtask install` builds the
release bundle, signs it with `Navori Local Signing`, runs `codesign --verify --strict
--deep`, installs to `/Applications/Navori.app` (the previous install is kept as
`/Applications/Navori.app.previous`) and launches it; then
`/Applications/Navori.app/Contents/Helpers/navori doctor` should show every check ✓ and
exit 0.

No notarization (no Apple Developer ID): the app is self-signed and the cask clears the
quarantine flag.

The workflow must only run on GitHub-hosted runners: the signing identity and the
`add-trusted-cert` trust entry live only on a discarded VM (ADR 0007). Never point it at a
self-hosted runner.

Known limit, accepted: the guarded hook command keeps `exec`, so a helper that is present
but cannot run (for example a bad interpreter during a half-finished upgrade) can end the
shell with a non-zero code past `|| true`; the harness then shows a non-blocking hook error.
Revisit if reported.

Release candidates and the stable release are separate casks. A pre-release tag installs as
`navori-rc` and never touches `Casks/navori.rb`; a final tag installs as `navori`. The two
conflict, so switching a user from rc to stable is
`brew uninstall --cask navori-rc && brew install --cask navori`; settings, hooks and the
saved AI key stay (uninstall without `--zap` leaves the data directories).

If the tap push fails after the Release is already public, the Release and the assets are
fine and the tap still names the previous version (which installs). Re-run the failed job
from the Actions tab: the rerun reuses the published assets and finishes the push, without a
second Release.

## Release setup (one time)

Do this once, before the first tag. Create nothing here from an agent session and commit
nothing secret.

1. Export the `Navori Local Signing` identity with its private key as a `.p12`: in Keychain
   Access select that single identity, File → Export Items…, format `.p12`. Choose a
   NON-EMPTY export passphrase and store it as `SIGNING_P12_PASSWORD`. Keep an OFFLINE copy
   (encrypted drive): losing the key means a new identity and a new designated requirement
   for every user. (CLI alternative: `security export -k <keychain> -t identities -f pkcs12
   -o navori.p12 -P <passphrase>` exports EVERY identity in that keychain, so use it only
   against a keychain that holds just this identity.)
2. Create the tap repository (`jayden-dang/homebrew-navori`) and a fine-grained token with
   `contents: write` on that tap ONLY. Verify the token's repository access lists only the
   tap (RELEASE-7.6).
3. Create the GitHub Environment `release`, restricted to tags matching `v*`, with secrets
   `SIGNING_P12` (`base64 < navori.p12`), `SIGNING_P12_PASSWORD` and `TAP_TOKEN` (the token
   from step 2; it must be non-empty before `TAP_REPOSITORY` is set).
4. Add a repository ruleset so only the owner can create `v*` tags.
5. Set two repository variables, which are separate switches:
   - `TAP_REPOSITORY` names the tap (`jayden-dang/homebrew-navori`). While it is set, every
     tag audits the cask in that tap (`brew style`, `brew audit`). While `PUBLISH_RELEASE`
     is off this is audit-only: nothing is published or pushed.
   - `PUBLISH_RELEASE` publishes. Leave it UNSET until the first public release; tags then
     produce draft Releases only. `PUBLISH_RELEASE=true` (any other value is off) publishes
     the Release and, when `TAP_REPOSITORY` is set, pushes the updated cask to the tap. With
     `PUBLISH_RELEASE` true and `TAP_REPOSITORY` empty, the Release is published and no tap
     is touched.
6. A tag runs the workflow only once `.github/workflows/release.yml` is on the default
   branch: `v0.1.0-rc.1`, pushed while the file existed only on a feature branch, started no
   run. Merge the workflow before the first tag.
7. Right before the first FINAL tag (not a pre-release), add `- **Compat obligation:**
   External` as a line of its own in the `## Project posture` bullet list (it must start
   the line); `cargo xtask release-check` enforces it.

## Paths

- Specs: `docs/specs/`
- ADRs: `docs/adr/`
- Glossary: `CONTEXT.md`
- Out-of-scope KB: `.out-of-scope/`
- Product vision (GOAL-N): `docs/product/vision.md`
- Architecture invariants (ARCH-N spine): `docs/architecture/INDEX.md`
- Architecture narrative: `docs/ARCHITECTURE.md`
- Engineering standards (SSOT, incl. Definition of Done): `docs/standards/`
- UI briefs: `docs/design/`
- Library docs: Context7 MCP (preferred) — connected in the user's MCP config; fall back to official docs when absent
