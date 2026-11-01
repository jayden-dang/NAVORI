//! Local IPC: JSON-RPC 2.0, one JSON object per line, over a Unix socket.
//!
//! Security model: the socket lives in a `0700` directory, the socket file is `0600`, and
//! every connection's peer UID must equal the UID that owns the socket. Nothing listens
//! on the network.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::future::BoxFuture;
use futures::{SinkExt, StreamExt};
use navori_protocol::rpc;
use serde_json::Value;
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinHandle;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec, LinesCodecError};
use tokio_util::sync::CancellationToken;

/// Largest request line the server accepts.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
/// Responses (event lists, status) can be larger than requests.
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

/// Answers one JSON-RPC method call.
pub trait Handler: Send + Sync + 'static {
    fn handle<'a>(
        &'a self,
        method: &'a str,
        params: Value,
    ) -> BoxFuture<'a, Result<Value, rpc::Error>>;
}

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("Navori is not running (no server at {0})")]
    NotRunning(PathBuf),
    #[error("another Navori instance is already serving {0}")]
    AlreadyRunning(PathBuf),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("rpc error {}: {}", .0.code, .0.message)]
    Rpc(rpc::Error),
}

#[derive(Debug)]
pub struct Server {
    path: PathBuf,
    cancel: CancellationToken,
    accept: Option<JoinHandle<()>>,
}

impl Server {
    /// Binds `path` (creating its parent as `0700`) and starts serving on the current runtime.
    /// A leftover socket file nobody listens on is replaced; a live one is never stolen.
    pub async fn bind(path: &Path, handler: Arc<dyn Handler>) -> Result<Self, IpcError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
        if path.exists() {
            if UnixStream::connect(path).await.is_ok() {
                return Err(IpcError::AlreadyRunning(path.to_owned()));
            }
            std::fs::remove_file(path)?;
        }

        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let owner_uid = std::fs::metadata(path)?.uid();

        let cancel = CancellationToken::new();
        let accept = tokio::spawn(accept_loop(listener, handler, owner_uid, cancel.clone()));
        tracing::info!(path = %path.display(), "ipc server listening");
        Ok(Self {
            path: path.to_owned(),
            cancel,
            accept: Some(accept),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stops accepting, closes open connections and removes the socket file.
    pub async fn shutdown(mut self) {
        self.cancel.cancel();
        if let Some(task) = self.accept.take() {
            let _ = task.await;
        }
        remove_socket(&self.path);
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.accept.is_some() {
            self.cancel.cancel();
            remove_socket(&self.path);
        }
    }
}

fn remove_socket(path: &Path) {
    if let Err(e) = std::fs::remove_file(path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(path = %path.display(), error = %e, "cannot remove socket");
    }
}

async fn accept_loop(
    listener: UnixListener,
    handler: Arc<dyn Handler>,
    owner_uid: u32,
    cancel: CancellationToken,
) {
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    match stream.peer_cred().map(|c| c.uid()) {
                        Ok(uid) if uid == owner_uid => {}
                        Ok(uid) => {
                            tracing::warn!(peer_uid = uid, owner_uid, "rejected ipc connection from another user");
                            continue;
                        }
                        // Typically a liveness probe that already hung up (e.g. `doctor`).
                        Err(e) => {
                            tracing::debug!(error = %e, "dropped ipc connection without readable credentials");
                            continue;
                        }
                    }
                    connections.spawn(serve(stream, handler.clone(), cancel.clone()));
                }
                Err(e) => tracing::warn!(error = %e, "ipc accept failed"),
            },
        }
    }
    connections.shutdown().await;
}

async fn serve(stream: UnixStream, handler: Arc<dyn Handler>, cancel: CancellationToken) {
    let (r, w) = stream.into_split();
    let mut reader = FramedRead::new(r, LinesCodec::new_with_max_length(MAX_LINE_BYTES));
    let mut writer = FramedWrite::new(w, LinesCodec::new());

    loop {
        let line = tokio::select! {
            () = cancel.cancelled() => return,
            line = reader.next() => line,
        };
        let (response, close) = match line {
            None => return,
            Some(Err(LinesCodecError::MaxLineLengthExceeded)) => (
                rpc::Response::err(
                    Value::Null,
                    rpc::code::INVALID_REQUEST,
                    format!("request exceeds {MAX_LINE_BYTES} bytes"),
                ),
                true,
            ),
            Some(Err(LinesCodecError::Io(e))) => {
                tracing::debug!(error = %e, "ipc connection closed");
                return;
            }
            Some(Ok(line)) => (respond(&*handler, &line).await, false),
        };
        let Ok(encoded) = serde_json::to_string(&response) else {
            return;
        };
        if writer.send(encoded).await.is_err() || close {
            return;
        }
    }
}

async fn respond(handler: &dyn Handler, line: &str) -> rpc::Response {
    let req: rpc::Request = match serde_json::from_str(line) {
        Ok(req) => req,
        Err(e) => return rpc::Response::err(Value::Null, rpc::code::PARSE_ERROR, e.to_string()),
    };
    if req.jsonrpc != rpc::VERSION {
        return rpc::Response::err(
            req.id,
            rpc::code::INVALID_REQUEST,
            "jsonrpc must be \"2.0\"",
        );
    }
    match handler.handle(&req.method, req.params).await {
        Ok(result) => rpc::Response::ok(req.id, result),
        Err(error) => rpc::Response {
            jsonrpc: rpc::VERSION.to_owned(),
            id: req.id,
            result: None,
            error: Some(error),
        },
    }
}

/// A connection to the running app. Calls are sequential on one connection.
#[derive(Debug)]
pub struct Client {
    reader: FramedRead<tokio::net::unix::OwnedReadHalf, LinesCodec>,
    writer: FramedWrite<tokio::net::unix::OwnedWriteHalf, LinesCodec>,
    next_id: u64,
}

impl Client {
    pub async fn connect(path: &Path) -> Result<Self, IpcError> {
        let stream = UnixStream::connect(path)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
                    IpcError::NotRunning(path.to_owned())
                }
                _ => IpcError::Io(e),
            })?;
        let (r, w) = stream.into_split();
        Ok(Self {
            reader: FramedRead::new(r, LinesCodec::new_with_max_length(MAX_RESPONSE_BYTES)),
            writer: FramedWrite::new(w, LinesCodec::new()),
            next_id: 1,
        })
    }

    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, IpcError> {
        let id = self.next_id;
        self.next_id += 1;
        let req = serde_json::to_string(&rpc::Request::new(id, method, params))
            .map_err(|e| IpcError::Protocol(e.to_string()))?;
        self.writer.send(req).await.map_err(codec_err)?;

        let line = self
            .reader
            .next()
            .await
            .ok_or_else(|| {
                IpcError::Protocol("connection closed before a response arrived".into())
            })?
            .map_err(codec_err)?;
        let resp: rpc::Response =
            serde_json::from_str(&line).map_err(|e| IpcError::Protocol(e.to_string()))?;
        if resp.id != id {
            return Err(IpcError::Protocol(format!(
                "response id {} does not match request id {id}",
                resp.id
            )));
        }
        match (resp.result, resp.error) {
            (_, Some(err)) => Err(IpcError::Rpc(err)),
            (Some(result), None) => Ok(result),
            (None, None) => Err(IpcError::Protocol(
                "response has neither result nor error".into(),
            )),
        }
    }
}

fn codec_err(e: LinesCodecError) -> IpcError {
    match e {
        LinesCodecError::Io(e) => IpcError::Io(e),
        LinesCodecError::MaxLineLengthExceeded => IpcError::Protocol("response too large".into()),
    }
}
