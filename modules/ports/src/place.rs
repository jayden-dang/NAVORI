use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use navori_procs::{Field, Repo, find_repo};

use crate::state::{BranchName, Placement};

/// Places a listener by its working directory, then by the directory of its executable.
pub fn place_listener(cwd: Field<PathBuf>, exe: Field<PathBuf>, home: &Path) -> Placement {
    place_listener_cached(cwd, exe, home, &mut HashMap::new())
}

/// [`place_listener`] that places each path once per `cache`, which a scan keeps for itself.
pub(crate) fn place_listener_cached(
    cwd: Field<PathBuf>,
    exe: Field<PathBuf>,
    home: &Path,
    cache: &mut HashMap<PathBuf, Placement>,
) -> Placement {
    let mut placed_at = |path: &Path| {
        cache
            .entry(path.to_path_buf())
            .or_insert_with(|| place(path, home))
            .clone()
    };
    if let Ok(cwd) = cwd {
        let placed = placed_at(&cwd);
        if placed.project.is_some() {
            return placed;
        }
    }
    let Some(dir) = exe.as_deref().ok().and_then(Path::parent) else {
        return Placement::default();
    };
    Placement {
        missing_path: None,
        ..placed_at(dir)
    }
}

/// Places a path in the nearest counted repo, or nowhere. A path that no longer exists keeps
/// only the project of its nearest existing ancestor.
pub fn place(path: &Path, home: &Path) -> Placement {
    if !in_counted_area(path, home) {
        return Placement::default();
    }
    let removed = is_removed(path);
    let probe = if removed {
        match path.ancestors().find(|dir| !is_removed(dir)) {
            Some(ancestor) => ancestor,
            None => return Placement::default(),
        }
    } else {
        path
    };
    let Some(repo) = counted(probe, home) else {
        return Placement::default();
    };
    if removed {
        return Placement {
            project: Some(repo.location.project),
            missing_path: Some(path.to_path_buf()),
            ..Placement::default()
        };
    }
    Placement {
        project: Some(repo.location.project),
        worktree: repo.location.worktree,
        branch: repo.location.branch.map(BranchName::from),
        missing_path: None,
        root: Some(repo.root),
    }
}

fn is_removed(path: &Path) -> bool {
    matches!(std::fs::metadata(path), Err(err) if err.kind() == std::io::ErrorKind::NotFound)
}

fn counted(path: &Path, home: &Path) -> Option<Repo> {
    if !in_counted_area(path, home) {
        return None;
    }
    find_repo(path).filter(|repo| in_counted_area(&repo.root, home))
}

fn in_counted_area(dir: &Path, home: &Path) -> bool {
    let Ok(rest) = dir.strip_prefix(home) else {
        return false;
    };
    match rest.components().next() {
        Some(Component::Normal(first)) => !first.to_string_lossy().starts_with('.'),
        _ => false,
    }
}
