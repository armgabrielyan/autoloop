use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::json;

use crate::cli::{EvalArgs, OutputFormat};
use crate::config::{Config, GuardrailConfig, GuardrailKind};
use crate::eval::confidence::confidence_score;
use crate::eval::guardrails::{parse_threshold, passes_threshold};
use crate::eval::{
    MetricCommandSpec, RuntimeFailure, assert_no_pending_eval, compile_regex, delta_from_baseline,
    derive_verdict, run_metric_command_with_retries, run_raw_command_capture,
};
use crate::experiments::{ExperimentRecord, ExperimentStatus, append_record, metric_observations};
use crate::git::{capture_working_tree, derive_experiment_worktree};
use crate::output::emit;
use crate::state::{
    EvalVerdict, GuardrailBaseline, GuardrailOutcome, LastEvalState, PendingEval, State,
    write_session_markdown,
};
use crate::ui::{Spinner, TableRow, Tone, banner, join_blocks, render_list, render_table};

pub fn run(args: EvalArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    let Some(mut state) = State::load_optional(&root)? else {
        bail!("autoloop is not initialized in this directory; run `autoloop init` first");
    };
    let Some(baseline) = state.baseline.clone() else {
        bail!("baseline is not recorded; run `autoloop baseline` first");
    };
    let config = Config::load(&root)?;
    let mut last_eval = LastEvalState::load_or_default(&root)?;
    assert_no_pending_eval(config.strictness, last_eval.pending_eval.is_some())?;

    let command = args.command.unwrap_or_else(|| config.eval.command.clone());
    let observations = metric_observations(&root, &config.metric.name)?;
    let spinner = Spinner::new("Running evaluation");
    let metric_regex = compile_regex(config.eval.regex.as_deref())?;

    let metric_spec = MetricCommandSpec {
        command: &command,
        retries: config.eval.retries,
        timeout_secs: config.eval.timeout,
        format: config.eval.format,
        regex: metric_regex.as_ref(),
        metric_name: &config.metric.name,
        unit: config.metric.unit.as_deref(),
    };
    let metric = match run_metric_command_with_retries(&metric_spec, &root) {
        Ok(metric) => metric,
        Err(failure) => {
            spinner.finish();
            return log_crash(&root, &mut state, &mut last_eval, failure, Vec::new());
        }
    };

    let mut guardrails = Vec::new();
    for guardrail in &config.guardrails {
        match evaluate_guardrail(guardrail, &config, &root, &state.baseline_guardrails) {
            Ok(outcome) => guardrails.push(outcome),
            Err(GuardrailEvaluationError::Runtime(failure)) => {
                spinner.finish();
                return log_crash(&root, &mut state, &mut last_eval, failure, guardrails);
            }
            Err(GuardrailEvaluationError::Config(error)) => {
                spinner.finish();
                return Err(error);
            }
        }
    }

    let delta = delta_from_baseline(baseline.value, metric.metric.value);
    let confidence = confidence_score(delta, &observations, config.confidence.min_experiments);
    let verdict = derive_verdict(
        config.metric.direction,
        baseline.value,
        metric.metric.value,
        confidence,
        config.confidence.keep_threshold,
        guardrails.iter().all(|guardrail| guardrail.passed),
    );
    let snapshot = capture_working_tree(&root)?;
    let experiment_worktree =
        derive_experiment_worktree(last_eval.prepared_experiment.as_ref(), &snapshot);

    let pending_eval = PendingEval {
        metric: metric.metric.clone(),
        delta_from_baseline: delta,
        confidence,
        verdict,
        command: metric.command.clone(),
        guardrails: guardrails.clone(),
        diff_fingerprint: snapshot.fingerprint,
        worktree: experiment_worktree,
    };
    last_eval.prepared_experiment = None;
    last_eval.pending_eval = Some(pending_eval.clone());
    last_eval.save(&root)?;
    spinner.finish();

    let payload = json!({
        "metric": {
            "name": metric.metric.name,
            "value": metric.metric.value,
            "unit": metric.metric.unit,
        },
        "delta_from_baseline": delta,
        "confidence": confidence,
        "verdict": verdict,
        "guardrails": guardrails,
        "pending_eval": pending_eval,
    });
    let guardrail_lines: Vec<String> = pending_eval
        .guardrails
        .iter()
        .map(render_guardrail_line)
        .collect();
    let table = render_table(&[
        TableRow::new("Workspace", root.display().to_string()),
        TableRow::new(
            "Metric",
            render_metric(
                &metric.metric.name,
                metric.metric.value,
                metric.metric.unit.as_deref(),
            ),
        ),
        TableRow::new("Delta from baseline", format!("{delta:+}")),
        TableRow::new("Confidence", render_confidence(confidence)),
        TableRow::new("Verdict", render_verdict(verdict)),
        TableRow::new("Pending eval", "recorded"),
    ]);
    let mut blocks = vec![banner(verdict_tone(verdict), "Evaluation complete"), table];
    if let Some(guardrail_block) = render_list("Guardrails", &guardrail_lines) {
        blocks.push(guardrail_block);
    }

    emit(output, join_blocks(blocks), &payload)
}

