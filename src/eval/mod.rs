pub mod confidence;
pub mod formats;
pub mod guardrails;

use std::path::Path;

use anyhow::{Result, bail};
use chrono::Utc;
use regex::Regex;

use crate::config::{MetricDirection, Strictness};
use crate::shell::{CommandOutput, run_shell_command};
use crate::state::{CommandCapture, EvalVerdict, GuardrailOutcome, MetricSnapshot};

#[derive(Debug, Clone)]
pub struct MetricExecution {
    pub metric: MetricSnapshot,
    pub command: CommandCapture,
}

#[derive(Debug, Clone)]
pub struct CompletedEvaluation {
    pub metric: MetricSnapshot,
    pub delta_from_baseline: f64,
    pub confidence: Option<f64>,
    pub verdict: EvalVerdict,
    pub command: CommandCapture,
    pub guardrails: Vec<GuardrailOutcome>,
}

#[derive(Debug, Clone)]
pub struct RuntimeFailure {
    pub message: String,
    pub command: CommandCapture,
}

pub fn compile_regex(pattern: Option<&str>) -> Result<Option<Regex>> {
    match pattern {
        Some(pattern) => Ok(Some(Regex::new(pattern)?)),
        None => Ok(None),
    }
}

pub fn run_metric_command(
    command: &str,
    timeout_secs: u64,
    format: formats::MetricFormat,
    regex: Option<&Regex>,
    metric_name: &str,
    unit: Option<&str>,
    cwd: &Path,
) -> std::result::Result<MetricExecution, RuntimeFailure> {
    let capture = run_command_capture(command, timeout_secs, cwd)?;
    let parse_source = combined_output(&capture);
    let value = formats::parse_metric_value(format, &parse_source, metric_name, regex).map_err(
        |error| RuntimeFailure {
            message: format!("failed to parse metric from `{command}`: {error}"),
            command: capture.clone(),
        },
    )?;

    Ok(MetricExecution {
        metric: MetricSnapshot {
            name: metric_name.to_string(),
            value,
            unit: unit.map(str::to_string),
            recorded_at: Utc::now(),
        },
        command: capture,
    })
}

pub fn run_metric_command_with_retries(
    command: &str,
    retries: u32,
    timeout_secs: u64,
    format: formats::MetricFormat,
    regex: Option<&Regex>,
    metric_name: &str,
    unit: Option<&str>,
    cwd: &Path,
) -> std::result::Result<MetricExecution, RuntimeFailure> {
    let attempts = retries + 1;
    let mut last_failure = None;

    for _ in 0..attempts {
        match run_metric_command(command, timeout_secs, format, regex, metric_name, unit, cwd) {
            Ok(metric) => return Ok(metric),
            Err(failure) => last_failure = Some(failure),
        }
    }

    Err(last_failure.expect("at least one attempt should have been made"))
}

pub fn run_command_capture(
    command: &str,
    timeout_secs: u64,
    cwd: &Path,
) -> std::result::Result<CommandCapture, RuntimeFailure> {
    let capture = run_raw_command_capture(command, timeout_secs, cwd)?;

    if capture.exit_code != Some(0) {
        return Err(RuntimeFailure {
            message: format!(
                "`{command}` exited with status {}",
                capture
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
            command: capture,
        });
    }

    Ok(capture)
}

pub fn run_raw_command_capture(
    command: &str,
    timeout_secs: u64,
    cwd: &Path,
) -> std::result::Result<CommandCapture, RuntimeFailure> {
    let output = match run_shell_command(command, cwd, timeout_secs) {
        Ok(output) => output,
        Err(error) => {
            let capture = CommandCapture {
                command: command.to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: error.to_string(),
                timed_out: false,
            };
            return Err(RuntimeFailure {
                message: format!("failed to execute `{command}`"),
                command: capture,
            });
        }
    };

    raw_capture_from_output(command, timeout_secs, output)
}

pub fn delta_from_baseline(baseline: f64, current: f64) -> f64 {
    current - baseline
}

pub fn is_improved(direction: MetricDirection, baseline: f64, current: f64) -> bool {
    match direction {
        MetricDirection::Lower => current < baseline,
        MetricDirection::Higher => current > baseline,
    }
}

pub fn derive_verdict(
    direction: MetricDirection,
    baseline: f64,
    current: f64,
    confidence: Option<f64>,
    keep_threshold: f64,
    guardrails_passed: bool,
) -> EvalVerdict {
    if !guardrails_passed {
        return EvalVerdict::Discard;
    }

    if !is_improved(direction, baseline, current) {
        return EvalVerdict::Discard;
    }

    match confidence {
        Some(value) if value >= keep_threshold => EvalVerdict::Keep,
        _ => EvalVerdict::Rerun,
    }
}

pub fn assert_no_pending_eval(strictness: Strictness, has_pending_eval: bool) -> Result<()> {
    if has_pending_eval {
        bail!(
            "a pending eval already exists; resolve it before running another eval{}",
            match strictness {
                Strictness::Advisory => "",
                Strictness::Strict => " in strict mode",
            }
        );
    }

    Ok(())
}

fn raw_capture_from_output(
    command: &str,
    timeout_secs: u64,
    output: CommandOutput,
) -> std::result::Result<CommandCapture, RuntimeFailure> {
    let capture = CommandCapture {
        command: output.command,
        exit_code: output.exit_code,
        stdout: output.stdout,
        stderr: output.stderr,
        timed_out: output.timed_out,
    };

    if capture.timed_out {
        return Err(RuntimeFailure {
            message: format!("`{command}` timed out after {}s", timeout_secs),
            command: capture,
        });
    }

    Ok(capture)
}

fn combined_output(capture: &CommandCapture) -> String {
    if capture.stdout.is_empty() {
        capture.stderr.clone()
    } else if capture.stderr.is_empty() {
        capture.stdout.clone()
    } else {
        format!("{}\n{}", capture.stdout, capture.stderr)
    }
}
