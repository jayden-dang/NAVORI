use std::time::{Duration, SystemTime, UNIX_EPOCH};

use navori_kernel::{ActionError, ModuleCtx, Request, Wake};
use navori_redact::redact_text;
use serde::Serialize;
use time::format_description::FormatItem;
use time::{Date, OffsetDateTime, UtcOffset};

use crate::Todo;
use crate::action::TodoAction;
use crate::state::{TodoState, TodoStatus};
use crate::stored;

const DUE_ON_FORMAT: &[FormatItem<'static>] =
    time::macros::format_description!("[year]-[month]-[day]");

#[derive(Debug, Clone, Serialize)]
pub struct AddReply {
    pub id: i64,
}

pub async fn run(module: &mut Todo, ctx: &mut ModuleCtx<Todo>) -> anyhow::Result<()> {
    stored::migrate(&ctx.store)?;
    reload_state(module, ctx)?;

    let mut wait = ctx.wait_handle();
    loop {
        let deadline = next_local_midnight(wait.now(), module.calendar.utc_offset(wait.now()));
        tokio::select! {
            req = ctx.recv() => {
                let Some(req) = req else {
                    return Ok(());
                };
                handle(module, ctx, req);
            }
            wake = wait.wait_until(deadline) => {
                match wake {
                    Wake::Due | Wake::PanelChanged => {
                        if let Err(err) = refresh_if_changed(module, ctx) {
                            tracing::warn!(error = %err, "todo day refresh failed");
                        }
                    }
                }
            }
        }
    }
}

fn handle(module: &Todo, ctx: &ModuleCtx<Todo>, req: Request<TodoAction>) {
    match &req.action {
        TodoAction::Add { title, due_on } => match add(module, ctx, title, due_on.as_deref()) {
            Ok(reply) => {
                if let Err(err) = reload_state(module, ctx) {
                    req.respond::<AddReply>(Err(ActionError::Failed(err.to_string())));
                    return;
                }
                req.respond(Ok(reply));
            }
            Err(err) => req.respond::<AddReply>(Err(err)),
        },
    }
}

fn add(
    module: &Todo,
    ctx: &ModuleCtx<Todo>,
    title: &str,
    due_on: Option<&str>,
) -> Result<AddReply, ActionError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(ActionError::Invalid(
            "title must not be empty or only whitespace".into(),
        ));
    }
    let due_on = match due_on {
        None => None,
        Some(raw) => Some(parse_due_on(raw.trim())?),
    };

    let now = ctx.wait_handle().now();
    let created_at_ms = millis(now)?;
    let _ = local_today(now, module.calendar.utc_offset(now))?;

    let title = redact_text(title).text;
    let id = stored::insert_open(&ctx.store, &title, due_on.as_deref(), created_at_ms)
        .map_err(ActionError::Failed)?;
    Ok(AddReply { id })
}

fn parse_due_on(raw: &str) -> Result<String, ActionError> {
    if raw.len() != 10 || raw.as_bytes()[4] != b'-' || raw.as_bytes()[7] != b'-' {
        return Err(ActionError::Invalid("due_on must be YYYY-MM-DD".into()));
    }
    for (i, b) in raw.bytes().enumerate() {
        if i == 4 || i == 7 {
            continue;
        }
        if !b.is_ascii_digit() {
            return Err(ActionError::Invalid("due_on must be YYYY-MM-DD".into()));
        }
    }
    Date::parse(raw, &DUE_ON_FORMAT)
        .map_err(|_| ActionError::Invalid("due_on must be YYYY-MM-DD".into()))?;
    Ok(raw.to_owned())
}

fn reload_state(module: &Todo, ctx: &ModuleCtx<Todo>) -> anyhow::Result<()> {
    let now = ctx.wait_handle().now();
    let offset = module.calendar.utc_offset(now);
    let today = local_today(now, offset).map_err(|err| anyhow::anyhow!("{err}"))?;
    let rows = stored::load_open(&ctx.store).map_err(|err| anyhow::anyhow!("{err}"))?;
    let open = stored::into_open_todos(rows, |due| status_for(due, &today));
    ctx.update_state(|state| {
        state.today = today;
        state.open = open;
    });
    Ok(())
}

fn refresh_if_changed(module: &Todo, ctx: &ModuleCtx<Todo>) -> anyhow::Result<()> {
    let now = ctx.wait_handle().now();
    let offset = module.calendar.utc_offset(now);
    let today = local_today(now, offset).map_err(|err| anyhow::anyhow!("{err}"))?;
    let rows = stored::load_open(&ctx.store).map_err(|err| anyhow::anyhow!("{err}"))?;
    let open = stored::into_open_todos(rows, |due| status_for(due, &today));
    let current = ctx.state();
    if current.today == today && current.open == open {
        return Ok(());
    }
    ctx.update_state(|state| {
        *state = TodoState { today, open };
    });
    Ok(())
}

fn status_for(due_on: Option<&str>, today: &str) -> TodoStatus {
    let Some(due_on) = due_on else {
        return TodoStatus::None;
    };
    let Ok(due) = Date::parse(due_on, &DUE_ON_FORMAT) else {
        return TodoStatus::None;
    };
    let Ok(today) = Date::parse(today, &DUE_ON_FORMAT) else {
        return TodoStatus::None;
    };
    let days = (due - today).whole_days();
    if days < 0 {
        TodoStatus::Overdue
    } else if days == 0 {
        TodoStatus::Today
    } else if days <= 3 {
        TodoStatus::Soon
    } else {
        TodoStatus::Later
    }
}

fn local_today(now: SystemTime, offset_secs: i32) -> Result<String, ActionError> {
    let odt = to_offset_datetime(now, offset_secs)?;
    odt.date()
        .format(&DUE_ON_FORMAT)
        .map_err(|err| ActionError::Failed(err.to_string()))
}

fn next_local_midnight(now: SystemTime, offset_secs: i32) -> Option<SystemTime> {
    let odt = to_offset_datetime(now, offset_secs).ok()?;
    let tomorrow = odt.date().next_day()?;
    let midnight = tomorrow.with_hms(0, 0, 0).ok()?.assume_offset(odt.offset());
    let secs = midnight.unix_timestamp();
    let Ok(secs) = u64::try_from(secs) else {
        return None;
    };
    Some(UNIX_EPOCH + Duration::new(secs, midnight.nanosecond()))
}

fn to_offset_datetime(now: SystemTime, offset_secs: i32) -> Result<OffsetDateTime, ActionError> {
    let duration = now
        .duration_since(UNIX_EPOCH)
        .map_err(|err| ActionError::Failed(err.to_string()))?;
    let secs =
        i64::try_from(duration.as_secs()).map_err(|err| ActionError::Failed(err.to_string()))?;
    let odt = OffsetDateTime::from_unix_timestamp(secs)
        .map_err(|err| ActionError::Failed(err.to_string()))?
        .replace_nanosecond(duration.subsec_nanos())
        .map_err(|err| ActionError::Failed(err.to_string()))?;
    let offset = UtcOffset::from_whole_seconds(offset_secs)
        .map_err(|err| ActionError::Failed(err.to_string()))?;
    Ok(odt.to_offset(offset))
}

fn millis(now: SystemTime) -> Result<i64, ActionError> {
    let ms = now
        .duration_since(UNIX_EPOCH)
        .map_err(|err| ActionError::Failed(err.to_string()))?
        .as_millis();
    i64::try_from(ms).map_err(|err| ActionError::Failed(err.to_string()))
}
