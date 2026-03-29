use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde_json::json;

use crate::cli::{OutputFormat, SessionEndArgs, SessionStartArgs};
use crate::output::emit;
use crate::state::{SessionState, State, write_session_markdown};
use crate::ui::{TableRow, Tone, banner, join_blocks, render_steps, render_table};

pub fn start(args: SessionStartArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    let mut state = State::load(&root)?;

    if state.active_session.is_some() {
        bail!("a session is already active; end it before starting a new one");
    }

    let started_at = Utc::now();
    let id = format!("s_{}", started_at.format("%Y%m%d_%H%M%S"));
    state.active_session = Some(SessionState {
        id: id.clone(),
        name: args.name.clone(),
        started_at,
    });
    state.save(&root)?;
    write_session_markdown(&root, &state)?;

    let payload = json!({
        "session_id": id,
        "name": args.name,
        "started_at": started_at,
    });
    let name = state
        .active_session
        .as_ref()
        .and_then(|session| session.name.as_deref())
        .unwrap_or("none");
    let table = render_table(&[
        TableRow::new("Workspace", root.display().to_string()),
        TableRow::new("Session ID", id.clone()),
        TableRow::new("Name", name),
        TableRow::new("Started at", started_at.to_rfc3339()),
    ]);
    let human = join_blocks(vec![
        banner(Tone::Success, "Started autoloop session"),
        table,
        render_steps(
            "Next",
            &[
                "Run `autoloop status` to confirm the active session".to_string(),
                "Run `autoloop baseline` after the eval command is ready".to_string(),
            ],
        )
        .unwrap_or_default(),
    ]);
    emit(output, human, &payload)
}

pub fn end(_args: SessionEndArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    let mut state = State::load(&root)?;
    let Some(session) = state.active_session.take() else {
        bail!("no active session to end");
    };

    let ended_at = Utc::now();
    state.save(&root)?;
    write_session_markdown(&root, &state)?;

    let payload = json!({
        "session_id": session.id,
        "name": session.name,
        "started_at": session.started_at,
        "ended_at": ended_at,
    });
    let table = render_table(&[
        TableRow::new("Workspace", root.display().to_string()),
        TableRow::new("Session ID", session.id),
        TableRow::new("Name", session.name.unwrap_or_else(|| "none".to_string())),
        TableRow::new("Started at", session.started_at.to_rfc3339()),
        TableRow::new("Ended at", ended_at.to_rfc3339()),
    ]);
    let human = join_blocks(vec![
        banner(Tone::Success, "Ended autoloop session"),
        table,
        render_steps(
            "Next",
            &["Run `autoloop status` to inspect the idle workspace".to_string()],
        )
        .unwrap_or_default(),
    ]);
    emit(output, human, &payload)
}
