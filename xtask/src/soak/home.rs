use std::fs;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::perf::home::{PerfHome, check_socket_len, current_uid};

#[derive(Debug)]
pub(crate) struct SoakHome {
    perf: PerfHome,
}

impl SoakHome {
    #[cfg_attr(test, expect(dead_code))]
    pub(crate) fn prepare(exes: &[&Path]) -> Result<Self> {
        let uid = current_uid()?;
        Self::prepare_in(
            &Path::new("/tmp").join(format!("navori-soak-{uid}")),
            uid,
            exes,
        )
    }

    pub(crate) fn prepare_in(base: &Path, uid: u32, exes: &[&Path]) -> Result<Self> {
        let root = base.join(format!("navori-perf-{uid}"));
        check_socket_len(&root.join("run-1"))?;
        ensure_private_base(base, uid)?;
        let previous = PerfHome { root };
        for exe in exes {
            previous.sweep_stale(exe)?;
        }
        let perf = PerfHome::prepare(base, uid)?;
        Ok(Self { perf })
    }

    pub(crate) fn template(&self) -> PathBuf {
        self.perf.template()
    }

    pub(crate) fn fresh_run(&self) -> Result<PathBuf> {
        let run = self.perf.fresh_run(1)?;
        drop_docker_link(&run)?;
        Ok(run)
    }

    pub(crate) fn record_pid(&self, pid: u32) -> Result<()> {
        self.perf.record_pid(pid)
    }
}

pub(crate) fn drop_docker_link(run: &Path) -> Result<bool> {
    let link = run.join(".docker/run/docker.sock");
    match fs::symlink_metadata(&link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            fs::remove_file(&link).with_context(|| format!("remove {}", link.display()))?;
            Ok(true)
        }
        Ok(_) => Ok(false),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err).with_context(|| format!("inspect {}", link.display())),
    }
}

