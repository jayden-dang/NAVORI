use std::fs;
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const REAP_TIMEOUT: Duration = Duration::from_secs(2);
// macOS O_NOFOLLOW. open fails when the last component is a symlink.
const O_NOFOLLOW: i32 = 0x0100;

#[derive(Debug)]
pub struct PerfHome {
    pub root: PathBuf,
}

pub fn current_uid() -> Result<u32> {
    let out = command_output("/usr/bin/id", &["-u"])?;
    ensure!(
        out.status.success(),
        "/usr/bin/id -u failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    let text = String::from_utf8(out.stdout).context("/usr/bin/id -u returned non-utf8")?;
    let text = text.trim();
    text.parse::<u32>()
        .with_context(|| format!("uid from /usr/bin/id -u: {text}"))
}

impl PerfHome {
    pub fn prepare(base: &Path, uid: u32) -> Result<Self> {
        let root = base.join(format!("navori-perf-{uid}"));
        match fs::symlink_metadata(&root) {
            Ok(meta) => replace_existing(&root, &meta, uid)?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(err).with_context(|| format!("inspect perf home {}", root.display()));
            }
        }
        create_dir_mode(&root, 0o700)?;
        Ok(Self { root })
    }

    pub fn template(&self) -> PathBuf {
        self.root.join("template")
    }

    pub fn fresh_run(&self, n: u32) -> Result<PathBuf> {
        let real_home = std::env::var_os("HOME").unwrap_or_default();
        self.fresh_run_linking(n, Path::new(&real_home))
    }

    fn fresh_run_linking(&self, n: u32, real_home: &Path) -> Result<PathBuf> {
        let dest = self.root.join(format!("run-{n}"));
        copy_dir(&self.template(), &dest)?;
        link_docker_socket(real_home, &dest)?;
        Ok(dest)
    }

    /// Whether the user's Docker socket was linked into this run directory.
    pub fn docker_linked(run: &Path) -> bool {
        fs::symlink_metadata(run.join(".docker/run/docker.sock")).is_ok()
    }

    pub fn record_pid(&self, pid: u32) -> Result<()> {
        let path = self.root.join("pids");
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("open {}", path.display()))?;
        writeln!(file, "{pid}").with_context(|| format!("record pid {pid}"))?;
        Ok(())
    }

    pub fn sweep_stale(&self, exe: &Path) -> Result<Vec<u32>> {
        let meta = match fs::symlink_metadata(&self.root) {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("inspect perf home {}", self.root.display()));
            }
        };
        if meta.file_type().is_symlink() {
            bail!(
                "refusing to sweep symlinked perf home {}",
                self.root.display()
            );
        }
        if !meta.is_dir() {
            bail!(
                "refusing to sweep non-directory perf home {}",
                self.root.display()
            );
        }
        let uid = current_uid()?;
        let owner = meta.uid();
        if owner != uid {
            bail!(
                "refusing to sweep perf home {} owned by {owner}, not {uid}",
                self.root.display()
            );
        }
        let path = self.root.join("pids");
        let Some(text) = read_pid_list(&path)? else {
            return Ok(Vec::new());
        };
        let mut killed = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let pid: u32 = line
                .parse()
                .with_context(|| format!("pid entry {line} in {}", path.display()))?;
            if process_comm(pid)?.as_deref() == Some(exe) {
                signal_kill(pid)?;
                killed.push(pid);
            }
        }
        Ok(killed)
    }
}

pub fn check_socket_len(home: &Path) -> Result<()> {
    let sock = home
        .join("Library")
        .join("Application Support")
        .join("Navori Dev")
        .join("run")
        .join("navori.sock");
    let bytes = sock.as_os_str().as_bytes().len();
    // sun_path holds 104 bytes including the trailing NUL.
    ensure!(
        bytes <= 103,
        "socket path {} is {bytes} bytes; the maximum is 103",
        sock.display()
    );
    Ok(())
}

