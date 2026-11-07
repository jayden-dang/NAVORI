//! `navori ask`: one prompt to the provider through the running app.

use std::pin::pin;
use std::time::Duration;

use navori_ipc::{Client, IpcError, MAX_LINE_BYTES};
use navori_protocol::rpc::Request;
use navori_protocol::rpc::method;
use navori_protocol::{AskReply, AskRequest};
use serde_json::Value;
use tokio::time::timeout;

use crate::{Failure, decode};

const NOTICE_AFTER: Duration = Duration::from_secs(3);
const ASK_WAIT: Duration = Duration::from_secs(150);

fn wait_limit() -> Duration {
    std::env::var("NAVORI_ASK_WAIT_MS")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(ASK_WAIT, Duration::from_millis)
}

fn notice_after() -> Duration {
    std::env::var("NAVORI_ASK_NOTICE_MS")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(NOTICE_AFTER, Duration::from_millis)
}

fn size_limit() -> usize {
    std::env::var("NAVORI_ASK_MAX_BYTES")
        .ok()
        .and_then(|bytes| bytes.parse().ok())
        .map_or(MAX_LINE_BYTES, |bytes: usize| bytes.min(MAX_LINE_BYTES))
}

fn params(model: &str, prompt: &str) -> Result<Value, Failure> {
    serde_json::to_value(AskRequest {
        model: model.to_owned(),
        text: prompt.to_owned(),
    })
    .map_err(|e| Failure::Failed(e.to_string()))
}

/// A fresh `Client` numbers its first call 1, so this is the exact line the server will read.
pub fn check_size(model: &str, prompt: &str) -> Result<(), Failure> {
    let line = serde_json::to_string(&Request::new(1, method::ASK, params(model, prompt)?))
        .map_err(|e| Failure::Failed(e.to_string()))?;
    let limit = size_limit();
    if line.len() > limit {
        return Err(Failure::Failed(format!(
            "the question is too large: the request would be {} bytes and the limit is {limit}",
            line.len()
        )));
    }
    Ok(())
}

/// A model's reply is untrusted, so the terminal only gets layout and printable text.
pub fn printable(reply: &str) -> String {
    let mut out = String::with_capacity(reply.len());
    let mut chars = reply.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.peek() {
                Some('[') => {
                    chars.next();
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                Some('\x1b') | None => {}
                Some(_) => {
                    chars.next();
                }
            },
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

pub async fn run(client: &mut Client, model: &str, prompt: &str) -> Result<String, Failure> {
    let params = params(model, prompt)?;
    let wait = wait_limit();
    let mut call = pin!(client.call(method::ASK, params));
    let notice = notice_after();
    let first = match timeout(wait.min(notice), &mut call).await {
        Ok(answer) => Some(answer),
        Err(_) if wait > notice => {
            eprintln!("waiting for the answer…");
            timeout(wait - notice, &mut call).await.ok()
        }
        Err(_) => None,
    };
    let Some(answer) = first else {
        return Err(Failure::Failed(format!(
            "the app did not answer within {} s (the request may still finish in the app)",
            wait.as_secs_f64()
        )));
    };
    let v = answer.map_err(|e| match e {
        IpcError::Rpc(refusal) => Failure::Failed(refusal.message),
        other => other.into(),
    })?;
    Ok(decode::<AskReply>(v)?.reply)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn line_length(model: &str, prompt: &str) -> usize {
        let params = serde_json::json!({ "model": model, "text": prompt });
        let request = Request::new(1, method::ASK, params);
        serde_json::to_string(&request).unwrap().len()
    }

    fn refusal(model: &str, prompt: &str) -> Option<String> {
        match check_size(model, prompt) {
            Ok(()) => None,
            Err(Failure::Failed(message)) => Some(message),
            Err(_) => panic!("expected a plain failure"),
        }
    }

    #[test]
    fn a_request_line_of_exactly_the_limit_is_accepted() {
        let prompt = "a".repeat(1_048_576 - line_length("m", ""));
        assert_eq!(line_length("m", &prompt), 1_048_576);
        assert_eq!(refusal("m", &prompt), None);
    }

    #[test]
    fn a_request_line_one_byte_over_the_limit_is_refused_naming_the_limit() {
        let prompt = "a".repeat(1_048_577 - line_length("m", ""));
        assert_eq!(line_length("m", &prompt), 1_048_577);
        let message = refusal("m", &prompt).unwrap();
        assert!(message.contains("1048576"), "{message}");
    }
}
