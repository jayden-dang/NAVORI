#![allow(clippy::unwrap_used)]

mod common;

use std::fs;
use std::io::Write;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use common::{Rig, canonical_temp_home};
use navori_macos::fake::FakeProcess;
use navori_mod_ports::{
    BackendPort, BranchName, Container, DockerError, Placement, PortEntry, PortKind, attribute,
    place,
};
use serde_json::{Value, json};

const FIXTURE_DIR: &str = "navori-fixture-compose-dir";

fn git(dir: &Path, args: &[&str]) {
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
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-b", "main"]);
    git(dir, &["commit", "--allow-empty", "-m", "init"]);
}

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

fn backend(pid: i32, port: u16, addresses: &[&str]) -> BackendPort {
    BackendPort {
        pid,
        start_us: 1,
        port,
        addresses: addresses.iter().map(|text| ip(text)).collect(),
        exe: None,
    }
}

fn container(id: &str, working_dir: Option<&Path>, ports: &[(&str, u16)]) -> Container {
    Container {
        id: id.to_owned(),
        name: format!("{id}-name"),
        working_dir: working_dir.map(Path::to_path_buf),
        ports: ports.iter().map(|(text, port)| (ip(text), *port)).collect(),
    }
}

struct Repos {
    _home: tempfile::TempDir,
    home: PathBuf,
    lane: PathBuf,
    gone: PathBuf,
}

fn repos() -> Repos {
    let (guard, home) = canonical_temp_home();
    let app = home.join("work").join("app");
    init_repo(&app);
    git(&app, &["worktree", "add", ".worktrees/lane", "-b", "lane"]);
    Repos {
        lane: app.join(".worktrees").join("lane"),
        gone: app.join("removed-lane"),
        _home: guard,
        home,
    }
}

fn attributed(repos: &Repos, backend: &[BackendPort], containers: &[Container]) -> Vec<PortEntry> {
    attribute(backend, containers, |path| place(path, &repos.home))
}

#[test]
fn a_container_in_a_live_worktree_is_placed_in_that_worktree() {
    let repos = repos();
    let entries = attributed(
        &repos,
        &[backend(7, 5432, &["127.0.0.1"])],
        &[container("db", Some(&repos.lane), &[("127.0.0.1", 5432)])],
    );
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.kind, PortKind::Docker);
    assert_eq!(entry.pid, 7);
    assert_eq!(entry.port, 5432);
    assert_eq!(entry.addresses, vec![ip("127.0.0.1")]);
    assert_eq!(
        entry.place,
        Placement {
            project: Some("app".to_owned()),
            worktree: Some("lane".to_owned()),
            branch: Some(BranchName::Named("lane".to_owned())),
            missing_path: None,
        }
    );
    let held = entry.container.as_ref().unwrap();
    assert_eq!(held.id, "db");
    assert_eq!(held.name, "db-name");
}

#[test]
fn a_container_whose_directory_was_removed_keeps_the_project_and_the_old_path() {
    let repos = repos();
    let entries = attributed(
        &repos,
        &[backend(7, 5432, &["127.0.0.1"])],
        &[container("db", Some(&repos.gone), &[("127.0.0.1", 5432)])],
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].place,
        Placement {
            project: Some("app".to_owned()),
            worktree: None,
            branch: None,
            missing_path: Some(repos.gone.clone()),
        }
    );
}

#[test]
fn two_containers_publishing_one_port_on_different_addresses_are_two_entries() {
    let repos = repos();
    let entries = attributed(
        &repos,
        &[backend(7, 5432, &["127.0.0.1", "127.0.0.2"])],
        &[
            container("one", None, &[("127.0.0.1", 5432)]),
            container("two", None, &[("127.0.0.2", 5432)]),
        ],
    );
    let seen: Vec<_> = entries
        .iter()
        .map(|entry| {
            (
                entry.container.as_ref().unwrap().id.clone(),
                entry.addresses.clone(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            ("one".to_owned(), vec![ip("127.0.0.1")]),
            ("two".to_owned(), vec![ip("127.0.0.2")]),
        ]
    );
}

#[test]
fn a_publish_on_all_addresses_matches_a_backend_bound_to_all_addresses() {
    let repos = repos();
    let entries = attributed(
        &repos,
        &[backend(7, 5432, &["0.0.0.0"])],
        &[container("db", None, &[("0.0.0.0", 5432)])],
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].container.as_ref().unwrap().id, "db");
    assert_eq!(entries[0].addresses, vec![ip("0.0.0.0")]);
}

#[test]
fn a_port_no_container_publishes_stays_a_docker_port_without_a_project() {
    let repos = repos();
    let entries = attributed(
        &repos,
        &[backend(7, 9999, &["127.0.0.1"])],
        &[container("db", Some(&repos.lane), &[("127.0.0.1", 5432)])],
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, PortKind::Docker);
    assert_eq!(entries[0].place, Placement::default());
    assert_eq!(entries[0].container, None);
    assert_eq!(entries[0].addresses, vec![ip("127.0.0.1")]);
}

#[test]
fn a_container_without_a_working_directory_has_no_project() {
    let repos = repos();
    let entries = attributed(
        &repos,
        &[backend(7, 5432, &["127.0.0.1"])],
        &[container("db", None, &[("127.0.0.1", 5432)])],
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].place, Placement::default());
    assert_eq!(entries[0].container.as_ref().unwrap().id, "db");
}

