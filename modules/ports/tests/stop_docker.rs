#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use common::{LogCapture, Served, canonical_temp_home, say, serve};
use navori_mod_ports::{Containers, DockerError, DockerSocket, Stopped};
use tokio::io::AsyncWriteExt;

const STOP: &str = "POST /containers/abc123/stop HTTP/1.1\r\nHost: docker\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const LIST: &str = "GET /containers/json HTTP/1.1\r\nHost: docker\r\nConnection: close\r\n\r\n";

fn reply(status: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

async fn stop(served: &Served) -> Result<Stopped, DockerError> {
    DockerSocket::new(served.path.clone()).stop("abc123").await
}

#[tokio::test]
async fn a_stop_sends_exactly_the_post_with_no_t_value() {
    let served = serve(say(reply("204 No Content", "")));
    stop(&served).await.unwrap();
    let request = served.request.await.unwrap();
    assert_eq!(request, STOP);
    assert!(!request.contains("t="), "{request}");
}

#[tokio::test]
async fn a_204_reply_means_stopped() {
    let served = serve(say(reply("204 No Content", "")));
    assert_eq!(stop(&served).await, Ok(Stopped::Stopped));
}

#[tokio::test]
async fn a_204_reply_that_closes_nothing_still_means_stopped() {
    let served = serve(|stream| {
        tokio::spawn(async move {
            let mut stream = stream;
            let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\n\r\n").await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        })
    });
    assert_eq!(stop(&served).await, Ok(Stopped::Stopped));
}

#[tokio::test]
async fn a_304_reply_means_already_gone() {
    let served = serve(say(b"HTTP/1.1 304 Not Modified\r\n\r\n".to_vec()));
    assert_eq!(stop(&served).await, Ok(Stopped::AlreadyGone));
}

#[tokio::test]
async fn a_404_reply_means_already_gone() {
    let served = serve(say(reply(
        "404 Not Found",
        r#"{"message":"No such container"}"#,
    )));
    assert_eq!(stop(&served).await, Ok(Stopped::AlreadyGone));
}

#[tokio::test]
async fn a_500_reply_fails_with_the_fixed_text_and_never_the_daemons_body() {
    let served = serve(say(reply(
        "500 Internal Server Error",
        r#"{"message":"secret-mount /work/app"}"#,
    )));
    let err = stop(&served).await.unwrap_err();
    assert_eq!(err, DockerError::BadStatus(500));
    assert_eq!(err.to_string(), "Docker answered with status 500.");
    assert!(!format!("{err} {err:?}").contains("secret"));
}

#[tokio::test(start_paused = true)]
async fn a_stop_reply_delayed_past_two_seconds_still_succeeds() {
    let served = serve(|stream| {
        tokio::spawn(async move {
            let mut stream = stream;
            tokio::time::sleep(Duration::from_secs(9)).await;
            let _ = stream.write_all(&reply("204 No Content", "")).await;
            let _ = stream.shutdown().await;
        })
    });
    assert_eq!(stop(&served).await, Ok(Stopped::Stopped));
}

#[tokio::test]
async fn the_list_request_bytes_the_two_second_limit_and_the_one_mebibyte_cap_are_unchanged() {
    let served = serve(say(reply("200 OK", "[]")));
    let socket = DockerSocket::new(served.path.clone());
    assert_eq!(socket.running().await, Ok(Vec::new()));
    assert_eq!(served.request.await.unwrap(), LIST);

    let big = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n[]", 1_048_577);
    let served = serve(say(big.into_bytes()));
    let socket = DockerSocket::new(served.path.clone());
    assert_eq!(socket.running().await, Err(DockerError::TooLarge));
}

#[tokio::test(start_paused = true)]
async fn the_list_still_times_out_after_two_seconds() {
    let served = serve(|stream| {
        tokio::spawn(async move {
            let _stream = stream;
            tokio::time::sleep(Duration::from_secs(3600)).await;
        })
    });
    let started = tokio::time::Instant::now();
    let socket = DockerSocket::new(served.path.clone());
    assert_eq!(socket.running().await, Err(DockerError::Timeout));
    assert_eq!(started.elapsed(), Duration::from_secs(2));
}

#[tokio::test]
async fn the_stop_writes_no_container_working_directory_or_environment_to_a_log() {
    let logs = LogCapture::start();
    let body = r#"{"message":"boom","WorkingDir":"/work/app-secret","Env":["TOKEN=env-secret"]}"#;
    for status in [
        "204 No Content",
        "404 Not Found",
        "500 Internal Server Error",
    ] {
        let served = serve(say(reply(status, body)));
        let _ = stop(&served).await;
    }
    let missing = DockerSocket::new(canonical_temp_home().1.join("none.sock"));
    let _ = missing.stop("abc123").await;
    let logged = logs.text();
    assert!(!logged.contains("app-secret"), "{logged}");
    assert!(!logged.contains("env-secret"), "{logged}");
}

#[tokio::test]
async fn docker_is_sent_no_request_other_than_list_and_stop() {
    let served = serve(say(reply("204 No Content", "")));
    stop(&served).await.unwrap();
    let request = served.request.await.unwrap();
    assert!(
        request.starts_with("POST /containers/abc123/stop "),
        "{request}"
    );
    assert_eq!(request.matches("HTTP/1.1").count(), 1);

    let served = serve(say(reply("200 OK", "[]")));
    DockerSocket::new(served.path.clone())
        .running()
        .await
        .unwrap();
    assert!(
        served
            .request
            .await
            .unwrap()
            .starts_with("GET /containers/json ")
    );
}

#[tokio::test]
async fn an_id_that_could_alter_the_request_is_never_sent() {
    let served = serve(say(reply("204 No Content", "")));
    let socket = DockerSocket::new(served.path.clone());
    for id in ["", "a b", "a/../b", "a\r\nHost: x", "a?t=0"] {
        assert_eq!(socket.stop(id).await, Err(DockerError::BadReply), "{id:?}");
    }
    served.request.abort();
}

#[tokio::test]
async fn a_missing_socket_fails_a_stop_as_absent() {
    let (_guard, home) = canonical_temp_home();
    let socket = DockerSocket::new(home.join("none.sock"));
    assert_eq!(socket.stop("abc123").await, Err(DockerError::Absent));
}