// remove_dir_all unlinks a symlink; refuse it so the link and its target stay.
fn replace_existing(root: &Path, meta: &fs::Metadata, uid: u32) -> Result<()> {
    if meta.file_type().is_symlink() {
        bail!(
            "refusing to replace symlinked perf home {}; remove the link yourself if it should be replaced",
            root.display()
        );
    }
    if !meta.is_dir() {
        bail!(
            "refusing to replace non-directory perf home {}",
            root.display()
        );
    }
    let owner = meta.uid();
    if owner != uid {
        bail!(
            "refusing to replace perf home {} owned by {owner}, not {uid}",
            root.display()
        );
    }
    fs::remove_dir_all(root).with_context(|| format!("remove stale perf home {}", root.display()))
}

fn create_dir_mode(path: &Path, mode: u32) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.mode(mode);
    builder
        .create(path)
        .with_context(|| format!("create {}", path.display()))?;
    let mut perms = fs::symlink_metadata(path)
        .with_context(|| format!("stat {}", path.display()))?
        .permissions();
    perms.set_mode(mode);
    fs::set_permissions(path, perms)
        .with_context(|| format!("chmod {mode:o} {}", path.display()))?;
    let actual = fs::symlink_metadata(path)
        .with_context(|| format!("stat {}", path.display()))?
        .permissions()
        .mode()
        & 0o777;
    ensure!(
        actual == mode,
        "{} mode is {actual:o}, not {mode:o}",
        path.display()
    );
    Ok(())
}

fn link_docker_socket(real_home: &Path, run: &Path) -> Result<()> {
    let socket = real_home.join(".docker/run/docker.sock");
    if !socket.is_absolute() || fs::symlink_metadata(&socket).is_err() {
        return Ok(());
    }
    let dir = run.join(".docker/run");
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let link = dir.join("docker.sock");
    std::os::unix::fs::symlink(&socket, &link).with_context(|| format!("link {}", link.display()))
}

fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(src).with_context(|| format!("read {}", src.display()))?;
    ensure!(meta.is_dir(), "{} is not a directory", src.display());
    create_dir_mode(dest, meta.permissions().mode() & 0o777)?;
    for entry in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let entry = entry.with_context(|| format!("read {}", src.display()))?;
        copy_entry(&entry.path(), &dest.join(entry.file_name()))?;
    }
    Ok(())
}

fn copy_entry(src: &Path, dest: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(src).with_context(|| format!("read {}", src.display()))?;
    let file_type = meta.file_type();
    if file_type.is_symlink() {
        // Reproduce the link. Following it could write outside this run directory.
        let target = fs::read_link(src).with_context(|| format!("read link {}", src.display()))?;
        std::os::unix::fs::symlink(&target, dest)
            .with_context(|| format!("link {}", dest.display()))?;
        return Ok(());
    }
    if file_type.is_dir() {
        return copy_dir(src, dest);
    }
    if file_type.is_file() {
        fs::copy(src, dest)
            .with_context(|| format!("copy {} to {}", src.display(), dest.display()))?;
        return Ok(());
    }
    bail!("cannot copy {} into a perf run", src.display());
}

fn read_pid_list(path: &Path) -> Result<Option<String>> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).with_context(|| format!("inspect {}", path.display())),
    };
    if meta.file_type().is_symlink() {
        bail!("refusing to read symlinked pid list {}", path.display());
    }
    if !meta.is_file() {
        bail!("pid list {} is not a file", path.display());
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    let mut text = String::new();
    file.read_to_string(&mut text)
        .with_context(|| format!("read {}", path.display()))?;
    Ok(Some(text))
}

fn process_comm(pid: u32) -> Result<Option<PathBuf>> {
    let out = command_output("/bin/ps", &["-o", "comm=", "-p", &pid.to_string()])?;
    if !out.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8(out.stdout).context("ps comm is not utf-8")?;
    let text = text.trim();
    if text.is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(text)))
    }
}

fn signal_kill(pid: u32) -> Result<()> {
    let out = command_output("/bin/kill", &["-KILL", &pid.to_string()])?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stderr.contains("No such process") {
        return Ok(());
    }
    bail!("kill -KILL {pid} failed: {}", stderr.trim());
}

