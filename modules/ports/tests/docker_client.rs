#![allow(clippy::unwrap_used)]

mod common;

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::canonical_temp_home;
use navori_mod_ports::{Container, Containers, DockerError, DockerSocket};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::task::JoinHandle;

const REQUEST: &str = "GET /containers/json HTTP/1.1\r\nHost: docker\r\nConnection: close\r\n\r\n";
const BODY: &str = r#"[{"Id":"abc123","Names":["/db"],"Image":"postgres","Command":"secret-command","Ports":[{"IP":"127.0.0.1","PrivatePort":5432,"PublicPort":5432,"Type":"tcp"},{"PrivatePort":9000,"Type":"tcp"}],"Labels":{"com.docker.compose.project.working_dir":"/work/app","other":"secret-label"},"Mounts":[{"Source":"/secret/mount"}]}]"#;

fn expected() -> Vec<Container> {
    vec![Container {
        id: "abc123".to_owned(),
        name: "db".to_owned(),
        working_dir: Some(PathBuf::from("/work/app")),
        image: Some("postgres".to_owned()),
        ports: vec![("127.0.0.1".parse::<IpAddr>().unwrap(), 5432)],
    }]
}

struct Served {
    _dir: tempfile::TempDir,
    path: PathBuf,
    request: JoinHandle<String>,
}

fn serve(reply: impl FnOnce(tokio::net::UnixStream) -> JoinHandle<()> + Send + 'static) -> Served {
    let guard = tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap();
    let path = std::fs::canonicalize(guard.path()).unwrap().join("d.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let request = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).await.unwrap() == 0 {
                break;
            }
            head.push(byte[0]);
        }
        let _ = reply(stream).await;
        String::from_utf8(head).unwrap()
    });
    Served {
        _dir: guard,
        path,
        request,
    }
}

fn say(bytes: Vec<u8>) -> impl FnOnce(tokio::net::UnixStream) -> JoinHandle<()> + Send + 'static {
    move |mut stream| {
        tokio::spawn(async move {
            let _ = stream.write_all(&bytes).await;
            let _ = stream.shutdown().await;
        })
    }
}

