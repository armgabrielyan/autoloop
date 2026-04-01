use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::json;

use crate::cli::{BaselineArgs, OutputFormat};
use crate::config::{Config, GuardrailKind};
use crate::eval::{
    MetricCommandSpec, compile_regex, run_command_capture, run_metric_command_with_retries,
};
use crate::experiments::{ExperimentRecord, ExperimentStatus, MetricRecord, append_record};
use crate::output::emit;
use crate::state::{
    GuardrailBaseline, GuardrailOutcome, LastEvalState, State, write_session_markdown,
};
use crate::ui::{
    Spinner, TableRow, Tone, banner, join_blocks, render_list, render_steps, render_table,
};

pub fn run(_args: BaselineArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    let Some(mut state) = State::load_optional(&root)? else {
        bail!("autoloop is not initialized in this directory; run `autoloop init` first");
    };
    let config = Config::load(&root)?;

    let spinner = Spinner::new("Recording baseline");
    let metric_regex = compile_regex(config.eval.regex.as_deref())?;
    let metric_spec = MetricCommandSpec {
        command: &config.eval.command,
        retries: config.eval.retries,
        timeout_secs: config.eval.timeout,
        format: config.eval.format,
        regex: metric_regex.as_ref(),
        metric_name: &config.metric.name,
        unit: config.metric.unit.as_deref(),
    };
    let metric = run_metric_command_with_retries(&metric_spec, &root)
        .map_err(|failure| anyhow!(failure.message))?;

    let guardrails = collect_baseline_guardrails(&config, &root)?;
    let baseline_guardrails: Vec<GuardrailBaseline> = guardrails
        .iter()
        .filter_map(|guardrail| {
            guardrail.value.map(|value| GuardrailBaseline {
                name: guardrail.name.clone(),
                value,
            })
        })
        .collect();

    let baseline_value = metric.metric.value;
    let baseline_unit = metric.metric.unit.clone();
    let baseline_name = metric.metric.name.clone();

    let record = ExperimentRecord {
        id: state.next_experiment_id,
        session_id: state
            .active_session
            .as_ref()
            .map(|session| session.id.clone()),
        timestamp: metric.metric.recorded_at,
        status: ExperimentStatus::Baseline,
        description: Some("baseline".to_string()),
        reason: None,
        metric: Some(MetricRecord {
            name: baseline_name.clone(),
            value: baseline_value,
            unit: baseline_unit.clone(),
            baseline: Some(baseline_value),
            delta_from_baseline: Some(0.0),
        }),
        confidence: None,
        verdict: None,
        guardrails: guardrails.clone(),
        command: Some(metric.command.clone()),
        tags: None,
        diff_summary: None,
        diff: None,
        commit_hash: None,
    };

    append_record(&root, &record)?;

    state.baseline = Some(metric.metric.clone());
    state.baseline_guardrails = baseline_guardrails;
    state.next_experiment_id += 1;
    state.save(&root)?;
    LastEvalState::default().save(&root)?;
    write_session_markdown(&root, &state)?;
    spinner.finish();

    let guardrail_lines: Vec<String> = guardrails.iter().map(render_guardrail_line).collect();
    let payload = json!({
        "metric": {
            "name": baseline_name,
            "value": baseline_value,
            "unit": baseline_unit,
        },
        "guardrails": guardrails,
        "experiment_id": record.id,
        "session_id": record.session_id,
    });

    let table = render_table(&[
        TableRow::new("Workspace", root.display().to_string()),
        TableRow::new(
            "Metric",
            render_metric(
                &config.metric.name,
                baseline_value,
                baseline_unit.as_deref(),
            ),
        ),
        TableRow::new(
            "Session",
            record.session_id.as_deref().unwrap_or("none").to_string(),
        ),
        TableRow::new("Experiment ID", record.id.to_string()),
    ]);

    let mut blocks = vec![banner(Tone::Success, "Recorded baseline"), table];
    if let Some(guardrail_block) = render_list("Guardrails", &guardrail_lines) {
        blocks.push(guardrail_block);
    }
    if let Some(next_block) = render_steps(
        "Next",
        &["Run `autoloop eval` after making a candidate change".to_string()],
    ) {
        blocks.push(next_block);
    }

    emit(output, join_blocks(blocks), &payload)
}

fn collect_baseline_guardrails(config: &Config, root: &Path) -> Result<Vec<GuardrailOutcome>> {
    let mut outcomes = Vec::new();

    for guardrail in &config.guardrails {
        match guardrail.kind {
            GuardrailKind::PassFail => {
                let capture = run_command_capture(&guardrail.command, config.eval.timeout, root)
                    .map_err(|failure| anyhow!(failure.message))?;
                outcomes.push(GuardrailOutcome {
                    name: guardrail.name.clone(),
                    kind: guardrail.kind,
                    passed: true,
                    value: None,
                    baseline: None,
                    threshold: None,
                    details: Some("baseline pass/fail guardrail passed".to_string()),
                    command: capture,
                });
            }
            GuardrailKind::Metric => {
                let regex = compile_regex(guardrail.regex.as_deref())?;
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
                    .map_err(|failure| anyhow!(failure.message))?;
                outcomes.push(GuardrailOutcome {
                    name: guardrail.name.clone(),
                    kind: guardrail.kind,
                    passed: true,
                    value: Some(metric.metric.value),
                    baseline: None,
                    threshold: guardrail.threshold.clone(),
                    details: Some("baseline guardrail recorded".to_string()),
                    command: metric.command,
                });
            }
        }
    }

    Ok(outcomes)
}

fn render_metric(name: &str, value: f64, unit: Option<&str>) -> String {
    match unit {
        Some(unit) => format!("{name}={value}{unit}"),
        None => format!("{name}={value}"),
    }
}

fn render_guardrail_line(guardrail: &GuardrailOutcome) -> String {
    match guardrail.value {
        Some(value) => format!("{} = {}", guardrail.name, value),
        None => format!("{} passed", guardrail.name),
    }
}
