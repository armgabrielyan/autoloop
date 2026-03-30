use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::cli::{OutputFormat, StatusArgs};
use crate::experiments::summarize_records;
use crate::output::emit;
use crate::state::{LastEvalState, State};
use crate::ui::{TableRow, Tone, banner, join_blocks, render_steps, render_table};

pub fn run(_args: StatusArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    let Some(state) = State::load_optional(&root)? else {
        bail!("autoloop is not initialized in this directory; run `autoloop init` first");
    };
    let last_eval = LastEvalState::load_or_default(&root)?;
    let summary = summarize_records(&root)?;

    let baseline = state.baseline.as_ref().map(|metric| {
        json!({
            "name": metric.name,
            "value": metric.value,
            "unit": metric.unit,
            "recorded_at": metric.recorded_at,
        })
    });

    let payload = json!({
        "initialized": true,
        "active_session": state.active_session,
        "baseline": baseline,
        "next_experiment_id": state.next_experiment_id,
        "pending_eval": last_eval.pending_eval,
        "summary": summary,
    });

    let session_label = state
        .active_session
        .as_ref()
        .map(|session| session.name.as_deref().unwrap_or(session.id.as_str()))
        .unwrap_or("none")
        .to_string();
    let baseline_label = state
        .baseline
        .as_ref()
        .map(|metric| match &metric.unit {
            Some(unit) => format!("{}={}{}", metric.name, metric.value, unit),
            None => format!("{}={}", metric.name, metric.value),
        })
        .unwrap_or_else(|| "not recorded".to_string());
    let pending_eval_label = last_eval
        .pending_eval
        .as_ref()
        .map(|pending| match pending.verdict {
            crate::state::EvalVerdict::Keep => "KEEP",
            crate::state::EvalVerdict::Discard => "DISCARD",
            crate::state::EvalVerdict::Rerun => "RERUN",
        })
        .unwrap_or("none");

    let table = render_table(&[
        TableRow::new("Workspace", root.display().to_string()),
        TableRow::new("Active session", session_label),
        TableRow::new("Baseline", baseline_label),
        TableRow::new("Pending eval", pending_eval_label),
        TableRow::new("Experiments logged", summary.total.to_string()),
        TableRow::new("Kept", summary.kept.to_string()),
        TableRow::new("Discarded", summary.discarded.to_string()),
        TableRow::new("Crashed", summary.crashed.to_string()),
        TableRow::new("Next experiment ID", state.next_experiment_id.to_string()),
    ]);

    let mut blocks = vec![banner(Tone::Info, "Autoloop status"), table];
    let mut steps = Vec::new();
    if state.active_session.is_none() {
        steps.push(
            "Run `autoloop session start --name \"first-run\"` to open a session".to_string(),
        );
    }
    if state.baseline.is_none() {
        steps.push("Run `autoloop baseline` once the eval command is configured".to_string());
    }
    if let Some(pending) = &last_eval.pending_eval {
        let command = match pending.verdict {
            crate::state::EvalVerdict::Keep => {
                "Run `autoloop keep --description \"...\"` to record the result"
            }
            _ => {
                "Run `autoloop discard --description \"...\" --reason \"...\"` to close the pending eval"
            }
        };
        steps.push(command.to_string());
    }
    if let Some(next_block) = render_steps("Next", &steps) {
        blocks.push(next_block);
    }

    emit(output, join_blocks(blocks), &payload)
}
