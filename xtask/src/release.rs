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
    use super::{is_prerelease, tag_version};

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
