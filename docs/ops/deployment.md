# Deployment

Status: Approved
Date: 2026-11-01

## Purpose and boundary

How Navori goes from source to a running app on the user's Mac, how a new version replaces the old one, and how to go back. Navori has no servers: "deployment" means building, signing and installing a macOS app bundle on the maintainer's Mac, and publishing a signed build that other developers install with Homebrew (ADR 0007).

Not covered here: the executable commands (`docs/agents/project.md`, release steps), process behaviour once running (`docs/architecture/runtime.md`), stored data (`docs/architecture/data.md`).

## Environments / pipeline

| Environment | Deploy path |
|---|---|
| Build (every change) | Rust static library for `navori-ffi` (Apple Silicon, deployment target macOS 26.0) → UniFFI Swift bindings, headers and module map → `NavoriCoreFFI.xcframework` → SwiftPM build of the Shell → assemble the `.app` (app binary, `Helpers/navori`, generated `Info.plist` with the Profile and bundle id) → sign the CLI, then the app, with hardened runtime. |
| `dev` | `build/dev/Navori Dev.app` (bundle id `dev.navori.app.debug`), rebuilt and relaunched in place by the dev run command; never touches the installed `release` copy or its data. |
| `release` (maintainer's Mac) | The full gate passes → release bundle signed with the stable self-signed identity `Navori Local Signing` → signature verified (`--strict --deep`) → installed to `/Applications/Navori.app` → `navori doctor` smoke check. Updating is a `git pull` followed by the same path. |
| `release` (public, Homebrew) | The maintainer runs `cargo xtask release-check` (tag free, tree clean, the full gate, the performance gate) and pushes a tag `v<version>` that equals the workspace version. GitHub Actions (`.github/workflows/release.yml`, GitHub-hosted runner only) then checks the tag sits on `main` and was pushed by the owner, runs the full gate, imports the signing identity from the `release` Environment, builds and signs the bundle, checks its designated requirement against the committed one, zips it and records its SHA-256, and creates a draft GitHub Release. When the repository variable `TAP_REPOSITORY` is set it audits the cask in that tap; when `PUBLISH_RELEASE` is `true` it also proves write access to the tap, publishes the Release, checks the public asset against its hash and pushes the cask. A final tag writes `navori` (`Casks/navori.rb`), a pre-release tag writes `navori-rc` (`Casks/navori-rc.rb`); the two casks conflict. The app is self-signed and not notarized, so the cask clears the quarantine flag. |
| `release` (later) | A Developer ID identity with notarization and stapling, an update feed (Sparkle) and the official `homebrew/cask` replace the self-signed path; each needs its own decision. |

Signing: every build, on the maintainer's Mac and on the release runner, is signed with one stable identity so macOS keeps Keychain access and privacy permissions (Accessibility, Screen Recording) across rebuilds and across `brew upgrade`. The private key exists only on the maintainer's Mac, in the `release` Environment secrets and in an offline backup; the release workflow refuses to sign ad-hoc and never runs on a self-hosted runner. If the identity is missing, the build falls back to ad-hoc signing and warns loudly; such a build must not be installed as `release`.

## Rollout / rollback

- One install at a time: the running `release` app is asked to quit (and waited for) before it is replaced.
- The current `/Applications/Navori.app` is kept as `/Applications/Navori.app.previous` before the new one is copied in; any older `.previous` is removed.
- If copying the new bundle fails, the previous app is restored automatically and the install reports failure.
- Manual rollback on the maintainer's Mac: quit Navori, move `Navori.app.previous` back to `Navori.app`, relaunch; or check out the previous tag and install again.
- Homebrew users: `brew upgrade --cask navori` moves forward only. Going back means uninstalling and installing the older version's cask file from the tap's git history; data rules below still apply. A release candidate user switches to the stable cask with `brew uninstall --cask navori-rc && brew install --cask navori`; settings stay.
- A failed tap push after the Release went public is healed by re-running the failed workflow job: it reuses the published zip after checking its hash, signature and version, and creates nothing twice.
- Data moves forward only: an upgrade that migrates existing data first copies the whole database to `backups/` (5 newest kept). Rolling the app back after such a migration means restoring the matching backup as `navori.db` while Navori is stopped — an older build does not understand a newer schema.
- A release is not done until the smoke check passes (`navori doctor`, every check ✓) and the Definition of Done in `docs/standards/INDEX.md` holds, including the performance gate.
- `dev` builds are never promoted to `release`; `release` is always built from a clean, tagged commit.
- The performance gate cannot run on a hosted runner; it runs locally in `cargo xtask release-check`. A release that goes out without it green is a recorded exception in the feature's acceptance record (v0.1.0 is one: `docs/specs/2026-11-04-public-release/acceptance.md`).

## Not executable release steps

Executable commands stay in `docs/agents/project.md` release steps. This is narrative only.
