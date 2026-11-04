# Requirements: Public release

Feature code: RELEASE
Status: Approved
Date: 2026-11-04

<!--
Rules:
- Every acceptance criterion gets a hierarchical ID: RELEASE-<story>.<criterion>.
- Criteria use EARS phrasing (WHEN / WHILE / IF…THEN / WHERE / ubiquitous).
- Guard requirements protect existing behavior: WHEN <condition> THE SYSTEM SHALL CONTINUE TO <behavior>.
- IDs are immutable once Status is Approved. Retire by strikethrough — never renumber.
-->

Roadmap item: ROAD-27 (MILE-9) · Tracker: #22 · Decisions: frame-change close package (2026-11-04), confirmed by the user; ADR 0007 (amends ADR 0004).

Delivery order: stories 1–6 can be built and tried now against a test tap. The first public release, `v0.1.0`, is cut only after MILE-2 closes and ROAD-17 and ROAD-18 ship (roadmap MILE-9 `Depends-on`), by setting the tap variable (RELEASE-6.1) and pushing the tag.

Terms used below:
- **The system** — the `cargo xtask` release commands, the release workflow in `.github/workflows/`, the cask in the tap, and the Integrations Module's hook commands.
- **Workspace version** — `[workspace.package] version` in `Cargo.toml`.
- **Release tag** — a git tag `v<semver>` whose `<semver>` is a valid Semantic Versioning 2.0 version, with or without a pre-release part (`v0.1.0`, `v0.2.0-rc.1`).
- **Pre-release tag** — a release tag whose version has a pre-release part.
- **Release Environment** — the GitHub Environment `release` on `jayden-dang/NAVORI`, which holds `SIGNING_P12`, `SIGNING_P12_PASSWORD` and `TAP_TOKEN`, and lets only jobs started by a `v*` tag read them.
- **Tap** — the repository named by the repository variable `TAP_REPOSITORY` (for the public release, `jayden-dang/homebrew-navori`); its cask is `Casks/navori.rb`.
- **Release zip** — `Navori-<version>.zip`, holding `Navori.app` at its top level.
- **Signing identity** — the self-signed certificate `Navori Local Signing` (ADR 0004, ADR 0007).
- **Designated requirement** — the code requirement macOS uses to decide that two builds are the same app (`codesign -d -r-`); Keychain access and privacy grants follow it.
- **Guarded hook command** — a hook command that runs the Navori helper only when the helper is a regular, executable file, and otherwise exits 0 doing nothing.

Files this change touches, and what in them is guarded:
- `xtask/src/main.rs` — bundle, sign, install, run, ci, perf: RELEASE-1.13, 1.14, 1.15, 2.8.
- `modules/integrations/src/domain/hookfile.rs`, `identity.rs` — hook command and recognition: RELEASE-5.6–5.10.
- `crates/store/` — startup backup and migrations, read not changed: RELEASE-4.5.
- `.github/workflows/release.yml` (new), the tap's `Casks/navori.rb` (new), the committed designated requirement file (new) — no behavior to guard.
- `docs/agents/project.md` — release steps text only: no behavior to guard.

## 1. Publish a release by pushing a tag

**Story:** As the maintainer, I want pushing the tag `v0.1.0` to publish a signed Navori on its GitHub Release, so that releasing never needs more than a tag.

