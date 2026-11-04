use serde::{Deserialize, Serialize};

use crate::IntegrationsEnv;
use crate::action::Harness;
use crate::domain::{self, Classification};
use crate::event;
use crate::infra::files::{self, Read};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    NotFound,
    NotInstalled,
    Installed,
    NeedsRepair,
    Unreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Offer {
    Install,
    Repair,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessStatus {
    pub harness: Harness,
    pub path: String,
    pub state: State,
    pub reason: Option<String>,
    pub offers: Vec<Offer>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IntegrationsState {
    pub harnesses: Vec<HarnessStatus>,
}

pub(crate) fn row(env: &IntegrationsEnv, harness: Harness) -> HarnessStatus {
    let path = files::hook_file(&env.home, harness);
    let (state, reason) = match files::read(env, harness) {
        Read::NoDir => (State::NotFound, Some(event::not_found(harness))),
        Read::NoFile => (State::NotInstalled, None),
        Read::Unreadable(failure) => (State::Unreadable, Some(event::unreadable(&failure))),
        Read::Doc(doc, _) => {
            let Classification { state, reason, .. } =
                domain::classify(&doc, harness, env.dev, &env.helper);
            (state, reason.map(|failure| event::unreadable(&failure)))
        }
    };
    let reason = match (harness, state) {
        (Harness::Codex, State::Installed | State::NeedsRepair) => Some(event::codex_needs_trust()),
        _ => reason,
    };
    HarnessStatus {
        harness,
        path: path.display().to_string(),
        state,
        reason,
        offers: offers_for(state),
    }
}

fn offers_for(state: State) -> Vec<Offer> {
    match state {
        State::NotInstalled => vec![Offer::Install],
        State::NeedsRepair => vec![Offer::Repair, Offer::Remove],
        State::Installed => vec![Offer::Remove],
        State::NotFound | State::Unreadable => Vec::new(),
    }
}
