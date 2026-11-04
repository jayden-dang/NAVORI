#![allow(clippy::unwrap_used)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::canonical_temp_home;
use navori_mod_ports::{BranchName, Placement, place_listener};
use navori_procs::Unavailable;

fn git(dir: &Path, args: &[&str]) {
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
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-b", "main"]);
    git(dir, &["commit", "--allow-empty", "-m", "init"]);
}

fn placed(
    cwd: Result<&Path, Unavailable>,
    exe: Result<&Path, Unavailable>,
    home: &Path,
) -> Placement {
    place_listener(cwd.map(Path::to_path_buf), exe.map(Path::to_path_buf), home)
}

fn project_only(project: &str) -> Placement {
    Placement {
        project: Some(project.to_owned()),
        worktree: None,
        branch: None,
        missing_path: None,
    }
}

struct Fixture {
    _home: tempfile::TempDir,
    _outside: tempfile::TempDir,
    home: PathBuf,
    outside: PathBuf,
}

fn fixture() -> Fixture {
    let (home_guard, home) = canonical_temp_home();
    let (outside_guard, outside) = canonical_temp_home();
    Fixture {
        _home: home_guard,
        _outside: outside_guard,
        home,
        outside,
    }
}

#[test]
fn each_listener_is_placed_by_the_first_rule_that_holds() {
    let fx = fixture();
    let home = &fx.home;

    let app = home.join("work").join("app");
    init_repo(&app);
    fs::create_dir_all(app.join("src")).unwrap();
    git(&app, &["worktree", "add", ".worktrees/lane", "-b", "lane"]);
    let lane = app.join(".worktrees").join("lane");

    let tool = home.join("tools").join("cli");
    init_repo(&tool);
    fs::create_dir_all(tool.join("bin")).unwrap();

    let opt = fx.outside.join("homebrew");
    init_repo(&opt);

    let dot = home.join(".oh-my-zsh");
    init_repo(&dot);

    let plain = home.join("plain");
    fs::create_dir_all(&plain).unwrap();

    let none = Placement::default();
    let node = Path::new("/usr/local/bin/node");

    let in_repo = placed(Ok(&app.join("src")), Ok(node), home);
    assert_eq!(
        in_repo,
        Placement {
            project: Some("app".to_owned()),
            worktree: None,
            branch: Some(BranchName::Named("main".to_owned())),
            missing_path: None,
        }
    );

    let in_worktree = placed(Ok(&lane), Ok(node), home);
    assert_eq!(
        in_worktree,
        Placement {
            project: Some("app".to_owned()),
            worktree: Some("lane".to_owned()),
            branch: Some(BranchName::Named("lane".to_owned())),
            missing_path: None,
        }
    );

    let by_exe = placed(Ok(&plain), Ok(&tool.join("bin").join("serve")), home);
    assert_eq!(
        by_exe,
        Placement {
            project: Some("cli".to_owned()),
            worktree: None,
            branch: Some(BranchName::Named("main".to_owned())),
            missing_path: None,
        }
    );

    assert_eq!(placed(Ok(&plain), Ok(node), home), none);
    assert_eq!(placed(Ok(&opt), Ok(node), home), none);
    assert_eq!(placed(Ok(&dot), Ok(node), home), none);
    assert_eq!(
        placed(Err(Unavailable::PermissionDenied), Ok(node), home),
        none
    );

    assert_eq!(
        placed(Ok(&plain), Err(Unavailable::PermissionDenied), home),
        none
    );
    assert_eq!(
        placed(
            Err(Unavailable::PermissionDenied),
            Err(Unavailable::Exited),
            home
        ),
        none
    );
    assert_eq!(
        placed(Ok(&opt), Ok(&tool.join("bin").join("serve")), home)
            .project
            .as_deref(),
        Some("cli")
    );

    fs::remove_dir_all(&lane).unwrap();
    let gone = placed(Ok(&lane.join("src")), Ok(node), home);
    assert_eq!(
        gone,
        Placement {
            missing_path: Some(lane.join("src")),
            ..project_only("app")
        }
    );
}

#[test]
fn a_repo_that_is_the_home_directory_itself_places_nothing() {
    let fx = fixture();
    init_repo(&fx.home);
    let sub = fx.home.join("notes");
    fs::create_dir(&sub).unwrap();

    let found = placed(Ok(&sub), Err(Unavailable::Exited), &fx.home);
    assert_eq!(found, Placement::default());
}