enum GuardrailEvaluationError {
    Runtime(RuntimeFailure),
    Config(anyhow::Error),
}

fn evaluate_guardrail(
    guardrail: &GuardrailConfig,
    config: &Config,
    root: &Path,
    baseline_guardrails: &[GuardrailBaseline],
) -> std::result::Result<GuardrailOutcome, GuardrailEvaluationError> {
    match guardrail.kind {
        GuardrailKind::PassFail => {
            let capture = run_raw_command_capture(&guardrail.command, config.eval.timeout, root)
                .map_err(GuardrailEvaluationError::Runtime)?;
            let passed = capture.exit_code == Some(0);
            let details = match capture.exit_code {
                Some(code) => format!("exit code {code}"),
                None => "command exited without a code".to_string(),
            };
            Ok(GuardrailOutcome {
                name: guardrail.name.clone(),
                kind: guardrail.kind,
                passed,
                value: None,
                baseline: None,
                threshold: None,
                details: Some(details),
                command: capture,
            })
        }
        GuardrailKind::Metric => {
            let Some(baseline_guardrail) = baseline_guardrails
                .iter()
                .find(|entry| entry.name == guardrail.name)
            else {
                return Err(GuardrailEvaluationError::Config(anyhow!(
                    "baseline is missing metric guardrail `{}`; rerun `autoloop baseline`",
                    guardrail.name
                )));
            };

            let threshold_spec = guardrail.threshold.clone().ok_or_else(|| {
                GuardrailEvaluationError::Config(anyhow!(
                    "metric guardrail `{}` requires a threshold",
                    guardrail.name
                ))
            })?;
            let threshold =
                parse_threshold(&threshold_spec).map_err(GuardrailEvaluationError::Config)?;
            let regex = compile_regex(guardrail.regex.as_deref())
                .map_err(GuardrailEvaluationError::Config)?;
            let metric_spec = MetricCommandSpec {
                command: &guardrail.command,
                retries: 0,
                timeout_secs: config.eval.timeout,
                format: guardrail.format,
                regex: regex.as_ref(),
                metric_name: &guardrail.name,
                unit: None,
            };
            let metric = run_metric_command_with_retries(&metric_spec, root)
                .map_err(GuardrailEvaluationError::Runtime)?;
            let passed = passes_threshold(metric.metric.value, baseline_guardrail.value, threshold)
                .map_err(GuardrailEvaluationError::Config)?;

            Ok(GuardrailOutcome {
                name: guardrail.name.clone(),
                kind: guardrail.kind,
                passed,
                value: Some(metric.metric.value),
                baseline: Some(baseline_guardrail.value),
                threshold: Some(threshold_spec),
                details: Some(format!(
                    "baseline {}, current {}",
                    baseline_guardrail.value, metric.metric.value
                )),
                command: metric.command,
            })
        }
    }
}

fn log_crash(
    root: &Path,
    state: &mut State,
    last_eval: &mut LastEvalState,
    failure: RuntimeFailure,
    guardrails: Vec<GuardrailOutcome>,
) -> Result<()> {
    last_eval.pending_eval = None;
    last_eval.prepared_experiment = None;
    last_eval.save(root)?;

    let record = ExperimentRecord {
        id: state.next_experiment_id,
        session_id: state
            .active_session
            .as_ref()
            .map(|session| session.id.clone()),
        timestamp: chrono::Utc::now(),
        status: ExperimentStatus::Crashed,
        description: Some("eval".to_string()),
        reason: Some(failure.message.clone()),
        metric: None,
        confidence: None,
        verdict: None,
        guardrails,
        command: Some(failure.command.clone()),
        tags: None,
        diff_summary: None,
        diff: None,
        commit_hash: None,
    };
    append_record(root, &record)?;
    state.next_experiment_id += 1;
    state.save(root)?;
    write_session_markdown(root, state)?;

    bail!(
        "{}; crash logged as experiment {}",
        failure.message,
        record.id
    );
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

fn verdict_tone(verdict: EvalVerdict) -> Tone {
    match verdict {
        EvalVerdict::Keep => Tone::Success,
        EvalVerdict::Discard => Tone::Warning,
        EvalVerdict::Rerun => Tone::Info,
    }
}

fn render_guardrail_line(guardrail: &GuardrailOutcome) -> String {
    match guardrail.kind {
        GuardrailKind::PassFail => format!(
            "{}: {} ({})",
            guardrail.name,
            if guardrail.passed { "PASS" } else { "FAIL" },
            guardrail
                .details
                .as_deref()
                .unwrap_or("no additional details"),
        ),
        GuardrailKind::Metric => format!(
            "{}: {} (baseline {}, current {}, threshold {})",
            guardrail.name,
            if guardrail.passed { "PASS" } else { "FAIL" },
            guardrail
                .baseline
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            guardrail
                .value
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            guardrail.threshold.as_deref().unwrap_or("n/a"),
        ),
    }
}
