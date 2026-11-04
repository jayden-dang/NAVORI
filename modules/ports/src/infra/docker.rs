use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::future::BoxFuture;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const REQUEST: &[u8] =
    b"GET /containers/json HTTP/1.1\r\nHost: docker\r\nConnection: close\r\n\r\n";
const LIMIT: Duration = Duration::from_secs(2);
const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 1024 * 1024;
const MAX_SIZE_LINE: usize = 32;

/// Why the running containers could not be read. The text is fixed and quotes nothing from the reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DockerError {
    #[error("Docker is not running: its socket is absent.")]
    Absent,
    #[error("Docker refused the connection.")]
    Refused,
    #[error("Docker did not answer in time.")]
    Timeout,
    #[error("Docker's answer was too large.")]
    TooLarge,
    #[error("Docker answered with status {0}.")]
    BadStatus(u16),
    #[error("Docker's answer could not be read.")]
    BadReply,
}

/// A running container, reduced to what placing its ports needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub working_dir: Option<PathBuf>,
    pub ports: Vec<(IpAddr, u16)>,
}

/// Lists the running containers.
pub trait Containers: Send + Sync {
    fn running(&self) -> BoxFuture<'_, Result<Vec<Container>, DockerError>>;
}

/// The Docker Engine's Unix socket.
#[derive(Debug, Clone)]
pub struct DockerSocket {
    path: PathBuf,
}

impl DockerSocket {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Containers for DockerSocket {
    fn running(&self) -> BoxFuture<'_, Result<Vec<Container>, DockerError>> {
        Box::pin(async move {
            tokio::time::timeout(LIMIT, exchange(&self.path))
                .await
                .unwrap_or(Err(DockerError::Timeout))
        })
    }
}

struct Wiped(Vec<u8>);

impl Drop for Wiped {
    fn drop(&mut self) {
        self.0.fill(0);
        std::hint::black_box(&mut self.0);
    }
}

enum Framing {
    Length(usize),
    Chunked,
    ToEnd,
}

struct Head {
    end: usize,
    framing: Framing,
}

async fn exchange(path: &Path) -> Result<Vec<Container>, DockerError> {
    let mut stream = UnixStream::connect(path).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            DockerError::Absent
        } else {
            DockerError::Refused
        }
    })?;
    stream
        .write_all(REQUEST)
        .await
        .map_err(|_| DockerError::BadReply)?;
    let mut raw = Wiped(Vec::with_capacity(MAX_HEAD + MAX_BODY));
    let mut head = None;
    loop {
        let read = stream
            .read_buf(&mut raw.0)
            .await
            .map_err(|_| DockerError::BadReply)?;
        let ended = read == 0;
        if head.is_none() {
            head = parse_head(&raw.0)?;
        }
        if let Some(head) = &head
            && let Some(body) = body(raw.0.get(head.end..).unwrap_or_default(), head, ended)?
        {
            return containers(&body.0);
        }
        if ended {
            return Err(DockerError::BadReply);
        }
    }
}

fn parse_head(raw: &[u8]) -> Result<Option<Head>, DockerError> {
    let Some(at) = find(raw, b"\r\n\r\n") else {
        return if raw.len() > MAX_HEAD {
            Err(DockerError::BadReply)
        } else {
            Ok(None)
        };
    };
    let end = at + 4;
    if end > MAX_HEAD {
        return Err(DockerError::BadReply);
    }
    let text = std::str::from_utf8(raw.get(..at).unwrap_or_default())
        .map_err(|_| DockerError::BadReply)?;
    let mut lines = text.split("\r\n");
    let status = lines.next().and_then(status).ok_or(DockerError::BadReply)?;
    if status != 200 {
        return Err(DockerError::BadStatus(status));
    }
    let mut framing = Framing::ToEnd;
    let mut chunked = false;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(DockerError::BadReply)?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            let length = value.trim().parse().map_err(|_| DockerError::BadReply)?;
            framing = Framing::Length(length);
        } else if name.trim().eq_ignore_ascii_case("transfer-encoding") {
            chunked |= value.to_ascii_lowercase().contains("chunked");
        }
    }
    if chunked {
        framing = Framing::Chunked;
    }
    Ok(Some(Head { end, framing }))
}

fn status(line: &str) -> Option<u16> {
    let mut parts = line.split_whitespace();
    parts
        .next()
        .filter(|version| version.starts_with("HTTP/1."))?;
    parts.next()?.parse().ok()
}

