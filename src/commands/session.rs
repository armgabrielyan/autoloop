use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde_json::json;

use crate::cli::{OutputFormat, SessionEndArgs, SessionStartArgs};
use crate::config::Config;
use crate::experiments::{CategoryRate, analyze_records};
use crate::output::emit;
use crate::state::{SessionState, State, write_session_markdown};
use crate::ui::{TableRow, Tone, banner, join_blocks, render_list, render_steps, render_table};

pub fn start(args: SessionStartArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    let mut state = State::load(&root)?;

    if state.active_session.is_some() {
        bail!("a session is already active; end it before starting a new one");
    }

    let started_at = Utc::now();
    let id = format!(
        "s_{}_{}",
        started_at.format("%Y%m%d_%H%M%S"),
        state.next_session_id
    );
    state.active_session = Some(SessionState {
        id: id.clone(),
        name: args.name.clone(),
        started_at,
    });
    state.next_session_id += 1;
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
    let config = Config::load(&root)?;
    let Some(session) = state.active_session.take() else {
        bail!("no active session to end");
    };

    let ended_at = Utc::now();
    state.save(&root)?;
    write_session_markdown(&root, &state)?;
    let analysis = analyze_records(&root, Some(session.id.as_str()), config.metric.direction)?;

    let payload = json!({
        "session_id": session.id,
        "name": session.name,
        "started_at": session.started_at,
        "ended_at": ended_at,
        "summary": analysis,
        "trigger_learn": true,
    });
    let table = render_table(&[
        TableRow::new("Workspace", root.display().to_string()),
        TableRow::new("Session ID", session.id),
        TableRow::new("Name", session.name.unwrap_or_else(|| "none".to_string())),
        TableRow::new("Started at", session.started_at.to_rfc3339()),
        TableRow::new("Ended at", ended_at.to_rfc3339()),
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
    ]);
    let mut blocks = vec![banner(Tone::Success, "Ended autoloop session"), table];
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
    if let Some(next_block) = render_steps(
        "Next",
        &[
            "Run `autoloop status --all` to inspect cumulative history".to_string(),
            "Run `autoloop learn --session` to extract patterns for this session".to_string(),
        ],
    ) {
        blocks.push(next_block);
    }
    let human = join_blocks(blocks);
    emit(output, human, &payload)
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
