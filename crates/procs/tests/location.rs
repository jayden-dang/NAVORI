//! Project, worktree and branch resolved from real temporary git trees.

use std::fs;
use std::path::Path;
use std::process::Command;

use navori_procs::{Branch, Location, resolve_location};

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args);
    for (key, _) in std::env::vars() {
        if key.starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed ({}): {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn init_repo(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    git(dir, &["init", "-b", "main"])?;
    git(dir, &["commit", "--allow-empty", "-m", "init"])?;
    Ok(())
}

fn temp_root() -> Result<tempfile::TempDir, String> {
    tempfile::tempdir().map_err(|e| e.to_string())
}

fn utf8_path(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("path is not utf-8: {}", path.display()))
}

#[test]
fn a_directory_inside_a_main_checkout_reports_its_project_and_branch() -> Result<(), String> {
    let root = temp_root()?;
    let repo = root.path().join("navori");
    init_repo(&repo)?;
    let src = repo.join("src");
    fs::create_dir(&src).map_err(|e| e.to_string())?;

    assert_eq!(
        resolve_location(&src),
        Location {
            project: "navori".to_owned(),
            worktree: None,
            branch: Some(Branch::Named("main".to_owned())),
        }
    );
    Ok(())
}

#[test]
fn a_linked_worktree_reports_the_main_project_and_the_worktree_name() -> Result<(), String> {
    let root = temp_root()?;
    let repo = root.path().join("navori");
    init_repo(&repo)?;
    git(&repo, &["worktree", "add", "../navori-feat", "-b", "feat"])?;

    assert_eq!(
        resolve_location(&root.path().join("navori-feat")),
        Location {
            project: "navori".to_owned(),
            worktree: Some("navori-feat".to_owned()),
            branch: Some(Branch::Named("feat".to_owned())),
        }
    );
    Ok(())
}

#[test]
fn a_submodule_is_its_own_project_and_not_a_worktree() -> Result<(), String> {
    let root = temp_root()?;
    let origin = root.path().join("lib-origin");
    init_repo(&origin)?;
    let repo = root.path().join("navori");
    init_repo(&repo)?;
    git(
        &repo,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            &utf8_path(&origin)?,
            "lib",
        ],
    )?;

    let location = resolve_location(&repo.join("lib"));
    assert_eq!(location.project, "lib");
    assert_eq!(location.worktree, None);
    Ok(())
}

#[test]
fn a_detached_head_reports_the_abbreviated_commit() -> Result<(), String> {
    let root = temp_root()?;
    let repo = root.path().join("navori");
    init_repo(&repo)?;
    git(&repo, &["checkout", "--detach"])?;
    let abbrev = git(&repo, &["rev-parse", "--short=7", "HEAD"])?;

    assert_eq!(
        resolve_location(&repo),
        Location {
            project: "navori".to_owned(),
            worktree: None,
            branch: Some(Branch::Detached { abbrev }),
        }
    );
    Ok(())
}

#[test]
fn a_directory_outside_any_repository_reports_its_own_name() -> Result<(), String> {
    let root = temp_root()?;
    let notes = root.path().join("notes");
    fs::create_dir(&notes).map_err(|e| e.to_string())?;

    assert_eq!(
        resolve_location(&notes),
        Location {
            project: "notes".to_owned(),
            worktree: None,
            branch: None,
        }
    );
    Ok(())
}
