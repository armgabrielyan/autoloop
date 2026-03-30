use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::cli::{OutputFormat, StatusArgs};
use crate::config::Config;
use crate::experiments::{CategoryRate, analyze_records, summarize_records};
use crate::output::emit;
use crate::state::{EvalVerdict, LastEvalState, State};
use crate::ui::{TableRow, Tone, banner, join_blocks, render_list, render_steps, render_table};

pub fn run(args: StatusArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    let Some(state) = State::load_optional(&root)? else {
        bail!("autoloop is not initialized in this directory; run `autoloop init` first");
    };
    let config = Config::load(&root)?;
    let last_eval = LastEvalState::load_or_default(&root)?;
    let record_summary = summarize_records(&root)?;

    let scope_session_id = if args.all {
        None
    } else {
        state
            .active_session
            .as_ref()
            .map(|session| session.id.as_str())
    };
    let analysis = analyze_records(&root, scope_session_id, config.metric.direction)?;

    let baseline = state.baseline.as_ref().map(|metric| {
        json!({
            "name": metric.name,
            "value": metric.value,
            "unit": metric.unit,
            "recorded_at": metric.recorded_at,
        })
    });
    let scope_label = if args.all {
        "all experiments".to_string()
    } else {
        state
            .active_session
            .as_ref()
            .map(|session| {
                format!(
                    "session {}",
                    session.name.as_deref().unwrap_or(session.id.as_str())
                )
            })
            .unwrap_or_else(|| "all experiments".to_string())
    };

    let payload = json!({
        "initialized": true,
        "scope": {
            "all": args.all || state.active_session.is_none(),
            "session_id": scope_session_id,
            "label": scope_label,
        },
        "active_session": state.active_session,
        "baseline": baseline,
        "next_experiment_id": state.next_experiment_id,
        "pending_eval": last_eval.pending_eval,
        "records": record_summary,
        "analysis": analysis,
    });

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
            EvalVerdict::Keep => "KEEP",
            EvalVerdict::Discard => "DISCARD",
            EvalVerdict::Rerun => "RERUN",
        })
        .unwrap_or("none");

    let mut rows = vec![
        TableRow::new("Workspace", root.display().to_string()),
        TableRow::new("Scope", scope_label),
        TableRow::new("Baseline", baseline_label),
        TableRow::new("Pending eval", pending_eval_label),
        TableRow::new("Experiments", analysis.experiments_run.to_string()),
        TableRow::new("Kept", analysis.kept.to_string()),
        TableRow::new("Discarded", analysis.discarded.to_string()),
        TableRow::new("Crashed", analysis.crashed.to_string()),
        TableRow::new(
            "Current streak",
            render_streak(analysis.current_streak.as_ref()),
        ),
        TableRow::new(
            "Best improvement",
            render_best_improvement(analysis.best_improvement.as_ref()),
        ),
        TableRow::new(
            "Cumulative improvement",
            render_percent(analysis.cumulative_improvement),
        ),
        TableRow::new("Next experiment ID", state.next_experiment_id.to_string()),
    ];
    if let Some(session) = &state.active_session {
        rows.insert(
            2,
            TableRow::new(
                "Active session",
                session
                    .name
                    .as_deref()
                    .unwrap_or(session.id.as_str())
                    .to_string(),
            ),
        );
    }

    let mut blocks = vec![banner(Tone::Info, "Autoloop status"), render_table(&rows)];
    if let Some(category_block) = render_list(
        "Category Success Rates",
        &analysis
            .category_rates
            .iter()
            .map(render_category_rate)
            .collect::<Vec<_>>(),
    ) {
        blocks.push(category_block);
    }

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
            EvalVerdict::Keep => "Run `autoloop keep --description \"...\"` to record the result",
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

fn render_streak(streak: Option<&crate::experiments::StreakSummary>) -> String {
    match streak {
        Some(streak) => match streak.kind {
            crate::experiments::StreakKind::Keep => format!("{} consecutive keeps", streak.count),
            crate::experiments::StreakKind::Failure => {
                format!("{} consecutive failures", streak.count)
            }
        },
        None => "none".to_string(),
    }
}

fn render_best_improvement(best: Option<&crate::experiments::BestImprovement>) -> String {
    match best {
        Some(best) => match best.percent_from_baseline {
            Some(percent) => format!("{percent:+.1}% (experiment #{})", best.experiment_id),
            None => format!(
                "{}{}/baseline (experiment #{})",
                best.metric_value,
                best.unit.as_deref().unwrap_or(""),
                best.experiment_id
            ),
        },
        None => "none".to_string(),
    }
}

fn render_percent(percent: Option<f64>) -> String {
    match percent {
        Some(percent) => format!("{percent:+.1}%"),
        None => "none".to_string(),
    }
}

fn render_category_rate(category: &CategoryRate) -> String {
    let total = category.kept + category.discarded;
    format!(
        "{}: {:.0}% ({}/{})",
        category.name,
        category.success_rate * 100.0,
        category.kept,
        total,
    )
}
