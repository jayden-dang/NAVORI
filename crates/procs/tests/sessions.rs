//! Agent sessions recognized from an in-memory process list.
//! Working directories that are read point at real temporary git trees.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};

use navori_macos::fake::{FakeProcess, FakeProcesses};
use navori_procs::{
    AgentSession, Branch, Harness, Location, Snapshot, TerminalHost, Unavailable, sessions,
};

struct Shape {
    pid: i32,
    exe: &'static str,
    argv: Option<&'static [&'static str]>,
    expected: Option<Harness>,
}

fn source() -> Arc<FakeProcesses> {
    Arc::new(FakeProcesses::new(501))
}

fn take(fake: &Arc<FakeProcesses>) -> Result<Snapshot, String> {
    let owned = Arc::clone(fake);
    Snapshot::take(owned).map_err(|err| err.to_string())
}

fn shapes() -> [Shape; 6] {
    [
        Shape {
            pid: 10,
            exe: "/Users/u/.local/share/claude/versions/2.1.316",
            argv: None,
            expected: Some(Harness::ClaudeCode),
        },
        Shape {
            pid: 11,
            exe: "/opt/homebrew/Caskroom/codex/0.166.0/bin/codex",
            argv: None,
            expected: Some(Harness::Codex),
        },
        Shape {
            pid: 12,
            exe: "/Users/u/.grok/downloads/grok-1.0.70-macos-aarch64",
            argv: None,
            expected: Some(Harness::Grok),
        },
        Shape {
            pid: 13,
            exe: "/opt/homebrew/Cellar/node/24.1.0/bin/node",
            argv: Some(&[
                "node",
                "/opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js",
            ]),
            expected: Some(Harness::ClaudeCode),
        },
        Shape {
            pid: 14,
            exe: "/Users/u/.bun/bin/bun",
            argv: Some(&["bun", "/Users/u/node_modules/@openai/codex/bin/codex.js"]),
            expected: Some(Harness::Codex),
        },
        Shape {
            pid: 15,
            exe: "/opt/homebrew/Cellar/node/24.1.0/bin/node",
            argv: Some(&["node", "/srv/app.js"]),
            expected: None,
        },
    ]
}

fn add_shape(fake: &FakeProcesses, shape: &Shape) {
    let mut process = FakeProcess::new(
        shape.pid,
        1,
        501,
        u64::from(shape.pid as u32) * 1_000,
        shape.exe,
    );
    if let Some(argv) = shape.argv {
        process = process.argv(argv);
    }
    fake.add(process);
}

fn snapshot_of(rows: &[Shape]) -> Result<(Arc<FakeProcesses>, Snapshot), String> {
    let fake = source();
    for row in rows {
        add_shape(&fake, row);
    }
    let snapshot = take(&fake)?;
    Ok((fake, snapshot))
}

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
    let out = cmd.output().map_err(|err| err.to_string())?;
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
    fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    git(dir, &["init", "-b", "main"])?;
    git(dir, &["commit", "--allow-empty", "-m", "init"])?;
    Ok(())
}

#[test]
fn an_executable_named_grok_is_a_grok_session() -> Result<(), String> {
    let rows = [Shape {
        pid: 16,
        exe: "/usr/local/bin/grok",
        argv: None,
        expected: Some(Harness::Grok),
    }];
    let (_fake, snapshot) = snapshot_of(&rows)?;
    let found = sessions(&snapshot);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].harness, Harness::Grok);
    assert_eq!(found[0].process.pid, 16);
    Ok(())
}

#[test]
fn each_harness_shape_is_recognized() -> Result<(), String> {
    let rows = shapes();
    let (_fake, snapshot) = snapshot_of(&rows)?;
    let found = sessions(&snapshot);

    let expected_count = rows.iter().filter(|row| row.expected.is_some()).count();
    assert_eq!(found.len(), expected_count);

    for row in &rows {
        let matches: Vec<_> = found
            .iter()
            .filter(|session| session.process.pid == row.pid)
            .collect();
        match row.expected {
            Some(harness) => {
                assert_eq!(matches.len(), 1, "pid {} ({})", row.pid, row.exe);
                assert_eq!(matches[0].harness, harness, "pid {}", row.pid);
            }
            None => assert!(
                matches.is_empty(),
                "pid {} ({}) is not a harness",
                row.pid,
                row.exe
            ),
        }
    }
    Ok(())
}

