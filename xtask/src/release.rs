use std::path::Path;

use anyhow::{Result, bail, ensure};

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

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the release build writes the cask once it packages the zip"
    )
)]
pub(crate) fn cask_text(version: &str, sha256: &str) -> String {
    include_str!("../assets/navori.rb.in")
        .replace("@VERSION@", version)
        .replace("@SHA256@", sha256)
}

pub(crate) fn run(_root: &Path, tag: &str, _out: Option<&Path>, check: bool) -> Result<()> {
    let version = tag_version(tag, env!("CARGO_PKG_VERSION"))?;
    if check {
        println!("version={version}");
        println!("prerelease={}", is_prerelease(version));
        return Ok(());
    }
    bail!("release build not implemented")
}

#[cfg(test)]
mod tests {
    use super::{cask_text, is_prerelease, tag_version};

    fn cask() -> String {
        cask_text("0.1.0", &"a".repeat(64))
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
}