fn sized(body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

async fn read(served: &Served) -> Result<Vec<Container>, DockerError> {
    DockerSocket::new(served.path.clone()).running().await
}

#[tokio::test]
async fn a_sized_reply_is_read_and_only_four_fields_are_kept() {
    let served = serve(say(sized(BODY)));
    let containers = read(&served).await.unwrap();
    assert_eq!(containers, expected());
    let shown = format!("{containers:?}");
    assert!(!shown.contains("secret"), "{shown}");
    assert_eq!(served.request.await.unwrap(), REQUEST);
}

async fn image_of(listed: &str) -> Option<String> {
    let body = format!(r#"[{{"Id":"a","Names":["/x"],"Image":"{listed}"}}]"#);
    let served = serve(say(sized(&body)));
    read(&served).await.unwrap().remove(0).image
}

#[tokio::test]
async fn a_container_image_is_reduced_to_its_repository_name() {
    assert_eq!(
        image_of("postgres:16-alpine").await.as_deref(),
        Some("postgres")
    );
    assert_eq!(
        image_of("localhost:5000/team/db:1@sha256:abc")
            .await
            .as_deref(),
        Some("localhost:5000/team/db")
    );
    assert_eq!(image_of("sha256:abc123").await, None);
}

#[tokio::test]
async fn a_container_listed_without_an_image_has_none() {
    let body = r#"[{"Id":"a","Names":["/x"]}]"#;
    let served = serve(say(sized(body)));
    assert_eq!(read(&served).await.unwrap()[0].image, None);
}

#[tokio::test]
async fn only_tcp_publishes_are_kept() {
    let body = r#"[{"Id":"a","Names":["/dns"],"Ports":[{"IP":"127.0.0.1","PrivatePort":53,"PublicPort":5353,"Type":"udp"}]},{"Id":"b","Names":["/both"],"Ports":[{"IP":"127.0.0.1","PrivatePort":80,"PublicPort":8080,"Type":"tcp"},{"IP":"127.0.0.1","PrivatePort":80,"PublicPort":8080,"Type":"udp"}]}]"#;
    let served = serve(say(sized(body)));
    let ports: Vec<_> = read(&served)
        .await
        .unwrap()
        .into_iter()
        .map(|container| (container.id, container.ports))
        .collect();
    let loopback = "127.0.0.1".parse::<IpAddr>().unwrap();
    assert_eq!(
        ports,
        vec![
            ("a".to_owned(), vec![]),
            ("b".to_owned(), vec![(loopback, 8080)]),
        ]
    );
}

#[tokio::test]
async fn a_chunked_reply_with_trailers_is_read() {
    let (first, second) = BODY.split_at(40);
    let reply = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{first}\r\n{:X};ext=1\r\n{second}\r\n0\r\nX-Trailer: one\r\nX-Other: two\r\n\r\n",
        first.len(),
        second.len()
    );
    let served = serve(say(reply.into_bytes()));
    assert_eq!(read(&served).await.unwrap(), expected());
}

#[tokio::test]
async fn a_reply_that_ends_at_end_of_stream_is_read() {
    let reply = format!("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{BODY}");
    let served = serve(say(reply.into_bytes()));
    assert_eq!(read(&served).await.unwrap(), expected());
}

#[tokio::test]
async fn a_header_block_over_sixteen_kib_is_a_bad_reply() {
    let padding = "a".repeat(20_000);
    let reply = format!("HTTP/1.1 200 OK\r\nX-Pad: {padding}\r\nContent-Length: 2\r\n\r\n[]");
    let served = serve(say(reply.into_bytes()));
    assert_eq!(read(&served).await, Err(DockerError::BadReply));
}

#[tokio::test]
async fn a_sized_body_over_one_mib_is_too_large() {
    let reply = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n[]", 1_048_577);
    let served = serve(say(reply.into_bytes()));
    assert_eq!(read(&served).await, Err(DockerError::TooLarge));
}

#[tokio::test]
async fn a_streamed_body_over_one_mib_is_too_large() {
    let mut reply = b"HTTP/1.1 200 OK\r\n\r\n".to_vec();
    reply.extend(std::iter::repeat_n(b' ', 1_048_577));
    let served = serve(say(reply));
    assert_eq!(read(&served).await, Err(DockerError::TooLarge));
}

#[tokio::test]
async fn a_chunked_body_over_one_mib_is_too_large() {
    let mut reply = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
    reply.extend(b"100001\r\n");
    reply.extend(std::iter::repeat_n(b' ', 0x0010_0001));
    reply.extend(b"\r\n0\r\n\r\n");
    let served = serve(say(reply));
    assert_eq!(read(&served).await, Err(DockerError::TooLarge));
}

#[tokio::test]
async fn a_status_other_than_ok_is_reported_as_that_number() {
    let served = serve(say(
        b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n".to_vec(),
    ));
    assert_eq!(read(&served).await, Err(DockerError::BadStatus(500)));
}

#[tokio::test]
async fn a_body_that_is_not_the_container_list_is_a_bad_reply_that_quotes_nothing() {
    let served = serve(say(sized(r#"[{"Id":7,"path":"/secret/mount"}]"#)));
    let err = read(&served).await.unwrap_err();
    assert_eq!(err, DockerError::BadReply);
    assert!(!format!("{err} {err:?}").contains("secret"));
}

#[tokio::test]
async fn a_peer_streaming_trailers_without_end_is_too_large_before_the_deadline() {
    let sent = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&sent);
    let served = serve(move |mut stream| {
        tokio::spawn(async move {
            let head = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n";
            if stream.write_all(head).await.is_err() {
                return;
            }
            let line = format!("X-Pad: {}\r\n", "a".repeat(1000));
            while stream.write_all(line.as_bytes()).await.is_ok() {
                counted.fetch_add(line.len(), Ordering::SeqCst);
            }
        })
    });
    assert_eq!(read(&served).await, Err(DockerError::TooLarge));
    let _ = served.request.await;
    assert!(sent.load(Ordering::SeqCst) < 2 * 1024 * 1024);
}

#[tokio::test]
async fn a_body_of_many_small_chunks_is_decoded_whole() {
    let body = format!("{BODY}{}", " ".repeat(100_000)).into_bytes();
    let mut reply = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
    for byte in &body {
        reply.extend(format!("1\r\n{}\r\n", *byte as char).bytes());
    }
    reply.extend(b"0\r\n\r\n");
    let served = serve(say(reply));
    assert_eq!(read(&served).await.unwrap(), expected());
}

#[tokio::test]
async fn a_content_length_that_is_repeated_conflicting_or_signed_is_a_bad_reply() {
    for lengths in [
        "Content-Length: 2\r\nContent-Length: 2",
        "Content-Length: 9\r\nContent-Length: 2",
        "Content-Length: +2",
    ] {
        let reply = format!("HTTP/1.1 200 OK\r\n{lengths}\r\n\r\n[]");
        let served = serve(say(reply.into_bytes()));
        assert_eq!(read(&served).await, Err(DockerError::BadReply), "{lengths}");
    }
}

#[tokio::test]
async fn only_the_exact_chunked_token_selects_chunked_framing() {
    let reply = "HTTP/1.1 200 OK\r\nTransfer-Encoding: notchunked\r\nContent-Length: 2\r\n\r\n[]";
    let served = serve(say(reply.as_bytes().to_vec()));
    assert_eq!(read(&served).await, Ok(Vec::new()));
}

#[test]
fn a_container_shows_only_its_id_when_printed() {
    let shown = format!("{:?}", expected());
    assert!(shown.contains("abc123"), "{shown}");
    assert!(!shown.contains("/work/app"), "{shown}");
    assert!(!shown.contains("\"db\""), "{shown}");
}

#[tokio::test]
async fn a_request_that_cannot_be_written_is_refused() {
    let guard = tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap();
    let path = std::fs::canonicalize(guard.path()).unwrap().join("d.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let socket = DockerSocket::new(path);
    let client = tokio::spawn(async move { socket.running().await });
    drop(listener.accept().await.unwrap());
    assert_eq!(client.await.unwrap(), Err(DockerError::Refused));
}

#[tokio::test(start_paused = true)]
async fn a_server_that_never_answers_times_out_after_two_seconds() {
    let served = serve(|stream| {
        tokio::spawn(async move {
            let _stream = stream;
            tokio::time::sleep(Duration::from_secs(3600)).await;
        })
    });
    let started = tokio::time::Instant::now();
    assert_eq!(read(&served).await, Err(DockerError::Timeout));
    assert_eq!(started.elapsed(), Duration::from_secs(2));
}

#[tokio::test]
async fn a_missing_socket_is_absent() {
    let (_guard, home) = canonical_temp_home();
    let socket = DockerSocket::new(home.join("none.sock"));
    assert_eq!(socket.running().await, Err(DockerError::Absent));
}

#[tokio::test]
async fn a_socket_file_nobody_listens_on_is_refused() {
    let mut served = serve(say(Vec::new()));
    served.request.abort();
    let _ = (&mut served.request).await;
    assert_eq!(read(&served).await, Err(DockerError::Refused));
}

#[tokio::test]
#[ignore = "reads the real Docker socket"]
async fn the_real_docker_socket_lists_its_containers_when_it_is_there() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let socket = DockerSocket::new(home.join(".docker/run/docker.sock"));
    match socket.running().await {
        Ok(containers) => {
            for container in containers {
                assert!(!container.id.is_empty());
                assert!(container.ports.iter().all(|(_, port)| *port != 0));
            }
        }
        Err(DockerError::Absent | DockerError::Refused) => {}
        Err(err) => panic!("{err}"),
    }
}