#[test]
fn argv_is_read_only_for_node_and_bun_processes() -> Result<(), String> {
    let rows = shapes();
    let (fake, snapshot) = snapshot_of(&rows)?;
    let _found = sessions(&snapshot);

    let argv_pids: Vec<i32> = fake
        .reads()
        .into_iter()
        .filter(|(_, kind)| *kind == "argv")
        .map(|(pid, _)| pid)
        .collect();

    for row in &rows {
        let is_runtime = row
            .exe
            .rsplit('/')
            .next()
            .is_some_and(|base| base == "node" || base == "bun");
        assert_eq!(
            argv_pids.contains(&row.pid),
            is_runtime,
            "argv read for pid {} ({})",
            row.pid,
            row.exe
        );
    }
    Ok(())
}

#[test]
fn a_harness_started_by_the_same_harness_is_not_a_second_session() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(
        100,
        1,
        501,
        100_000,
        "/Users/u/.local/share/claude/versions/2.1.316",
    ));
    fake.add(FakeProcess::new(101, 100, 501, 101_000, "/bin/zsh"));
    fake.add(FakeProcess::new(
        102,
        101,
        501,
        102_000,
        "/opt/homebrew/Caskroom/codex/0.166.0/bin/codex",
    ));
    fake.add(FakeProcess::new(
        103,
        102,
        501,
        103_000,
        "/Users/u/.local/bin/claude",
    ));

    let snapshot = take(&fake)?;
    let found = sessions(&snapshot);
    let mut pids: Vec<i32> = found.iter().map(|session| session.process.pid).collect();
    pids.sort_unstable();

    assert_eq!(pids, vec![100, 102]);
    assert_eq!(session_harness(&found, 100)?, Harness::ClaudeCode);
    assert_eq!(session_harness(&found, 102)?, Harness::Codex);
    Ok(())
}

fn session_harness(found: &[AgentSession], pid: i32) -> Result<Harness, String> {
    found
        .iter()
        .find(|session| session.process.pid == pid)
        .map(|session| session.harness)
        .ok_or_else(|| format!("pid {pid} is not a session"))
}

#[test]
fn a_session_reports_the_location_of_its_working_directory() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(|err| err.to_string())?;
    let repo = root.path().join("navori");
    init_repo(&repo)?;
    let src = repo.join("src");
    fs::create_dir(&src).map_err(|err| err.to_string())?;

    let fake = source();
    fake.add(
        FakeProcess::new(
            20,
            1,
            501,
            20_000,
            "/Users/u/.local/share/claude/versions/2.1.316",
        )
        .cwd(src.as_path()),
    );

    let snapshot = take(&fake)?;
    let found = sessions(&snapshot);

    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].location,
        Ok(Location {
            project: "navori".to_owned(),
            worktree: None,
            branch: Some(Branch::Named("main".to_owned())),
        })
    );
    Ok(())
}

#[test]
fn a_session_whose_working_directory_cannot_be_read_has_an_unavailable_location()
-> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(
        21,
        1,
        501,
        21_000,
        "/opt/homebrew/Caskroom/codex/0.166.0/bin/codex",
    ));

    let snapshot = take(&fake)?;
    let found = sessions(&snapshot);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].harness, Harness::Codex);
    assert_eq!(found[0].process.pid, 21);
    assert_eq!(found[0].location, Err(Unavailable::PermissionDenied));
    Ok(())
}

struct HostRow {
    ancestors: &'static [&'static str],
    expected: Option<TerminalHost>,
}

