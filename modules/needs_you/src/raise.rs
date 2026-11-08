use std::time::{SystemTime, UNIX_EPOCH};

use navori_kernel::{ActionError, Module, ModuleCtx};
use navori_protocol::{Envelope, Risk};
use navori_redact::{redact_json, redact_text};
use serde::Serialize;

use crate::NeedsYou;
use crate::action::Choice;
use crate::stored::{self, OpenKind, RaiseRow};

#[derive(Debug, Clone, Serialize)]
pub struct RaiseReply {
    pub id: i64,
    pub revision: u32,
}

pub fn raise(
    ctx: &ModuleCtx<NeedsYou>,
    key: String,
    title: String,
    reason: String,
    expires_at_ms: Option<u64>,
    primary: Choice,
    secondary: Choice,
) -> Result<RaiseReply, ActionError> {
    if blank(&key) || blank(&title) || blank(&primary.label) || blank(&secondary.label) {
        return Err(ActionError::Invalid(
            "key, title and both labels must not be empty".into(),
        ));
    }

    let now = ctx.wait_handle().now();
    let now_ms = millis(now)?;
    if let Some(expires) = expires_at_ms {
        let expires =
            i64::try_from(expires).map_err(|err| ActionError::Invalid(err.to_string()))?;
        if expires <= now_ms {
            return Err(ActionError::Invalid(
                "expires_at_ms must be in the future".into(),
            ));
        }
    }

    let primary_risk = choice_risk(ctx, &primary)?;
    let secondary_risk = choice_risk(ctx, &secondary)?;

    let existing = stored::find_open_key(&ctx.store, &key).map_err(ActionError::Failed)?;
    if existing
        .as_ref()
        .is_some_and(|row| row.kind == OpenKind::Answering)
    {
        return Err(ActionError::Invalid(
            "a Decision with this key is already being answered".into(),
        ));
    }

    refuse_secret_envelope(primary.envelope.as_ref())?;
    refuse_secret_envelope(secondary.envelope.as_ref())?;

    let title = redact_text(&title).text;
    let reason = redact_text(&reason).text;
    let primary_label = redact_text(&primary.label).text;
    let secondary_label = redact_text(&secondary.label).text;

    let row = RaiseRow {
        key,
        title,
        reason,
        raised_at_ms: now_ms,
        expires_at_ms: expires_at_ms
            .map(i64::try_from)
            .transpose()
            .map_err(|err| ActionError::Invalid(err.to_string()))?,
        primary_label,
        primary_envelope: envelope_json(primary.envelope.as_ref())?,
        primary_risk: primary_risk.map(risk_name),
        secondary_label,
        secondary_envelope: envelope_json(secondary.envelope.as_ref())?,
        secondary_risk: secondary_risk.map(risk_name),
    };

    let (id, revision) = match existing {
        Some(open) => stored::replace_open(&ctx.store, open.id, open.revision, &row),
        None => stored::insert_open(&ctx.store, &row),
    }
    .map_err(ActionError::Failed)?;

    Ok(RaiseReply { id, revision })
}

fn choice_risk(ctx: &ModuleCtx<NeedsYou>, choice: &Choice) -> Result<Option<Risk>, ActionError> {
    let Some(envelope) = choice.envelope.as_ref() else {
        return Ok(None);
    };
    if envelope.module == NeedsYou::ID {
        return Err(ActionError::Invalid(
            "a choice must not target needs_you".into(),
        ));
    }
    match ctx.dispatcher().risk_of(envelope) {
        Some(risk) => Ok(Some(risk)),
        None => Err(ActionError::Invalid(
            "a choice Envelope names an unknown Module or Action".into(),
        )),
    }
}

fn refuse_secret_envelope(envelope: Option<&Envelope>) -> Result<(), ActionError> {
    let Some(envelope) = envelope else {
        return Ok(());
    };
    let mut copy =
        serde_json::to_value(envelope).map_err(|err| ActionError::Failed(err.to_string()))?;
    if redact_json(&mut copy) > 0 {
        return Err(ActionError::Invalid(
            "a choice Envelope holds text the Sensitive Data Shield would redact".into(),
        ));
    }
    Ok(())
}

fn envelope_json(envelope: Option<&Envelope>) -> Result<Option<String>, ActionError> {
    envelope
        .map(|envelope| {
            serde_json::to_string(envelope).map_err(|err| ActionError::Failed(err.to_string()))
        })
        .transpose()
}

fn blank(value: &str) -> bool {
    value.trim().is_empty()
}

fn risk_name(risk: Risk) -> String {
    match risk {
        Risk::Read => "read".to_owned(),
        Risk::Write => "write".to_owned(),
        Risk::Destructive => "destructive".to_owned(),
    }
}

fn millis(now: SystemTime) -> Result<i64, ActionError> {
    let duration = now
        .duration_since(UNIX_EPOCH)
        .map_err(|err| ActionError::Failed(err.to_string()))?;
    i64::try_from(duration.as_millis()).map_err(|err| ActionError::Failed(err.to_string()))
}