fn command_output(program: &str, args: &[&str]) -> Result<Output> {
    let rendered = format!("{program} {}", args.join(" "));
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("cannot run {rendered}"))?;
    let mut stdout = child
        .stdout
        .take()
        .with_context(|| format!("{rendered} has no stdout"))?;
    let mut stderr = child
        .stderr
        .take()
        .with_context(|| format!("{rendered} has no stderr"))?;
    let stdout_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).map(|_| buf)
    });
    let stderr_handle = thread::spawn(move || {
        let mut buf = Vec::new();
        stderr.read_to_end(&mut buf).map(|_| buf)
    });
    let status = wait_child(&mut child, &rendered);
    let stdout = join_reader(stdout_handle, "stdout");
    let stderr = join_reader(stderr_handle, "stderr");
    Ok(Output {
        status: status?,
        stdout: stdout?,
        stderr: stderr?,
    })
}

fn wait_child(child: &mut Child, rendered: &str) -> Result<ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("wait for {rendered}"))?
        {
            return Ok(status);
        }
        if started.elapsed() >= COMMAND_TIMEOUT {
            if let Err(err) = child.kill()
                && !err.to_string().contains("No such process")
                && !err.to_string().contains("can't kill an exited process")
            {
                eprintln!(
                    "{rendered} timed out and kill failed for pid {}: {err}",
                    child.id()
                );
            }
            let reap_started = Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if reap_started.elapsed() >= REAP_TIMEOUT => break,
                    Ok(None) => thread::sleep(Duration::from_millis(20)),
                    Err(err) => {
                        return Err(err).with_context(|| format!("reap {rendered}"));
                    }
                }
            }
            bail!("{rendered} timed out after 10s");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn join_reader(
    handle: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    which: &str,
) -> Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| anyhow::anyhow!("{which} reader panicked"))?
        .with_context(|| format!("read {which}"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{PerfHome, check_socket_len, current_uid, signal_kill};

    fn temp_base() -> tempfile::TempDir {
        tempfile::Builder::new().tempdir_in("/tmp").unwrap()
    }

    fn mode(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn creates_root_owned_by_user_with_mode_0700() {
        let base = temp_base();
        let uid = current_uid().unwrap();
        let home = PerfHome::prepare(base.path(), uid).unwrap();

        assert_eq!(home.root, base.path().join(format!("navori-perf-{uid}")));
        let meta = fs::symlink_metadata(&home.root).unwrap();
        assert!(meta.is_dir());
        assert_eq!(meta.uid(), uid);
        assert_eq!(mode(&home.root), 0o700);
    }

    #[test]
    fn replaces_a_stale_tree_it_owns() {
        let base = temp_base();
        let uid = current_uid().unwrap();
        let root = base.path().join(format!("navori-perf-{uid}"));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("stale"), b"old").unwrap();
        fs::create_dir(root.join("nested")).unwrap();
        fs::write(root.join("nested").join("file"), b"x").unwrap();

        let home = PerfHome::prepare(base.path(), uid).unwrap();

        assert_eq!(home.root, root);
        assert!(!home.root.join("stale").exists());
        assert!(!home.root.join("nested").exists());
        assert_eq!(mode(&home.root), 0o700);
        assert_eq!(fs::symlink_metadata(&home.root).unwrap().uid(), uid);
    }

    #[test]
    fn refuses_a_symlinked_root() {
        let base = temp_base();
        let uid = current_uid().unwrap();
        let target = base.path().join("real-home");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"secret").unwrap();
        let link = base.path().join(format!("navori-perf-{uid}"));
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let err = PerfHome::prepare(base.path(), uid).unwrap_err();

        assert_eq!(fs::read(target.join("keep")).unwrap(), b"secret");
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&link).unwrap(), target);
        assert!(format!("{err:#}").contains("symlink"), "{err:#}");
    }

    #[test]
    fn fresh_run_copies_template_contents() {
        let base = temp_base();
        let uid = current_uid().unwrap();
        let home = PerfHome::prepare(base.path(), uid).unwrap();
        let template = home.template();
        assert_eq!(template, home.root.join("template"));
        let nested = template
            .join("Library")
            .join("Application Support")
            .join("Navori Dev");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("marker"), b"warm").unwrap();
        fs::create_dir_all(template.join("sub").join("dir")).unwrap();
        fs::write(template.join("sub").join("dir").join("f"), b"nested").unwrap();
        fs::create_dir(template.join("empty")).unwrap();

        let run = home.fresh_run(3).unwrap();

        assert_eq!(run, home.root.join("run-3"));
        assert_eq!(
            fs::read(
                run.join("Library")
                    .join("Application Support")
                    .join("Navori Dev")
                    .join("marker")
            )
            .unwrap(),
            b"warm"
        );
        assert_eq!(
            fs::read(run.join("sub").join("dir").join("f")).unwrap(),
            b"nested"
        );
        assert!(run.join("empty").is_dir());
        fs::write(run.join("sub").join("dir").join("f"), b"changed").unwrap();
        assert_eq!(
            fs::read(template.join("sub").join("dir").join("f")).unwrap(),
            b"nested"
        );
        let again = home.fresh_run(4).unwrap();
        assert_eq!(
            fs::read(again.join("sub").join("dir").join("f")).unwrap(),
            b"nested"
        );
    }

    #[test]
    fn rejects_a_home_whose_socket_path_exceeds_103_bytes() {
        let base = temp_base();
        let ok_home = home_with_socket_len(base.path(), 103);
        let over_home = home_with_socket_len(base.path(), 104);
        assert_eq!(socket_string(&ok_home).len(), 103);
        assert_eq!(socket_string(&over_home).len(), 104);

        assert!(check_socket_len(&ok_home).is_ok());
        let err = check_socket_len(&over_home).unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains("104"), "{message}");
        assert!(message.contains("103"), "{message}");
        assert!(message.contains(&socket_string(&over_home)), "{message}");
    }

    #[test]
    fn sweep_kills_only_recorded_pids_running_the_given_exe() {
        let base = temp_base();
        let uid = current_uid().unwrap();
        let home = PerfHome::prepare(base.path(), uid).unwrap();
        let mut recorded = Proc::spawn("/bin/sleep", &["30"]);
        let mut unrecorded = Proc::spawn("/bin/sleep", &["30"]);
        let mut other_exe = Proc::spawn("/usr/bin/perl", &["-e", "sleep 30"]);
        home.record_pid(recorded.id()).unwrap();
        home.record_pid(other_exe.id()).unwrap();

        let killed = home.sweep_stale(Path::new("/bin/sleep")).unwrap();

        assert_eq!(killed, vec![recorded.id()]);
        assert!(
            wait_until_exited(&mut recorded),
            "recorded sleep still running"
        );
        assert!(still_alive(&mut unrecorded), "unrecorded sleep was killed");
        assert!(still_alive(&mut other_exe), "recorded perl was killed");
    }

    #[test]
    fn killing_an_already_exited_pid_is_not_a_sweep_failure() {
        let mut sleep = Proc::spawn("/bin/sleep", &["30"]);
        let pid = sleep.id();
        std::process::Command::new("/bin/kill")
            .args(["-KILL", &pid.to_string()])
            .output()
            .unwrap();
        assert!(wait_until_exited(&mut sleep), "sleep did not exit");
        signal_kill(pid).unwrap();
    }

    #[test]
    fn sweep_refuses_a_symlinked_root_without_killing() {
        let base = temp_base();
        let target = base.path().join("outside-home");
        fs::create_dir(&target).unwrap();
        let mut sleep = Proc::spawn("/bin/sleep", &["30"]);
        fs::write(target.join("pids"), format!("{}\n", sleep.id())).unwrap();
        let link = base.path().join("navori-perf-link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let home = PerfHome { root: link };

        let result = home.sweep_stale(Path::new("/bin/sleep"));

        assert!(
            stayed_alive(&mut sleep),
            "sweep killed a sleep listed through a symlinked home"
        );
        let err = result.unwrap_err();
        assert!(format!("{err:#}").contains("symlink"), "{err:#}");
    }

    #[test]
    fn sweep_refuses_a_symlinked_pid_list_without_killing() {
        let base = temp_base();
        let uid = current_uid().unwrap();
        let home = PerfHome::prepare(base.path(), uid).unwrap();
        let listed = base.path().join("listed-pids");
        let mut sleep = Proc::spawn("/bin/sleep", &["30"]);
        fs::write(&listed, format!("{}\n", sleep.id())).unwrap();
        std::os::unix::fs::symlink(&listed, home.root.join("pids")).unwrap();

        let result = home.sweep_stale(Path::new("/bin/sleep"));

        assert!(
            stayed_alive(&mut sleep),
            "sweep killed a sleep listed through a symlinked pid file"
        );
        let err = result.unwrap_err();
        assert!(format!("{err:#}").contains("symlink"), "{err:#}");
    }

    #[test]
    fn fresh_run_reproduces_a_symlink_without_copying_its_target() {
        let base = temp_base();
        let outside = temp_base();
        let uid = current_uid().unwrap();
        let home = PerfHome::prepare(base.path(), uid).unwrap();
        let secret = outside.path().join("secret");
        fs::write(&secret, b"outside").unwrap();
        let template = home.template();
        fs::create_dir(&template).unwrap();
        std::os::unix::fs::symlink(&secret, template.join("linked")).unwrap();

        let run = home.fresh_run(1).unwrap();
        let link = run.join("linked");

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&link).unwrap(), secret);
        assert_eq!(fs::read(&secret).unwrap(), b"outside");
        assert!(
            fs::read_dir(&run)
                .unwrap()
                .all(|entry| { entry.unwrap().file_name() != std::ffi::OsStr::new("secret") })
        );
    }

    #[test]
    fn a_run_copy_links_the_docker_socket_when_it_exists() {
        let base = temp_base();
        let real_home = temp_base();
        let socket = real_home.path().join(".docker/run/docker.sock");
        fs::create_dir_all(socket.parent().unwrap()).unwrap();
        fs::write(&socket, b"").unwrap();
        let home = PerfHome::prepare(base.path(), current_uid().unwrap()).unwrap();
        fs::create_dir(home.template()).unwrap();

        let run = home.fresh_run_linking(1, real_home.path()).unwrap();

        let link = run.join(".docker/run/docker.sock");
        assert_eq!(fs::read_link(&link).unwrap(), socket);
        assert!(!home.template().join(".docker").exists());
        assert!(PerfHome::docker_linked(&run));
    }

    #[test]
    fn a_run_copy_has_no_docker_link_when_there_is_none() {
        let base = temp_base();
        let real_home = temp_base();
        let home = PerfHome::prepare(base.path(), current_uid().unwrap()).unwrap();
        fs::create_dir(home.template()).unwrap();

        let run = home.fresh_run_linking(1, real_home.path()).unwrap();

        assert!(!run.join(".docker").exists());
        assert!(!PerfHome::docker_linked(&run));
    }

    const SOCKET_SUFFIX: &str = "Library/Application Support/Navori Dev/run/navori.sock";

    fn home_with_socket_len(prefix: &Path, total: usize) -> PathBuf {
        let overhead = prefix.as_os_str().len() + 2 + SOCKET_SUFFIX.len();
        assert!(
            total > overhead,
            "temp path leaves no room for a {total}-byte socket"
        );
        prefix.join("n".repeat(total - overhead))
    }

    fn socket_string(home: &Path) -> String {
        format!("{}/{}", home.display(), SOCKET_SUFFIX)
    }

    struct Proc(Child);

    impl Proc {
        fn spawn(program: &str, args: &[&str]) -> Self {
            let child = Command::new(program)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            Self(child)
        }

        fn id(&self) -> u32 {
            self.0.id()
        }
    }

    impl Drop for Proc {
        fn drop(&mut self) {
            if matches!(self.0.try_wait(), Ok(None)) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }

    fn still_alive(proc: &mut Proc) -> bool {
        matches!(proc.0.try_wait(), Ok(None))
    }

    fn stayed_alive(proc: &mut Proc) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(200) {
            match proc.0.try_wait() {
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                _ => return false,
            }
        }
        true
    }

    fn wait_until_exited(proc: &mut Proc) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            match proc.0.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(_) => return false,
            }
        }
        false
    }
}
