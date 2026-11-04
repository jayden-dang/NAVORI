# Tasks: Public release

> **For agentic workers:** after plan approval, pick one execute skill —
> `build-in-waves` (continuous dependency-aware scheduler), `build-by-story` (human-gated story review
> units), or `build-inline` (controller implements, no implementer subagents).
> The chosen skill writes `Execution-mode:`. Steps use checkbox (`- [ ]`) syntax
> for tracking.

Feature code: RELEASE
Status: Approved
Date: 2026-11-04
Execution-mode: continuous
Max-concurrency: auto
Requirements: ./requirements.md
Design: ./design.md

**Goal:** Pushing a `v<version>` tag builds, signs and publishes Navori on its GitHub Release and in the Homebrew tap, a local `cargo xtask release-check` gates the tag on `ci` and `perf`, and Navori's hooks do nothing once the app is gone.

**Architecture:**
- A new `xtask/src/release.rs` holds every release decision as pure functions plus two commands: `cargo xtask release` (CI: tag check, signed build, designated-requirement check, zip, sha256, cask) and `cargo xtask release-check` (local: preconditions, `ci`, `perf`).
- `.github/workflows/release.yml` is a thin pipeline over those commands, `gh` and `brew`; a lint in `cargo xtask ci` keeps its actions pinned by SHA.
- The Integrations Module writes guarded hook commands and still recognizes the earlier form as Navori's, needing repair.

**Tech Stack:** Rust 2024 (rust-version 1.90), `anyhow`, `clap`, `serde_json`, `toml`, `tempfile` (dev) — no new dependency; macOS `codesign`, `security`, `ditto`, `shasum`; GitHub Actions on `macos-26`; `gh`; Homebrew. `cargo test -p xtask`, `cargo test -p navori-mod-integrations`, `cargo xtask ci`, `cargo xtask perf`.

**Source classes:** every value in `requirements.md` and `design.md` is a Lock (frame-change close package, approved 2026-11-04) or `agent-decided` in the design. A task never promotes an `agent-decided` value to the operator's choice.

## Global Constraints

Source of truth (referenced, not copied — read before every task):

| Source | Content hash (`git hash-object`, first 12) |
|---|---|
| `docs/standards/INDEX.md` | `f83e09733ee4` |
| `docs/standards/testing.md` | `b1a9b42fc1d3` |
| `docs/standards/errors-logging.md` | `68aadebff863` |
| `docs/standards/performance.md` | `c4b26d17a09b` |
| `docs/architecture/INDEX.md` (ARCH-N) | `2e318bd23ff5` |

Compact delta every task inherits:

- **Test-first at agreed seams** (`design.md` "Seams for testing"): test names are sentences in domain language; no requirement ID in test or production source; expected values are literals.
- **Pure functions first.** Every release decision is a pure function in `xtask/src/release.rs` tested in its inline `#[cfg(test)] mod tests`, the style of `fake_feature_violation` (`xtask/src/main.rs`); shell-outs go through the existing `sh()` / `output()` helpers.
- **Secrets:** no step prints, writes or commits a secret, a `.p12`, a password or a token. Tests use no real certificate.
- **No outward action without the operator:** no task pushes a tag, creates a GitHub Release, creates a repository, or sets a secret or variable. Task 9 lists those as steps the operator runs.
- **ARCH-8, 12, 14** hold: no secret enters the bundle; migrations and backups are untouched; only the release profile is packaged and zapped.
- **Gate:** every task: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, then the task's `cargo test -p <crate>`; `cargo xtask ci` in Tasks 1, 6, 7 and 9.
- **Shared files** (load-subgraph cluster(RELEASE), advisory): `modules/integrations/**` is owned by HOOKS (In-progress, ROAD-9); `xtask/src/main.rs` by BUILD and PERF. Rebase on `main` before each task that touches them.

## File Structure

