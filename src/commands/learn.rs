use std::fs;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::cli::{LearnArgs, OutputFormat};
use crate::config::Config;
use crate::experiments::{
    DeadEndCategory, FilePattern, RankedExperiment, SessionTrajectory, latest_session_id,
    learn_report, summarize_records,
};
use crate::output::emit;
use crate::state::{State, learnings_path};
use crate::ui::{TableRow, Tone, banner, join_blocks, render_list, render_steps, render_table};

pub fn run(args: LearnArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    let Some(state) = State::load_optional(&root)? else {
        bail!("autoloop is not initialized in this directory; run `autoloop init` first");
    };
    let config = Config::load(&root)?;
    let scope = resolve_scope(&root, &state, &args)?;
    let records = summarize_records(&root)?;
    let report = learn_report(&root, scope.session_id.as_deref(), config.metric.direction)?;
    let learnings_path = learnings_path(&root);
    let learnings_markdown = render_learnings_markdown(&scope, &report);
    fs::write(&learnings_path, learnings_markdown)
        .with_context(|| format!("failed to write {}", learnings_path.display()))?;

    let payload = json!({
        "scope": {
            "all": scope.all,
            "session_id": scope.session_id,
            "label": scope.label,
        },
        "learnings_path": learnings_path,
        "records": records,
        "report": report,
    });

    let mut blocks = vec![
        banner(Tone::Info, "Autoloop learn"),
        render_table(&[
            TableRow::new("Workspace", root.display().to_string()),
            TableRow::new("Scope", scope.label.clone()),
            TableRow::new("Experiments", report.summary.experiments_run.to_string()),
            TableRow::new("Kept", report.summary.kept.to_string()),
            TableRow::new("Discarded", report.summary.discarded.to_string()),
            TableRow::new("Crashed", report.summary.crashed.to_string()),
            TableRow::new(
                "Current streak",
                render_streak(report.summary.current_streak.as_ref()),
            ),
            TableRow::new(
                "Best improvement",
                render_best_improvement(report.summary.best_improvement.as_ref()),
            ),
            TableRow::new("Learnings file", learnings_path.display().to_string()),
            TableRow::new("Sessions seen", report.sessions_seen.to_string()),
            TableRow::new(
                "Dead-end categories",
                report.dead_end_categories.len().to_string(),
            ),
            TableRow::new("File patterns", report.file_patterns.len().to_string()),
        ]),
    ];

    if let Some(best_block) = render_list(
        "Best Experiments",
        &report
            .best_experiments
            .iter()
            .map(render_ranked_experiment)
            .collect::<Vec<_>>(),
    ) {
        blocks.push(best_block);
    }
    if let Some(worst_block) = render_list(
        "Worst Experiments",
        &report
            .worst_experiments
            .iter()
            .map(render_ranked_experiment)
            .collect::<Vec<_>>(),
    ) {
        blocks.push(worst_block);
    }
    if let Some(dead_end_block) = render_list(
        "Dead-end Categories",
        &report
            .dead_end_categories
            .iter()
            .map(render_dead_end)
            .collect::<Vec<_>>(),
    ) {
        blocks.push(dead_end_block);
    }
    if let Some(file_pattern_block) = render_list(
        "Consistent File Patterns",
        &report
            .file_patterns
            .iter()
            .map(render_file_pattern)
            .collect::<Vec<_>>(),
    ) {
        blocks.push(file_pattern_block);
    }
    if let Some(trajectory_block) = render_list(
        "Session Trajectory",
        &report
            .session_trajectory
            .iter()
            .map(render_session_trajectory)
            .collect::<Vec<_>>(),
    ) {
        blocks.push(trajectory_block);
    }

    let mut steps = vec!["Review the refreshed `.autoloop/learnings.md` summary".to_string()];
    if !scope.all {
        steps.push("Run `autoloop learn --all` to inspect cross-session patterns".to_string());
    }
    if let Some(next_block) = render_steps("Next", &steps) {
        blocks.push(next_block);
    }

    emit(output, join_blocks(blocks), &payload)
}

#[derive(Debug, Clone)]
struct LearnScope {
    all: bool,
    session_id: Option<String>,
    label: String,
}

