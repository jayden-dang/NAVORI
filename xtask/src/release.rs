use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

use super::{Flavor, Profile, bundle, output, sh};

/// The version a release tag names, which must equal the workspace version.
pub(crate) fn tag_version<'a>(tag: &'a str, workspace: &str) -> Result<&'a str> {
    let Some(version) = tag.strip_prefix('v') else {
        bail!("tag `{tag}` must start with `v`, e.g. `v{workspace}`");
    };
    ensure!(
        version == workspace,
        "tag `{tag}` names version {version} but the workspace version is {workspace}"
    );
    Ok(version)
}

/// A version is a pre-release when a `-` appears before any `+` build metadata.
pub(crate) fn is_prerelease(version: &str) -> bool {
    let core = version.split_once('+').map_or(version, |(core, _)| core);
    core.contains('-')
}

/// The cask file a version publishes: release candidates have their own cask.
pub(crate) fn cask_file(prerelease: bool) -> &'static str {
    if prerelease {
        "navori-rc.rb"
    } else {
        "navori.rb"
    }
}

pub(crate) fn cask_text(version: &str, sha256: &str, prerelease: bool) -> String {
    let (cask, desc_suffix, conflicts, caveat_extra) = if prerelease {
        (
            "navori-rc",
            " (RC)",
            "navori",
            "    This is a release candidate. `brew install --cask navori` is the stable cask.\n",
        )
    } else {
        ("navori", "", "navori-rc", "")
    };
    include_str!("../assets/navori.rb.in")
        .replace("    @CAVEAT_EXTRA@\n", caveat_extra)
        .replace("@CASK@", cask)
        .replace("@DESC_SUFFIX@", desc_suffix)
        .replace("@CONFLICTS@", conflicts)
        .replace("@VERSION@", version)
        .replace("@SHA256@", sha256)
}

/// The `designated =>` lines of `codesign -d -r-` stdout, trimmed.
pub(crate) fn designated_lines(codesign_stdout: &str) -> Vec<String> {
    codesign_stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("designated =>"))
        .map(str::to_owned)
        .collect()
}

/// Text showing the expected and actual requirements when they differ.
pub(crate) fn requirement_mismatch(expected: &str, actual: &[String]) -> Option<String> {
    let expected: Vec<&str> = expected
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if expected == actual {
        return None;
    }
    Some(format!(
        "designated requirement mismatch\nexpected:\n  {}\nactual:\n  {}",
        expected.join("\n  "),
        actual.join("\n  ")
    ))
}

/// Whether a `.sha256` file line names exactly the release zip as its file.
pub(crate) fn sha_names_zip(sha_line: &str, zip_name: &str) -> bool {
    let mut fields = sha_line.split_whitespace();
    let (Some(_hash), Some(name), None) = (fields.next(), fields.next(), fields.next()) else {
        return false;
    };
    name == zip_name
}

/// Text naming both versions when the app's bundle version is not the tag's.
pub(crate) fn bundle_version_mismatch(actual: &str, expected: &str) -> Option<String> {
    (actual.trim() != expected).then(|| {
        format!(
            "the app's CFBundleShortVersionString is \"{}\" but the tag names {expected}",
            actual.trim()
        )
    })
}

/// Create the output directory only when nothing is wrong with the identity.
pub(crate) fn create_out_unless(problem: Option<String>, out: &Path) -> Result<()> {
    if let Some(problem) = problem {
        bail!(problem);
    }
    std::fs::create_dir_all(out).with_context(|| format!("create {}", out.display()))
}

/// Why the release cannot be signed, given `NAVORI_SIGN_IDENTITY` and the
/// stdout of `security find-identity -v -p codesigning`.
pub(crate) fn identity_problem(env: Option<&str>, find_identity_stdout: &str) -> Option<String> {
    let Some(identity) = env.filter(|identity| !identity.is_empty()) else {
        return Some(
            "NAVORI_SIGN_IDENTITY is not set; a release is never signed ad-hoc".to_owned(),
        );
    };
    let listed = find_identity_stdout
        .lines()
        .any(|line| line.contains(&format!("\"{identity}\"")));
    (!listed).then(|| {
        format!("signing identity \"{identity}\" (NAVORI_SIGN_IDENTITY) is not a valid code-signing identity in the keychain")
    })
}