| File | New / changed | Responsibility |
|---|---|---|
| `xtask/src/release.rs` | new | Pure release functions (`tag_version`, `is_prerelease`, `cask_text`, `designated_lines`, `requirement_mismatch`, `identity_problem`, `preflight`, `unpinned_actions`) and the `release` / `release-check` commands |
| `xtask/src/main.rs` | changed | `mod release;`, `Cmd::Release`, `Cmd::ReleaseCheck`, one call to the pin lint inside `ci()` |
| `xtask/assets/navori.rb.in` | new | Cask template with `@VERSION@` and `@SHA256@` |
| `xtask/assets/designated-requirement.txt` | new | The two committed `designated =>` lines (app, helper) |
| `.github/workflows/release.yml` | new | Tag-triggered release pipeline |
| `modules/integrations/src/domain/hookfile.rs` | changed | `command_for` writes the guarded form |
| `modules/integrations/src/domain/identity.rs` | changed | `is_navori_command` accepts the guarded and the earlier form |
| `modules/integrations/tests/common/mod.rs` | changed | `command()` builds the guarded form; new `legacy_command()` |
| `modules/integrations/tests/install.rs`, `status.rs`, `repair.rs`, `remove.rs` | changed | Expect the guarded form; earlier form reads as needs-repair; lookalikes |
| `modules/integrations/tests/guard_shell.rs` | new | Runs the guarded command through `/bin/sh -c` |
| `modules/integrations/tests/fixtures/claude_guarded.json`, `grok_guarded.json` | new | Claude and Grok hook files holding the guarded form |
| `docs/features/integrations.md` | changed | The hook command form |
| `docs/specs/2026-11-03-harness-hooks/requirements.md`, `design.md` | changed | HOOKS-4.4 and HOOKS-5.2 amended in place to the guarded form, each with an `(Amended 2026-11-04 by RELEASE-5.1 / RELEASE-5.11)` note; design decision 5 updated |
| `docs/agents/project.md` | changed | Release steps and one-time release setup |
| `README.md` | changed | Install section |
| `docs/specs/2026-11-04-public-release/acceptance.md` | new | Record of the by-hand checks of Task 9 |

---

### Task 1: Hooks do nothing once Navori is gone

**Files:**
- Modify: `modules/integrations/src/domain/hookfile.rs`, `modules/integrations/src/domain/identity.rs`, `modules/integrations/tests/common/mod.rs`, `modules/integrations/tests/install.rs`, `modules/integrations/tests/status.rs`, `modules/integrations/tests/repair.rs`, `modules/integrations/tests/remove.rs`, `docs/features/integrations.md`, `docs/specs/2026-11-03-harness-hooks/requirements.md`, `docs/specs/2026-11-03-harness-hooks/design.md`
- Create: `modules/integrations/tests/guard_shell.rs`, `modules/integrations/tests/fixtures/claude_guarded.json`, `modules/integrations/tests/fixtures/grok_guarded.json`

**Reuse:** rung 2 — `command_for` (`modules/integrations/src/domain/hookfile.rs:51`), `is_navori_command` / `unquote` / `ends_in_helper` (`identity.rs:5-38`), and `classify()`'s existing "ours but not the desired command → needs repair" path that already handles a stale helper path (repair tests with `/Volumes/Old/...`)

**Interfaces:**
- Consumes: none.
- Produces: guarded command text, for helper path `P` quoted as `Q` (`'` → `'\''`): release `[ -f 'Q' ] && [ -x 'Q' ] && exec 'Q' hook || true`; dev `[ -f 'Q' ] && [ -x 'Q' ] && exec 'Q' --profile dev hook || true`. `is_navori_command(command: &str, dev: bool) -> bool` true for that form (all three paths equal, ending `Contents/Helpers/navori`) and for the earlier `'Q' hook` / `'Q' --profile dev hook`. Test helpers `command(helper: &str, dev: bool) -> String` (guarded) and `legacy_command(helper: &str, dev: bool) -> String` (earlier form).

**Depends-on:** none

