use std::io::Write as _;
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::IntegrationsEnv;
use crate::action::Harness;
use crate::domain::{self, Changed};
use crate::event;
use crate::state::State;

/// The file a write lands on: a link is followed so the link itself stays.
#[derive(Debug)]
pub(crate) struct Target {
    pub(crate) path: PathBuf,
    pub(crate) mode: u32,
}

#[derive(Debug)]
pub(crate) enum Read {
    NoDir,
    NoFile,
    Doc(Value, Target),
    Unreadable(String),
}

pub(crate) fn dir_name(harness: Harness) -> &'static str {
    match harness {
        Harness::Claude => ".claude",
        Harness::Codex => ".codex",
        Harness::Grok => ".grok",
    }
}

fn config_dir(home: &Path, harness: Harness) -> PathBuf {
    home.join(dir_name(harness))
}

/// Why Navori's hooks cannot be written for `harness` yet, when that is so.
pub(crate) fn not_installable_yet(harness: Harness) -> Option<String> {
    (harness == Harness::Codex).then(event::codex_unverified)
}

pub(crate) fn hook_file(home: &Path, harness: Harness) -> PathBuf {
    let dir = config_dir(home, harness);
    match harness {
        Harness::Claude => dir.join("settings.json"),
        Harness::Codex => dir.join("hooks.json"),
        Harness::Grok => dir.join("hooks").join("navori.json"),
    }
}

pub(crate) fn read(env: &IntegrationsEnv, harness: Harness) -> Read {
    if !config_dir(&env.home, harness).is_dir() {
        return Read::NoDir;
    }
    let path = hook_file(&env.home, harness);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Read::NoFile,
        Err(e) => return Read::Unreadable(format!("cannot look at the file ({e})")),
    };
    let resolved = if meta.file_type().is_symlink() {
        match std::fs::canonicalize(&path) {
            Ok(resolved) => resolved,
            Err(_) => return Read::Unreadable("the file is a link that leads nowhere".to_owned()),
        }
    } else {
        path
    };
    let mode = match std::fs::metadata(&resolved) {
        Ok(meta) => meta.permissions().mode() & 0o7777,
        Err(e) => return Read::Unreadable(format!("cannot look at the file ({e})")),
    };
    let bytes = match std::fs::read(&resolved) {
        Ok(bytes) => bytes,
        Err(e) => return Read::Unreadable(format!("cannot read the file ({e})")),
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(doc) => Read::Doc(
            doc,
            Target {
                path: resolved,
                mode,
            },
        ),
        Err(e) => Read::Unreadable(format!("it is not valid JSON ({e})")),
    }
}

const NEW_FILE_MODE: u32 = 0o600;