const COMPAT_LINE: &str = "- **Compat obligation:** External";

/// Every reason a release must not be cut yet, from the outputs of the git queries and project.md.
pub(crate) fn preflight(
    version: &str,
    local_tags: &str,
    remote_tags: &str,
    dirty: &str,
    project_md: &str,
) -> Result<(), String> {
    let tag = format!("v{version}");
    if !local_tags.trim().is_empty() {
        return Err(format!("tag {tag} already exists locally"));
    }
    if !remote_tags.trim().is_empty() {
        return Err(format!("tag {tag} already exists on origin"));
    }
    if !dirty.trim().is_empty() {
        return Err(format!(
            "uncommitted changes; commit or stash them first:\n{}",
            dirty.trim_end()
        ));
    }
    if !is_prerelease(version) && !project_md.lines().any(|l| l.starts_with(COMPAT_LINE)) {
        return Err(format!(
            "a final version needs a line starting `{COMPAT_LINE}` in docs/agents/project.md"
        ));
    }
    Ok(())
}

pub(crate) fn check(root: &Path) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let tag = format!("v{version}");
    let local_tags = output(
        Command::new("git")
            .args(["tag", "-l", &tag])
            .current_dir(root),
    )?;
    let remote_tags = output(
        Command::new("git")
            .args(["ls-remote", "--tags", "origin", &tag])
            .current_dir(root),
    )?;
    let dirty = output(
        Command::new("git")
            .args(["status", "--porcelain", "--untracked-files=no"])
            .current_dir(root),
    )?;
    let path = root.join("docs/agents/project.md");
    let project_md =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    preflight(version, &local_tags, &remote_tags, &dirty, &project_md)
        .map_err(|problem| anyhow::anyhow!(problem))?;
    super::ci(root)?;
    ensure!(super::perf::run(root, false)?, "perf gate failed");
    ensure!(
        super::soak::gate(root, &root.join("build/perf/Navori Dev.app"))?,
        "soak gate failed"
    );
    println!("{tag}");
    println!("git tag {tag} && git push origin {tag}");
    Ok(())
}

pub(crate) fn run(root: &Path, tag: &str, out: Option<&Path>, check: bool) -> Result<()> {
    let version = tag_version(tag, env!("CARGO_PKG_VERSION"))?;
    if check {
        println!("version={version}");
        println!("prerelease={}", is_prerelease(version));
        return Ok(());
    }
    let out = out.context("--out <dir> is required unless --check is given")?;
    let zip_name = format!("Navori-{version}.zip");
    let sha_path = out.join(format!("{zip_name}.sha256"));

    if out.join(&zip_name).exists() && sha_path.exists() {
        verify_reused(out, &zip_name, &sha_path, version)?;
    } else {
        build_zip(root, out, &zip_name, &sha_path)?;
    }

    let sha_text = std::fs::read_to_string(&sha_path)
        .with_context(|| format!("read {}", sha_path.display()))?;
    let hash = sha_text
        .split_whitespace()
        .next()
        .with_context(|| format!("{} is empty", sha_path.display()))?;
    let prerelease = is_prerelease(version);
    let cask = out.join(cask_file(prerelease));
    std::fs::write(&cask, cask_text(version, hash, prerelease))
        .with_context(|| format!("write {}", cask.display()))?;
    println!("{}", out.join(&zip_name).display());
    println!("{}", sha_path.display());
    println!("{}", cask.display());
    Ok(())
}