- [x] **Step 1: Write the failing tests.**
  - `install.rs`: `installing_writes_a_command_that_runs_the_helper_only_when_it_exists` expects exactly `[ -f '/Applications/Navori.app/Contents/Helpers/navori' ] && [ -x '/Applications/Navori.app/Contents/Helpers/navori' ] && exec '/Applications/Navori.app/Contents/Helpers/navori' hook || true`; the quote-in-path and dev cases at today's `:369`, `:384`, `:399` move to the guarded form.
  - `status.rs`: `hooks_written_in_the_earlier_form_need_repair` (loads `claude_installed.json`); the three tests that read the earlier form as installed move to guarded input — `a_complete_correct_set_is_installed` (`:77`, loads `claude_guarded.json` and `grok_guarded.json`), `a_symlinked_hook_file_is_read_through_the_link` (`:186`, links to `claude_guarded.json`), `a_quote_inside_the_helper_path_is_read_back_as_ours` (`:342`, the string at `:344` in the guarded form); `guarded_lookalikes_are_not_navoris` — paths that differ between the three places, a missing `exec`, `|| false`, the other profile's arguments, a path not ending `Contents/Helpers/navori`.
  - `repair.rs`: `repair_rewrites_earlier_form_hooks_as_guarded_and_keeps_other_hooks`.
  - `remove.rs`: `remove_deletes_the_running_profiles_hooks_in_both_forms_and_nothing_else`.
  - `guard_shell.rs` (runs `/bin/sh -c <command>` built by the module's install path into a temp Hook file, then read back): `a_missing_helper_exits_zero_with_no_output`; `a_directory_in_place_of_the_helper_exits_zero_with_no_output`; `a_helper_without_the_execute_bit_exits_zero_with_no_output`; `a_present_helper_gets_the_same_arguments_and_stdin` (a fake helper script writes `"$@"` and its stdin to files; expect `hook` and the exact payload bytes); `the_helper_runs_as_the_shell_process_itself` (the fake writes `$$`; expect it to equal the `sh` child's pid); `the_command_finishes_well_inside_the_hook_timeout` (under 2 s with the fake present). `guard_shell.rs` defines its own install helper over `Rig::new(dev, helper)` (the one in `install.rs` is local). The Session Lease through a real Harness (RELEASE-5.7) is exercised in Task 9 Step 4.
- [x] **Step 2: Run** `cargo test -p navori-mod-integrations` — expect: the new and moved tests fail on the earlier command text.
- [x] **Step 3: Implement** the guarded `command_for` and the two-form recognizer; switch `tests/common/mod.rs` `command()` to the guarded form and add `legacy_command()`; add `claude_guarded.json` and `grok_guarded.json`. `crates/ffi/tests/integrations.rs` needs no edit: its installed cases install through the Core and pick up the guarded form, and `:150` stays a needs-repair case.
- [x] **Step 4: Update the docs:** `docs/features/integrations.md:60`; amend HOOKS-4.4 (`requirements.md:96`) and HOOKS-5.2 (`:117`) in place to say the guarded command and its two-form recognition, each ending `*(Amended 2026-11-04 by RELEASE-5.1 — guarded hook command.)*` / `*(Amended 2026-11-04 by RELEASE-5.11.)*` — amended, not struck, because HOOKS `tasks.md:239,265` cite them and HOOKS is still In-progress; update HOOKS `design.md` decision 5 (`:27`) and the lines that say only the single-quoted form is recognized.
- [x] **Step 5: Run** `cargo test -p navori-mod-integrations && cargo xtask ci` — expect: pass.
- [x] **Step 6: Commit** `git commit -m "feat(integrations): write hooks that do nothing once the Navori helper is gone"`

_Requirements: RELEASE-5.1, RELEASE-5.2, RELEASE-5.3, RELEASE-5.4, RELEASE-5.6, RELEASE-5.7, RELEASE-5.8, RELEASE-5.9, RELEASE-5.10, RELEASE-5.11, RELEASE-7.2_

---

### Task 2: A release tag must name the workspace version

**Files:**
- Create: `xtask/src/release.rs`
- Modify: `xtask/src/main.rs`
- Test: `xtask/src/release.rs` (inline `mod tests`)

**Reuse:** rung 2 — `bundle(root, Profile::Release, Flavor::Release, out)` (`xtask/src/main.rs:296`), `sign()` with `NAVORI_SIGN_IDENTITY` (`:432`), `info_plist()` version from `CARGO_PKG_VERSION` (`:399`) and `build_number()` (`:381`), `sh()` / `output()` helpers (`:165`, `:190`); the checks are new pure functions in a new `xtask/src/release.rs` (rung 7)

