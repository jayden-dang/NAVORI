#![allow(clippy::unwrap_used, clippy::await_holding_lock)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{ORIGIN_MS, Rig, serve};
use futures::future::BoxFuture;
use navori_kernel::{Event, Risk, Severity};
use navori_macos::fake::FakeProcess;
use navori_macos::{ProcessSource, Signal, SignalTarget};
use navori_mod_ports::{Container, Containers, DockerError, DockerSocket, Stopped};
use serde_json::{Value, json};
use tokio::time::Instant;

const START_US: u64 = 1_730_000_000_000_000;
const OWN: i32 = 4242;
const CONTAINER: &str = "4f2a9c0d11ee";
const OTHER_CONTAINER: &str = "9a8b7c6d5e4f";
const DEADLINE: Duration = Duration::from_secs(25);
const PAUSE: Duration = Duration::from_millis(5);

fn seat(rig: &Rig) {
    rig.processes
        .add(FakeProcess::new(1, 0, 0, START_US, "/sbin/launchd"));
    rig.processes
        .add(FakeProcess::new(4000, 1, 501, START_US, "/bin/zsh"));
    rig.processes
        .add(FakeProcess::new(OWN, 4000, 501, START_US, "/navori"));
    rig.signals.set_group(1, 1);
    rig.signals.set_group(4000, 4000);
    rig.signals.set_group(OWN, OWN);
}

fn server(rig: &Rig, pid: i32, port: u16, group: i32) {
    rig.processes.add(
        FakeProcess::new(pid, 1, 501, START_US, "/usr/local/bin/node").listen("127.0.0.1", port),
    );
    rig.signals.set_group(pid, group);
}

fn obedient_server(rig: &Rig, pid: i32, port: u16) {
    server(rig, pid, port, 150 + pid);
    rig.signals.obeys(pid, &rig.processes);
}

fn docker_with(rig: &Rig, ids: &[&str]) {
    rig.processes.add(
        FakeProcess::new(
            700,
            1,
            501,
            START_US,
            "/Applications/Docker.app/Contents/MacOS/com.docker.backend",
        )
        .listen("127.0.0.1", 5432),
    );
    rig.signals.set_group(700, 700);
    let held = ids
        .iter()
        .map(|id| Container {
            id: (*id).to_owned(),
            name: "checkout-db-secret".to_owned(),
            working_dir: None,
            image: Some("nginx".to_owned()),
            ports: vec![("127.0.0.1".parse().unwrap(), 5432)],
        })
        .collect();
    rig.containers.reply(Ok(held));
}

fn kill(targets: &[Value]) -> Value {
    json!({"kill": {"selector": {"project": null}, "targets": targets}})
}

fn process(pid: i32) -> Value {
    json!({"process": {"pid": pid, "start_us": START_US}})
}

fn container(id: &str) -> Value {
    json!({"container": {"id": id}})
}

