#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::Rig;
use navori_kernel::{ActionError, DispatchError, Module, ModuleAction, Risk};
use navori_macos::fake::FakeProcess;
use navori_mod_ports::{Ports, PortsAction};
use serde_json::{Value, json};

const START_US: u64 = 1_730_000_000_000_000;
const CONTAINER: &str = "4f2a9c0d11ee";

fn kill(selector: Value, targets: Value) -> Value {
    json!({"kill": {"selector": selector, "targets": targets}})
}

fn process(pid: i32) -> Value {
    json!({"process": {"pid": pid, "start_us": START_US}})
}

fn invalid_message(result: Result<Value, DispatchError>) -> String {
    match result {
        Err(DispatchError::Action(ActionError::Invalid(message))) => message,
        other => panic!("expected an invalid input error, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn an_empty_target_list_is_rejected_and_nothing_is_signalled() {
    let rig = Rig::new();
    let message = invalid_message(
        rig.dispatch(kill(json!({"project": "klynt"}), json!([])))
            .await,
    );
    assert!(message.contains("target"), "{message}");
    assert!(rig.signals.sent().is_empty());
}

#[tokio::test(start_paused = true)]
async fn the_three_selector_forms_and_both_target_forms_are_accepted() {
    let rig = Rig::new();
    let container = json!({"container": {"id": CONTAINER}});
    let requests = [
        kill(json!({"pid": 412}), json!([process(412)])),
        kill(
            json!({"worktree": {"project": "klynt", "worktree": "feat-a"}}),
            json!([process(412), container.clone()]),
        ),
        kill(
            json!({"project": "klynt"}),
            json!([process(412), container]),
        ),
        kill(json!({"project": null}), json!([process(412)])),
    ];
    for request in requests {
        let reply = rig.dispatch(request.clone()).await;
        assert!(reply.is_ok(), "{request}: {reply:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_pid_selector_with_two_targets_is_rejected() {
    let rig = Rig::new();
    let message = invalid_message(
        rig.dispatch(kill(
            json!({"pid": 412}),
            json!([process(412), process(412)]),
        ))
        .await,
    );
    assert!(message.contains("pid"), "{message}");
    assert!(rig.signals.sent().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_pid_selector_whose_target_has_another_pid_is_rejected() {
    let rig = Rig::new();
    let message = invalid_message(
        rig.dispatch(kill(json!({"pid": 412}), json!([process(413)])))
            .await,
    );
    assert!(message.contains("412"), "{message}");
    let container = json!([{"container": {"id": CONTAINER}}]);
    let message = invalid_message(rig.dispatch(kill(json!({"pid": 412}), container)).await);
    assert!(message.contains("412"), "{message}");
    assert!(rig.signals.sent().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_container_id_that_is_not_hex_is_rejected() {
    let rig = Rig::new();
    let too_long = "a".repeat(65);
    for id in ["abc/../x", too_long.as_str(), ""] {
        let targets = json!([{"container": {"id": id}}]);
        let message = invalid_message(
            rig.dispatch(kill(json!({"project": "klynt"}), targets))
                .await,
        );
        assert!(message.contains("container"), "{id}: {message}");
    }
    let full = "a".repeat(64);
    let targets = json!([{"container": {"id": full}}]);
    let reply = rig
        .dispatch(kill(json!({"project": "klynt"}), targets))
        .await;
    assert!(reply.is_ok(), "{reply:?}");
    assert!(rig.signals.sent().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_process_target_with_a_pid_of_one_or_less_is_rejected() {
    let rig = Rig::new();
    for pid in [1, 0, -1, i32::MIN] {
        let message = invalid_message(
            rig.dispatch(kill(json!({"project": "klynt"}), json!([process(pid)])))
                .await,
        );
        assert!(message.contains("pid"), "{pid}: {message}");
        let message = invalid_message(
            rig.dispatch(kill(json!({"pid": pid}), json!([process(pid)])))
                .await,
        );
        assert!(message.contains("pid"), "{pid}: {message}");
    }
    assert!(rig.signals.sent().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_request_with_more_than_256_targets_is_rejected() {
    let rig = Rig::new();
    let many = |count: i32| json!((0..count).map(|at| process(1000 + at)).collect::<Vec<_>>());
    let message = invalid_message(
        rig.dispatch(kill(json!({"project": "klynt"}), many(257)))
            .await,
    );
    assert!(message.contains("256"), "{message}");
    let reply = rig
        .dispatch(kill(json!({"project": "klynt"}), many(256)))
        .await;
    assert!(reply.is_ok(), "{reply:?}");
}

#[tokio::test(start_paused = true)]
async fn the_catalog_lists_kill_as_destructive_and_list_and_open_unchanged() {
    let docs = PortsAction::catalog();
    let listed: Vec<(&str, Risk)> = docs.iter().map(|doc| (doc.name, doc.risk)).collect();
    assert_eq!(
        listed,
        vec![
            ("list", Risk::Read),
            ("open", Risk::Read),
            ("kill", Risk::Destructive)
        ]
    );
    let kill_doc = docs.iter().find(|doc| doc.name == "kill").unwrap();
    for needed in [
        "selector",
        "never adds targets",
        "start time",
        "one result per target",
    ] {
        assert!(kill_doc.description.contains(needed), "{needed}");
    }
    let action: PortsAction =
        serde_json::from_value(kill(json!({"pid": 412}), json!([process(412)]))).unwrap();
    assert_eq!(action.risk(), Risk::Destructive);
    let list: PortsAction = serde_json::from_value(json!("list")).unwrap();
    assert_eq!(list.risk(), Risk::Read);

    let rig = Rig::new();
    let specs: Vec<_> = rig
        .kernel
        .actions()
        .into_iter()
        .filter(|spec| spec.module == Ports::ID)
        .collect();
    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    assert_eq!(names, vec!["list", "open", "kill"]);
    let spec = specs.iter().find(|spec| spec.name == "kill").unwrap();
    assert_eq!(spec.risk, Risk::Destructive);
    assert!(spec.input_schema.to_string().contains("selector"));
}

#[tokio::test(start_paused = true)]
async fn ports_list_and_open_still_reply_from_the_last_scan_without_scanning() {
    let rig = Rig::new();
    rig.processes.add(
        FakeProcess::new(100, 1, 501, 10_100, "/usr/local/bin/node").listen("127.0.0.1", 5173),
    );
    let before = rig.gate.attempts();
    let message = invalid_message(rig.dispatch(json!({"open": {"port": 5173}})).await);
    assert!(message.contains("5173"), "{message}");
    assert_eq!(rig.gate.attempts(), before);

    rig.advance(Duration::from_secs(5)).await;
    rig.settle().await;
    let scans = rig.gate.attempts();
    let reply = rig.dispatch(json!({"open": {"port": 5173}})).await.unwrap();
    assert_eq!(reply, json!({"url": "http://localhost:5173"}));
    let listed = rig.dispatch(json!("list")).await.unwrap();
    assert!(listed["scanned_at_ms"].is_number());
    assert_eq!(rig.gate.attempts(), scans);
}

#[tokio::test(start_paused = true)]
async fn the_ports_module_still_accepts_every_config_file_it_accepted_before() {
    let configs = [
        json!({}),
        json!({"anything": [1, 2, 3]}),
        json!("text"),
        json!(null),
        json!({"protected": ["postgres"]}),
    ];
    for config in configs {
        let mut sections = serde_json::Map::new();
        sections.insert("ports".to_owned(), config.clone());
        let rig = Rig::with_config(sections);
        assert!(rig.kernel.config_issues().is_empty(), "{config}");
        let listed = rig.dispatch(json!("list")).await;
        assert!(listed.is_ok(), "{config}: {listed:?}");
    }
}