**Interfaces:**
- Consumes: none.
- Produces: `pub(crate) fn tag_version<'a>(tag: &'a str, workspace: &str) -> anyhow::Result<&'a str>` (strips `v`; error names both versions); `pub(crate) fn is_prerelease(version: &str) -> bool` (a `-` before any `+`); `Cmd::Release { tag: String, out: Option<PathBuf>, check: bool }` with clap flags `--tag`, `--out`, `--check`; `release::run(root: &Path, tag: &str, out: Option<&Path>, check: bool) -> anyhow::Result<()>`; with `--check` it prints `version=<v>` and `prerelease=<true|false>` on two lines and returns.

**Depends-on:** none

- [x] **Step 1: Write the failing tests:** `a_tag_naming_the_workspace_version_passes` (`v0.1.0`, `0.1.0` → `0.1.0`); `a_tag_naming_another_version_fails_naming_both` (`v0.2.0`, `0.1.0` → error text contains `0.2.0` and `0.1.0`); `a_tag_without_the_v_prefix_fails`; `a_version_with_a_pre_release_part_is_a_pre_release` (`0.2.0-rc.1` true, `1.0.0-rc.1+b.7` true); `build_metadata_alone_is_not_a_pre_release` (`1.0.0+build-1` false, `0.1.0` false).
- [x] **Step 2: Run** `cargo test -p xtask release::` — expect: does not compile (`release` module missing).
- [x] **Step 3: Implement** `release.rs` with the two functions and `run` handling `--check` only (the build path arrives in Task 4: until then `run` without `--check` bails with "release build not implemented"); add `mod release;` and the `Cmd::Release` arm in `main.rs`, the workspace version taken from `env!("CARGO_PKG_VERSION")`.
- [x] **Step 4: Run** `cargo test -p xtask && cargo xtask release --tag v0.1.0 --check` — expect: pass, then `version=0.1.0` and `prerelease=false`.
- [x] **Step 5: Commit** `git commit -m "feat(xtask): check that a release tag names the workspace version"`

_Requirements: RELEASE-1.3, RELEASE-1.10_

---

### Task 3: The cask that installs Navori

**Files:**
- Create: `xtask/assets/navori.rb.in`
- Modify: `xtask/src/release.rs`
- Test: `xtask/src/release.rs` (inline `mod tests`)

**Reuse:** rung 3 — `str::replace` over `include_str!("../assets/navori.rb.in")`; no templating crate

**Interfaces:**
- Consumes: `release.rs` (Task 2).
- Produces: `pub(crate) fn cask_text(version: &str, sha256: &str) -> String`; the template text exactly as in `design.md` "Cask template".

**Depends-on:** Task 2

- [x] **Step 1: Write the failing tests:** `the_cask_names_the_version_and_the_zip_hash` (`cask_text("0.1.0", "<64 × a>")` contains `version "0.1.0"` and `sha256 "aaaa…"`, and no `@`); `the_cask_needs_an_arm64_mac_on_macos_26` (`depends_on arch: :arm64`, `depends_on macos: :tahoe`); `the_cask_installs_the_app_and_links_the_cli` (`app "Navori.app"`, `binary "#{appdir}/Navori.app/Contents/Helpers/navori"`); `the_cask_clears_quarantine_after_install` (postflight with `/usr/bin/xattr`, `-dr`, `com.apple.quarantine`); `zap_moves_only_the_release_profiles_directories_to_the_trash` (both release paths present, `Navori Dev` absent); `the_caveats_say_self_signed_not_notarized_and_how_to_remove_hooks`.
- [x] **Step 2: Run** `cargo test -p xtask release::` — expect: fails (`cask_text` missing).
- [x] **Step 3: Implement** the template file and `cask_text`.
- [x] **Step 4: Run** `cargo test -p xtask` — expect: pass. Then write `cask_text("0.1.0", <64 × a>)` to `Casks/navori.rb` in a temporary git repo, `brew tap local/navori-check <that repo>`, `brew trust --cask local/navori-check/navori` and `brew style --cask local/navori-check/navori` — expect: no offenses (adjust the template until clean); a copy with `depends_on arch: :bogus` — expect: an error; `brew untap local/navori-check`.
- [x] **Step 5: Commit** `git commit -m "feat(xtask): generate the Homebrew cask for a release"`