fn ensure_private_base(base: &Path, uid: u32) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700);
    match builder.create(base) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(err) => {
            return Err(err).with_context(|| format!("create soak base {}", base.display()));
        }
    }
    let meta = fs::symlink_metadata(base)
        .with_context(|| format!("inspect soak base {}", base.display()))?;
    if meta.file_type().is_symlink() {
        bail!(
            "soak base {} is a symlink; remove it so the soak can create its own private directory",
            base.display()
        );
    }
    if !meta.is_dir() {
        bail!(
            "soak base {} is not a directory; remove it so the soak can create its own",
            base.display()
        );
    }
    let owner = meta.uid();
    if owner != uid {
        bail!(
            "soak base {} is owned by {owner}, not {uid}; remove it or use another user",
            base.display()
        );
    }
    let actual = meta.permissions().mode() & 0o777;
    if actual != 0o700 {
        bail!(
            "soak base {} has mode {actual:o}, not 700; run chmod 700 on it or remove it",
            base.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{SoakHome, drop_docker_link};
    use crate::perf::home::current_uid;

    fn temp_dir() -> tempfile::TempDir {
        tempfile::Builder::new().tempdir_in("/tmp").unwrap()
    }

    fn mode(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_new_soak_base_is_created_private_to_the_user() {
        let tmp = temp_dir();
        let uid = current_uid().unwrap();
        let base = tmp.path().join("soak");

        SoakHome::prepare_in(&base, uid, &[]).unwrap();

        let meta = fs::symlink_metadata(&base).unwrap();
        assert!(meta.is_dir());
        assert_eq!(meta.uid(), uid);
        assert_eq!(mode(&base), 0o700);
    }

    #[test]
    fn a_soak_base_that_is_a_symlink_is_refused() {
        let tmp = temp_dir();
        let uid = current_uid().unwrap();
        let target = tmp.path().join("elsewhere");
        fs::create_dir(&target).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
        let base = tmp.path().join("soak");
        std::os::unix::fs::symlink(&target, &base).unwrap();

        let err = SoakHome::prepare_in(&base, uid, &[]).unwrap_err();

        assert!(format!("{err:#}").contains(&base.display().to_string()));
        assert!(fs::read_dir(&target).unwrap().next().is_none());
    }

    #[test]
    fn a_soak_base_with_a_wider_mode_is_refused() {
        let tmp = temp_dir();
        let uid = current_uid().unwrap();
        let base = tmp.path().join("soak");
        fs::create_dir(&base).unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o755)).unwrap();

        let err = SoakHome::prepare_in(&base, uid, &[]).unwrap_err();

        let message = format!("{err:#}");
        assert!(message.contains(&base.display().to_string()), "{message}");
        assert!(message.contains("755"), "{message}");
    }

    #[test]
    fn an_existing_private_soak_base_is_reused() {
        let tmp = temp_dir();
        let uid = current_uid().unwrap();
        let base = tmp.path().join("soak");
        fs::create_dir(&base).unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(base.join("keep"), b"x").unwrap();

        SoakHome::prepare_in(&base, uid, &[]).unwrap();

        assert_eq!(fs::read(base.join("keep")).unwrap(), b"x");
    }

    #[test]
    fn the_run_home_is_nested_under_the_soak_base() {
        let tmp = temp_dir();
        let uid = current_uid().unwrap();
        let base = tmp.path().join("soak");
        let home = SoakHome::prepare_in(&base, uid, &[]).unwrap();
        fs::create_dir(home.template()).unwrap();

        let run = home.fresh_run().unwrap();

        assert_eq!(run, base.join(format!("navori-perf-{uid}")).join("run-1"));
    }

    #[test]
    fn a_docker_link_in_a_run_directory_is_removed() {
        let tmp = temp_dir();
        let target = tmp.path().join("docker-target");
        fs::write(&target, b"sock").unwrap();
        let run = tmp.path().join("run-1");
        let link = run.join(".docker/run/docker.sock");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let removed = drop_docker_link(&run).unwrap();

        assert!(removed);
        assert_eq!(
            fs::symlink_metadata(&link).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
        assert_eq!(fs::read(&target).unwrap(), b"sock");
    }

    #[test]
    fn a_run_directory_without_a_docker_link_is_left_alone() {
        let tmp = temp_dir();

        assert!(!drop_docker_link(tmp.path()).unwrap());
    }

    #[test]
    fn a_fresh_run_directory_has_no_docker_link() {
        let tmp = temp_dir();
        let uid = current_uid().unwrap();
        let home = SoakHome::prepare_in(&tmp.path().join("soak"), uid, &[]).unwrap();
        fs::create_dir(home.template()).unwrap();

        let run = home.fresh_run().unwrap();

        assert_eq!(
            fs::symlink_metadata(run.join(".docker/run/docker.sock"))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }

    #[test]
    fn a_stale_app_recorded_by_an_earlier_soak_is_stopped_before_the_home_is_replaced() {
        let tmp = temp_dir();
        let uid = current_uid().unwrap();
        let base = tmp.path().join("soak");
        let first = SoakHome::prepare_in(&base, uid, &[]).unwrap();
        let mut sleeper = Command::new("/bin/sleep")
            .arg("60")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        first.record_pid(sleeper.id()).unwrap();

        SoakHome::prepare_in(&base, uid, &[Path::new("/bin/sleep")]).unwrap();

        let gone = exited_within(&mut sleeper, Duration::from_secs(2));
        if !gone {
            let _ = sleeper.kill();
            let _ = sleeper.wait();
        }
        assert!(gone, "recorded sleep still running");
    }

    #[test]
    fn a_base_that_would_make_the_socket_path_too_long_is_refused() {
        let tmp = temp_dir();
        let uid = current_uid().unwrap();
        let base = tmp.path().join("n".repeat(100));

        let err = SoakHome::prepare_in(&base, uid, &[]).unwrap_err();

        let message = format!("{err:#}");
        assert!(message.contains("103"), "{message}");
        assert!(message.contains("navori.sock"), "{message}");
        assert!(!base.exists(), "base created before the length check");
    }

    fn exited_within(child: &mut Child, limit: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < limit {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }
}
