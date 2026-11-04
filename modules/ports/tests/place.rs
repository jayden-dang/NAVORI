#![allow(clippy::unwrap_used)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::{LogCapture, canonical_temp_home, git, init_repo};
use navori_macos::fake::{FakeProcess, FakeProcesses};
use navori_mod_ports::{BranchName, HarnessMemo, Placement, Scanner, place_listener};
use navori_procs::Unavailable;

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

fn scanned_placement(home: &Path, process: FakeProcess) -> Placement {
    let source = Arc::new(FakeProcesses::new(501));
    source.add(process.listen("127.0.0.1", 3000));
    let scan = Scanner::new(source, home.to_path_buf())
        .scan(HarnessMemo::new())
        .0
        .unwrap();
    assert_eq!(scan.entries.len(), 1);
    scan.entries[0].place.clone()
}

fn server(exe: &str) -> FakeProcess {
    FakeProcess::new(100, 1, 501, 10_000, exe)
}

#[test]
fn a_scanned_listener_is_placed_by_its_working_directory() {
    let fx = fixture();
    let app = fx.home.join("work").join("app");
    init_repo(&app);
    git(&app, &["worktree", "add", ".worktrees/lane", "-b", "lane"]);
    let lane = app.join(".worktrees").join("lane");

    let in_repo = scanned_placement(&fx.home, server("/usr/local/bin/node").cwd(&app));
    assert_eq!(
        in_repo,
        Placement {
            branch: Some(BranchName::Named("main".to_owned())),
            ..project_only("app")
        }
    );

    let in_worktree = scanned_placement(&fx.home, server("/usr/local/bin/node").cwd(&lane));
    assert_eq!(
        in_worktree,
        Placement {
            project: Some("app".to_owned()),
            worktree: Some("lane".to_owned()),
            branch: Some(BranchName::Named("lane".to_owned())),
            missing_path: None,
        }
    );
}

#[test]
fn a_scanned_listener_without_a_repo_by_directory_is_placed_by_its_executable() {
    let fx = fixture();
    let tool = fx.home.join("tools").join("cli");
    init_repo(&tool);
    fs::create_dir_all(tool.join("bin")).unwrap();
    let plain = fx.home.join("plain");
    fs::create_dir_all(&plain).unwrap();
    let exe = tool.join("bin").join("serve");

    let placement = scanned_placement(&fx.home, server(exe.to_str().unwrap()).cwd(&plain));
    assert_eq!(
        placement,
        Placement {
            branch: Some(BranchName::Named("main".to_owned())),
            ..project_only("cli")
        }
    );
}

#[test]
fn a_scanned_listener_with_neither_directory_nor_executable_in_a_repo_has_no_project() {
    let fx = fixture();
    let plain = fx.home.join("plain");
    fs::create_dir_all(&plain).unwrap();

    let placement = scanned_placement(&fx.home, server("/usr/local/bin/node").cwd(&plain));
    assert_eq!(placement, Placement::default());
}

#[test]
fn a_removed_directory_without_a_counted_repo_above_it_falls_through_to_the_executable() {
    let fx = fixture();
    let tool = fx.home.join("tools").join("cli");
    init_repo(&tool);
    fs::create_dir_all(tool.join("bin")).unwrap();
    let plain = fx.home.join("plain");
    let gone = plain.join("deleted");
    fs::create_dir_all(&plain).unwrap();

    let by_exe = placed(Ok(&gone), Ok(&tool.join("bin").join("serve")), &fx.home);
    assert_eq!(by_exe.project.as_deref(), Some("cli"));
    assert_eq!(by_exe.missing_path, None);

    let nowhere = placed(Ok(&gone), Ok(Path::new("/usr/local/bin/node")), &fx.home);
    assert_eq!(nowhere, Placement::default());
}

#[test]
fn a_removed_executable_directory_is_placed_by_its_nearest_existing_ancestor_and_keeps_no_missing_path()
 {
    let fx = fixture();
    let tool = fx.home.join("tools").join("cli");
    init_repo(&tool);
    let plain = fx.home.join("plain");
    fs::create_dir_all(&plain).unwrap();
    let exe = tool.join("gone").join("serve");

    let placement = placed(Ok(&plain), Ok(&exe), &fx.home);
    assert_eq!(placement.project.as_deref(), Some("cli"));
    assert_eq!(placement.worktree, None);
    assert_eq!(placement.branch, None);
    assert_eq!(placement.missing_path, None);
}

#[test]
fn a_path_outside_home_is_never_looked_at_for_a_repository() {
    let fx = fixture();
    let file = fx.outside.join("file");
    fs::write(&file, "").unwrap();
    let below_a_file = file.join("child");
    let logs = LogCapture::start();

    let found = placed(Ok(&below_a_file), Err(Unavailable::Exited), &fx.home);

    assert_eq!(found, Placement::default());
    assert!(!logs.text().contains("unreadable git metadata"));
}
