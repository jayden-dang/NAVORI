//! Real Unix sockets in a temp dir.
#![allow(clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use futures::FutureExt;
use futures::future::BoxFuture;
use navori_ipc::{Client, Handler, IpcError, MAX_LINE_BYTES, Server};
use navori_protocol::rpc;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

struct Echo;

impl Handler for Echo {
    fn handle<'a>(
        &'a self,
        method: &'a str,
        params: Value,
    ) -> BoxFuture<'a, Result<Value, rpc::Error>> {
        async move {
            match method {
                "echo" => Ok(json!({ "method": method, "params": params })),
                "fail" => Err(rpc::Error {
                    code: rpc::code::ACTION_FAILED,
                    message: "nope".into(),
                    data: None,
                }),
                _ => Err(rpc::Error {
                    code: rpc::code::METHOD_NOT_FOUND,
                    message: method.to_owned(),
                    data: None,
                }),
            }
        }
        .boxed()
    }
}

fn sock(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("run/navori.sock")
}

#[tokio::test]
async fn a_client_call_returns_the_handler_result() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::bind(&sock(&dir), Arc::new(Echo)).await.unwrap();
    let mut client = Client::connect(server.path()).await.unwrap();
    let out = client.call("echo", json!({ "x": 1 })).await.unwrap();
    assert_eq!(out, json!({ "method": "echo", "params": { "x": 1 } }));
    let out = client.call("echo", json!([2])).await.unwrap();
    assert_eq!(out["params"], json!([2]));
    server.shutdown().await;
}

#[tokio::test]
async fn handler_errors_reach_the_client_with_their_code() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::bind(&sock(&dir), Arc::new(Echo)).await.unwrap();
    let mut client = Client::connect(server.path()).await.unwrap();
    let err = client.call("fail", Value::Null).await.unwrap_err();
    assert!(
        matches!(err, IpcError::Rpc(ref e) if e.code == rpc::code::ACTION_FAILED && e.message == "nope"),
        "{err}"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn several_clients_are_served_concurrently() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::bind(&sock(&dir), Arc::new(Echo)).await.unwrap();
    let calls = (0..8).map(|i| {
        let path = server.path().to_owned();
        tokio::spawn(async move {
            let mut c = Client::connect(&path).await.unwrap();
            c.call("echo", json!(i)).await.unwrap()["params"].clone()
        })
    });
    let results: Vec<Value> = futures::future::join_all(calls)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();
    assert_eq!(results, (0..8).map(|i| json!(i)).collect::<Vec<_>>());
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_lines_get_a_parse_error_and_the_connection_survives() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::bind(&sock(&dir), Arc::new(Echo)).await.unwrap();
    let stream = UnixStream::connect(server.path()).await.unwrap();
    let (r, mut w) = stream.into_split();
    let mut lines = BufReader::new(r).lines();

    w.write_all(b"this is not json\n").await.unwrap();
    let resp: rpc::Response =
        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(resp.error.unwrap().code, rpc::code::PARSE_ERROR);
    assert_eq!(resp.id, Value::Null);

    let req = serde_json::to_string(&rpc::Request::new(7, "echo", json!("ok"))).unwrap();
    w.write_all(format!("{req}\n").as_bytes()).await.unwrap();
    let resp: rpc::Response =
        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(resp.id, json!(7));
    assert_eq!(resp.result.unwrap()["params"], json!("ok"));
    server.shutdown().await;
}

#[tokio::test]
async fn oversized_requests_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::bind(&sock(&dir), Arc::new(Echo)).await.unwrap();
    let stream = UnixStream::connect(server.path()).await.unwrap();
    let (r, mut w) = stream.into_split();
    let mut lines = BufReader::new(r).lines();
    let huge = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"echo\",\"params\":\"{}\"}}\n",
        "a".repeat(MAX_LINE_BYTES + 10)
    );
    let _ = w.write_all(huge.as_bytes()).await;
    let line = lines.next_line().await.unwrap().unwrap();
    let resp: rpc::Response = serde_json::from_str(&line).unwrap();
    assert_eq!(resp.error.unwrap().code, rpc::code::INVALID_REQUEST);
    server.shutdown().await;
}

#[tokio::test]
async fn the_socket_is_private_to_the_user() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::bind(&sock(&dir), Arc::new(Echo)).await.unwrap();
    let file_mode = std::fs::metadata(server.path())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    let dir_mode = std::fs::metadata(server.path().parent().unwrap())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(file_mode, 0o600);
    assert_eq!(dir_mode, 0o700);
    server.shutdown().await;
}

#[tokio::test]
async fn a_stale_socket_file_is_replaced_but_a_live_one_is_not_stolen() {
    let dir = tempfile::tempdir().unwrap();
    let path = sock(&dir);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // A leftover file from a crashed process: nobody listens on it.
    drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
    assert!(path.exists());

    let server = Server::bind(&path, Arc::new(Echo)).await.unwrap();
    let err = Server::bind(&path, Arc::new(Echo)).await.unwrap_err();
    assert!(matches!(err, IpcError::AlreadyRunning(_)), "{err}");

    let mut client = Client::connect(&path).await.unwrap();
    assert!(
        client.call("echo", Value::Null).await.is_ok(),
        "first server must keep serving"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn after_shutdown_the_socket_is_gone_and_clients_see_not_running() {
    let dir = tempfile::tempdir().unwrap();
    let path = sock(&dir);
    let server = Server::bind(&path, Arc::new(Echo)).await.unwrap();
    server.shutdown().await;
    assert!(!path.exists());
    let err = Client::connect(&path).await.unwrap_err();
    assert!(matches!(err, IpcError::NotRunning(_)), "{err}");
}
