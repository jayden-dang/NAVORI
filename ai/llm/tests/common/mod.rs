#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub struct Responder {
    pub base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
    connections: Arc<AtomicUsize>,
}

impl Responder {
    pub async fn start(status: u16, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(AtomicUsize::new(0));
        let (seen, accepted) = (requests.clone(), connections.clone());
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                accepted.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(serve(stream, status, body, seen.clone()));
            }
        });
        Self {
            base_url,
            requests,
            connections,
        }
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }
}

async fn serve(
    mut stream: tokio::net::TcpStream,
    status: u16,
    body: &'static str,
    seen: Arc<Mutex<Vec<String>>>,
) {
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let Some(request) = take_request(&mut pending) else {
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(n) => {
                    pending.extend_from_slice(&chunk[..n]);
                    continue;
                }
            }
        };
        seen.lock().unwrap().push(request);
        let reply = format!(
            "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        if stream.write_all(reply.as_bytes()).await.is_err() {
            return;
        }
    }
}

fn take_request(pending: &mut Vec<u8>) -> Option<String> {
    let head_end = pending.windows(4).position(|w| w == b"\r\n\r\n")? + 4;
    let head = String::from_utf8_lossy(&pending[..head_end]).to_string();
    let length = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    if pending.len() < head_end + length {
        return None;
    }
    let raw = String::from_utf8_lossy(&pending[..head_end + length]).to_string();
    pending.drain(..head_end + length);
    Some(raw)
}

pub async fn refused_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}