/// Check a zip and hash downloaded from an earlier run before it is reused.
fn verify_reused(out: &Path, zip_name: &str, sha_path: &Path, version: &str) -> Result<()> {
    let sha_text = std::fs::read_to_string(sha_path)
        .with_context(|| format!("read {}", sha_path.display()))?;
    ensure!(
        sha_names_zip(sha_text.trim(), zip_name),
        "{} must name exactly {zip_name}",
        sha_path.display()
    );
    output(
        Command::new("shasum")
            .args(["-a", "256", "-c"])
            .arg(sha_path.file_name().unwrap_or_default())
            .current_dir(out),
    )?;

    let dir = std::env::temp_dir().join(format!("navori-reuse-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let result = verify_unzipped(&out.join(zip_name), &dir, version);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn verify_unzipped(zip: &Path, dir: &Path, version: &str) -> Result<()> {
    sh(Command::new("ditto").arg("-x").arg("-k").arg(zip).arg(dir))?;
    let app = dir.join("Navori.app");
    verify_signed_app(&app)?;
    let plist = app.join("Contents/Info.plist");
    let actual = output(
        Command::new("/usr/libexec/PlistBuddy")
            .args(["-c", "Print :CFBundleShortVersionString"])
            .arg(&plist),
    )?;
    if let Some(problem) = bundle_version_mismatch(&actual, version) {
        bail!(problem);
    }
    Ok(())
}

/// The app's signature must verify and carry the pinned designated requirements.
fn verify_signed_app(app: &Path) -> Result<()> {
    sh(Command::new("codesign")
        .args(["--verify", "--strict", "--deep"])
        .arg(app))?;
    let mut actual = Vec::new();
    for target in [app.to_path_buf(), app.join("Contents/Helpers/navori")] {
        let stdout = output(Command::new("codesign").args(["-d", "-r-"]).arg(&target))?;
        actual.extend(designated_lines(&stdout));
    }
    if let Some(problem) = requirement_mismatch(
        include_str!("../assets/designated-requirement.txt"),
        &actual,
    ) {
        bail!(problem);
    }
    Ok(())
}

fn build_zip(root: &Path, out: &Path, zip_name: &str, sha_path: &Path) -> Result<()> {
    let count = output(
        Command::new("git")
            .args(["rev-list", "--count", "HEAD"])
            .current_dir(root),
    )
    .context(
        "the build number comes from `git rev-list --count HEAD`; a release needs full history",
    )?;
    ensure!(
        !count.trim().is_empty(),
        "git rev-list printed no commit count"
    );

    let identities =
        output(Command::new("security").args(["find-identity", "-v", "-p", "codesigning"]))?;
    create_out_unless(
        identity_problem(
            std::env::var("NAVORI_SIGN_IDENTITY").ok().as_deref(),
            &identities,
        ),
        out,
    )?;

    let app = bundle(root, Profile::Release, Flavor::Release, out)?;
    verify_signed_app(&app)?;

    let zip = out.join(zip_name);
    let _ = std::fs::remove_file(&zip);
    sh(Command::new("ditto")
        .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
        .arg(&app)
        .arg(&zip))?;
    let sum = output(
        Command::new("shasum")
            .args(["-a", "256", zip_name])
            .current_dir(out),
    )?;
    std::fs::write(sha_path, sum).with_context(|| format!("write {}", sha_path.display()))?;
    Ok(())
}

/// The `uses:` lines that are not pinned to an immutable ref, with their 1-based line numbers.
pub(crate) fn unpinned_actions(workflow: &str) -> Vec<(usize, String)> {
    workflow
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| {
            let Some(value) = uses_value(line) else {
                return false;
            };
            if value.starts_with("./") {
                return false;
            }
            !is_immutable_pin(value)
        })
        .map(|(number, line)| (number, line.to_owned()))
        .collect()
}

fn is_immutable_pin(value: &str) -> bool {
    let all_hex = |pin: &str| pin.bytes().all(|b| b.is_ascii_hexdigit());
    if let Some(image) = value.strip_prefix("docker://") {
        let digest = image
            .rsplit_once("@sha256:")
            .map_or("", |(_, digest)| digest);
        return digest.len() == 64 && all_hex(digest);
    }
    let pin = value.rsplit_once('@').map_or("", |(_, pin)| pin);
    pin.len() == 40 && all_hex(pin)
}

fn uses_value(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("- ").map_or(line, str::trim_start);
    let rest = rest.strip_prefix("uses:")?.trim();
    let value = match rest.chars().next()? {
        quote @ ('"' | '\'') => rest[1..].split(quote).next()?,
        _ => rest.split(" #").next()?.trim(),
    };
    Some(value)
}

pub(crate) fn check_workflows(root: &Path) -> Result<()> {
    let dir = root.join(".github/workflows");
    if !dir.is_dir() {
        return Ok(());
    }
    let mut violations = Vec::new();
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        paths.push(entry?.path());
    }
    paths.sort();
    for path in paths {
        if !matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yml" | "yaml")
        ) {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        for (number, line) in unpinned_actions(&text) {
            violations.push(format!("{}:{number}: {line}", path.display()));
        }
    }
    ensure!(
        violations.is_empty(),
        "workflow actions must be pinned to a full commit SHA:\n  {}",
        violations.join("\n  ")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        bundle_version_mismatch, cask_file, cask_text, check_workflows, create_out_unless,
        designated_lines, identity_problem, is_prerelease, preflight, requirement_mismatch,
        sha_names_zip, tag_version, unpinned_actions,
    };

    const COMPAT: &str = "# Project\n- **Compat obligation:** External\n";

    const LEAF: &str = "8c56e155bdd4ed3c038035509193fb7d33d302c1";
    const IDENTITIES: &str = "  1) 8C56E155BDD4ED3C038035509193FB7D33D302C1 \"Navori Local Signing\"\n     1 valid identities found\n";

    fn expected() -> String {
        include_str!("../assets/designated-requirement.txt").to_owned()
    }

    fn app_requirement(leaf: &str) -> Vec<String> {
        designated_lines(&format!(
            "designated => identifier \"dev.navori.app\" and certificate leaf = H\"{leaf}\"\n\
             designated => identifier \"dev.navori.app.cli\" and certificate leaf = H\"{leaf}\"\n"
        ))
    }

    #[test]
    fn a_free_tag_on_a_clean_tree_passes() {
        assert_eq!(preflight("0.1.0-rc.1", "", "", "", ""), Ok(()));
    }

    #[test]
    fn a_tag_that_already_exists_locally_fails_naming_it() {
        let err = preflight("0.1.0-rc.1", "v0.1.0-rc.1\n", "", "", "").unwrap_err();
        assert!(
            err.contains("v0.1.0-rc.1") && err.contains("locally"),
            "{err}"
        );
    }

    #[test]
    fn a_tag_that_already_exists_on_origin_fails_naming_it() {
        let remote = "0123456789abcdef0123456789abcdef01234567\trefs/tags/v0.1.0\n";
        let err = preflight("0.1.0", "", remote, "", COMPAT).unwrap_err();
        assert!(err.contains("v0.1.0") && err.contains("origin"), "{err}");
    }

    #[test]
    fn uncommitted_changes_fail_listing_the_files() {
        let err = preflight("0.1.0-rc.1", "", "", " M Cargo.toml\n", "").unwrap_err();
        assert!(err.contains("Cargo.toml"), "{err}");
    }

    #[test]
    fn a_final_version_needs_the_external_compat_line() {
        let err = preflight("0.1.0", "", "", "", "# Project\n").unwrap_err();
        assert!(err.contains("- **Compat obligation:** External"), "{err}");
        assert_eq!(preflight("0.1.0", "", "", "", COMPAT), Ok(()));
    }

    #[test]
    fn a_compat_line_quoted_in_prose_does_not_count() {
        let prose = "Add a `- **Compat obligation:** External` line when ready.\n";
        assert!(preflight("0.1.0", "", "", "", prose).is_err());
    }

    #[test]
    fn a_pre_release_does_not_need_the_compat_line() {
        assert_eq!(preflight("0.1.0-rc.1", "", "", "", "# Project\n"), Ok(()));
    }

    #[test]
    fn only_the_designated_lines_of_codesign_output_are_compared() {
        let stdout = "Executable=/tmp/Navori.app/Contents/MacOS/Navori\n\
                      designated => identifier \"a\"\n\
                      # designated => not a requirement line\n\
                      \x20 designated => identifier \"b\"  \n";
        assert_eq!(
            designated_lines(stdout),
            [
                "designated => identifier \"a\"",
                "designated => identifier \"b\""
            ]
        );
    }

    #[test]
    fn a_designated_requirement_with_another_certificate_is_a_mismatch() {
        let actual = app_requirement("0000000000000000000000000000000000000000");
        let text = requirement_mismatch(&expected(), &actual).unwrap();
        assert!(text.contains(LEAF), "{text}");
        assert!(
            text.contains("0000000000000000000000000000000000000000"),
            "{text}"
        );
    }

    #[test]
    fn matching_requirements_are_not_a_mismatch() {
        assert_eq!(
            requirement_mismatch(&expected(), &app_requirement(LEAF)),
            None
        );
    }

    #[test]
    fn a_missing_signing_identity_stops_the_release() {
        let unset = identity_problem(None, IDENTITIES).unwrap();
        assert!(unset.contains("NAVORI_SIGN_IDENTITY"), "{unset}");
        let unlisted = identity_problem(Some("Other Identity"), IDENTITIES).unwrap();
        assert!(unlisted.contains("Other Identity"), "{unlisted}");
        assert_eq!(
            identity_problem(Some("Navori Local Signing"), IDENTITIES),
            None
        );
    }

    fn cask() -> String {
        cask_text("0.1.0", &"a".repeat(64), false)
    }

    fn rc_cask() -> String {
        cask_text("0.2.0-rc.1", &"b".repeat(64), true)
    }

    fn caveat_lines(cask: &str) -> Vec<&str> {
        let body = cask
            .split("caveats <<~EOS\n")
            .nth(1)
            .and_then(|rest| rest.split("\n  EOS").next())
            .expect("a caveats heredoc");
        body.lines().collect()
    }

    #[test]
    fn the_final_cask_caveats_start_with_the_self_signed_line_and_are_indented_alike() {
        let cask = cask();
        assert!(
            cask.contains("caveats <<~EOS\n    Navori is self-signed"),
            "{cask}"
        );
        let lines = caveat_lines(&cask);
        assert_eq!(lines.len(), 3, "{cask}");
        assert!(
            lines
                .iter()
                .all(|l| l.starts_with("    ") && !l.starts_with("     ")),
            "{cask}"
        );
    }

    #[test]
    fn the_release_candidate_cask_caveats_are_indented_alike() {
        let cask = rc_cask();
        let lines = caveat_lines(&cask);
        assert_eq!(lines.len(), 4, "{cask}");
        assert!(
            lines
                .iter()
                .all(|l| l.starts_with("    ") && !l.starts_with("     ")),
            "{cask}"
        );
        assert!(
            lines[0].contains("release candidate") && lines[0].contains("navori"),
            "{cask}"
        );
        assert!(lines[1].starts_with("    Navori is self-signed"), "{cask}");
    }

    #[test]
    fn the_final_cask_is_named_navori_and_conflicts_with_the_release_candidate_cask() {
        let cask = cask();
        assert!(cask.starts_with("cask \"navori\" do"), "{cask}");
        assert!(
            cask.contains("conflicts_with cask: \"navori-rc\""),
            "{cask}"
        );
    }

    #[test]
    fn the_release_candidate_cask_is_named_navori_rc_and_conflicts_with_the_final_cask() {
        let cask = rc_cask();
        assert!(cask.starts_with("cask \"navori-rc\" do"), "{cask}");
        assert!(cask.contains("conflicts_with cask: \"navori\"\n"), "{cask}");
    }

    #[test]
    fn the_final_cask_never_mentions_release_candidates() {
        let cask = cask();
        assert!(!cask.contains("release candidate"), "{cask}");
        assert_eq!(cask.matches("navori-rc").count(), 1, "{cask}");
        assert!(
            cask.contains("desc \"AI-first menubar cockpit for developers running coding agents\""),
            "{cask}"
        );
    }

    #[test]
    fn the_release_candidate_cask_says_so_in_its_description_and_points_to_the_stable_cask() {
        let cask = rc_cask();
        assert!(cask.contains("running coding agents (RC)\""), "{cask}");
        assert!(cask.contains("release candidate"), "{cask}");
        assert!(cask.contains("brew install --cask navori"), "{cask}");
        assert!(cask.contains("version \"0.2.0-rc.1\""), "{cask}");
    }

    #[test]
    fn the_release_candidate_cask_keeps_everything_the_final_cask_installs() {
        let cask = rc_cask();
        for needle in [
            "depends_on arch: :arm64",
            "depends_on macos: :tahoe",
            "app \"Navori.app\"",
            "binary \"#{appdir}/Navori.app/Contents/Helpers/navori\"",
            "postflight_steps do",
            "zap trash:",
            "self-signed and not notarized",
            "Settings → Integrations",
        ] {
            assert!(cask.contains(needle), "{needle}: {cask}");
        }
        assert!(!cask.contains('@'), "{cask}");
    }

    #[test]
    fn a_pre_release_cask_goes_to_navori_rc_rb_and_a_final_one_to_navori_rb() {
        assert_eq!(cask_file(true), "navori-rc.rb");
        assert_eq!(cask_file(false), "navori.rb");
    }

    #[test]
    fn the_cask_names_the_version_and_the_zip_hash() {
        let cask = cask();
        assert!(cask.contains("version \"0.1.0\""), "{cask}");
        assert!(
            cask.contains(&format!("sha256 \"{}\"", "a".repeat(64))),
            "{cask}"
        );
        assert!(!cask.contains('@'), "{cask}");
    }

    #[test]
    fn the_cask_needs_an_arm64_mac_on_macos_26() {
        let cask = cask();
        assert!(cask.contains("depends_on arch: :arm64"), "{cask}");
        assert!(cask.contains("depends_on macos: :tahoe"), "{cask}");
    }

    #[test]
    fn the_cask_installs_the_app_and_links_the_cli() {
        let cask = cask();
        assert!(cask.contains("app \"Navori.app\""), "{cask}");
        assert!(
            cask.contains("binary \"#{appdir}/Navori.app/Contents/Helpers/navori\""),
            "{cask}"
        );
    }

    #[test]
    fn the_cask_clears_quarantine_after_install() {
        let cask = cask();
        assert!(cask.contains("postflight_steps do"), "{cask}");
        assert!(cask.contains("/usr/bin/xattr"), "{cask}");
        assert!(cask.contains("\"-dr\""), "{cask}");
        assert!(cask.contains("com.apple.quarantine"), "{cask}");
        assert!(cask.contains("{{appdir}}/Navori.app"), "{cask}");
    }

    #[test]
    fn zap_moves_only_the_release_profiles_directories_to_the_trash() {
        let cask = cask();
        assert!(cask.contains("zap trash:"), "{cask}");
        assert!(
            cask.contains("~/Library/Application Support/Navori\""),
            "{cask}"
        );
        assert!(cask.contains("~/Library/Logs/Navori\""), "{cask}");
        assert!(!cask.contains("Navori Dev"), "{cask}");
    }

    #[test]
    fn the_caveats_say_self_signed_not_notarized_and_how_to_remove_hooks() {
        let cask = cask();
        assert!(cask.contains("self-signed and not notarized"), "{cask}");
        assert!(cask.contains("Settings → Integrations"), "{cask}");
    }

    #[test]
    fn a_tag_naming_the_workspace_version_passes() {
        assert_eq!(tag_version("v0.1.0", "0.1.0").unwrap(), "0.1.0");
    }

    #[test]
    fn a_tag_naming_another_version_fails_naming_both() {
        let err = tag_version("v0.2.0", "0.1.0").unwrap_err().to_string();
        assert!(err.contains("0.2.0"), "{err}");
        assert!(err.contains("0.1.0"), "{err}");
    }

    #[test]
    fn a_tag_without_the_v_prefix_fails() {
        assert!(tag_version("0.1.0", "0.1.0").is_err());
    }

    #[test]
    fn a_version_with_a_pre_release_part_is_a_pre_release() {
        assert!(is_prerelease("0.2.0-rc.1"));
        assert!(is_prerelease("1.0.0-rc.1+b.7"));
    }

    #[test]
    fn build_metadata_alone_is_not_a_pre_release() {
        assert!(!is_prerelease("1.0.0+build-1"));
        assert!(!is_prerelease("0.1.0"));
    }

    #[test]
    fn a_hash_line_naming_the_release_zip_is_accepted() {
        let line = format!("{}  Navori-0.1.0.zip", "a".repeat(64));
        assert!(sha_names_zip(&line, "Navori-0.1.0.zip"));
    }

    #[test]
    fn a_hash_line_naming_another_file_is_rejected() {
        let hash = "a".repeat(64);
        assert!(!sha_names_zip(
            &format!("{hash}  Navori-0.0.9.zip"),
            "Navori-0.1.0.zip"
        ));
        assert!(!sha_names_zip(
            &format!("{hash}  ../Navori-0.1.0.zip"),
            "Navori-0.1.0.zip"
        ));
        assert!(!sha_names_zip(
            &format!("{hash} *Navori-0.1.0.zip"),
            "Navori-0.1.0.zip"
        ));
        assert!(!sha_names_zip(
            &format!("{hash}  Navori-0.1.0.zip extra"),
            "Navori-0.1.0.zip"
        ));
        assert!(!sha_names_zip(&hash, "Navori-0.1.0.zip"));
        assert!(!sha_names_zip("", "Navori-0.1.0.zip"));
    }

    #[test]
    fn an_app_with_another_bundle_version_fails_naming_both() {
        let text = bundle_version_mismatch("0.1.0-rc.8\n", "0.1.0-rc.9").unwrap();
        assert!(
            text.contains("0.1.0-rc.8") && text.contains("0.1.0-rc.9"),
            "{text}"
        );
        assert_eq!(bundle_version_mismatch("0.1.0-rc.9\n", "0.1.0-rc.9"), None);
    }

    #[test]
    fn an_identity_failure_leaves_no_output_directory_behind() {
        let out = std::env::temp_dir().join(format!("navori-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let err = create_out_unless(Some("no identity".to_owned()), &out).unwrap_err();
        assert!(err.to_string().contains("no identity"));
        assert!(!out.exists());
        create_out_unless(None, &out).unwrap();
        assert!(out.is_dir());
        let _ = std::fs::remove_dir_all(&out);
    }

    const SHA: &str = "11bd71901bbe5b1630ceea73d27597364c9af683";

    #[test]
    fn an_action_pinned_to_a_tag_is_reported() {
        let found = unpinned_actions("steps:\n      - uses: actions/checkout@v4\n");
        assert_eq!(found, vec![(2, "- uses: actions/checkout@v4".to_owned())]);
    }

    #[test]
    fn an_action_pinned_to_a_full_sha_passes() {
        let wf = format!("steps:\n  - uses: actions/checkout@{SHA}\n");
        assert!(unpinned_actions(&wf).is_empty());
    }

    #[test]
    fn a_local_action_passes() {
        assert!(unpinned_actions("      - uses: ./.github/actions/x\n").is_empty());
    }

    #[test]
    fn a_docker_ref_pinned_to_a_sha256_digest_passes_and_a_tag_is_reported() {
        let digest = "a".repeat(64);
        let pinned = format!("uses: docker://alpine@sha256:{digest}\n");
        assert!(unpinned_actions(&pinned).is_empty());
        assert_eq!(unpinned_actions("uses: docker://alpine:3.20\n").len(), 1);
        let short = "uses: docker://alpine@sha256:abc123\n";
        assert_eq!(unpinned_actions(short).len(), 1);
    }

    #[test]
    fn the_workflow_directory_check_reads_yml_and_yaml_files_and_names_each_line() {
        let root = std::env::temp_dir().join(format!("navori-wf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        check_workflows(&root).unwrap();

        let dir = root.join(".github/workflows");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), "uses: actions/x@v1\n").unwrap();
        check_workflows(&root).unwrap();

        std::fs::write(dir.join("b.yaml"), "a:\n  uses: actions/y@v2\n").unwrap();
        std::fs::write(dir.join("a.yml"), "uses: actions/x@v1\n").unwrap();
        let err = check_workflows(&root).unwrap_err().to_string();
        let _ = std::fs::remove_dir_all(&root);
        let first = err.find("a.yml:1:").expect(&err);
        let second = err.find("b.yaml:2:").expect(&err);
        assert!(first < second, "{err}");
    }

    #[test]
    fn a_short_sha_is_reported() {
        assert_eq!(
            unpinned_actions("uses: actions/checkout@11bd719\n").len(),
            1
        );
    }

    #[test]
    fn quoted_uses_values_are_read() {
        assert_eq!(unpinned_actions("uses: \"actions/checkout@v4\"\n").len(), 1);
        let wf = format!("uses: 'actions/checkout@{SHA}'\n");
        assert!(unpinned_actions(&wf).is_empty());
    }

    #[test]
    fn a_sha_pin_followed_by_a_version_comment_passes() {
        let wf = format!("uses: actions/checkout@{SHA} # v4.2.2\n");
        assert!(unpinned_actions(&wf).is_empty());
    }
}