/// What `apply` is asked to do to a Hook file.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Plan {
    Install,
    Repair,
    Remove,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Nothing,
    Written,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ApplyError {
    #[error("{0}")]
    Unavailable(String),
    #[error("Navori does not know where its helper program is, so it cannot write a hook")]
    NoHelper,
    #[error("{} does not exist, so {} is not set up; Navori does not create it", .dir.display(), .harness)]
    NoConfigDir { dir: PathBuf, harness: &'static str },
    #[error("{} cannot be changed: {reason}", .path.display())]
    Unreadable { path: PathBuf, reason: String },
    #[error("cannot {step} {}: {source}", .path.display())]
    Io {
        step: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
}

fn io(step: &'static str, path: &Path) -> impl FnOnce(std::io::Error) -> ApplyError {
    let path = path.to_path_buf();
    move |source| ApplyError::Io { step, path, source }
}

/// Changes a Hook file the way `plan` says. Any failure before the final rename leaves the
/// original file exactly as it was.
pub(crate) fn apply(
    env: &IntegrationsEnv,
    harness: Harness,
    plan: Plan,
) -> Result<Outcome, ApplyError> {
    if let Some(why) = not_installable_yet(harness) {
        return Err(ApplyError::Unavailable(format!(
            "Hooks for {} cannot be installed yet. {why}",
            event::name(harness)
        )));
    }
    if env.helper.as_os_str().is_empty() {
        return Err(ApplyError::NoHelper);
    }
    let file = hook_file(&env.home, harness);
    let (mut doc, target, existed) = match read(env, harness) {
        Read::NoDir => {
            return Err(ApplyError::NoConfigDir {
                dir: config_dir(&env.home, harness),
                harness: event::name(harness),
            });
        }
        Read::Unreadable(reason) => return Err(ApplyError::Unreadable { path: file, reason }),
        Read::NoFile => (
            json!({}),
            Target {
                path: file.clone(),
                mode: NEW_FILE_MODE,
            },
            false,
        ),
        Read::Doc(doc, target) => (doc, target, true),
    };

    let verdict = domain::classify(&doc, harness, env.dev, &env.helper);
    if verdict.state == State::Unreadable {
        return Err(ApplyError::Unreadable {
            path: target.path,
            reason: verdict.reason.unwrap_or_default(),
        });
    }
    let removing = matches!(plan, Plan::Remove);
    let changed = if removing {
        domain::strip(&mut doc, env.dev)
    } else {
        domain::converge(&mut doc, harness, env.dev, &env.helper)
    };
    if changed == Changed::No && (existed || removing) {
        return Ok(Outcome::Nothing);
    }
    let delete = removing
        && harness == Harness::Grok
        && doc == json!({})
        && !std::fs::symlink_metadata(&file).is_ok_and(|meta| meta.file_type().is_symlink());

    let dir = target
        .path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    if !dir.is_dir() {
        std::fs::create_dir(&dir).map_err(io("create the directory", &dir))?;
    }
    if existed {
        copy_beside(&target.path, target.mode, unix_seconds())?;
    }
    if delete {
        std::fs::remove_file(&file).map_err(io("delete", &file))?;
    } else {
        write_atomically(&target, &dir, &doc, existed)?;
    }
    Ok(Outcome::Written)
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Copies `path` to `<name>.navori-bak-<seconds>` in the same directory, adding `-1`, `-2` on a
/// collision, and never replacing a file that is already there.
fn copy_beside(path: &Path, mode: u32, seconds: u64) -> Result<PathBuf, ApplyError> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let base = format!("{name}.navori-bak-{seconds}");
    let mut source = std::fs::File::open(path).map_err(io("read", path))?;
    let mut attempt = 0u32;
    loop {
        let candidate = if attempt == 0 {
            path.with_file_name(&base)
        } else {
            path.with_file_name(format!("{base}-{attempt}"))
        };
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&candidate)
        {
            Ok(mut copy) => {
                let done = std::io::copy(&mut source, &mut copy)
                    .and_then(|_| copy.sync_all())
                    .map_err(io("write the backup", &candidate));
                if done.is_err() {
                    let _ = std::fs::remove_file(&candidate);
                }
                done?;
                return Ok(candidate);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => attempt += 1,
            Err(e) => return Err(io("create the backup", &candidate)(e)),
        }
    }
}

fn write_atomically(
    target: &Target,
    dir: &Path,
    doc: &Value,
    replace: bool,
) -> Result<(), ApplyError> {
    let mut text = serde_json::to_vec_pretty(doc).map_err(|e| ApplyError::Io {
        step: "serialize",
        path: target.path.clone(),
        source: e.into(),
    })?;
    text.push(b'\n');
    let mut temp =
        tempfile::NamedTempFile::new_in(dir).map_err(io("create a temporary file in", dir))?;
    temp.write_all(&text).map_err(io("write", temp.path()))?;
    temp.as_file()
        .set_permissions(std::fs::Permissions::from_mode(target.mode))
        .map_err(io("set the mode of", temp.path()))?;
    temp.as_file().sync_all().map_err(io("sync", temp.path()))?;
    // A file that was absent when read and has appeared since has no Backup, so it is never replaced.
    let persisted = if replace {
        temp.persist(&target.path)
    } else {
        temp.persist_noclobber(&target.path)
    };
    persisted.map_err(|e| io("replace", &target.path)(e.error))?;
    Ok(())
}

// These two have no public seam: the clock and a file appearing mid-apply cannot be driven from outside.
#[cfg(test)]
mod tests {
    use super::{Target, copy_beside, write_atomically};

    #[test]
    fn a_backup_name_taken_in_the_same_second_gets_a_numeric_suffix() {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, "original").unwrap();

        let first = copy_beside(&file, 0o600, 7).unwrap();
        let second = copy_beside(&file, 0o600, 7).unwrap();
        let third = copy_beside(&file, 0o600, 7).unwrap();

        let names: Vec<_> = [first, second, third]
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "settings.json.navori-bak-7",
                "settings.json.navori-bak-7-1",
                "settings.json.navori-bak-7-2"
            ]
        );
        for name in names {
            assert_eq!(
                std::fs::read_to_string(dir.path().join(name)).unwrap(),
                "original"
            );
        }
    }

    #[test]
    fn a_file_that_appeared_after_the_read_is_not_replaced_without_a_backup() {
        let dir = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let file = dir.path().join("navori.json");
        std::fs::write(&file, "someone else's").unwrap();
        let target = Target {
            path: file.clone(),
            mode: 0o600,
        };

        let result = write_atomically(&target, dir.path(), &serde_json::json!({}), false);

        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "someone else's");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
