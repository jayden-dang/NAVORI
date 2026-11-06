//! A scripted test double for the provider, enabled by feature `fake` and never linked into the app.
use std::collections::VecDeque;
use std::fmt;
use std::sync::{Mutex, MutexGuard, PoisonError};

use futures::channel::oneshot;
use futures::future::BoxFuture;
use navori_secrets::Secret;

use crate::{ChatReply, ChatRequest, LlmError, LlmProvider};

struct State {
    script: VecDeque<Result<ChatReply, LlmError>>,
    requests: Vec<ChatRequest>,
    holds: VecDeque<oneshot::Receiver<()>>,
}

pub struct FakeLlm {
    state: Mutex<State>,
}

impl fmt::Debug for FakeLlm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeLlm")
            .field("requests", &self.lock().requests.len())
            .finish()
    }
}

impl FakeLlm {
    pub fn new(script: Vec<Result<ChatReply, LlmError>>) -> Self {
        Self {
            state: Mutex::new(State {
                script: script.into(),
                requests: Vec::new(),
                holds: VecDeque::new(),
            }),
        }
    }

    pub fn requests(&self) -> Vec<ChatRequest> {
        self.lock().requests.clone()
    }

    pub fn hold_next(&self) -> Release {
        let (sender, receiver) = oneshot::channel();
        self.lock().holds.push_back(receiver);
        Release(sender)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug)]
pub struct Release(oneshot::Sender<()>);

impl Release {
    pub fn release(self) {
        let _ = self.0.send(());
    }
}

impl LlmProvider for FakeLlm {
    fn complete<'a>(
        &'a self,
        req: ChatRequest,
        _key: &'a Secret,
    ) -> BoxFuture<'a, Result<ChatReply, LlmError>> {
        Box::pin(async move {
            let hold = {
                let mut state = self.lock();
                state.requests.push(req);
                state.holds.pop_front()
            };
            if let Some(hold) = hold {
                let _ = hold.await;
            }
            self.lock()
                .script
                .pop_front()
                .unwrap_or(Err(LlmError::Malformed))
        })
    }
}