_Requirements: RELEASE-3.2, RELEASE-3.3, RELEASE-3.4, RELEASE-3.5, RELEASE-5.5_

---

### Task 4: Build, sign and check one release

**Files:**
- Create: `xtask/assets/designated-requirement.txt`
- Modify: `xtask/src/release.rs`
- Test: `xtask/src/release.rs` (inline `mod tests`)

**Reuse:** rung 2 — `bundle(root, Profile::Release, Flavor::Release, out)` (`xtask/src/main.rs:296`), `sign()` with `NAVORI_SIGN_IDENTITY` (`:432`), `info_plist()` version from `CARGO_PKG_VERSION` (`:399`) and `build_number()` (`:381`), `sh()` / `output()` helpers (`:165`, `:190`); the checks are new pure functions in a new `xtask/src/release.rs` (rung 7)

**Interfaces:**
- Consumes: `tag_version`, `run` (Task 2); `cask_text` (Task 3).
- Produces: `pub(crate) fn designated_lines(codesign_stdout: &str) -> Vec<String>` (keeps lines starting `designated =>`, trimmed); `release::run` without `--check` takes the build number from `git rev-list --count HEAD` and fails if git fails (no timestamp fallback, unlike `build_number()`), and writes `<out>/Navori-<v>.zip`, `<out>/Navori-<v>.zip.sha256` (`<hash>  Navori-<v>.zip`) and `<out>/navori.rb`, or, when the zip and `.sha256` already exist in `<out>`, runs `shasum -a 256 -c` on them and writes only `navori.rb`. `xtask/assets/designated-requirement.txt` holds, in order:
  - `designated => identifier "dev.navori.app" and certificate leaf = H"8c56e155bdd4ed3c038035509193fb7d33d302c1"`
  - `designated => identifier "dev.navori.app.cli" and certificate leaf = H"8c56e155bdd4ed3c038035509193fb7d33d302c1"`

**Depends-on:** Task 2, Task 3

- [x] **Step 1: Write the failing tests:** `only_the_designated_lines_of_codesign_output_are_compared` (input with an `Executable=` line, a `# designated =>` comment-free line and two `designated =>` lines → the two lines); `a_designated_requirement_with_another_certificate_is_a_mismatch` (a pure `requirement_mismatch(expected: &str, actual: &[String]) -> Option<String>` returns text showing both); `matching_requirements_are_not_a_mismatch`; `a_missing_signing_identity_stops_the_release` (pure `identity_problem(env: Option<&str>, find_identity_stdout: &str) -> Option<String>`: `None` env → problem; env set but not listed → problem; listed → none).
- [x] **Step 2: Run** `cargo test -p xtask release::` — expect: fails (functions missing).
- [x] **Step 3: Implement** the build path in the order of `design.md` "`cargo xtask release`" steps 1–5, including the reuse path for an existing zip; record `designated-requirement.txt` from `codesign -d -r- /Applications/Navori.app` and its helper on this Mac.
- [x] **Step 4: Run** `cargo test -p xtask`, then on this Mac `NAVORI_SIGN_IDENTITY="Navori Local Signing" cargo xtask release --tag v0.1.0 --out "$(mktemp -d)"` — expect: three files, and `codesign -dv` of the unzipped app shows `CFBundleShortVersionString` `0.1.0`; and `CFBundleVersion` equal to `git rev-list --count HEAD`; run the same command again into the same directory — expect: only `navori.rb` rewritten, the zip and `.sha256` untouched, and the cask's `sha256` equal to the `.sha256` file; then the same with `NAVORI_SIGN_IDENTITY` unset — expect: fails naming the identity, no zip written; then `cargo xtask install` and `cargo xtask run` — expect: both install or launch as before (`sign()`'s ad-hoc fallback is not touched by this task: `git diff` shows no change to `sign`).
- [x] **Step 5: Commit** `git commit -m "feat(xtask): build, sign and check a release bundle for publishing"`

_Requirements: RELEASE-1.5, RELEASE-1.6, RELEASE-1.7, RELEASE-1.8, RELEASE-1.13, RELEASE-1.14, RELEASE-1.15_

