//! Project, worktree and branch for a working directory.

use std::path::{Component, Path, PathBuf};

/// Project and, when the directory is a linked checkout, its worktree and branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub project: String,
    pub worktree: Option<String>,
    pub branch: Option<Branch>,
}

/// The branch checked out in the resolved repository, or a detached commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Branch {
    Named(String),
    Detached { abbrev: String },
}

/// Resolves the project, worktree and branch that contain `cwd`.
pub fn resolve_location(cwd: &Path) -> Location {
    let mut dir = Some(cwd);
    while let Some(current) = dir {
        let git = current.join(".git");
        if git.is_dir() {
            return Location {
                project: dir_name(current),
                worktree: None,
                branch: read_branch(&git.join("HEAD")),
            };
        }
        if git.is_file() {
            return from_git_file(current, &git);
        }
        note_unreadable(&git);
        dir = current.parent();
    }
    Location {
        project: dir_name(cwd),
        worktree: None,
        branch: None,
    }
}

fn from_git_file(work_tree: &Path, git_file: &Path) -> Location {
    let Some(gitdir) = read_gitdir(git_file) else {
        return Location {
            project: dir_name(work_tree),
            worktree: None,
            branch: None,
        };
    };
    let branch = read_branch(&gitdir.join("HEAD"));
    if parent_named(&gitdir, "worktrees") {
        let project = read_common_dir(&gitdir)
            .as_deref()
            .map(project_of_common)
            .unwrap_or_default();
        return Location {
            project,
            worktree: Some(dir_name(work_tree)),
            branch,
        };
    }
    Location {
        project: dir_name(work_tree),
        worktree: None,
        branch,
    }
}

fn read_gitdir(git_file: &Path) -> Option<PathBuf> {
    let line = first_line(git_file)?;
    let raw = line.strip_prefix("gitdir:")?.trim();
    if raw.is_empty() {
        return None;
    }
    // Relative to the directory containing the `.git` file, not to `cwd`.
    git_file.parent().map(|base| normalize(&base.join(raw)))
}

fn read_common_dir(gitdir: &Path) -> Option<PathBuf> {
    let raw = first_line(&gitdir.join("commondir"))?;
    Some(normalize(&gitdir.join(raw)))
}

/// A common dir ending in `/.git` names its parent checkout. Otherwise the last
/// component is the project (a worktree whose common dir is under `/.git/modules/`).
fn project_of_common(common: &Path) -> String {
    if common.file_name().is_some_and(|name| name == ".git") {
        return common.parent().map(dir_name).unwrap_or_default();
    }
    dir_name(common)
}

fn read_branch(head: &Path) -> Option<Branch> {
    let line = first_line(head)?;
    if let Some(reference) = line.strip_prefix("ref:") {
        let name = reference.trim().strip_prefix("refs/heads/")?;
        if name.is_empty() {
            return None;
        }
        return Some(Branch::Named(name.to_owned()));
    }
    if line.len() >= 7 && line.chars().all(|c| c.is_ascii_hexdigit()) {
        let abbrev: String = line.chars().take(7).collect();
        return Some(Branch::Detached {
            abbrev: abbrev.to_ascii_lowercase(),
        });
    }
    None
}

fn note_unreadable(path: &Path) {
    if let Err(err) = std::fs::metadata(path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        tracing::debug!("unreadable git metadata");
    }
}

fn first_line(path: &Path) -> Option<String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::debug!("unreadable git metadata");
            }
            return None;
        }
    };
    let line = text.lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_owned())
    }
}

fn parent_named(path: &Path, name: &str) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|parent| parent == name)
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let top = out.components().next_back();
                if matches!(top, Some(Component::Normal(_))) {
                    out.pop();
                } else if !matches!(top, Some(Component::RootDir | Component::Prefix(_))) {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
