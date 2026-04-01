use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::cli::{OutputFormat, PreArgs};
use crate::experiments::{
    CategorySignal, PreflightReport, PreflightVerdict, QuerySource, SimilarExperiment,
    preflight_report,
};
use crate::git::{capture_working_tree, recorded_worktree_from_snapshot};
use crate::output::emit;
use crate::state::{LastEvalState, PreparedExperiment, State};
use crate::tags::{
    derive_categories, derive_paths_from_description, derive_terms_from_description,
};
use crate::ui::{TableRow, Tone, banner, join_blocks, render_list, render_steps, render_table};

pub fn run(args: PreArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    if State::load_optional(&root)?.is_none() {
        bail!("autoloop is not initialized in this directory; run `autoloop init` first");
    }

    let snapshot = capture_working_tree(&root)?;
    let prepared_worktree = recorded_worktree_from_snapshot(&snapshot);
    let (source, file_paths, categories) = if snapshot.has_changes {
        (
            QuerySource::WorkingTree,
            snapshot.file_paths.clone(),
            snapshot.auto_categories.clone(),
        )
    } else {
        let paths: Vec<String> = derive_paths_from_description(&args.description)
            .into_iter()
            .collect();
        let mut categories = derive_categories(paths.iter().map(String::as_str));
        categories.extend(derive_terms_from_description(&args.description));
        (
            QuerySource::Description,
            paths,
            categories.into_iter().collect(),
        )
    };

    let report = preflight_report(&root, &args.description, source, &file_paths, &categories)?;
    let mut last_eval = LastEvalState::load_or_default(&root)?;
    last_eval.prepared_experiment = Some(PreparedExperiment {
        description: Some(args.description.clone()),
        worktree: prepared_worktree,
    });
    last_eval.save(&root)?;
    let payload = json!({
        "query": {
            "description": report.description.clone(),
            "source": report.source,
            "file_paths": report.file_paths.clone(),
            "categories": report.categories.clone(),
        },
        "report": &report,
    });

    let mut blocks = vec![
        banner(tone_for_verdict(report.verdict), "Autoloop pre-flight"),
        render_table(&[
            TableRow::new("Workspace", root.display().to_string()),
            TableRow::new("Description", report.description.clone()),
            TableRow::new("Source", render_source(report.source)),
            TableRow::new("Exact matches", report.exact_matches.to_string()),
            TableRow::new(
                "Similar experiments",
                report.similar_experiments.to_string(),
            ),
            TableRow::new("Kept", report.kept.to_string()),
            TableRow::new("Discarded", report.discarded.to_string()),
            TableRow::new("Crashed", report.crashed.to_string()),
            TableRow::new("Verdict", render_verdict(report.verdict)),
        ]),
    ];

    if let Some(file_block) = render_list("Files Likely Affected", &report.file_paths) {
        blocks.push(file_block);
    }
    if let Some(category_block) = render_list("Categories", &report.categories) {
        blocks.push(category_block);
    }
    if let Some(signal_block) = render_list(
        "Category History",
        &report
            .category_signals
            .iter()
            .map(render_category_signal)
            .collect::<Vec<_>>(),
    ) {
        blocks.push(signal_block);
    }
    if let Some(matches_block) = render_list(
        "Similar Experiments",
        &report
            .matches
            .iter()
            .map(render_similar_experiment)
            .collect::<Vec<_>>(),
    ) {
        blocks.push(matches_block);
    }
    if let Some(reason_block) =
        render_list("Assessment", std::slice::from_ref(&report.verdict_reason))
    {
        blocks.push(reason_block);
    }

    let steps = next_steps(&report);
    if let Some(next_block) = render_steps("Next", &steps) {
        blocks.push(next_block);
    }

    emit(output, join_blocks(blocks), &payload)
}

fn render_source(source: QuerySource) -> &'static str {
    match source {
        QuerySource::WorkingTree => "working tree",
        QuerySource::Description => "description",
    }
}

fn render_verdict(verdict: PreflightVerdict) -> &'static str {
    match verdict {
        PreflightVerdict::Proceed => "PROCEED",
        PreflightVerdict::Caution => "CAUTION",
        PreflightVerdict::Avoid => "AVOID",
    }
}

fn tone_for_verdict(verdict: PreflightVerdict) -> Tone {
    match verdict {
        PreflightVerdict::Proceed => Tone::Success,
        PreflightVerdict::Caution => Tone::Warning,
        PreflightVerdict::Avoid => Tone::Error,
    }
}

fn render_category_signal(signal: &CategorySignal) -> String {
    format!(
        "{}: {:.0}% success ({}/{}), sampling {:.2}",
        signal.name,
        signal.success_rate * 100.0,
        signal.kept,
        signal.attempts,
        signal.sampling_probability,
    )
}

fn render_similar_experiment(experiment: &SimilarExperiment) -> String {
    let label = match experiment.status {
        crate::experiments::ExperimentStatus::Kept => "KEPT",
        crate::experiments::ExperimentStatus::Discarded => "DISCARDED",
        crate::experiments::ExperimentStatus::Crashed => "CRASHED",
        crate::experiments::ExperimentStatus::Baseline => "BASELINE",
    };
    let description = experiment
        .description
        .as_deref()
        .unwrap_or("no description provided");
    let delta = experiment
        .percent_from_baseline
        .map(|value| format!("{value:+.1}%"))
        .or_else(|| {
            experiment
                .delta_from_baseline
                .map(|value| format!("{value:+}"))
        })
        .unwrap_or_else(|| "n/a".to_string());

    let mut annotations = BTreeSet::new();
    if experiment.exact_description_match {
        annotations.insert("exact description".to_string());
    }
    if !experiment.shared_file_paths.is_empty() {
        annotations.insert(format!(
            "files: {}",
            experiment.shared_file_paths.join(", ")
        ));
    }
    if !experiment.shared_categories.is_empty() {
        annotations.insert(format!(
            "categories: {}",
            experiment.shared_categories.join(", ")
        ));
    }

    if annotations.is_empty() {
        format!(
            "#{} [{}] {} ({})",
            experiment.experiment_id, label, description, delta
        )
    } else {
        format!(
            "#{} [{}] {} ({}) [{}]",
            experiment.experiment_id,
            label,
            description,
            delta,
            annotations.into_iter().collect::<Vec<_>>().join("; "),
        )
    }
}

fn next_steps(report: &PreflightReport) -> Vec<String> {
    match report.verdict {
        PreflightVerdict::Proceed => vec![
            "Proceed with the candidate change and run `autoloop eval` after it lands".to_string(),
        ],
        PreflightVerdict::Caution => vec![
            "Proceed carefully and validate quickly with `autoloop eval`".to_string(),
            "Run `autoloop learn --all` if you want broader history before continuing".to_string(),
        ],
        PreflightVerdict::Avoid => vec![
            "Try a different category or file area before spending more iterations here"
                .to_string(),
            "Run `autoloop learn --all` to inspect broader history and dead ends".to_string(),
        ],
    }
}
