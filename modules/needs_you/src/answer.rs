use std::time::{SystemTime, UNIX_EPOCH};

use navori_kernel::{ActionError, DispatchError, ModuleCtx};
use navori_protocol::Envelope;
use navori_redact::redact_text;

use crate::NeedsYou;
use crate::action::Pick;
use crate::stored::{self, OpenKind};

pub const STOPPED_MID_ANSWER: &str =
    "Navori stopped before this finished — it may already have run";
pub const VANISHED_ACTION: &str = "this action no longer exists";

pub enum AnswerOutcome {
    /// Decision closed with no Envelope to run.
    Closed,
    /// Answering marker written; caller must reply Ok then dispatch.
    Dispatch(Envelope),
}

pub fn answer(
    ctx: &ModuleCtx<NeedsYou>,
    id: i64,
    revision: u32,
    choice: Pick,
) -> Result<AnswerOutcome, ActionError> {
    let row = stored::find_for_answer(&ctx.store, id)
        .map_err(ActionError::Failed)?
        .ok_or_else(|| ActionError::Invalid("no Decision with that id".into()))?;

    if row.revision != revision {
        return Err(ActionError::Invalid(
            "Decision revision does not match".into(),
        ));
    }
    match row.kind {
        Some(OpenKind::Answering) => {
            return Err(ActionError::Invalid(
                "this Decision is already being answered".into(),
            ));
        }
        Some(OpenKind::Open | OpenKind::Failed) => {}
        None => {
            return Err(ActionError::Invalid(
                "Decision is not open for answering".into(),
            ));
        }
    }

    let now_ms = millis(ctx.wait_handle().now())?;
    if let Some(expires) = row.expires_at_ms
        && expires <= now_ms
    {
        stored::expire_to_update(&ctx.store, id, &row.title, now_ms)
            .map_err(ActionError::Failed)?;
        return Err(ActionError::Invalid("Decision has expired".into()));
    }

    let raw = match choice {
        Pick::Primary => row.primary_envelope,
        Pick::Secondary => row.secondary_envelope,
    };
    let choice_name = choice.as_str();

    let Some(raw) = raw else {
        stored::mark_answering(&ctx.store, id, choice_name).map_err(ActionError::Failed)?;
        stored::mark_answered(&ctx.store, id, now_ms).map_err(ActionError::Failed)?;
        return Ok(AnswerOutcome::Closed);
    };

    let envelope: Envelope =
        serde_json::from_str(&raw).map_err(|err| ActionError::Failed(err.to_string()))?;
    if ctx.dispatcher().risk_of(&envelope).is_none() {
        stored::mark_failed(&ctx.store, id, choice_name, VANISHED_ACTION)
            .map_err(ActionError::Failed)?;
        return Ok(AnswerOutcome::Closed);
    }

    stored::mark_answering(&ctx.store, id, choice_name).map_err(ActionError::Failed)?;
    Ok(AnswerOutcome::Dispatch(envelope))
}

pub fn finish_ok(ctx: &ModuleCtx<NeedsYou>, id: i64) -> Result<(), String> {
    let now_ms = millis(ctx.wait_handle().now()).map_err(|err| err.to_string())?;
    stored::mark_answered(&ctx.store, id, now_ms)?;
    Ok(())
}

pub fn finish_err(
    ctx: &ModuleCtx<NeedsYou>,
    id: i64,
    choice: &str,
    err: DispatchError,
) -> Result<(), String> {
    let message = format_pending_error(err);
    stored::mark_failed(&ctx.store, id, choice, &message)?;
    Ok(())
}

pub fn fail_answering_on_restart(ctx: &ModuleCtx<NeedsYou>) -> Result<(), String> {
    for (id, choice) in stored::answering_ids(&ctx.store)? {
        stored::mark_failed(&ctx.store, id, &choice, STOPPED_MID_ANSWER)?;
    }
    Ok(())
}

pub fn refresh_choice_risks(ctx: &ModuleCtx<NeedsYou>) -> Result<(), String> {
    for row in stored::open_envelopes(&ctx.store)? {
        let primary_risk = envelope_risk(ctx, row.primary_envelope.as_deref())?;
        let secondary_risk = envelope_risk(ctx, row.secondary_envelope.as_deref())?;
        stored::update_choice_risks(
            &ctx.store,
            row.id,
            primary_risk.as_deref(),
            secondary_risk.as_deref(),
        )?;
    }
    Ok(())
}

fn envelope_risk(ctx: &ModuleCtx<NeedsYou>, raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let envelope: Envelope = serde_json::from_str(raw).map_err(|err| err.to_string())?;
    Ok(ctx.dispatcher().risk_of(&envelope).map(risk_name))
}

fn risk_name(risk: navori_protocol::Risk) -> String {
    match risk {
        navori_protocol::Risk::Read => "read".to_owned(),
        navori_protocol::Risk::Write => "write".to_owned(),
        navori_protocol::Risk::Destructive => "destructive".to_owned(),
    }
}

/// Formats an error from `ModuleDispatcher::send` failing before the target runs.
pub fn format_send_error(err: DispatchError) -> String {
    redact_text(&err.to_string()).text
}

/// Formats an error from a queued `Pending` resolving.
pub fn format_pending_error(err: DispatchError) -> String {
    let uncertain = matches!(
        err,
        DispatchError::Timeout(_) | DispatchError::Unavailable(_)
    );
    let text = redact_text(&err.to_string()).text;
    if uncertain {
        format!("{text} — it may already have run")
    } else {
        text
    }
}

pub fn finish_send_err(
    ctx: &ModuleCtx<NeedsYou>,
    id: i64,
    choice: &str,
    err: DispatchError,
) -> Result<(), String> {
    stored::mark_failed(&ctx.store, id, choice, &format_send_error(err))?;
    Ok(())
}

fn millis(now: SystemTime) -> Result<i64, ActionError> {
    let duration = now
        .duration_since(UNIX_EPOCH)
        .map_err(|err| ActionError::Failed(err.to_string()))?;
    i64::try_from(duration.as_millis()).map_err(|err| ActionError::Failed(err.to_string()))
}
