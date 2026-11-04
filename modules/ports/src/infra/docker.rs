use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::future::BoxFuture;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const REQUEST: &[u8] =
    b"GET /containers/json HTTP/1.1\r\nHost: docker\r\nConnection: close\r\n\r\n";
const STOP_STATUSES: &[u16] = &[204, 304, 404];
const LIMIT: Duration = Duration::from_secs(2);
const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 1024 * 1024;
const MAX_SIZE_LINE: usize = 32;
const MAX_FRAMING: usize = 64 * 1024;
const RAW_CAP: usize = MAX_HEAD + MAX_BODY + MAX_FRAMING;

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
    #[error("The container id is not one Navori sends.")]
    BadId,
}

const MAX_CONTAINER_ID: usize = 64;

/// The one rule for a container id Navori accepts or sends: 1 to 64 hex digits.
pub(crate) fn is_container_id(id: &str) -> bool {
    (1..=MAX_CONTAINER_ID).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// How a container stop ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    Stopped,
    AlreadyGone,
}

/// A running container, reduced to what placing its ports needs.
#[derive(Clone, PartialEq, Eq)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub working_dir: Option<PathBuf>,
    pub image: Option<String>,
    pub ports: Vec<(IpAddr, u16)>,
}

impl std::fmt::Debug for Container {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Container")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// Lists the running containers and stops one.
pub trait Containers: Send + Sync {
    fn running(&self) -> BoxFuture<'_, Result<Vec<Container>, DockerError>>;
    fn stop(&self, id: &str) -> BoxFuture<'_, Result<Stopped, DockerError>>;
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
            tokio::time::timeout(LIMIT, list(&self.path))
                .await
                .unwrap_or(Err(DockerError::Timeout))
        })
    }

    fn stop(&self, id: &str) -> BoxFuture<'_, Result<Stopped, DockerError>> {
        if !is_container_id(id) {
            return Box::pin(async { Err(DockerError::BadId) });
        }
        let request = format!(
            "POST /containers/{id}/stop HTTP/1.1\r\nHost: docker\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        Box::pin(async move {
            let reply = exchange(&self.path, request.as_bytes(), STOP_STATUSES).await?;
            Ok(if reply.status == 204 {
                Stopped::Stopped
            } else {
                Stopped::AlreadyGone
            })
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
    status: u16,
    end: usize,
    framing: Framing,
}

struct Reply {
    status: u16,
    raw: Wiped,
    body: Body,
}

impl Reply {
    fn bytes(&self) -> &[u8] {
        match &self.body {
            Body::Raw(range) => self.raw.0.get(range.clone()).unwrap_or_default(),
            Body::Owned(body) => &body.0,
        }
    }
}

async fn list(path: &Path) -> Result<Vec<Container>, DockerError> {
    containers(exchange(path, REQUEST, &[200]).await?.bytes())
}

async fn exchange(path: &Path, request: &[u8], accepted: &[u16]) -> Result<Reply, DockerError> {
    let mut stream = UnixStream::connect(path).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            DockerError::Absent
        } else {
            DockerError::Refused
        }
    })?;
    stream
        .write_all(request)
        .await
        .map_err(|_| DockerError::Refused)?;
    let mut raw = Wiped(Vec::with_capacity(RAW_CAP));
    let mut head = None;
    let mut cursor = Cursor::default();
    loop {
        let read = stream
            .read_buf(&mut raw.0)
            .await
            .map_err(|_| DockerError::BadReply)?;
        let ended = read == 0;
        if head.is_none() {
            head = parse_head(&raw.0, accepted)?;
        }
        if let Some(head) = &head
            && let Some(body) = body(&raw.0, head, &mut cursor, ended)?
        {
            return Ok(Reply {
                status: head.status,
                raw,
                body,
            });
        }
        if ended {
            return Err(DockerError::BadReply);
        }
        if raw.0.len() >= RAW_CAP {
            return Err(DockerError::TooLarge);
        }
    }
}

