#![allow(clippy::unwrap_used)]

mod common;

use std::net::TcpListener;
use std::time::Duration;

use common::Rig;
use navori_kernel::{ActionError, DispatchError, Risk};
use navori_macos::fake::FakeProcess;
use serde_json::json;

async fn scanned_with(rig: &Rig, listens: &[(&str, u16)]) {
    let mut process = FakeProcess::new(100, 1, 501, 10_100, "/usr/local/bin/node");
    for (address, port) in listens {
        process = process.listen(address, *port);
    }
    rig.processes.add(process);
    rig.advance(Duration::from_secs(5)).await;
    rig.settle().await;
}

fn invalid_message(result: Result<serde_json::Value, DispatchError>) -> String {
    match result {
        Err(DispatchError::Action(ActionError::Invalid(message))) => message,
        other => panic!("expected an invalid input error, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn open_returns_the_url_for_a_listed_port() {
    let table = [
        ("127.0.0.1", 5173, "http://localhost:5173"),
        ("0.0.0.0", 7000, "http://localhost:7000"),
        ("::1", 3000, "http://localhost:3000"),
        ("192.168.1.5", 8080, "http://192.168.1.5:8080"),
        ("2001:db8::5", 8080, "http://[2001:db8::5]:8080"),
    ];
    for (address, port, url) in table {
        let rig = Rig::new();
        scanned_with(&rig, &[(address, port)]).await;
        let reply = rig.dispatch(json!({"open": {"port": port}})).await.unwrap();
        assert_eq!(reply, json!({"url": url}), "{address}");
    }
}

#[tokio::test(start_paused = true)]
async fn opening_an_unlisted_port_names_the_port() {
    let rig = Rig::new();
    scanned_with(&rig, &[("127.0.0.1", 3000)]).await;
    let message = invalid_message(rig.dispatch(json!({"open": {"port": 4321}})).await);
    assert!(message.contains("4321"), "{message}");
}

#[tokio::test(start_paused = true)]
async fn opening_a_port_before_any_scan_names_the_port() {
    let rig = Rig::new();
    let message = invalid_message(rig.dispatch(json!({"open": {"port": 3000}})).await);
    assert!(message.contains("3000"), "{message}");
}

#[tokio::test(start_paused = true)]
async fn a_port_bound_to_two_addresses_with_different_urls_asks_for_one() {
    let rig = Rig::new();
    scanned_with(&rig, &[("127.0.0.1", 8080), ("192.168.1.5", 8080)]).await;
    let message = invalid_message(rig.dispatch(json!({"open": {"port": 8080}})).await);
    assert!(message.contains("127.0.0.1"), "{message}");
    assert!(message.contains("192.168.1.5"), "{message}");

    let named = rig
        .dispatch(json!({"open": {"port": 8080, "address": "192.168.1.5"}}))
        .await
        .unwrap();
    assert_eq!(named, json!({"url": "http://192.168.1.5:8080"}));
}

#[tokio::test(start_paused = true)]
async fn a_port_bound_to_two_addresses_with_one_url_needs_no_choice() {
    let rig = Rig::new();
    scanned_with(&rig, &[("127.0.0.1", 3000), ("::1", 3000)]).await;
    let reply = rig.dispatch(json!({"open": {"port": 3000}})).await.unwrap();
    assert_eq!(reply, json!({"url": "http://localhost:3000"}));
}

#[tokio::test(start_paused = true)]
async fn naming_an_address_the_port_is_not_bound_to_lists_the_addresses() {
    let rig = Rig::new();
    scanned_with(&rig, &[("127.0.0.1", 3000)]).await;
    let message = invalid_message(
        rig.dispatch(json!({"open": {"port": 3000, "address": "10.0.0.9"}}))
            .await,
    );
    assert!(message.contains("10.0.0.9"), "{message}");
    assert!(message.contains("127.0.0.1"), "{message}");
}

#[tokio::test(start_paused = true)]
async fn open_is_a_read_action() {
    let rig = Rig::new();
    let open = rig
        .kernel
        .actions()
        .into_iter()
        .find(|spec| spec.module == "ports" && spec.name == "open")
        .unwrap();
    assert_eq!(open.risk, Risk::Read);
    assert!(open.description.contains(' ') && open.description.ends_with('.'));
    assert!(open.input_schema.to_string().contains("port"));
}

#[tokio::test(start_paused = true)]
async fn open_connects_to_nothing() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let rig = Rig::new();
    scanned_with(&rig, &[("127.0.0.1", port)]).await;
    rig.dispatch(json!({"open": {"port": port}})).await.unwrap();
    listener.set_nonblocking(true).unwrap();
    let error = listener.accept().unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
}