#[test]
fn the_nearest_terminal_app_is_the_terminal_host() -> Result<(), String> {
    let rows = [
        HostRow {
            ancestors: &["/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"],
            expected: Some(TerminalHost::Terminal),
        },
        HostRow {
            ancestors: &["/Applications/iTerm.app/Contents/MacOS/iTerm2"],
            expected: Some(TerminalHost::ITerm2),
        },
        HostRow {
            ancestors: &["/Applications/Ghostty.app/Contents/MacOS/ghostty"],
            expected: Some(TerminalHost::Ghostty),
        },
        HostRow {
            ancestors: &["/Applications/WezTerm.app/Contents/MacOS/wezterm-gui"],
            expected: Some(TerminalHost::WezTerm),
        },
        HostRow {
            ancestors: &["/Applications/kitty.app/Contents/MacOS/kitty"],
            expected: Some(TerminalHost::Kitty),
        },
        HostRow {
            ancestors: &["/Applications/Alacritty.app/Contents/MacOS/alacritty"],
            expected: Some(TerminalHost::Alacritty),
        },
        HostRow {
            ancestors: &["/Applications/Warp.app/Contents/MacOS/stable"],
            expected: Some(TerminalHost::Warp),
        },
        HostRow {
            ancestors: &[
                "/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper (Plugin).app/Contents/MacOS/Code Helper (Plugin)",
            ],
            expected: Some(TerminalHost::VsCode),
        },
        HostRow {
            ancestors: &["/Applications/Cursor.app/Contents/MacOS/Cursor"],
            expected: Some(TerminalHost::Cursor),
        },
        HostRow {
            ancestors: &["/Applications/Zed.app/Contents/MacOS/zed"],
            expected: Some(TerminalHost::Zed),
        },
        HostRow {
            ancestors: &["/Applications/Cloudflare WARP.app/Contents/MacOS/Cloudflare WARP"],
            expected: None,
        },
        HostRow {
            ancestors: &[
                "/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal",
                "/Applications/iTerm.app/Contents/MacOS/iTerm2",
            ],
            expected: Some(TerminalHost::ITerm2),
        },
    ];

    for row in &rows {
        let fake = source();
        let mut parent = 1;
        let mut pid = 30;
        for exe in row.ancestors {
            fake.add(FakeProcess::new(
                pid,
                parent,
                501,
                u64::from(pid as u32) * 1_000,
                exe,
            ));
            parent = pid;
            pid += 1;
        }
        fake.add(FakeProcess::new(
            pid,
            parent,
            501,
            u64::from(pid as u32) * 1_000,
            "/Users/u/.grok/downloads/grok-1.0.70-macos-aarch64",
        ));

        let snapshot = take(&fake)?;
        let found = sessions(&snapshot);
        assert_eq!(found.len(), 1, "{:?}", row.ancestors);
        assert_eq!(found[0].process.pid, pid, "{:?}", row.ancestors);
        assert_eq!(found[0].terminal, row.expected, "{:?}", row.ancestors);
    }
    Ok(())
}

#[test]
fn a_session_under_tmux_has_no_terminal_host() -> Result<(), String> {
    let fake = source();
    fake.add(FakeProcess::new(
        2627,
        1,
        501,
        2_627_000,
        "/opt/homebrew/bin/tmux",
    ));
    fake.add(FakeProcess::new(2628, 2627, 501, 2_628_000, "/bin/zsh").argv(&["-zsh"]));
    fake.add(FakeProcess::new(
        2629,
        2628,
        501,
        2_629_000,
        "/Users/u/.local/share/claude/versions/2.1.316",
    ));

    let snapshot = take(&fake)?;
    let found = sessions(&snapshot);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].process.pid, 2629);
    assert_eq!(found[0].harness, Harness::ClaudeCode);
    assert_eq!(found[0].terminal, None);
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

#[test]
fn a_node_process_whose_arguments_cannot_be_read_is_not_a_session() -> Result<(), String> {
    let log = capture()?;
    let fake = source();
    fake.add(FakeProcess::new(
        4242,
        1,
        501,
        4_242,
        "/opt/homebrew/Cellar/node/24.1.0/bin/node",
    ));
    let snapshot = take(&fake)?;

    assert!(
        sessions(&snapshot).is_empty(),
        "an unreadable node process was a session"
    );
    let text = log_text(&log);
    if text.contains("homebrew") || text.contains("node_modules") {
        return Err("log contains a path".to_owned());
    }
    if !text.contains("pid=4242") {
        return Err("a failed argument read was not logged".to_owned());
    }
    Ok(())
}