fn parse_head(raw: &[u8], accepted: &[u16]) -> Result<Option<Head>, DockerError> {
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
    if !accepted.contains(&status) {
        return Err(DockerError::BadStatus(status));
    }
    let mut framing = Framing::ToEnd;
    let mut chunked = false;
    let mut sized = false;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(DockerError::BadReply)?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            if sized {
                return Err(DockerError::BadReply);
            }
            sized = true;
            framing = Framing::Length(length(value)?);
        } else if name.trim().eq_ignore_ascii_case("transfer-encoding") {
            chunked |= value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("chunked"));
        }
    }
    if chunked {
        framing = Framing::Chunked;
    }
    Ok(Some(Head {
        status,
        end,
        framing,
    }))
}

fn length(value: &str) -> Result<usize, DockerError> {
    let digits = value.trim();
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(DockerError::BadReply);
    }
    digits.parse().map_err(|_| DockerError::BadReply)
}

fn status(line: &str) -> Option<u16> {
    let mut parts = line.split_whitespace();
    parts
        .next()
        .filter(|version| version.starts_with("HTTP/1."))?;
    parts.next()?.parse().ok()
}

enum Body {
    Raw(std::ops::Range<usize>),
    Owned(Wiped),
}

#[derive(Default)]
struct Cursor {
    at: usize,
    decoded: usize,
    trailing: bool,
}

fn body(
    raw: &[u8],
    head: &Head,
    cursor: &mut Cursor,
    ended: bool,
) -> Result<Option<Body>, DockerError> {
    let rest = raw.get(head.end..).unwrap_or_default();
    if matches!(head.status, 204 | 304) {
        return Ok(Some(Body::Raw(head.end..head.end)));
    }
    match head.framing {
        Framing::Length(length) if length > MAX_BODY => Err(DockerError::TooLarge),
        Framing::Length(length) if rest.len() >= length => {
            Ok(Some(Body::Raw(head.end..head.end + length)))
        }
        Framing::Length(_) => incomplete(ended),
        Framing::ToEnd if rest.len() > MAX_BODY => Err(DockerError::TooLarge),
        Framing::ToEnd if ended => Ok(Some(Body::Raw(head.end..raw.len()))),
        Framing::ToEnd => Ok(None),
        Framing::Chunked => {
            if advance(cursor, rest, ended)? {
                gather(rest, cursor.decoded).map(|out| Some(Body::Owned(out)))
            } else {
                Ok(None)
            }
        }
    }
}

fn incomplete<T>(ended: bool) -> Result<Option<T>, DockerError> {
    if ended {
        Err(DockerError::BadReply)
    } else {
        Ok(None)
    }
}

fn advance(cursor: &mut Cursor, rest: &[u8], ended: bool) -> Result<bool, DockerError> {
    loop {
        let cap = if cursor.trailing {
            MAX_HEAD
        } else {
            MAX_SIZE_LINE
        };
        let Some(line) = line(rest, cursor.at, cap)? else {
            return incomplete::<()>(ended).map(|_| false);
        };
        if cursor.trailing {
            if line.text.is_empty() {
                return Ok(true);
            }
            cursor.at = line.next;
            continue;
        }
        let size = chunk_size(line.text)?;
        if size == 0 {
            cursor.trailing = true;
            cursor.at = line.next;
            continue;
        }
        if cursor.decoded.saturating_add(size) > MAX_BODY {
            return Err(DockerError::TooLarge);
        }
        let Some(chunk) = rest.get(line.next..line.next + size + 2) else {
            return incomplete::<()>(ended).map(|_| false);
        };
        if !chunk.ends_with(b"\r\n") {
            return Err(DockerError::BadReply);
        }
        cursor.decoded += size;
        cursor.at = line.next + size + 2;
    }
}

fn gather(rest: &[u8], decoded: usize) -> Result<Wiped, DockerError> {
    let mut out = Wiped(Vec::with_capacity(decoded));
    let mut at = 0;
    loop {
        let line = line(rest, at, MAX_SIZE_LINE)?.ok_or(DockerError::BadReply)?;
        let size = chunk_size(line.text)?;
        if size == 0 {
            return Ok(out);
        }
        let chunk = rest
            .get(line.next..line.next + size)
            .ok_or(DockerError::BadReply)?;
        out.0.extend_from_slice(chunk);
        at = line.next + size + 2;
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
    #[serde(rename = "Image", default)]
    image: Option<String>,
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
    #[serde(rename = "Type")]
    protocol: Option<String>,
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
        .filter(|port| port.protocol.as_deref() == Some("tcp"))
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
        image: listed.image.as_deref().and_then(crate::image::image_name),
        ports,
    }
}