- **RELEASE-1.1** WHEN a release tag is pushed to `jayden-dang/NAVORI` THE SYSTEM SHALL start the release workflow for that tag.
- **RELEASE-1.2** WHEN a commit is pushed to any branch without a release tag THE SYSTEM SHALL publish no Release and change no cask.
- **RELEASE-1.3** IF the version in the release tag differs from the workspace version at the tagged commit THEN THE SYSTEM SHALL fail the workflow before building, naming both versions.
- **RELEASE-1.4** WHEN the release workflow runs THE SYSTEM SHALL run `cargo xtask ci` on the tagged commit before it builds the release bundle.
- **RELEASE-1.5** WHEN the release workflow builds the bundle THE SYSTEM SHALL sign `Navori.app` and its helper with the signing identity read from the Release Environment.
- **RELEASE-1.6** IF the signing identity cannot be imported or is not found in the workflow THEN THE SYSTEM SHALL fail the workflow and SHALL NOT sign ad-hoc.
- **RELEASE-1.7** IF the designated requirement of the built `Navori.app` differs from the one committed in the repository THEN THE SYSTEM SHALL fail the workflow before publishing, showing both requirements.
- **RELEASE-1.8** WHEN the release bundle is built from a release tag THE SYSTEM SHALL set `CFBundleShortVersionString` to the tag's version and `CFBundleVersion` to the number of commits reachable from the tagged commit.
- **RELEASE-1.9** WHEN the bundle is built and verified THE SYSTEM SHALL publish a GitHub Release named `v<version>` on `jayden-dang/NAVORI` carrying the release zip and a file `Navori-<version>.zip.sha256` holding the zip's SHA-256.
- **RELEASE-1.10** WHEN the release tag is a pre-release tag THE SYSTEM SHALL mark its GitHub Release as a pre-release.
- **RELEASE-1.11** IF any step before the GitHub Release is created fails THEN THE SYSTEM SHALL create no Release and change no cask.
- **RELEASE-1.12** WHEN the release workflow is run again for a tag whose GitHub Release already exists THE SYSTEM SHALL finish the remaining steps without creating a second Release or a second asset of the same name.
- **RELEASE-1.13** (guard) WHEN `cargo xtask install` runs on the maintainer's Mac THE SYSTEM SHALL CONTINUE TO build the release bundle, sign it with the local signing identity, verify it with `codesign --verify --strict --deep`, keep the previous install as `/Applications/Navori.app.previous` and launch the new one.
- **RELEASE-1.14** (guard) WHEN `cargo xtask bundle`, `run` or `install` runs where the signing identity is missing THE SYSTEM SHALL CONTINUE TO sign ad-hoc with the existing warning.
- **RELEASE-1.15** (guard) WHEN `cargo xtask run` builds the dev profile THE SYSTEM SHALL CONTINUE TO produce `build/dev/Navori Dev.app` with the dev bundle id, data, logs and socket.

## 2. Check a release locally before tagging

**Story:** As the maintainer, I want `cargo xtask release-check` to tell me the exact tag to push once the gate and the perf budget pass, so that no release skips the GOAL-11 budget that CI cannot measure.

- **RELEASE-2.1** WHEN `cargo xtask release-check` runs THE SYSTEM SHALL run `cargo xtask ci` and then `cargo xtask perf`, stopping at the first that fails.
- **RELEASE-2.2** WHEN both pass THE SYSTEM SHALL print the release tag `v<workspace version>` and the `git tag` and `git push` commands that publish it.
- **RELEASE-2.3** IF the workspace version is not a valid Semantic Versioning 2.0 version THEN THE SYSTEM SHALL fail before running the gate, naming the version.
- **RELEASE-2.4** IF the tag `v<workspace version>` already exists locally or on `origin` THEN THE SYSTEM SHALL fail before running the gate, naming the tag.
- **RELEASE-2.5** IF the working tree has uncommitted changes to tracked files THEN THE SYSTEM SHALL fail before running the gate, listing the changed files.
- **RELEASE-2.6** IF the workspace version has no pre-release part and `docs/agents/project.md` does not state `**Compat obligation:** External` THEN THE SYSTEM SHALL fail before running the gate, naming the missing line.
- **RELEASE-2.7** THE SYSTEM SHALL NOT create or push any tag from `cargo xtask release-check`.
- **RELEASE-2.8** (guard) WHEN `cargo xtask ci` or `cargo xtask perf` runs on its own THE SYSTEM SHALL CONTINUE TO run every check it runs today, with the same exit codes; the only check added is the one RELEASE-7.4 asks for.

## 3. Install Navori from the tap

**Story:** As a developer on an Apple Silicon Mac with macOS 26, I want `brew install --cask navori` from the Navori tap to give me a Navori that opens without a Gatekeeper prompt, so that I can use it without building it.