fn body(rest: &[u8], head: &Head, ended: bool) -> Result<Option<Wiped>, DockerError> {
    match head.framing {
        Framing::Length(length) if length > MAX_BODY => Err(DockerError::TooLarge),
        Framing::Length(length) => match rest.get(..length) {
            Some(body) => Ok(Some(Wiped(body.to_vec()))),
            None => incomplete(ended),
        },
        Framing::ToEnd if rest.len() > MAX_BODY => Err(DockerError::TooLarge),
        Framing::ToEnd if ended => Ok(Some(Wiped(rest.to_vec()))),
        Framing::ToEnd => Ok(None),
        Framing::Chunked => dechunk(rest, ended),
    }
}

fn incomplete<T>(ended: bool) -> Result<Option<T>, DockerError> {
    if ended {
        Err(DockerError::BadReply)
    } else {
        Ok(None)
    }
}

fn dechunk(rest: &[u8], ended: bool) -> Result<Option<Wiped>, DockerError> {
    let mut out = Wiped(Vec::new());
    let mut at = 0;
    loop {
        let Some(line) = line(rest, at, MAX_SIZE_LINE)? else {
            return incomplete(ended);
        };
        let size = chunk_size(line.text)?;
        let data = line.next;
        if size == 0 {
            return trailers(rest, data, ended).map(|done| done.then_some(out));
        }
        if out.0.len().saturating_add(size) > MAX_BODY {
            return Err(DockerError::TooLarge);
        }
        let Some(chunk) = rest.get(data..data + size + 2) else {
            return incomplete(ended);
        };
        let (bytes, end) = chunk.split_at(size);
        if end != b"\r\n" {
            return Err(DockerError::BadReply);
        }
        out.0.extend_from_slice(bytes);
        at = data + size + 2;
    }
}

fn trailers(rest: &[u8], mut at: usize, ended: bool) -> Result<bool, DockerError> {
    loop {
        let Some(line) = line(rest, at, MAX_HEAD)? else {
            return if ended {
                Err(DockerError::BadReply)
            } else {
                Ok(false)
            };
        };
        if line.text.is_empty() {
            return Ok(true);
        }
        at = line.next;
    }
}

struct Line<'a> {
    text: &'a [u8],
    next: usize,
}

fn line(rest: &[u8], at: usize, cap: usize) -> Result<Option<Line<'_>>, DockerError> {
    let tail = rest.get(at..).unwrap_or_default();
    let Some(end) = tail.iter().position(|byte| *byte == b'\n') else {
        return if tail.len() > cap {
            Err(DockerError::BadReply)
        } else {
            Ok(None)
        };
    };
    if end + 1 > cap {
        return Err(DockerError::BadReply);
    }
    let text = tail.get(..end).unwrap_or_default();
    let text = text.strip_suffix(b"\r").unwrap_or(text);
    Ok(Some(Line {
        text,
        next: at + end + 1,
    }))
}

fn chunk_size(line: &[u8]) -> Result<usize, DockerError> {
    let size = line.split(|byte| *byte == b';').next().unwrap_or_default();
    let size = std::str::from_utf8(size).map_err(|_| DockerError::BadReply)?;
    let size = usize::from_str_radix(size.trim(), 16).map_err(|_| DockerError::BadReply)?;
    if size > MAX_BODY {
        return Err(DockerError::TooLarge);
    }
    Ok(size)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[derive(Deserialize)]
struct Listed {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Names", default)]
    names: Vec<String>,
    #[serde(rename = "Ports", default)]
    ports: Vec<Published>,
    #[serde(rename = "Labels", default)]
    labels: Option<Labels>,
}

#[derive(Deserialize)]
struct Published {
    #[serde(rename = "IP")]
    ip: Option<String>,
    #[serde(rename = "PublicPort")]
    public: Option<u16>,
}

#[derive(Deserialize)]
struct Labels {
    #[serde(rename = "com.docker.compose.project.working_dir")]
    working_dir: Option<String>,
}

fn containers(body: &[u8]) -> Result<Vec<Container>, DockerError> {
    let listed: Vec<Listed> = serde_json::from_slice(body).map_err(|_| DockerError::BadReply)?;
    Ok(listed.into_iter().map(container).collect())
}

fn container(listed: Listed) -> Container {
    let name = listed
        .names
        .first()
        .map(|name| name.strip_prefix('/').unwrap_or(name).to_owned())
        .unwrap_or_default();
    let ports = listed
        .ports
        .into_iter()
        .filter_map(|port| Some((port.ip?.parse().ok()?, port.public?)))
        .collect();
    let working_dir = listed
        .labels
        .and_then(|labels| labels.working_dir)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from);
    Container {
        id: listed.id,
        name,
        working_dir,
        ports,
    }
}
