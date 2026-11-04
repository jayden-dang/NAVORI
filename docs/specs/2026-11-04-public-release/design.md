# Design: Public release

Feature code: RELEASE
Status: Approved
Date: 2026-11-04
Requirements: ./requirements.md

## Context

Today a release is a local act: `cargo xtask install` builds the release bundle, signs it with the per-machine self-signed identity `Navori Local Signing`, verifies it and copies it to `/Applications` (`xtask/src/main.rs:505-544`). Nothing packages it, nothing hosts it, and the repo has no `.github/` directory. Everything a release needs to be reproducible on another machine already exists as xtask functions — `bundle()` (`:296`), `sign()` reading `NAVORI_SIGN_IDENTITY` (`:432-455`), `info_plist()` taking its version from the workspace (`:398-399`), `build_number()` counting commits (`:381`), `ci()` (`:1166`) and `perf::run` — so this feature is mostly glue around them, plus one behavior change in the Integrations Module's hook command.

The binding constraint is the signing identity. Keychain access and privacy grants follow the designated requirement, which for Navori is `identifier "dev.navori.app" and certificate leaf = H"8c56e155…"` — the SHA-1 of this Mac's self-signed certificate. A release built on CI keeps users' grants across upgrades only if CI signs with that very certificate, exported as a `.p12`. That rules out the obvious CI shortcut of generating a fresh certificate per run (`signing-setup`, `:548-606`), or signing ad-hoc when the identity is missing: either would give every release a new designated requirement and make every user re-grant Keychain access after each `brew upgrade`. So the release path refuses to sign without the identity and checks the designated requirement against a committed copy before anything is published.

The second constraint is that the perf gate cannot run on CI: the baseline is tied to this Mac's hardware and macOS (`docs/standards/performance.md`). So the gate is split: a local `cargo xtask release-check` runs `ci` and `perf` and prints the tag to push; CI re-runs `ci` only and does the build, sign, publish and cask update.

The hook change follows from uninstall: today's hook command `'<path>' hook` fails in the Harness once the app is gone. A guarded form `[ -f '<path>' ] && [ -x '<path>' ] && exec '<path>' hook || true` runs the same helper with the same arguments and stdin when it exists and exits 0 otherwise. `exec` keeps the process tree as it is today (the helper replaces the shell, so its parent is still the Harness's shell); Awake resolves the Harness by walking ancestors anyway (`crates/procs/src/lookup.rs:28-52`). The recognizer in `identity.rs` only knows the old form, so it must learn the new one, or status, repair and remove would stop seeing Navori's hooks.

Retrieval (fresh, catalog query over `xtask/`, `modules/integrations/`, `.github/`; terms release, tag, cask, hook, sign): neighbors are BUILD (`xtask/`, Shipped, ROAD-4) — this feature extends its commands; PERF (`xtask/`, Implemented) — reused unchanged by `release-check`; HOOKS (`modules/integrations/`, In-progress, ROAD-9) — this feature changes its hook command form (HOOKS-4.4, HOOKS-5.2) and is the owner of that change from here; SHELL — untouched. OWNS coverage: BUILD and SHELL have no Files blocks, so overlap there is judged from surface roots only.

## Decisions

1. Releases are signed with the existing `Navori Local Signing` certificate exported once as a `.p12` into the `release` Environment; the release path never signs ad-hoc. (ADR 0007, amending ADR 0004.)
2. A release runs only on a pushed `v*` tag; the tag's version must equal the workspace version, which is bumped by a normal commit before tagging.
3. `cargo xtask release-check` (local) runs `ci` then `perf`; CI runs `ci` and never `perf`.
4. CI builds and checks through one new command, `cargo xtask release --tag <tag> --out <dir>`, so the workflow holds no build logic and the logic is unit-tested in Rust.
5. The designated requirements of `Navori.app` and its helper are committed in `xtask/assets/designated-requirement.txt`; a mismatch fails the release before publishing.
6. The cask is generated from a template in `xtask/assets/navori.rb.in` and pushed to the tap named by the repository variable `TAP_REPOSITORY`; while that variable is unset, the Release stays a draft and no cask changes.
7. No new dependencies: Cargo already rejects a workspace version that is not SemVer, so `release-check` only needs to split off a pre-release part; SHA-256 comes from `shasum -a 256`, the zip from `ditto`, and the workflow lint reads YAML line by line.
8. Hooks are written in the guarded form `[ -f 'P' ] && [ -x 'P' ] && exec 'P' hook || true` (dev: `… exec 'P' --profile dev hook || true`); the earlier unguarded form is still recognized as Navori's, and is reported as needing repair.
9. Third-party actions in the workflow are pinned to full commit SHAs, enforced by a lint inside `cargo xtask ci`.