- **RELEASE-3.1** WHEN a GitHub Release is published and `TAP_REPOSITORY` is set THE SYSTEM SHALL write `Casks/navori.rb` in the tap with the release's version, the release zip's download URL and its SHA-256, in one commit.
- **RELEASE-3.2** THE SYSTEM SHALL declare in the cask that Navori needs an arm64 Mac and macOS 26 or later.
- **RELEASE-3.3** WHEN the cask is installed THE SYSTEM SHALL place `Navori.app` in the Homebrew app directory and link `Contents/Helpers/navori` onto the Homebrew `bin` path as `navori`.
- **RELEASE-3.4** WHEN the cask is installed or upgraded THE SYSTEM SHALL remove the `com.apple.quarantine` attribute from the installed `Navori.app` and everything inside it.
- **RELEASE-3.5** WHEN the cask is installed THE SYSTEM SHALL print caveats saying that Navori is self-signed and not notarized, that the cask removed the quarantine flag, and that hooks are removed from Settings → Integrations before uninstalling.
- **RELEASE-3.6** IF `brew style` or `brew audit --cask` reports an error for the written cask THEN THE SYSTEM SHALL push nothing to the tap and fail the workflow.
- **RELEASE-3.7** WHEN a person on a macOS 26 account that has never run Navori runs `brew tap`, `brew trust --cask` and `brew install --cask` for the tap's `navori` and opens Navori THE SYSTEM SHALL show the menubar icon with no Gatekeeper dialog — checked by hand on a clean account.
- **RELEASE-3.8** WHEN that person installs the Claude Code hooks from Settings → Integrations and works in a Claude Code session THE SYSTEM SHALL list the assertion `Navori: keeping this Mac awake` in `pmset -g assertions` — checked by hand on the same account.

## 4. Upgrade without losing anything

**Story:** As a Navori user, I want `brew upgrade` to move me to the new version with my settings, history, hooks and saved keys still in place, so that updating never costs me setup.

- **RELEASE-4.1** WHEN the cask is upgraded THE SYSTEM SHALL leave `config.toml`, `navori.db` and `backups/` under `~/Library/Application Support/Navori/` in place, changed only by the new version's own migrations.
- **RELEASE-4.2** WHEN the upgraded Navori starts THE SYSTEM SHALL read the Keychain items saved by the previous release with no Keychain prompt.
- **RELEASE-4.3** WHEN the upgraded Navori starts THE SYSTEM SHALL report Navori's installed hooks as installed, not as needing repair.
- **RELEASE-4.4** WHEN a person upgrades from one release to the next on the clean account of RELEASE-3.7 THE SYSTEM SHALL keep the settings, hooks and saved AI key from before the upgrade — checked by hand from one pre-release to the next.
- **RELEASE-4.5** (guard) WHEN a storage owner that already has data applies pending migrations THE SYSTEM SHALL CONTINUE TO back up the database first and run each migration in its own transaction (ARCH-12).

## 5. Uninstall without leaving broken hooks

**Story:** As a Navori user who uninstalls it, I want Claude Code, Codex and Grok to keep working with no hook errors, so that removing Navori never breaks my other tools.

- **RELEASE-5.1** WHEN Navori installs or repairs its hooks THE SYSTEM SHALL write each one as a guarded hook command.
- **RELEASE-5.2** WHEN a guarded hook command runs and the helper is missing, is not a regular file, or is not executable THE SYSTEM SHALL exit 0 with no output.
- **RELEASE-5.3** WHEN a guarded hook command runs and the helper is executable THE SYSTEM SHALL run it with the same arguments and the same standard input as today's hook command (`hook` for release, `--profile dev hook` for dev).
- **RELEASE-5.4** WHEN a Hook file holds Navori's hooks in the earlier unguarded form THE SYSTEM SHALL report them as needing repair, and repair SHALL rewrite them as guarded hook commands.
- **RELEASE-5.5** WHEN `brew uninstall --zap` removes the cask THE SYSTEM SHALL move `~/Library/Application Support/Navori/` and `~/Library/Logs/Navori/` to the Trash, and SHALL leave the dev profile's directories alone (ARCH-14).
- **RELEASE-5.11** WHEN Navori reads a Hook file THE SYSTEM SHALL count a guarded hook command for the running profile as one of Navori's installed hooks, so that status reports it installed, repair adds no second copy and remove deletes it.
- **RELEASE-5.6** (guard) WHEN Navori's hook entries are installed, repaired or removed THE SYSTEM SHALL CONTINUE TO leave every other hook in the Hook file unchanged (TB-7, THR-9).
- **RELEASE-5.7** (guard) WHEN a guarded hook command runs inside an Agent session THE SYSTEM SHALL CONTINUE TO resolve that session's Harness and hold its Session Lease, as with today's hook command.
- **RELEASE-5.8** (guard) WHEN Navori's hooks are removed THE SYSTEM SHALL CONTINUE TO remove the running profile's hooks, in the guarded and in the earlier form.
- **RELEASE-5.9** (guard) WHEN release and dev hooks are both in one Hook file THE SYSTEM SHALL CONTINUE TO tell them apart by profile.
- **RELEASE-5.10** (guard) WHEN Navori writes its hooks THE SYSTEM SHALL CONTINUE TO give `SessionEnd` a 1-second timeout and every other event a 2-second timeout.

