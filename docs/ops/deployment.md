# Deployment

Status: Approved
Date: 2026-11-01

## Purpose and boundary

How Navori goes from source to a running app on the user's Mac, how a new version replaces the old one, and how to go back. Navori has no servers: "deployment" means building, signing and installing a macOS app bundle for a single user, plus (later) publishing a signed build for other developers to install themselves.

Not covered here: the executable commands (`docs/agents/project.md`, release steps), process behaviour once running (`docs/architecture/runtime.md`), stored data (`docs/architecture/data.md`).

## Environments / pipeline

| Environment | Deploy path |
|---|---|
| Build (every change) | Rust static library for `navori-ffi` (Apple Silicon, deployment target macOS 26.0) → UniFFI Swift bindings, headers and module map → `NavoriCoreFFI.xcframework` → SwiftPM build of the Shell → assemble the `.app` (app binary, `Helpers/navori`, generated `Info.plist` with the Profile and bundle id) → sign the CLI, then the app, with hardened runtime. |
| `dev` | `build/dev/Navori Dev.app` (bundle id `dev.navori.app.debug`), rebuilt and relaunched in place by the dev run command; never touches the installed `release` copy or its data. |
| `release` (now — single user) | The full gate passes → release bundle signed with the stable self-signed identity `Navori Local Signing` → signature verified (`--strict --deep`) → installed to `/Applications/Navori.app` → `navori doctor` smoke check → version tag. Updating is a `git pull` followed by the same path. |
| `release` (later — open source) | Same pipeline with a Developer ID identity, plus notarization and stapling, a DMG, a Sparkle update feed, and a Homebrew cask / download page so other developers install and update it themselves. Requires choosing a license first. |

Signing: every build is signed with one stable identity so macOS keeps Keychain access and privacy permissions (Accessibility, Screen Recording) across rebuilds. If the identity is missing, the build falls back to ad-hoc signing and warns loudly; such a build must not be installed as `release`.

## Rollout / rollback

- One install at a time: the running `release` app is asked to quit (and waited for) before it is replaced.
- The current `/Applications/Navori.app` is kept as `/Applications/Navori.app.previous` before the new one is copied in; any older `.previous` is removed.
- If copying the new bundle fails, the previous app is restored automatically and the install reports failure.
- Manual rollback: quit Navori, move `Navori.app.previous` back to `Navori.app`, relaunch; or check out the previous tag and install again.
- Data moves forward only: an upgrade that migrates existing data first copies the whole database to `backups/` (5 newest kept). Rolling the app back after such a migration means restoring the matching backup as `navori.db` while Navori is stopped — an older build does not understand a newer schema.
- A release is not done until the smoke check passes (`navori doctor`, every check ✓) and the Definition of Done in `docs/standards/INDEX.md` holds, including the performance gate.
- `dev` builds are never promoted to `release`; `release` is always built from a clean, tagged commit.

## Not executable release steps

Executable commands stay in `docs/agents/project.md` release steps. This is narrative only.