## Architecture

Vocabulary for every section below: **module**, **interface**, **implementation**,
**seam** (public surface where behavior is observable and substitutable). Prefer
deep modules — an interface much simpler than what it hides.

### `cargo xtask release` — build, sign and check one release

Satisfies: RELEASE-1.3, RELEASE-1.5, RELEASE-1.6, RELEASE-1.7, RELEASE-1.8
Reuse: rung 2 — `bundle(root, Profile::Release, Flavor::Release, out)` (`xtask/src/main.rs:296`), `sign()` with `NAVORI_SIGN_IDENTITY` (`:432`), `info_plist()` version from `CARGO_PKG_VERSION` (`:399`) and `build_number()` (`:381`), `sh()` / `output()` helpers (`:165`, `:190`); the checks are new pure functions in a new `xtask/src/release.rs` (rung 7)
Respects: ARCH-14 (only the release profile is packaged)
Interface: `cargo xtask release --tag <vX.Y.Z[-pre]> --check` → only the tag check (1.3), for the workflow to run before `ci`, printing `version=<v>` and `prerelease=true|false` for `$GITHUB_OUTPUT`; `cargo xtask release --tag <tag> --out <dir>` → writes `<dir>/Navori-<v>.zip`, `<dir>/Navori-<v>.zip.sha256`, `<dir>/navori.rb`, or, when the zip and `.sha256` are already in `<dir>` (downloaded from an existing Release), checks the zip against its `.sha256` and writes only `navori.rb`; prints the paths; exits non-zero naming the first failed check. Pure functions: `tag_version(tag) -> Result<&str>`, `is_prerelease(version) -> bool` (a `-` before any `+`), `designated_lines(codesign_stdout) -> Vec<String>`.
Depth: a caller (the workflow) knows only "tag in, three files out, or a named failure"; the tag rule, the identity rule, the designated-requirement comparison and the reuse of an already-published zip stay inside.
Locality: new `xtask/src/release.rs` plus one `Cmd::Release` arm in `main.rs`; `bundle`, `sign`, `info_plist` — leave.

Flow:
1. `tag_version(tag)` strips `v` and compares with `env!("CARGO_PKG_VERSION")` (xtask uses `version.workspace = true`); a mismatch bails with both values before any build (1.3).
2. Requires `NAVORI_SIGN_IDENTITY` to be set and listed by `security find-identity -v -p codesigning`; otherwise bails — it never reaches `sign()`'s ad-hoc fallback (1.5, 1.6).
3. `bundle()` the release profile into `<dir>/Navori.app`. `CFBundleShortVersionString` is already the workspace version, which step 1 made equal to the tag; `CFBundleVersion` is `git rev-list --count HEAD`, correct because the workflow checks out the tagged commit with full history (1.8).
4. `codesign --verify --strict --deep`, then `codesign -d -r-` on the app and on `Contents/Helpers/navori`; `designated_lines()` keeps only the `designated =>` lines from stdout (the path-dependent `Executable=` line goes to stderr and is ignored) and compares them with `xtask/assets/designated-requirement.txt`; a mismatch bails showing both (1.7).
5. `ditto -c -k --sequesterRsrc --keepParent Navori.app Navori-<v>.zip`, `shasum -a 256` into `.sha256` (`<hash>  Navori-<v>.zip`), and the cask from its template (next section). When the zip and `.sha256` were already in `<dir>`, steps 2–4 and the zip are skipped: the cask is written from the existing `.sha256` after `shasum -c` confirms it, so a rerun never publishes a cask hash that differs from the asset users download.

### Cask template