---

### Task 5: Check a release locally before tagging

**Files:**
- Modify: `xtask/src/release.rs`, `xtask/src/main.rs`
- Test: `xtask/src/release.rs` (inline `mod tests`)

**Reuse:** rung 2 — `ci(root)` (`:1166`) and `perf::run(&root, false)` (`main.rs:139-140`); rung 4 — Cargo itself refuses a non-SemVer `version`, so 2.3 is enforced before xtask even compiles; git via `output()`

**Interfaces:**
- Consumes: `is_prerelease` (Task 2).
- Produces: `pub(crate) fn preflight(version: &str, local_tags: &str, remote_tags: &str, dirty: &str, project_md: &str) -> Result<(), String>`; `Cmd::ReleaseCheck`; `release::check(root: &Path) -> anyhow::Result<()>` — preflight from `git tag -l v<v>`, `git ls-remote --tags origin v<v>`, `git status --porcelain --untracked-files=no` and `docs/agents/project.md` (a final version needs a line starting `- **Compat obligation:** External`); then `ci(root)`; then `perf::run(root, false)` (bail on `false`); then prints `v<v>` and `git tag v<v> && git push origin v<v>`. `release.rs` calls `super::ci` and `super::perf::run` (`main.rs:141`) directly.

**Depends-on:** Task 4

- [x] **Step 1: Write the failing tests:** `a_free_tag_on_a_clean_tree_passes` (pre-release version, empty tag lists, empty dirty, any project text); `a_tag_that_already_exists_locally_fails_naming_it`; `a_tag_that_already_exists_on_origin_fails_naming_it` (`ls-remote` line `<sha>\trefs/tags/v0.1.0`); `uncommitted_changes_fail_listing_the_files` (` M Cargo.toml` → text contains `Cargo.toml`); `a_final_version_needs_the_external_compat_line` (`0.1.0` with no line starting `- **Compat obligation:** External` → fails naming the line; with that line → passes); `a_compat_line_quoted_in_prose_does_not_count` (the text appears mid-sentence, not as a line start → fails); `a_pre_release_does_not_need_the_compat_line`.
- [x] **Step 2: Run** `cargo test -p xtask release::` — expect: fails (`preflight` missing).
- [x] **Step 3: Implement** `preflight`, `check` and the `Cmd::ReleaseCheck` arm; `check` runs no `git tag` or `git push`.
- [x] **Step 4: Run** `cargo test -p xtask`, then `cargo xtask release-check` on this Mac — expect: on today's tree it fails at preflight (uncommitted changes or the missing Compat line for `0.1.0`), naming the cause.
- [x] **Step 5: Commit** `git commit -m "feat(xtask): add release-check, the local gate that prints the tag to push"`

_Requirements: RELEASE-2.1, RELEASE-2.2, RELEASE-2.3, RELEASE-2.4, RELEASE-2.5, RELEASE-2.6, RELEASE-2.7, RELEASE-7.1_

---

### Task 6: Keep workflow actions pinned by SHA

**Files:**
- Modify: `xtask/src/release.rs`, `xtask/src/main.rs`
- Test: `xtask/src/release.rs` (inline `mod tests`)

**Reuse:** rung 2 — the `check_deps` pattern (`:618-777`): read files, collect violation strings, bail with the list

**Interfaces:**
- Consumes: `release.rs` (Task 2).
- Produces: `pub(crate) fn unpinned_actions(workflow: &str) -> Vec<String>` (each offending `uses:` line, trimmed); `release::check_workflows(root: &Path) -> anyhow::Result<()>` — reads every `*.yml` / `*.yaml` under `.github/workflows/`, nothing to do when the directory is absent; called from `ci()` right after `lint_probe` (`main.rs:1178`).

**Depends-on:** Task 5

