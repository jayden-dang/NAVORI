#![allow(clippy::unwrap_used)]

mod common;

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use common::Rig;
use serde_json::{Value, json};
use tempfile::TempDir;

/// A rig whose helper lives at `<apps>/Navori.app/Contents/Helpers/navori`, with the Claude hooks
/// installed; `command` is what Navori wrote for the Stop event.
struct Installed {
    _rig: Rig,
    apps: TempDir,
    command: String,
}

impl Installed {
    async fn new() -> Self {
        Self::with_profile(false).await
    }

    async fn dev() -> Self {
        Self::with_profile(true).await
    }

    async fn with_profile(dev: bool) -> Self {
        let apps = tempfile::Builder::new().tempdir_in("/tmp").unwrap();
        let helper = apps.path().join("Navori.app/Contents/Helpers/navori");
        let rig = Rig::new(dev, helper.to_str().unwrap());
        rig.make_dir(".claude");
        rig.dispatch(json!({ "install": { "harness": "claude" } }))
            .await
            .unwrap();
        let doc: Value =
            serde_json::from_slice(&std::fs::read(rig.path(".claude/settings.json")).unwrap())
                .unwrap();
        let command = doc["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .to_owned();
        Self {
            _rig: rig,
            apps,
            command,
        }
    }

    fn helper(&self) -> PathBuf {
        self.apps.path().join("Navori.app/Contents/Helpers/navori")
    }

    fn place_helper(&self, script: &str, mode: u32) {
        let helper = self.helper();
        std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
        // Write under another name and rename into place, so no writable descriptor to the
        // script is still open when the shell execs it (ETXTBSY).
        let staging = helper.with_file_name("navori.staging");
        std::fs::write(&staging, script).unwrap();
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(mode)).unwrap();
        std::fs::rename(&staging, &helper).unwrap();
    }

    /// Runs the command through `/bin/sh -c`; returns its output and the shell's pid.
    fn run(&self, stdin: &[u8]) -> (Output, u32) {
        let mut child = Command::new("/bin/sh")
            .args(["-c", &self.command])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        child.stdin.take().unwrap().write_all(stdin).unwrap();
        (child.wait_with_output().unwrap(), pid)
    }

    fn record(&self, name: &str) -> PathBuf {
        self.apps.path().join(name)
    }

    fn place_recording_helper(&self) -> (PathBuf, PathBuf, PathBuf) {
        let (args, stdin, pid) = (
            self.record("args"),
            self.record("stdin"),
            self.record("pid"),
        );
        self.place_helper(&recording_helper(&args, &stdin, &pid), 0o755);
        (args, stdin, pid)
    }
}

fn recording_helper(args: &Path, stdin: &Path, pid: &Path) -> String {
    format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncat > '{}'\necho $$ > '{}'\n",
        args.display(),
        stdin.display(),
        pid.display()
    )
}

fn assert_silent_success(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[tokio::test]
async fn a_missing_helper_exits_zero_with_no_output() {
    let installed = Installed::new().await;
    let (output, _) = installed.run(b"{}");
    assert_silent_success(&output);
}

#[tokio::test]
async fn a_directory_in_place_of_the_helper_exits_zero_with_no_output() {
    let installed = Installed::new().await;
    std::fs::create_dir_all(installed.helper()).unwrap();
    let (output, _) = installed.run(b"{}");
    assert_silent_success(&output);
}

#[tokio::test]
async fn a_helper_without_the_execute_bit_exits_zero_with_no_output() {
    let installed = Installed::new().await;
    installed.place_helper("#!/bin/sh\necho ran\n", 0o644);
    let (output, _) = installed.run(b"{}");
    assert_silent_success(&output);
}

#[tokio::test]
async fn a_present_helper_gets_the_same_arguments_and_stdin() {
    let installed = Installed::new().await;
    let (args, stdin, _) = installed.place_recording_helper();

    let (output, _) = installed.run(b"{\"session\":\"s1\"}\n\x00\xff");

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(std::fs::read_to_string(&args).unwrap(), "hook\n");
    assert_eq!(
        std::fs::read(&stdin).unwrap(),
        b"{\"session\":\"s1\"}\n\x00\xff"
    );
}

#[tokio::test]
async fn the_dev_command_passes_the_profile_and_the_same_stdin() {
    let installed = Installed::dev().await;
    let (args, stdin, _) = installed.place_recording_helper();

    let (output, _) = installed.run(b"{\"session\":\"s1\"}\n");

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        std::fs::read_to_string(&args).unwrap(),
        "--profile\ndev\nhook\n"
    );
    assert_eq!(std::fs::read(&stdin).unwrap(), b"{\"session\":\"s1\"}\n");
}

#[tokio::test]
async fn the_helper_runs_as_the_shell_process_itself() {
    let installed = Installed::new().await;
    let (_, _, pid) = installed.place_recording_helper();

    let (_, shell_pid) = installed.run(b"{}");

    let helper_pid: u32 = std::fs::read_to_string(&pid)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(helper_pid, shell_pid);
}

#[tokio::test]
async fn the_command_finishes_inside_the_shortest_hook_timeout() {
    let installed = Installed::new().await;
    installed.place_recording_helper();

    let started = Instant::now();
    let (output, _) = installed.run(b"{}");

    let elapsed = started.elapsed();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        elapsed < Duration::from_secs(1),
        "the command took {elapsed:?}; {output:?}"
    );
}