#[test]
fn one_container_published_on_both_wildcards_with_both_bound_is_one_entry_with_two_addresses() {
    let repos = repos();
    let entries = attributed(
        &repos,
        &[backend(7, 5432, &["0.0.0.0", "::"])],
        &[container("db", None, &[("0.0.0.0", 5432), ("::", 5432)])],
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].addresses, vec![ip("0.0.0.0"), ip("::")]);
}

#[test]
fn the_same_container_with_only_the_v6_wildcard_bound_is_one_entry() {
    let repos = repos();
    let entries = attributed(
        &repos,
        &[backend(7, 5432, &["::"])],
        &[container("db", None, &[("0.0.0.0", 5432), ("::", 5432)])],
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].addresses, vec![ip("::")]);
}

fn docker_process() -> FakeProcess {
    FakeProcess::new(
        100,
        1,
        501,
        10_100,
        "/Applications/Docker.app/Contents/MacOS/com.docker.backend",
    )
    .listen("127.0.0.1", 5432)
}

async fn first_scan(rig: &Rig) -> Value {
    rig.advance(Duration::from_secs(5)).await;
    rig.settle().await;
    rig.dispatch(json!("list")).await.unwrap()
}

fn docker_entries(state: &Value) -> Vec<Value> {
    state["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|project| project["worktrees"].as_array().unwrap())
        .flat_map(|worktree| worktree["processes"].as_array().unwrap())
        .flat_map(|process| process["entries"].as_array().unwrap().clone())
        .filter(|entry| entry["kind"] == "docker")
        .collect()
}

#[tokio::test(start_paused = true)]
async fn docker_is_not_contacted_when_no_backend_listens() {
    let rig = Rig::new();
    rig.processes.add(
        FakeProcess::new(100, 1, 501, 10_100, "/usr/local/bin/node").listen("127.0.0.1", 3000),
    );
    let state = first_scan(&rig).await;
    assert_eq!(rig.containers.calls(), 0);
    assert_eq!(state["docker"], json!("not_seen"));
}

#[tokio::test(start_paused = true)]
async fn docker_is_read_once_for_a_scan_that_found_a_backend() {
    let rig = Rig::new();
    rig.processes.add(docker_process());
    rig.containers
        .reply(Ok(vec![container("db", None, &[("127.0.0.1", 5432)])]));
    let state = first_scan(&rig).await;
    assert_eq!(rig.containers.calls(), 1);
    assert_eq!(state["docker"], json!("read"));
    let entries = docker_entries(&state);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["container"]["id"], "db");
}

#[tokio::test(start_paused = true)]
async fn a_docker_read_that_times_out_keeps_the_ports_and_says_docker_was_unreadable() {
    let rig = Rig::new();
    rig.processes.add(docker_process());
    rig.containers.reply(Err(DockerError::Timeout));
    let state = first_scan(&rig).await;
    let entries = docker_entries(&state);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["port"], 5432);
    assert_eq!(entries[0]["project"], Value::Null);
    assert_eq!(
        state["docker"],
        json!({"unreadable": "Docker did not answer in time."})
    );
    assert_eq!(state["problem"], Value::Null);
}

#[tokio::test(start_paused = true)]
async fn each_scan_publishes_once_with_its_docker_result() {
    let rig = Rig::new();
    rig.processes.add(docker_process());
    rig.containers
        .reply(Ok(vec![container("db", None, &[("127.0.0.1", 5432)])]));
    let revision = rig.kernel.revision();
    rig.advance(Duration::from_millis(4_999)).await;
    let before = *revision.borrow();
    rig.advance(Duration::from_millis(1)).await;
    rig.settle().await;
    rig.idle().await;
    assert_eq!(*revision.borrow(), before + 1);
    assert_eq!(rig.state()["docker"], json!("read"));
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

#[tokio::test(start_paused = true)]
async fn no_working_directory_or_container_path_reaches_a_log_or_event() {
    let writer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let captured = Arc::clone(&writer);
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || Captured(Arc::clone(&captured)))
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let rig = Rig::new();
    rig.processes.add(docker_process());
    let secret = rig.home.join(FIXTURE_DIR);
    rig.containers.reply(Ok(vec![container(
        "db",
        Some(&secret),
        &[("127.0.0.1", 5432)],
    )]));
    first_scan(&rig).await;
    rig.containers.reply(Err(DockerError::BadReply));

    let logged = String::from_utf8(writer.lock().unwrap().clone()).unwrap();
    assert!(!logged.contains(FIXTURE_DIR), "{logged}");
    let events = format!("{:?}", rig.kernel.recent_events(1000).unwrap());
    assert!(!events.contains(FIXTURE_DIR), "{events}");
}