- [x] **Step 1: Write the failing tests:** `an_action_pinned_to_a_tag_is_reported` (`      - uses: actions/checkout@v4`); `an_action_pinned_to_a_full_sha_passes` (`uses: actions/checkout@` + 40 hex); `a_local_action_passes` (`uses: ./.github/actions/x`); `a_short_sha_is_reported` (7 hex); `quoted_uses_values_are_read` (`uses: "actions/checkout@v4"` reported); `a_sha_pin_followed_by_a_version_comment_passes` (`uses: actions/checkout@` + 40 hex + ` # v4.2.2`).
- [x] **Step 2: Run** `cargo test -p xtask release::` — expect: fails (function missing).
- [x] **Step 3: Implement** both functions and the one call in `ci()`; no existing check in `ci()` changes.
- [x] **Step 4: Run** `cargo test -p xtask && cargo xtask ci` — expect: pass (no workflow yet).
- [x] **Step 5: Commit** `git commit -m "feat(xtask): reject workflow actions not pinned to a commit SHA"`

_Requirements: RELEASE-7.4, RELEASE-2.8_

---

### Task 7: Publish a release when a tag is pushed

**Files:**
- Create: `.github/workflows/release.yml`

**Reuse:** rung 4 — GitHub Actions (`push.tags`, Environments, `vars`, `concurrency`), the `gh` CLI preinstalled on the `macos-26` runner, Homebrew preinstalled on the runner; `actions/checkout` pinned by SHA

**Interfaces:**
- Consumes: `cargo xtask release --tag <t> --check` (Task 2) — `version=` / `prerelease=` lines appended to `$GITHUB_OUTPUT`; `cargo xtask release --tag <t> --out dist` (Task 4); `cargo xtask ci` with the pin lint (Task 6).
- Produces: workflow `release` — `on: push: tags: ['v*']`; job `release`, `runs-on: macos-26`, `environment: release`, `permissions: contents: write`, `concurrency: release-${{ github.ref_name }}`; secrets `SIGNING_P12`, `SIGNING_P12_PASSWORD`, `TAP_TOKEN`; variable `TAP_REPOSITORY`.

**Depends-on:** Task 4, Task 6

- [ ] **Step 1: Resolve the pin** for `actions/checkout`'s latest release tag with `gh api repos/actions/checkout/releases/latest --jq .tag_name` and `gh api repos/actions/checkout/git/ref/tags/<tag> --jq .object.sha` (dereference an annotated tag through `git/tags/<sha>`); write it as `actions/checkout@<40-hex> # <tag>`.
- [ ] **Step 2: Write** `release.yml` with steps 1–10 of `design.md` "Release workflow" in that order: checkout `fetch-depth: 0`; tag check before `ci`; `ci`; find the Release by `tag_name` through `gh api repos/${{ github.repository }}/releases` and download existing assets; import signing into a temporary keychain with `sudo security add-trusted-cert -p codeSign`; `cargo xtask release`; `gh release create --draft --verify-tag [--prerelease]` only when absent, then `gh release upload --clobber`; stop while `vars.TAP_REPOSITORY == ''`; otherwise checkout the tap with `TAP_TOKEN`, copy the cask, `brew trust --cask` and `brew tap` the checkout, `brew style` and `brew audit --cask`, `gh release edit --draft=false`, commit `navori <version>` and push only on a diff; `if: always()` keychain cleanup. No `run:` line echoes a secret.
- [ ] **Step 3: Run** `cargo xtask ci` — expect: pass, the pin lint reading `release.yml`. Then change the pin to `@v4` locally — expect: `ci` fails naming that line; restore.
- [ ] **Step 4: Commit** `git commit -m "ci: publish a signed release and update the tap when a version tag is pushed"`

_Requirements: RELEASE-1.1, RELEASE-1.2, RELEASE-1.4, RELEASE-1.9, RELEASE-1.11, RELEASE-1.12, RELEASE-3.1, RELEASE-3.6, RELEASE-6.1, RELEASE-6.2, RELEASE-7.3, RELEASE-7.5, RELEASE-7.7_

---

### Task 8: Write the release steps and the one-time setup

**Files:**
- Modify: `docs/agents/project.md`, `README.md`

**Reuse:** rung 4 — GitHub settings (Environment, ruleset, fine-grained token) and macOS user accounts; `docs/agents/project.md` `## release steps` (`:135-148`)

