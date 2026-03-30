use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use serde_json::json;

use crate::cli::{KeepArgs, OutputFormat};
use crate::config::{Config, Strictness};
use crate::experiments::{
    ExperimentRecord, ExperimentStatus, ExperimentTags, MetricRecord, append_record,
    summarize_records,
};
use crate::git::{WorkingTreeSnapshot, capture_working_tree, commit_all};
use crate::output::emit;
use crate::state::{EvalVerdict, LastEvalState, State, write_session_markdown};
use crate::ui::{TableRow, Tone, banner, join_blocks, render_list, render_steps, render_table};

pub fn run(args: KeepArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    let Some(mut state) = State::load_optional(&root)? else {
        bail!("autoloop is not initialized in this directory; run `autoloop init` first");
    };
    let config = Config::load(&root)?;
    let mut last_eval = LastEvalState::load_or_default(&root)?;
    let pending_eval = last_eval
        .pending_eval
        .clone()
        .ok_or_else(|| anyhow!("no pending eval is recorded; run `autoloop eval` first"))?;

    if matches!(config.strictness, Strictness::Strict)
        && !matches!(pending_eval.verdict, EvalVerdict::Keep)
    {
        bail!(
            "strict mode requires a KEEP verdict before `autoloop keep`; current verdict is {}",
            render_verdict(pending_eval.verdict)
        );
    }

    let snapshot = capture_working_tree(&root)?;
    ensure_matching_snapshot(
        pending_eval.diff_fingerprint.as_deref(),
        snapshot.fingerprint.as_deref(),
    )?;

    let mut git_notes = Vec::new();
    let mut commit_hash = None;
    if args.commit {
        if !config.git.enabled {
            git_notes.push("commit skipped because `[git].enabled` is false".to_string());
        } else if !snapshot.has_changes {
            git_notes.push("commit skipped because the working tree has no changes".to_string());
        } else {
            match commit_all(
                &root,
                &format!("{} {}", config.git.commit_prefix, args.description),
            ) {
                Ok(hash) => commit_hash = Some(hash),
                Err(error) => git_notes.push(format!("commit failed: {error}")),
            }
        }
    }

    let record = ExperimentRecord {
        id: state.next_experiment_id,
        session_id: state
            .active_session
            .as_ref()
            .map(|session| session.id.clone()),
        timestamp: Utc::now(),
        status: ExperimentStatus::Kept,
        description: Some(args.description.clone()),
        reason: None,
        metric: Some(MetricRecord {
            name: pending_eval.metric.name.clone(),
            value: pending_eval.metric.value,
            unit: pending_eval.metric.unit.clone(),
            baseline: state.baseline.as_ref().map(|metric| metric.value),
            delta_from_baseline: Some(pending_eval.delta_from_baseline),
        }),
        confidence: pending_eval.confidence,
        verdict: Some(pending_eval.verdict),
        guardrails: pending_eval.guardrails.clone(),
        command: Some(pending_eval.command.clone()),
        tags: snapshot_tags(&snapshot),
        diff_summary: snapshot.diff_summary.clone(),
        diff: snapshot.diff.clone(),
        commit_hash: commit_hash.clone(),
    };

    append_record(&root, &record)?;
    state.next_experiment_id += 1;
    state.save(&root)?;
    last_eval.pending_eval = None;
    last_eval.save(&root)?;
    write_session_markdown(&root, &state)?;

    let summary = summarize_records(&root)?;
    let payload = json!({
        "status": "kept",
        "record": record,
        "git_notes": git_notes,
        "summary": summary,
    });

    let mut blocks = vec![
        banner(Tone::Success, "Recorded kept experiment"),
        render_table(&[
            TableRow::new("Workspace", root.display().to_string()),
            TableRow::new(
                "Experiment ID",
                state.next_experiment_id.saturating_sub(1).to_string(),
            ),
            TableRow::new(
                "Metric",
                render_metric(
                    &pending_eval.metric.name,
                    pending_eval.metric.value,
                    pending_eval.metric.unit.as_deref(),
                ),
            ),
            TableRow::new("Confidence", render_confidence(pending_eval.confidence)),
            TableRow::new("Verdict", render_verdict(pending_eval.verdict)),
            TableRow::new(
                "Commit",
                commit_hash.unwrap_or_else(|| "not created".to_string()),
            ),
            TableRow::new("Kept experiments", summary.kept.to_string()),
            TableRow::new("Discarded experiments", summary.discarded.to_string()),
            TableRow::new("Crashed experiments", summary.crashed.to_string()),
        ]),
    ];
    if let Some(git_block) = render_list("Git", &git_notes) {
        blocks.push(git_block);
    }
    if let Some(next_block) = render_steps(
        "Next",
        &["Run `autoloop eval` after the next candidate change".to_string()],
    ) {
        blocks.push(next_block);
    }

    emit(output, join_blocks(blocks), &payload)
}

fn ensure_matching_snapshot(expected: Option<&str>, actual: Option<&str>) -> Result<()> {
    if expected != actual {
        bail!(
            "working tree no longer matches the recorded pending eval; rerun `autoloop eval` before finalizing it"
        );
    }

    Ok(())
}

fn snapshot_tags(snapshot: &WorkingTreeSnapshot) -> Option<ExperimentTags> {
    if snapshot.file_paths.is_empty() && snapshot.auto_categories.is_empty() {
        return None;
    }

    Some(ExperimentTags {
        file_paths: snapshot.file_paths.clone(),
        auto_categories: snapshot.auto_categories.clone(),
    })
}

fn render_metric(name: &str, value: f64, unit: Option<&str>) -> String {
    match unit {
        Some(unit) => format!("{name}={value}{unit}"),
        None => format!("{name}={value}"),
    }
}

fn render_confidence(confidence: Option<f64>) -> String {
    match confidence {
        Some(value) => format!("{value:.2}"),
        None => "not enough data".to_string(),
    }
}

fn render_verdict(verdict: EvalVerdict) -> &'static str {
    match verdict {
        EvalVerdict::Keep => "KEEP",
        EvalVerdict::Discard => "DISCARD",
        EvalVerdict::Rerun => "RERUN",
    }
}
