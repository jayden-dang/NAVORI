# Acceptance: Public release (RELEASE, ROAD-27)

Run of Task 9, 2026-11-04 to 2026-11-05, against the test tap `jayden-dang/homebrew-navori-test` (private). The public tap `jayden-dang/homebrew-navori` and the first final tag `v0.1.0` do not exist yet.

## Proved on GitHub's hosted runner

| Check | Requirement | Result | Evidence |
|---|---|---|---|
| A push to a branch starts no release run | 1.2 | not isolated | The workflow's only trigger is `push: tags: ['v*']` (read, and parsed with PyYAML). Every run on GitHub so far belongs to a tag; no test pushed a branch alone to watch for the absence of a run, and `main` was pushed several times with no run of its own |
| A tag pushed before `release.yml` is on `main` (caveat for 1.1: a tag starts the workflow only once it is on the default branch) | 1.1 | observed, cause not isolated | `v0.1.0-rc.1` (workflow on the tag's commit, not yet on `main`) started no run in about ten minutes; `v0.1.0-rc.2`, pushed after `main` had the workflow, ran at once. The likely cause is that GitHub registers a workflow from the default branch, but that was not tested |
| Tag, CI gate, signing import, build, draft Release, upload run end to end | 1.1, 1.4, 1.5, 1.8, 1.9, 1.10 | pass | Run 41842439979 (rc.4) up to the upload; run 41954661262 (rc.8) to the end |
| The signed app keeps the committed designated requirement | 1.7 | pass | rc.4 and rc.8 zips: `designated => identifier "dev.navori.app" and certificate leaf = H"8c56e155bdd4ed3c038035509193fb7d33d302c1"`, same for the helper; `CFBundleShortVersionString` equals the tag |
| Zip matches its `.sha256` | 1.9 | pass | `shasum -a 256 -c` on the downloaded rc.4 and rc.8 assets |
| Publish switch off: Release stays a draft, no cask pushed | 6.1 | pass | Run 41854710186 (rc.5, cancelled in cleanup) and run 41954661262 first rerun: steps 13 and 14 skipped, the tap held only `Initial commit` |
| Publish switch off with a tap set: the cask is still audited | 6.4, 3.6 | pass | Same runs: step 12 (`brew tap`, `brew trust`, `brew style`, `brew audit`) succeeded |
| Publish switch on: Release published, then the cask pushed | 6.2, 3.1, 3.9 | pass | Run 41954661262, second rerun: `v0.1.0-rc.8` public as a pre-release, not "Latest" (API `releases/latest` answered 404); tap commit `navori-rc 0.1.0-rc.8` added `Casks/navori-rc.rb` and no `Casks/navori.rb` |
| A rerun creates no second Release and no new tap commit | 1.12 | pass | Third rerun: Releases named `v0.1.0-rc.8` stayed 1, tap commits stayed 2, assets stayed 2 |
| `brew` can tap, trust and fetch the rc cask and verify its hash | 3.1 | pass | `brew tap`, `brew trust --cask`, `brew info`, `brew fetch --cask jayden-dang/navori-test/navori-rc` on the maintainer's Mac; not installed there |
| The cleanup step ends | 7.5 | pass after a fix | rc.4 and rc.5 hung in `sudo security remove-trusted-cert`; removed in commit 2ba11b8; rc.6 onward finish |

A wrong tag version that must stop before `ci` (1.3), a deliberately broken tap step (7.7) and the `release-check` run with a clean tree (2.1, 2.2) were not exercised on GitHub: 1.3 is covered by unit tests and by the tag check step succeeding for every matching tag; 7.7 only by the rerun above; `release-check` stopped at the performance gate on 2026-11-04 (`menu_open_after_launch_ms` 82.9 ms against 80, `snapshot_ms` 12.6 ms against 10) and the operator chose to push release candidates without it.

## Found by running the pipeline, outside RELEASE

Fixed in separate commits on `main`: Rust toolchain pinned to 1.97.1 and `as_chunks` (a newer clippy rejected `chunks_exact`); a flaky log-capture test in `ai/context`; same-millisecond journal events returned in random order (`ORDER BY ts_ms DESC, rowid DESC`); a 10 ms exit-latency assertion that a shared runner cannot hold (skipped when `CI` is set).

## Done by the operator on a clean macOS 26 account (reported 2026-11-05)

The operator reported each group below as passing; no log or screenshot was attached to this record. The cask installed was `navori-rc` from the private test tap; the upgrade went from `0.1.0-rc.8` to `0.1.0-rc.9`.

| Check | Requirement | Result |
|---|---|---|
| `brew tap`, `brew trust --cask`, `brew install --cask navori-rc`; Navori opens with no Gatekeeper dialog | 3.7, 3.4 | pass (operator) |
| Claude Code hooks installed from Settings → Integrations, a session worked: `pmset -g assertions` shows `Navori: keeping this Mac awake` | 3.8, 5.7 | pass (operator) |
| `brew upgrade --cask navori-rc` from rc.8 to rc.9 keeps settings, the saved AI key (no Keychain prompt) and hooks (installed, not needing repair); no Gatekeeper dialog after the upgrade, so `postflight_steps` runs on upgrade | 4.1, 4.2, 4.3, 4.4, 3.4 | pass (operator) |
| `brew uninstall --zap --cask navori-rc`, then a Claude Code session shows no hook error; the release directories went to the Trash | 5.1, 5.2, 5.5 | pass (operator) |

Not exercised: a deliberately broken tap step (wrong token) followed by a rerun (7.7); `docs/agents/project.md` still has no `Compat obligation: External` line, which `release-check` requires for a final tag.

## Release decision for `v0.1.0` (2026-11-05)

`cargo xtask release-check` was run in an isolated worktree at commit `7bfb34a`: the preflight and `cargo xtask ci` passed (Rust and Swift tests included). The performance gate then failed one of twelve metrics: `menu_open_after_launch_ms` 85.9 ms against a budget of 80 ms; the other eleven were within budget (cold start 111.9 ms, `snapshot_ms` 6.7 ms, app size 16.5 MB, idle footprint 24.3 MB, and the rest). The Mac was busy with builds from another project (load average 6.7 to 10.5), and the same metric measured 58.9 ms in a standalone `cargo xtask perf` run on an idle Mac earlier the same day and 82.9 ms and 98 ms on busy ones.

The operator decided on 2026-11-05 to skip the rest of `release-check` and release `v0.1.0` with the measurements taken. This is a recorded exception to RELEASE-2.1 and RELEASE-7.1 for this release only: the gate did not pass on a quiet Mac for the tagged commit. Re-measure on an idle Mac and run `cargo xtask perf --record` when convenient.

## Before `v0.1.0`

- Add the line `- **Compat obligation:** External` to Project posture in `docs/agents/project.md`.
- Create `jayden-dang/homebrew-navori` and a fine-grained token for it only; store it as `TAP_TOKEN`; set `TAP_REPOSITORY` to it and `PUBLISH_RELEASE` to `true`.
- `cargo xtask release-check` must pass, performance gate included.
- Delete the test tap, its token (`navori-release-tap-test`), the release-candidate Releases and the `v0.1.0-rc.*` tags.

## Review findings accepted

Found by the whole-branch review and accepted without a change:

- `.p12` passphrase on argv: the runner is an ephemeral VM and the value is a masked secret, so exposure is limited to that process list.
- The hook recognizer matches by path suffix: a second Navori install's hooks would be treated as Navori's; one user, one install, so it does not occur.
- The guarded hook command keeps `exec`, so an unrunnable but present helper (for example a bad interpreter during a half-finished upgrade) can end the shell with a non-zero code past `|| true`; the harness shows a non-blocking hook error. Accepted, revisit if reported.
- The `rowid` tie-break in `journal_recent` can change after `VACUUM`; only the order of same-millisecond events is affected, and no code relies on it.
- The shared hook-command template is duplicated in `hookfile.rs` and `identity.rs`; tests cover both and a shared helper is not worth a new seam yet.
- A `Channel` type instead of a bool for rc versus stable is deferred; there are two channels and one call site.
- The terms "earlier form" and "legacy" versus "pre-release" and "rc" are used loosely in the docs; they name different things (old hook form versus release channel) and no reader has confused them.
- `add-trusted-cert` is hosted-runner only: a self-hosted runner would keep a system trust root, so the docs forbid self-hosted runners for this workflow (ADR 0007, `docs/agents/project.md`).
