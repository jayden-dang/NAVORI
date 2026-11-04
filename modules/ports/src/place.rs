use std::path::{Component, Path};

use navori_procs::{Branch, Field, Repo, find_repo};

use crate::state::{BranchName, Placement};

/// Places a listener by its working directory, then by the directory of its executable.
pub fn place_listener(
    cwd: Field<std::path::PathBuf>,
    exe: Field<std::path::PathBuf>,
    home: &Path,
) -> Placement {
    if let Ok(cwd) = cwd {
        let placed = place(&cwd, home);
        if placed.project.is_some() {
            return placed;
        }
    }
    let Some(dir) = exe.as_deref().ok().and_then(Path::parent) else {
        return Placement::default();
    };
    Placement {
        missing_path: None,
        ..place(dir, home)
    }
}

/// Places a path in the nearest counted repo, or nowhere. A path that no longer exists keeps
/// only the project of its nearest existing ancestor.
pub fn place(path: &Path, home: &Path) -> Placement {
    if !is_removed(path) {
        return counted(path, home).map_or_else(Placement::default, |repo| Placement {
            project: Some(repo.location.project),
            worktree: repo.location.worktree,
            branch: repo.location.branch.map(branch_name),
            missing_path: None,
        });
    }
    let Some(ancestor) = path.ancestors().find(|dir| !is_removed(dir)) else {
        return Placement::default();
    };
    counted(ancestor, home).map_or_else(Placement::default, |repo| Placement {
        project: Some(repo.location.project),
        missing_path: Some(path.to_path_buf()),
        ..Placement::default()
    })
}

fn is_removed(path: &Path) -> bool {
    matches!(std::fs::metadata(path), Err(err) if err.kind() == std::io::ErrorKind::NotFound)
}

fn counted(path: &Path, home: &Path) -> Option<Repo> {
    find_repo(path).filter(|repo| in_counted_area(&repo.root, home))
}

fn in_counted_area(root: &Path, home: &Path) -> bool {
    let Ok(rest) = root.strip_prefix(home) else {
        return false;
    };
    match rest.components().next() {
        Some(Component::Normal(first)) => !first.to_string_lossy().starts_with('.'),
        _ => false,
    }
}

fn branch_name(branch: Branch) -> BranchName {
    match branch {
        Branch::Named(name) => BranchName::Named(name),
        Branch::Detached { abbrev } => BranchName::Detached(abbrev),
    }
}
