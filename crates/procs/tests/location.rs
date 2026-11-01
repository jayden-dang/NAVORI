//! Project, worktree and branch resolved from real temporary git trees.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};

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
    assert_eq!(location.branch, Some(Branch::Named("main".to_owned())));
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

struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn capture() -> Result<Arc<Mutex<Vec<u8>>>, String> {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&buf);
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .without_time()
        .with_writer(move || Captured(Arc::clone(&captured)))
        .try_init()
        .map_err(|_| "tracing subscriber was already installed".to_owned())?;
    Ok(buf)
}

fn log_text(buf: &Arc<Mutex<Vec<u8>>>) -> String {
    let bytes = buf.lock().unwrap_or_else(PoisonError::into_inner).clone();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Restores read permission so the temp directory can be removed.
struct Readable(PathBuf);

impl Drop for Readable {
    fn drop(&mut self) {
        let Ok(meta) = fs::metadata(&self.0) else {
            return;
        };
        let mut perms = meta.permissions();
        perms.set_mode(0o644);
        let _ = fs::set_permissions(&self.0, perms);
    }
}

fn deny_read(path: &Path) -> Result<Readable, String> {
    let meta = fs::metadata(path).map_err(|_| "cannot stat the git file".to_owned())?;
    let mut perms = meta.permissions();
    perms.set_mode(0o0);
    fs::set_permissions(path, perms).map_err(|_| "cannot deny the git file".to_owned())?;
    let still = fs::read(path);
    if still.is_ok() {
        return Err("the git file stayed readable".to_owned());
    }
    Ok(Readable(path.to_path_buf()))
}

fn assert_logged_without_path(
    log: &Arc<Mutex<Vec<u8>>>,
    before: usize,
    path: &Path,
) -> Result<(), String> {
    let text = log_text(log);
    if text.len() <= before {
        return Err("an unreadable git file was not logged".to_owned());
    }
    if let Some(text) = path.to_str()
        && log_text(log).contains(text)
    {
        return Err("log contains a path".to_owned());
    }
    Ok(())
}

#[test]
fn an_unreadable_git_file_head_or_commondir_is_logged() -> Result<(), String> {
    let log = capture()?;
    unreadable_git_file(&log)?;
    unreadable_head(&log)?;
    unreadable_commondir(&log)?;
    Ok(())
}

fn unreadable_git_file(log: &Arc<Mutex<Vec<u8>>>) -> Result<(), String> {
    let root = temp_root()?;
    let notes = root.path().join("notes");
    fs::create_dir(&notes).map_err(|_| "cannot create the directory".to_owned())?;

    let before = log_text(log).len();
    assert_eq!(
        resolve_location(&notes),
        Location {
            project: "notes".to_owned(),
            worktree: None,
            branch: None,
        }
    );
    if log_text(log).len() != before {
        return Err("a missing git file was logged".to_owned());
    }

    let git = notes.join(".git");
    fs::write(&git, "gitdir: SECRET_GITDIR\n")
        .map_err(|_| "cannot write the git file".to_owned())?;
    let _deny = deny_read(&git)?;
    let before = log_text(log).len();
    assert_eq!(
        resolve_location(&notes),
        Location {
            project: "notes".to_owned(),
            worktree: None,
            branch: None,
        }
    );
    assert_logged_without_path(log, before, &notes)?;
    if log_text(log).contains("SECRET_GITDIR") {
        return Err("log contains file contents".to_owned());
    }
    Ok(())
}

fn unreadable_head(log: &Arc<Mutex<Vec<u8>>>) -> Result<(), String> {
    let root = temp_root()?;
    let repo = root.path().join("navori");
    init_repo(&repo)?;
    let head = repo.join(".git").join("HEAD");
    let _deny = deny_read(&head)?;
    let before = log_text(log).len();

    assert_eq!(
        resolve_location(&repo),
        Location {
            project: "navori".to_owned(),
            worktree: None,
            branch: None,
        }
    );
    assert_logged_without_path(log, before, &repo)?;
    Ok(())
}

fn unreadable_commondir(log: &Arc<Mutex<Vec<u8>>>) -> Result<(), String> {
    let root = temp_root()?;
    let repo = root.path().join("navori");
    init_repo(&repo)?;
    git(&repo, &["worktree", "add", "../navori-feat", "-b", "feat"])?;
    let common = repo
        .join(".git")
        .join("worktrees")
        .join("navori-feat")
        .join("commondir");
    let _deny = deny_read(&common)?;
    let before = log_text(log).len();

    assert_eq!(
        resolve_location(&root.path().join("navori-feat")),
        Location {
            project: String::new(),
            worktree: Some("navori-feat".to_owned()),
            branch: Some(Branch::Named("feat".to_owned())),
        }
    );
    assert_logged_without_path(log, before, root.path())?;
    Ok(())
}