fn answers(reply: &Value) -> Vec<(String, Option<String>)> {
    reply["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|each| {
            (
                each["result"].as_str().unwrap().to_owned(),
                each["reason"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

fn answer(result: &str) -> (String, Option<String>) {
    (result.to_owned(), None)
}

fn answer_with(result: &str, reason: &str) -> (String, Option<String>) {
    (result.to_owned(), Some(reason.to_owned()))
}

async fn until(rig: &Rig, what: &str, ready: impl Fn() -> bool) {
    for _ in 0..400 {
        rig.idle().await;
        if ready() {
            return;
        }
        std::thread::sleep(PAUSE);
    }
    panic!("never saw {what}");
}

async fn until_sent(rig: &Rig, count: usize) {
    until(rig, "the signals", || rig.signals.sent().len() >= count).await;
}

fn stopped_events(rig: &Rig) -> Vec<Event> {
    rig.kernel
        .recent_events(50)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "ports.stopped")
        .collect()
}

#[tokio::test(start_paused = true)]
async fn a_kill_request_is_answered_once_with_one_result_per_target_in_request_order() {
    let rig = Rig::new();
    seat(&rig);
    obedient_server(&rig, 200, 3000);
    obedient_server(&rig, 201, 3001);
    let started = Instant::now();

    let reply = rig
        .dispatch(kill(&[process(201), process(200), process(999)]))
        .await
        .unwrap();

    assert_eq!(
        answers(&reply),
        vec![
            answer("stopped_alone"),
            answer("stopped_alone"),
            answer("already_gone")
        ]
    );
    assert_eq!(reply["results"][0]["target"], process(201));
    assert_eq!(reply["results"][1]["target"], process(200));
    assert_eq!(reply["results"][2]["target"], process(999));
    assert_eq!(started.elapsed(), Duration::from_secs(3));
    let mut sent = rig.signals.sent();
    sent.sort_by_key(|(to, _)| format!("{to:?}"));
    assert_eq!(
        sent,
        vec![
            (SignalTarget::Process(200), Signal::Term),
            (SignalTarget::Process(201), Signal::Term)
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_process_and_a_container_stop_at_the_same_time_inside_one_deadline() {
    let rig = Rig::new();
    seat(&rig);
    obedient_server(&rig, 200, 3000);
    docker_with(&rig, &[CONTAINER]);
    rig.containers.delay_stop(Duration::from_secs(10));
    let started = Instant::now();

    let reply = rig
        .dispatch(kill(&[process(200), container(CONTAINER)]))
        .await
        .unwrap();

    assert_eq!(
        answers(&reply),
        vec![answer("stopped_alone"), answer("stopped")]
    );
    assert_eq!(started.elapsed(), Duration::from_secs(10));
    assert_eq!(rig.containers.stops(), vec![CONTAINER.to_owned()]);
}

#[tokio::test(start_paused = true)]
async fn a_target_still_stopping_at_twenty_five_seconds_answers_still_stopping() {
    let rig = Rig::new();
    seat(&rig);
    obedient_server(&rig, 200, 3000);
    docker_with(&rig, &[CONTAINER]);
    rig.containers.delay_stop(Duration::from_secs(600));
    let started = Instant::now();

    let reply = rig
        .dispatch(kill(&[process(200), container(CONTAINER)]))
        .await
        .unwrap();

    assert_eq!(
        answers(&reply),
        vec![
            answer("stopped_alone"),
            answer_with("still_stopping", "It is still stopping")
        ]
    );
    assert_eq!(started.elapsed(), DEADLINE);
}

#[tokio::test(start_paused = true)]
async fn a_stalled_process_read_fails_every_target_and_signals_nothing() {
    let rig = Rig::new();
    seat(&rig);
    obedient_server(&rig, 200, 3000);
    docker_with(&rig, &[CONTAINER]);
    let hold = rig.gate.block();
    let started = Instant::now();

    let (reply, ()) = tokio::join!(
        rig.dispatch(kill(&[process(200), container(CONTAINER)])),
        async {
            rig.idle().await;
            tokio::time::advance(DEADLINE).await;
        }
    );
    drop(hold);

    let reason = "The process list could not be read in time";
    assert_eq!(
        answers(&reply.unwrap()),
        vec![answer_with("failed", reason), answer_with("failed", reason)]
    );
    assert_eq!(started.elapsed(), DEADLINE);
    assert!(rig.signals.sent().is_empty());
    assert!(rig.containers.stops().is_empty());
}

#[tokio::test(start_paused = true)]
async fn ports_list_and_open_are_answered_while_a_stop_waits() {
    let rig = Rig::new();
    seat(&rig);
    docker_with(&rig, &[CONTAINER]);
    rig.containers.delay_stop(Duration::from_secs(20));

    let (reply, ()) = tokio::join!(rig.dispatch(kill(&[container(CONTAINER)])), async {
        until(&rig, "the container stop", || {
            !rig.containers.stops().is_empty()
        })
        .await;
        let listed = tokio::time::timeout(Duration::from_secs(1), rig.dispatch(json!("list")))
            .await
            .unwrap();
        assert!(listed.is_ok(), "{listed:?}");
        let opened = tokio::time::timeout(
            Duration::from_secs(1),
            rig.dispatch(json!({"open": {"port": 5432}})),
        )
        .await
        .unwrap();
        assert!(opened.is_err(), "{opened:?}");
    });

    assert_eq!(answers(&reply.unwrap()), vec![answer("stopped")]);
}

#[tokio::test(start_paused = true)]
async fn the_scan_cadence_is_unchanged_while_a_stop_waits() {
    let rig = Rig::new();
    seat(&rig);
    docker_with(&rig, &[CONTAINER]);
    rig.containers.delay_stop(Duration::from_secs(20));

    let (reply, ()) = tokio::join!(rig.dispatch(kill(&[container(CONTAINER)])), async {
        until(&rig, "the container stop", || {
            !rig.containers.stops().is_empty()
        })
        .await;
        rig.advance(Duration::from_secs(5)).await;
        rig.settle_at(ORIGIN_MS + 5_000).await;
        assert_eq!(rig.containers.stops().len(), 1);
    });

    assert_eq!(answers(&reply.unwrap()), vec![answer("stopped")]);
}

#[tokio::test(start_paused = true)]
async fn a_valid_kill_from_the_cli_runs_without_a_further_approval() {
    let rig = Rig::new();
    seat(&rig);
    obedient_server(&rig, 200, 3000);
    let spec = rig
        .kernel
        .actions()
        .into_iter()
        .find(|spec| spec.module == "ports" && spec.name == "kill")
        .unwrap();
    assert_eq!(spec.risk, Risk::Destructive);

    let reply = rig.dispatch(kill(&[process(200)])).await.unwrap();

    assert_eq!(answers(&reply), vec![answer("stopped_alone")]);
    assert_eq!(
        rig.signals.sent(),
        vec![(SignalTarget::Process(200), Signal::Term)]
    );
}

#[tokio::test(start_paused = true)]
async fn unreachable_docker_does_not_stop_the_process_targets_from_stopping() {
    let rig = Rig::new();
    seat(&rig);
    obedient_server(&rig, 200, 3000);
    docker_with(&rig, &[CONTAINER]);
    rig.containers.reply(Err(DockerError::Refused));

    let reply = rig
        .dispatch(kill(&[container(CONTAINER), process(200)]))
        .await
        .unwrap();

    assert_eq!(
        answers(&reply),
        vec![
            answer_with("failed", "Docker is not reachable"),
            answer("stopped_alone")
        ]
    );
    assert!(rig.containers.stops().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_container_stop_still_running_at_the_deadline_answers_still_stopping() {
    let rig = Rig::new();
    seat(&rig);
    docker_with(&rig, &[CONTAINER, OTHER_CONTAINER]);
    rig.containers.delay_stop(Duration::from_secs(600));
    let started = Instant::now();

    let reply = rig
        .dispatch(kill(&[container(CONTAINER), container(OTHER_CONTAINER)]))
        .await
        .unwrap();

    let still = answer_with("still_stopping", "It is still stopping");
    assert_eq!(answers(&reply), vec![still.clone(), still]);
    assert_eq!(started.elapsed(), DEADLINE);
}

struct ListedHere {
    listing: Arc<common::FakeContainers>,
    engine: DockerSocket,
}

impl Containers for ListedHere {
    fn running(&self) -> BoxFuture<'_, Result<Vec<Container>, DockerError>> {
        self.listing.running()
    }

    fn stop(&self, id: &str) -> BoxFuture<'_, Result<Stopped, DockerError>> {
        self.engine.stop(id)
    }
}

#[tokio::test(start_paused = true)]
async fn a_real_engine_socket_that_stays_silent_answers_still_stopping_at_the_deadline() {
    let silent = serve(|stream| {
        tokio::spawn(async move {
            let _stream = stream;
            tokio::time::sleep(Duration::from_secs(3600)).await;
        })
    });
    let path = silent.path.clone();
    let rig = Rig::with_engine(|listing| {
        Arc::new(ListedHere {
            listing: Arc::clone(listing),
            engine: DockerSocket::new(path),
        })
    });
    seat(&rig);
    docker_with(&rig, &[CONTAINER]);
    let started = Instant::now();

    let reply = rig.dispatch(kill(&[container(CONTAINER)])).await.unwrap();

    assert_eq!(
        answers(&reply),
        vec![answer_with("still_stopping", "It is still stopping")]
    );
    assert_eq!(started.elapsed(), DEADLINE);
}

#[tokio::test(start_paused = true)]
async fn a_module_restart_during_a_stop_sends_no_further_signal() {
    let rig = Rig::new();
    seat(&rig);
    obedient_server(&rig, 200, 3000);
    rig.signals.ignore_term(200);
    rig.advance(Duration::from_secs(5)).await;
    rig.settle_at(ORIGIN_MS + 5_000).await;
    rig.kernel.scheduler().set_panel_visible(true);
    rig.idle().await;
    rig.advance(Duration::from_secs(1)).await;

    let (reply, ()) = tokio::join!(rig.dispatch(kill(&[process(200)])), async {
        until_sent(&rig, 1).await;
        rig.gate.panic_once();
        rig.advance(Duration::from_secs(2)).await;
        for _ in 0..20 {
            rig.idle().await;
            std::thread::sleep(PAUSE);
        }
        rig.settle_restart().await;
        rig.advance(Duration::from_secs(10)).await;
    });

    assert!(reply.is_err(), "{reply:?}");
    assert_eq!(
        rig.signals.sent(),
        vec![(SignalTarget::Process(200), Signal::Term)]
    );
}

#[tokio::test(start_paused = true)]
async fn after_the_reply_a_scan_starts_at_once_and_the_row_leaves_the_next_state() {
    let rig = Rig::new();
    seat(&rig);
    obedient_server(&rig, 200, 3000);
    rig.advance(Duration::from_secs(5)).await;
    rig.settle_at(ORIGIN_MS + 5_000).await;
    assert_eq!(rig.state()["groups"].as_array().unwrap().len(), 1);
    let before = rig.gate.attempts();

    let reply = rig.dispatch(kill(&[process(200)])).await.unwrap();

    assert_eq!(answers(&reply), vec![answer("stopped_alone")]);
    rig.settle_scan_after(before + 1).await;
    until(&rig, "the row to leave", || {
        rig.state()["groups"] == json!([])
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_reply_that_arrives_while_a_scan_runs_starts_one_more_scan_after_it_and_never_aborts_it()
{
    let rig = Rig::new();
    seat(&rig);
    docker_with(&rig, &[CONTAINER]);
    rig.containers.delay_stop(Duration::from_secs(10));

    let (reply, ()) = tokio::join!(rig.dispatch(kill(&[container(CONTAINER)])), async {
        until(&rig, "the container stop", || {
            !rig.containers.stops().is_empty()
        })
        .await;
        let one_read = rig.gate.attempts();
        let hold = rig.gate.block();
        rig.advance(Duration::from_secs(5)).await;
        until(&rig, "the blocked scan", || rig.gate.attempts() > one_read).await;
        let during = rig.gate.attempts();
        rig.advance(Duration::from_secs(5)).await;
        rig.idle().await;
        assert_eq!(rig.gate.attempts(), during);
        drop(hold);
        rig.settle_at(ORIGIN_MS + 10_000).await;
        assert_eq!(rig.gate.attempts(), 3 * one_read);
    });

    assert_eq!(answers(&reply.unwrap()), vec![answer("stopped")]);
}

#[tokio::test(start_paused = true)]
async fn one_event_with_counts_and_no_process_path_or_container_name_is_emitted() {
    let rig = Rig::new();
    seat(&rig);
    obedient_server(&rig, 200, 3000);
    docker_with(&rig, &[CONTAINER]);

    rig.dispatch(kill(&[process(200), container(CONTAINER), process(999)]))
        .await
        .unwrap();
    rig.idle().await;

    let events = stopped_events(&rig);
    assert_eq!(events.len(), 1, "{events:?}");
    let event = &events[0];
    assert_eq!(event.module, "ports");
    assert_eq!(event.severity, Severity::Info);
    assert_eq!(event.summary, "1 stopped, 1 stopped alone, 1 already gone.");
    assert_eq!(event.payload["stopped_alone"], 1);
    assert_eq!(event.payload["stopped"], 1);
    assert_eq!(event.payload["failed"], 0);
    let shown = format!("{} {}", event.summary, event.payload);
    for hidden in ["node", "usr", "secret", CONTAINER, "200"] {
        assert!(!shown.contains(hidden), "{hidden} in {shown}");
    }
}

#[tokio::test(start_paused = true)]
async fn the_event_is_a_warning_when_any_target_failed() {
    let rig = Rig::new();
    seat(&rig);
    obedient_server(&rig, 200, 3000);
    rig.signals.deny(200);

    let reply = rig.dispatch(kill(&[process(200)])).await.unwrap();
    rig.idle().await;

    assert_eq!(
        answers(&reply),
        vec![answer_with("failed", "The signal was refused")]
    );
    let events = stopped_events(&rig);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].severity, Severity::Warn);
    assert_eq!(events[0].summary, "1 failed.");
}

#[tokio::test(start_paused = true)]
async fn an_overlapping_second_request_does_not_signal_a_target_twice() {
    let rig = Rig::new();
    seat(&rig);
    obedient_server(&rig, 200, 3000);
    rig.signals.ignore_term(200);

    let (first, second) = tokio::join!(rig.dispatch(kill(&[process(200)])), async {
        until_sent(&rig, 1).await;
        let second = rig.dispatch(kill(&[process(200)])).await.unwrap();
        assert_eq!(rig.signals.sent().len(), 1);
        second
    });

    assert_eq!(
        answers(&second),
        vec![answer_with("still_stopping", "It is still stopping")]
    );
    assert_eq!(answers(&first.unwrap()), vec![answer("stopped_alone")]);
    assert_eq!(
        rig.signals.sent(),
        vec![
            (SignalTarget::Process(200), Signal::Term),
            (SignalTarget::Process(200), Signal::Kill)
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn no_timer_runs_while_the_menu_is_closed_and_no_stop_is_in_flight() {
    let rig = Rig::new();
    seat(&rig);
    obedient_server(&rig, 200, 3000);
    let before = rig.gate.attempts();

    rig.dispatch(kill(&[process(200)])).await.unwrap();
    rig.settle_scan_after(before + 1).await;
    let mark = rig.processes.list_calls();
    rig.advance(Duration::from_secs(29)).await;

    assert_eq!(rig.list_calls_since(mark), 0);
    assert!(rig.containers.stops().is_empty());
    rig.advance(Duration::from_secs(2)).await;
    assert!(rig.list_calls_since(mark) > 0);
}

#[tokio::test(start_paused = true)]
async fn a_kill_in_one_profile_writes_its_event_only_to_that_profiles_journal() {
    let here = Rig::new();
    let there = Rig::new();
    seat(&here);
    seat(&there);
    obedient_server(&here, 200, 3000);

    here.dispatch(kill(&[process(200)])).await.unwrap();
    here.idle().await;
    there.idle().await;

    assert_eq!(stopped_events(&here).len(), 1);
    assert!(stopped_events(&there).is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_leader_whose_group_mates_are_all_confirmed_is_signalled_as_a_group_once() {
    let rig = Rig::new();
    seat(&rig);
    server(&rig, 200, 3000, 200);
    server(&rig, 201, 3001, 200);
    rig.signals.obeys(200, &rig.processes);
    rig.signals.obeys(201, &rig.processes);

    let reply = rig
        .dispatch(kill(&[process(200), process(201)]))
        .await
        .unwrap();

    assert_eq!(answers(&reply), vec![answer("stopped"), answer("stopped")]);
    assert_eq!(
        rig.signals.sent(),
        vec![(SignalTarget::Group(200), Signal::Term)]
    );
}

#[tokio::test(start_paused = true)]
async fn a_leader_with_a_group_mate_that_was_not_asked_for_is_signalled_alone() {
    let rig = Rig::new();
    seat(&rig);
    server(&rig, 200, 3000, 200);
    server(&rig, 201, 3001, 200);
    rig.signals.obeys(200, &rig.processes);
    rig.signals.obeys(201, &rig.processes);

    let reply = rig.dispatch(kill(&[process(200)])).await.unwrap();

    assert_eq!(answers(&reply), vec![answer("stopped_alone")]);
    assert_eq!(
        rig.signals.sent(),
        vec![(SignalTarget::Process(200), Signal::Term)]
    );
    assert!(rig.processes.entry(201).is_ok());
}