Satisfies: RELEASE-3.2, RELEASE-3.3, RELEASE-3.4, RELEASE-3.5, RELEASE-5.5
Reuse: rung 3 — `str::replace` over `include_str!("../assets/navori.rb.in")`; no templating crate
Respects: ARCH-14 (zap names only the release profile's directories)
Interface: `cask_text(version: &str, sha256: &str) -> String`
Depth: n/a — a two-field fill of a reviewable Ruby template; the cask knowledge lives in the template, not in code.
Locality: new `xtask/assets/navori.rb.in`; `release.rs` — extend.

Template shape (placeholders `@VERSION@`, `@SHA256@`):

```ruby
cask "navori" do
  version "@VERSION@"
  sha256 "@SHA256@"
  url "https://github.com/jayden-dang/NAVORI/releases/download/v#{version}/Navori-#{version}.zip"
  name "Navori"
  desc "AI-first menubar cockpit for developers running coding agents"
  homepage "https://github.com/jayden-dang/NAVORI"
  depends_on arch: :arm64
  depends_on macos: :tahoe
  app "Navori.app"
  binary "#{appdir}/Navori.app/Contents/Helpers/navori"
  postflight do
    system_command "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "#{appdir}/Navori.app"]
  end
  zap trash: ["~/Library/Application Support/Navori", "~/Library/Logs/Navori"]
  caveats <<~EOS
    Navori is self-signed and not notarized. This cask removed the quarantine
    flag so macOS opens it without a Gatekeeper prompt.
    Before uninstalling, remove Navori's hooks in Settings → Integrations.
  EOS
end
```

`zap trash:` moves the two directories to the Trash, the Homebrew convention, so a mistaken zap is recoverable (RELEASE-5.5 amended to say so). The exact stanza order and the `macos:` value are whatever `brew style` / `brew audit` accept (checked in the workflow, RELEASE-3.6); that `postflight` also runs on `brew upgrade` is checked by RELEASE-4.4.

### `cargo xtask release-check` — the local gate before tagging

Satisfies: RELEASE-2.1, RELEASE-2.2, RELEASE-2.3, RELEASE-2.4, RELEASE-2.5, RELEASE-2.6, RELEASE-2.7, RELEASE-7.1
Reuse: rung 2 — `ci(root)` (`:1166`) and `perf::run(&root, false)` (`main.rs:139-140`); rung 4 — Cargo itself refuses a non-SemVer `version`, so 2.3 is enforced before xtask even compiles; git via `output()`
Interface: `cargo xtask release-check` → on success prints `v<version>` and `git tag v<version> && git push origin v<version>`; exits non-zero naming the first failure. Pure function: `preflight(version, local_tags, remote_tags, dirty_files, project_md) -> Result<(), String>`.
Depth: the maintainer knows one command and one printed tag; the four preconditions and the ci → perf order stay inside.
Locality: `release.rs` — extend; one `Cmd::ReleaseCheck` arm in `main.rs`.

Order: `preflight` first (2.3–2.6) using `git tag -l v<v>`, `git ls-remote --tags origin v<v>`, `git status --porcelain --untracked-files=no` and the text of `docs/agents/project.md` (a final version needs the line `**Compat obligation:** External`); then `ci`, then `perf` (2.1, 7.1); then print (2.2). It runs no `git tag` or `git push` (2.7).

### Workflow pin lint inside `cargo xtask ci`

Satisfies: RELEASE-7.4
Reuse: rung 2 — the `check_deps` pattern (`:618-777`): read files, collect violation strings, bail with the list
Surface: `cargo xtask ci` (`:1166-1189`) — gains one check after `lint_probe` (`:1178`); readers are the maintainer and the release workflow, both `replace` (they run the new `ci` as is). RELEASE-2.8 (amended at design time) keeps every existing check and exit code and allows only this added check.
Interface: `unpinned_actions(workflow_text) -> Vec<String>`; `ci()` calls it for every file in `.github/workflows/` (none present → nothing to check).
Depth: n/a — extends `ci()` with one pure check.
Locality: `release.rs` — extend; `ci()` — one added call.

A `uses:` line passes when its ref after `@` is 40 hex characters or it starts with `./` (a local action).

### Release workflow — `.github/workflows/release.yml`

Satisfies: RELEASE-1.1, RELEASE-1.2, RELEASE-1.4, RELEASE-1.9, RELEASE-1.10, RELEASE-1.11, RELEASE-1.12, RELEASE-3.1, RELEASE-3.6, RELEASE-6.1, RELEASE-6.2, RELEASE-7.3, RELEASE-7.5, RELEASE-7.7
Reuse: rung 4 — GitHub Actions (`push.tags`, Environments, `vars`, `concurrency`), the `gh` CLI preinstalled on the `macos-26` runner, Homebrew preinstalled on the runner; `actions/checkout` pinned by SHA
Security: TB-7 (writes into another repository's cask only through a token scoped to it)
Interface: trigger `on: push: tags: ['v*']`; inputs: secrets `SIGNING_P12`, `SIGNING_P12_PASSWORD`, `TAP_TOKEN` (Environment `release`), variable `TAP_REPOSITORY`; outputs: a GitHub Release `v<version>` and, when the variable is set, one commit to `Casks/navori.rb` in the tap.
Depth: n/a — a pipeline of existing commands; all decisions live in `cargo xtask release`.
Locality: new file; no existing file changes.

One job, `runs-on: macos-26`, `environment: release`, `permissions: contents: write`, `concurrency: release-${{ github.ref_name }}`:
1. Checkout the tag with `fetch-depth: 0` (1.8 needs the full count).
2. `cargo xtask release --tag "$GITHUB_REF_NAME" --check` — a tag that does not match the workspace version fails here, before `ci` or any build (1.3).
3. `cargo xtask ci` (1.4). A failure stops the job: nothing is created (1.11).
4. Find the Release by tag through `gh api repos/{owner}/{repo}/releases` filtered on `tag_name`, which also lists drafts (`gh release view` by tag may not). If it exists and already carries both assets, download them into `dist/` — the build is skipped and the published bytes are reused (1.12).
5. Otherwise import signing: decode `SIGNING_P12` into a temp file, create a temporary keychain with a random password, import with `-T /usr/bin/codesign`, `security set-key-partition-list`, add it to the search list, trust the certificate for code signing (`sudo security add-trusted-cert -p codeSign`), export `NAVORI_SIGN_IDENTITY="Navori Local Signing"`, delete the temp `.p12` file. No secret is echoed (7.3).
6. `cargo xtask release --tag "$GITHUB_REF_NAME" --out dist` (1.5–1.8; with downloaded assets it only writes the cask).
7. If no Release exists, `gh release create --draft --verify-tag`, with `--prerelease` when step 2 printed `prerelease=true` (1.10); then `gh release upload --clobber` the zip and `.sha256` (1.9). The Release always starts as a draft, so nobody can download a half-uploaded one.
8. While `vars.TAP_REPOSITORY` is empty, stop here: the Release stays a draft and no cask changes (6.1).
9. When it is set: check out the tap with `TAP_TOKEN`, copy `dist/navori.rb` to `Casks/navori.rb`, trust and `brew tap` the checkout locally, run `brew style` and `brew audit --cask` — any error stops before anything is published (3.6); then `gh release edit --draft=false` (6.2); then commit `navori <version>` and push only when the file changed (3.1). The cask is pushed only after its Release is public, so it never points at a draft.
10. `if: always()`: delete the temporary keychain and restore the default search list (7.5).

Partial failure (7.7): a failure before step 9's publish leaves at most a draft Release and an unchanged tap; a failure between publish and push leaves a public Release while the tap still names the previous version, which installs fine. Rerunning the job reaches step 4, reuses the assets, and finishes the remaining steps; a second rerun finds the cask unchanged and pushes nothing. Setting `TAP_REPOSITORY` later and rerunning the job for a draft tag publishes it the same way.

### Guarded hook command — Integrations Module

Satisfies: RELEASE-5.1, RELEASE-5.2, RELEASE-5.3, RELEASE-5.4, RELEASE-5.11, RELEASE-7.2
Reuse: rung 2 — `command_for` (`modules/integrations/src/domain/hookfile.rs:51`), `is_navori_command` / `unquote` / `ends_in_helper` (`identity.rs:5-38`), and `classify()`'s existing "ours but not the desired command → needs repair" path that already handles a stale helper path (repair tests with `/Volumes/Old/...`)
Security: TB-7, THR-9 (other hooks in the file stay untouched; only recognition widens)
Surface:
- `command_for` / `desired()` (`hookfile.rs:51-61`) — `replace`: emit the guarded form.
- `is_navori_command` (`identity.rs:5`, only reader `hookfile.rs:294`) — `replace`: accept the guarded form and the earlier form, for the running profile only.
- Hook files on users' Macs holding the earlier form (persisted, only the author today) — `frozen`: built around — still recognized as Navori's, classified as needing repair, rewritten by repair (5.4).
- `modules/integrations/tests/install.rs:369,384,399` (exact strings) — `replace`.
- `modules/integrations/tests/common/mod.rs:87-93` `command()` — `replace` (guarded); add `legacy_command()` for 5.4 cases.
- `modules/integrations/tests/status.rs:320-329` lookalikes, `:344` quote round-trip — `replace`: add guarded lookalikes that must stay foreign (wrong path tail, wrong args, missing `exec`, other profile).
- `modules/integrations/tests/repair.rs:74,227,279` — `replace` via `command()`.
- Fixtures `claude_installed.json`, `claude_both_profiles.json`, `grok_installed.json` — kept as the earlier form; tests that load them as "installed" are `replace`d to expect needs-repair, and one guarded fixture is added.
- `crates/ffi/tests/integrations.rs:150` — `replace`.
- `docs/features/integrations.md:60`; HOOKS requirements HOOKS-4.4 (`:96`), HOOKS-5.2 (`:117`); HOOKS design decision 5 (`design.md:27,100,102`) — `replace`: HOOKS-4.4 and HOOKS-5.2 amended in place with a dated note pointing to RELEASE-5.1 and RELEASE-5.11 (HOOKS is In-progress and its tasks cite both, so striking them would break the trace), the design and feature doc updated.
- Swift Settings → Integrations and `doctor` show state only, never the command — no edit, no change in what they receive except that the earlier form now reads as needs-repair.
Interface: unchanged signatures — `desired(harness, dev, helper)` and `is_navori_command(command, dev) -> bool`.
Depth: n/a — extends `hookfile.rs` and `identity.rs`.
Locality: `hookfile.rs` and `identity.rs` — extend; Awake and the CLI — leave (the helper's arguments and stdin are identical, `apps/cli/src/main.rs:117-139`).

Grammar the recognizer accepts, with `Q(P)` a POSIX single-quoted path ending `Contents/Helpers/navori` and `A` the profile's arguments (` hook` or ` --profile dev hook`):
- guarded: `[ -f ` Q(P) ` ] && [ -x ` Q(P) ` ] && exec ` Q(P) A ` || true` — all three `Q(P)` the same path;
- earlier: Q(P) A.

`exec` makes the helper replace the shell, so pids and stdin behave as today; if the helper is missing, not a regular file (a directory passes `-x`), or not executable, a test fails and `|| true` exits 0 with no output (5.2). The form uses only POSIX `sh` syntax, which Claude Code's `sh -c` and Codex's `$SHELL -lc` (zsh, bash, fish ≥ 3) accept.

### Upgrade continuity

Satisfies: RELEASE-4.1, RELEASE-4.2, RELEASE-4.3
Reuse: rung 1 — nothing new: the cask has no `uninstall` stanza touching data, so `brew upgrade` replaces only `Navori.app` (4.1); the designated-requirement check in `cargo xtask release` keeps every release the same app to Keychain (4.2); the cask installs to the same app directory every time, so the helper path written into hooks does not change (4.3)
Interface: none
Depth: n/a — no module; the properties come from the sections above.
Locality: leave.

### Unchanged behavior this feature leans on (guards)

Satisfies: RELEASE-1.13, RELEASE-1.14, RELEASE-1.15, RELEASE-2.8, RELEASE-4.5, RELEASE-5.6, RELEASE-5.7, RELEASE-5.8, RELEASE-5.9, RELEASE-5.10
Reuse: rung 2 — `install()` (`:505-544`), `sign()` ad-hoc fallback (`:431-448`), `run` (`:481-501`), `ci()` and `perf` (`:139-140`, `:1166`), `Store::migrate` backup and per-migration transaction (`crates/store/src/lib.rs:128-168`), `strip()` (`hookfile.rs:204-239`), profile-by-arguments (`identity.rs:7-14`), timeouts (`hookfile.rs:65`), Harness resolution by ancestors (`crates/procs/src/lookup.rs:28-52`)
Respects: ARCH-12, ARCH-14
Interface: unchanged
Depth: n/a — guards.
Locality: leave; `release.rs` calls `bundle()` and `ci()` but changes neither.

### Release steps, accounts and acceptance

Satisfies: RELEASE-3.7, RELEASE-3.8, RELEASE-4.4, RELEASE-6.3, RELEASE-7.6
Reuse: rung 4 — GitHub settings (Environment, ruleset, fine-grained token) and macOS user accounts; `docs/agents/project.md` `## release steps` (`:135-148`)
Surface: `docs/agents/project.md:135-148` — `replace` (steps rewritten); `cut-release` reads these steps verbatim — `replace` (it follows the new text).
Interface: the written release steps
Depth: n/a — documentation and one-time setup.
Locality: `docs/agents/project.md` — extend; README gains an Install section.

One-time setup, written into `docs/agents/project.md`: export `Navori Local Signing` with its private key from the login keychain as a `.p12` (Keychain Access or `security export -t identities -f pkcs12`) and keep an offline copy; create the Environment `release` limited to tags `v*` with the three secrets; add a tag ruleset so only the owner creates `v*`; create `jayden-dang/homebrew-navori` and a fine-grained token with contents write on it only (7.6); commit the designated requirement file from `codesign -d -r-` of the current install. Release steps: `cargo xtask release-check`, `git tag`, `git push`, then on a clean macOS 26 account the install check (3.7), the Claude Code session check (3.8) and, between two pre-releases, the upgrade check (4.4). Before the first final release, add `**Compat obligation:** External` to Project posture (checked by RELEASE-2.6).

## Seams for testing

| Seam | Kind | Covers |
|---|---|---|
| `xtask/src/release.rs` pure functions (`tag_version`, `is_prerelease`, `designated_lines`, `cask_text`, `preflight`, `unpinned_actions`) — new seam | unit | RELEASE-1.3, 1.7, 1.10, 2.3, 2.4, 2.5, 2.6, 3.2, 3.3, 3.4, 3.5, 5.5, 7.4 |
| `cargo xtask release` / `release-check` on this Mac with the local signing identity | integration (local, by hand) | RELEASE-1.5, 1.6, 1.8, 2.1, 2.2, 2.7, 7.1 |
| Integrations Module through its Actions (`modules/integrations/tests/*`, existing) | integration | RELEASE-5.1, 5.4, 5.11, 5.6, 5.8, 5.9, 5.10 |
| The guarded command run through `/bin/sh -c` with a missing helper and a fake helper script (in `modules/integrations/tests/`) | integration | RELEASE-5.2, 5.3, 5.7, 7.2 |
| Existing xtask, store and procs tests (`cargo xtask ci`) | unit / integration | RELEASE-1.13, 1.14, 1.15, 2.8, 4.5 |
| A pre-release tag pushed with `TAP_REPOSITORY` unset, then set to a test tap, then a failed rerun | e2e (GitHub, by hand) | RELEASE-1.1, 1.2, 1.4, 1.9, 1.10, 1.11, 1.12, 3.1, 3.6, 6.1, 6.2, 7.3, 7.5, 7.7 |
| Clean macOS 26 account: install, Claude Code session, upgrade between two pre-releases, `brew uninstall --zap` | e2e (by hand) | RELEASE-3.7, 3.8, 4.1, 4.2, 4.3, 4.4 |
| Review of `docs/agents/project.md` and the token's settings | review | RELEASE-6.3, 7.6 |

## Coverage check

Every ID in requirements.md (57) appears in exactly one `Satisfies:` line: 1.1–1.15, 2.1–2.8, 3.1–3.8, 4.1–4.5, 5.1–5.11, 6.1–6.3, 7.1–7.7. None is deliberately unmapped.

Risks to check on the first pre-release run, not decisions (each is caught by a workflow step failing, never by a broken user install): the `macos-26` runner label and its Rust toolchain; Homebrew Tap Trust in CI (`brew trust --cask` on the local tap before `brew style` / `brew audit`); the `depends_on macos:` value `brew style` accepts for macOS 26; fish ≥ 3 running the guarded hook (verified locally only for sh, bash 3.2 and zsh); whether `codesign` on the runner accepts the imported self-signed identity after `add-trusted-cert` (if not, sign by the certificate's SHA-1 instead of its name); whether `brew audit --cask` without `--online` passes for a draft Release URL (the cask is only pushed after the Release is published in step 6, so audit can run after `gh release edit --draft=false` if it needs the URL).