**Interfaces:**
- Consumes: the commands of Tasks 2, 4 and 5; the workflow inputs of Task 7.
- Produces: `docs/agents/project.md` `## release steps` rewritten: (1) `cargo xtask release-check`, (2) `git tag v<version>`, (3) `git push origin v<version>`, (4) the clean-account checks of RELEASE-3.7, RELEASE-3.8 and, between two pre-releases, RELEASE-4.4; a `## Release setup (one time)` section: export `Navori Local Signing` with its key as `.p12` and keep an offline copy, Environment `release` limited to `v*` with the three secrets, tag ruleset for `v*`, tap repository and its fine-grained token (contents write on the tap only), `TAP_REPOSITORY` left unset until the first public release, and the rule to add `**Compat obligation:** External` before the first final tag. `README.md` gains `## Install` with the `brew tap`, `brew trust --cask`, `brew install --cask` lines and the self-signed note.

**Depends-on:** Task 5, Task 7

- [ ] **Step 1: Write** both docs from the Produces list; remove "No notarization or public distribution yet" and point to ADR 0007.
- [ ] **Step 2: Check** every command in the new text exists (`cargo xtask --help` lists `release` and `release-check`) and every requirement named is in `requirements.md`.
- [ ] **Step 3: Commit** `git commit -m "docs: release steps and one-time setup for tagged public releases"`

_Requirements: RELEASE-6.3, RELEASE-7.6_

---

### Task 9: Prove it on GitHub and on a clean Mac account

**Files:**
- Create: `docs/specs/2026-11-04-public-release/acceptance.md`

**Reuse:** rung 4 — GitHub settings (Environment, ruleset, fine-grained token) and macOS user accounts; `docs/agents/project.md` `## release steps` (`:135-148`)

**Interfaces:**
- Consumes: everything above.
- Produces: `acceptance.md` — one row per check: what was run, by whom, when, the observed result (Release URL, run URL, `pmset` line), pass / fail.

**Depends-on:** Task 1, Task 7, Task 8

- [ ] **Step 1: Operator setup** (the operator runs these; the agent only lists them): the one-time setup of Task 8 against a **test tap** repository, `TAP_REPOSITORY` unset.
- [ ] **Step 2: Draft run:** bump the workspace version to `0.1.0-rc.1` on a branch and commit; `cargo xtask release-check` — expect: `ci` and `perf` pass and it prints `v0.1.0-rc.1` with the push commands, and `git tag -l` shows no new tag; push tag `v0.1.0-rc.9` — expect: the run fails at the tag check naming `0.1.0-rc.9` and `0.1.0-rc.1`, no Release; push tag `v0.1.0-rc.1` — expect: a draft pre-release with the zip and `.sha256`, no tap change; a push to a branch without a tag starts no release run.
- [ ] **Step 3: Tap run:** set `TAP_REPOSITORY` to the test tap, rerun the job — expect: the Release is published, the cask commit lands; rerun again — expect: no second Release, no new tap commit. Break the tap step once (wrong token) and rerun after fixing — expect: it completes.
- [ ] **Step 4: Clean account:** on a new macOS 26 user, `brew tap`, `brew trust --cask`, `brew install --cask` from the test tap, open Navori — expect: menubar icon, no Gatekeeper dialog; install Claude Code hooks, work in a session — expect: `Navori: keeping this Mac awake` in `pmset -g assertions`.
- [ ] **Step 5: Upgrade:** save an AI key and change a setting, push `v0.1.0-rc.2`, `brew upgrade` — expect: setting, key (no Keychain prompt) and hooks (installed, not needing repair) still there; `navori doctor` all ✓.
- [ ] **Step 6: Uninstall:** `brew uninstall --zap --cask navori`, then a Claude Code session — expect: no hook error, the two release directories in the Trash, the dev profile's untouched.
- [ ] **Step 7: Record** every result in `acceptance.md`, run `cargo xtask ci`, and commit `git commit -m "docs(release): record the release acceptance run"`. The test tap is then deleted or kept private; the public tap is created only when MILE-2 closes and ROAD-17 and ROAD-18 ship.

_Requirements: RELEASE-3.7, RELEASE-3.8, RELEASE-4.1, RELEASE-4.2, RELEASE-4.3, RELEASE-4.4, RELEASE-4.5_