fn resolve_scope(root: &std::path::Path, state: &State, args: &LearnArgs) -> Result<LearnScope> {
    if args.all {
        return Ok(LearnScope {
            all: true,
            session_id: None,
            label: "all experiments".to_string(),
        });
    }

    if let Some(session) = &state.active_session
        && (args.session || !args.all)
    {
        return Ok(LearnScope {
            all: false,
            session_id: Some(session.id.clone()),
            label: format!(
                "session {}",
                session.name.as_deref().unwrap_or(session.id.as_str())
            ),
        });
    }

    if args.session {
        let Some(session_id) = latest_session_id(root)? else {
            bail!("no recorded sessions are available to learn from yet");
        };
        return Ok(LearnScope {
            all: false,
            label: format!("latest session {session_id}"),
            session_id: Some(session_id),
        });
    }

    Ok(LearnScope {
        all: true,
        session_id: None,
        label: "all experiments".to_string(),
    })
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

fn render_ranked_experiment(experiment: &RankedExperiment) -> String {
    let metric = match (
        experiment.metric_name.as_deref(),
        experiment.metric_value,
        experiment.unit.as_deref(),
    ) {
        (Some(name), Some(value), Some(unit)) => format!("{name}={value}{unit}"),
        (Some(name), Some(value), None) => format!("{name}={value}"),
        _ => "metric unavailable".to_string(),
    };
    let delta = experiment
        .percent_from_baseline
        .map(|value| format!("{value:+.1}%"))
        .or_else(|| {
            experiment
                .delta_from_baseline
                .map(|value| format!("{value:+}"))
        })
        .unwrap_or_else(|| "n/a".to_string());
    let description = experiment
        .description
        .as_deref()
        .unwrap_or("no description provided");

    format!(
        "#{} {} | {} | {}",
        experiment.experiment_id, description, metric, delta
    )
}

fn render_dead_end(dead_end: &DeadEndCategory) -> String {
    format!(
        "{}: {} attempts, {} discarded, {} crashed",
        dead_end.name, dead_end.attempts, dead_end.discarded, dead_end.crashed
    )
}

fn render_file_pattern(pattern: &FilePattern) -> String {
    format!(
        "{}: {} over {} attempts ({:.0}% kept)",
        pattern.path,
        pattern.signal,
        pattern.attempts,
        pattern.success_rate * 100.0,
    )
}

fn render_session_trajectory(session: &SessionTrajectory) -> String {
    let label = session.session_id.as_deref().unwrap_or("sessionless");
    let best = session
        .best_improvement
        .map(|value| format!("{value:+.1}% best"))
        .unwrap_or_else(|| "no kept improvement".to_string());
    format!(
        "{}: {} experiments ({} kept, {} discarded, {} crashed), {}",
        label, session.experiments_run, session.kept, session.discarded, session.crashed, best
    )
}

fn render_learnings_markdown(
    scope: &LearnScope,
    report: &crate::experiments::LearnReport,
) -> String {
    let mut lines = vec![
        "# Learnings".to_string(),
        String::new(),
        format!("Scope: {}", scope.label),
        format!(
            "Summary: {} experiments, {} kept, {} discarded, {} crashed",
            report.summary.experiments_run,
            report.summary.kept,
            report.summary.discarded,
            report.summary.crashed
        ),
        String::new(),
        "## What Helped".to_string(),
    ];

    if report.best_experiments.is_empty() {
        lines.push("- No kept improvements were recorded in this scope.".to_string());
    } else {
        for experiment in &report.best_experiments {
            lines.push(format!("- {}", render_ranked_experiment(experiment)));
        }
    }

    lines.push(String::new());
    lines.push("## What Failed".to_string());
    if report.worst_experiments.is_empty() {
        lines.push("- No failed or regressed experiments were recorded in this scope.".to_string());
    } else {
        for experiment in &report.worst_experiments {
            lines.push(format!("- {}", render_ranked_experiment(experiment)));
        }
    }

    lines.push(String::new());
    lines.push("## Watchouts".to_string());
    if report.dead_end_categories.is_empty() && report.file_patterns.is_empty() {
        lines.push("- No repeated dead ends or file-level patterns were detected yet.".to_string());
    } else {
        for dead_end in &report.dead_end_categories {
            lines.push(format!("- {}", render_dead_end(dead_end)));
        }
        for pattern in &report.file_patterns {
            lines.push(format!("- {}", render_file_pattern(pattern)));
        }
    }

    lines.push(String::new());
    lines.push("## Next Ideas".to_string());
    if report.session_trajectory.is_empty() {
        lines.push("- Run more experiments to build reliable learning signals.".to_string());
    } else {
        for trajectory in &report.session_trajectory {
            lines.push(format!("- {}", render_session_trajectory(trajectory)));
        }
    }

    lines.push(String::new());
    lines.join("\n")
}