## 6. Prepare now, publish when told

**Story:** As the maintainer, I want tags pushed before I am ready to produce only a draft Release that nobody can install, so that the whole pipeline can be built and tried before the first public release.

- **RELEASE-6.1** WHILE the repository variable `TAP_REPOSITORY` is unset THE SYSTEM SHALL create the GitHub Release as a draft and SHALL change no cask.
- **RELEASE-6.2** WHEN `TAP_REPOSITORY` is set THE SYSTEM SHALL publish the GitHub Release and update that tap's cask (RELEASE-3.1).
- **RELEASE-6.3** WHEN the first final release is published THE SYSTEM SHALL list in `docs/agents/project.md` release steps, in order: `cargo xtask release-check`, `git tag v<version>`, `git push origin v<version>`, and the clean-account checks of RELEASE-3.7, RELEASE-3.8 and RELEASE-4.4.

## 7. Quality attributes

**Section-kind:** nfr

**Story:** As a stakeholder, I want measurable quality targets for this feature, so that how-well is not left implicit.

- **Performance:** **RELEASE-7.1** WHEN `cargo xtask release-check` passes THE SYSTEM SHALL have held the release `.app` to ≤ 25 MB and every other GOAL-11 budget — verified by `cargo xtask perf` inside the check (`docs/standards/performance.md`).
- **Performance:** **RELEASE-7.2** WHEN a guarded hook command runs with the helper present THE SYSTEM SHALL finish within the hook's existing 2-second timeout (1 second for `SessionEnd`) — verified by a test that runs the guarded command through `/bin/sh -c`.
- **Security:** **RELEASE-7.3** THE SYSTEM SHALL expose `SIGNING_P12`, `SIGNING_P12_PASSWORD` and `TAP_TOKEN` only to jobs of the release workflow started by a `v*` tag, and SHALL never print them or write them into the repository, an artifact or the Release — verified by the Environment's deployment rule and a review of the workflow.
- **Security:** **RELEASE-7.4** THE SYSTEM SHALL pin every third-party action in the release workflow to a full commit SHA — verified by a check in `cargo xtask ci` that rejects a `uses:` line without a 40-character SHA.
- **Security:** **RELEASE-7.5** WHEN the release workflow ends, whether it passed or failed, THE SYSTEM SHALL delete the temporary keychain that held the signing identity — verified by the workflow's always-run cleanup step.
- **Security:** **RELEASE-7.6** THE SYSTEM SHALL use a `TAP_TOKEN` that can write only to the tap repository — verified when the token is created (fine-grained, contents write on that repository only).
- **Reliability:** **RELEASE-7.7** WHEN a release workflow fails after the GitHub Release exists THE SYSTEM SHALL leave the Release and the tap in a state a rerun completes (RELEASE-1.12) — verified by rerunning a failed pre-release run on a test tap.
- **Accessibility:** None — the feature has no screen; the only text a person reads is terminal output (caveats, `release-check`).

## Out of Scope

- Developer ID signing, notarization and stapling.
- Auto-update inside the app (Sparkle or any update feed).
- Submitting the cask to the official `homebrew/cask`.
- Hiding or cutting Modules, menu rows, Settings tabs or CLI commands for the public build — the whole app ships.
- A Homebrew formula that builds Navori from source.
- Running `cargo xtask perf` on CI.
- A DMG or a download page.
- Intel Macs and macOS versions before 26.
- Removing Navori's hooks automatically when the cask is uninstalled.
- Moving the app somewhere other than the Homebrew app directory and keeping hooks working there.
- Publishing `v0.1.0` before MILE-2 closes and ROAD-17 and ROAD-18 ship.

## Open Questions

None.
