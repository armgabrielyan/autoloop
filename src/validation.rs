use std::path::Path;

use serde::Serialize;

use crate::config::{Config, GuardrailConfig, GuardrailKind, default_config};
use crate::eval::guardrails::parse_threshold;
use crate::eval::{
    MetricCommandSpec, RuntimeFailure, compile_regex, run_metric_command_with_retries,
    run_raw_command_capture,
};
use crate::state::{CommandCapture, MetricSnapshot};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    Eval,
    PassFailGuardrail,
    MetricGuardrail,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationCheck {
    pub name: String,
    pub kind: CheckKind,
    pub command: String,
    pub status: CheckStatus,
    pub message: String,
    #[serde(default)]
    pub metric: Option<MetricSnapshot>,
    pub capture: CommandCapture,
}

impl ValidationCheck {
    pub fn is_pass(&self) -> bool {
        matches!(self.status, CheckStatus::Pass)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub healthy: bool,
    pub eval: ValidationCheck,
    pub guardrails: Vec<ValidationCheck>,
    pub warnings: Vec<String>,
}

pub fn validate_config(root: &Path, config: &Config) -> ValidationReport {
    let mut warnings = Vec::new();
    if config.guardrails.is_empty() {
        warnings
            .push("No guardrails configured; only the primary metric will be checked.".to_string());
    }

    let eval = validate_eval(root, config);
    let guardrails = config
        .guardrails
        .iter()
        .map(|guardrail| validate_guardrail(root, config, guardrail))
        .collect::<Vec<_>>();

    let healthy = eval.is_pass() && guardrails.iter().all(ValidationCheck::is_pass);
    ValidationReport {
        healthy,
        eval,
        guardrails,
        warnings,
    }
}

fn validate_eval(root: &Path, config: &Config) -> ValidationCheck {
    let command = config.eval.command.trim();
    if command.is_empty() {
        return failed_check(
            "eval",
            CheckKind::Eval,
            config.eval.command.clone(),
            "Eval command is empty.",
        );
    }
    if command == default_config().eval.command {
        return failed_check(
            "eval",
            CheckKind::Eval,
            config.eval.command.clone(),
            "Eval command still uses the default autoloop placeholder; replace it with a repo-specific metric command.",
        );
    }

    let regex = match compile_regex(config.eval.regex.as_deref()) {
        Ok(regex) => regex,
        Err(error) => {
            return failed_check(
                "eval",
                CheckKind::Eval,
                config.eval.command.clone(),
                format!("Eval regex is invalid: {error}"),
            );
        }
    };

    let metric_spec = MetricCommandSpec {
        command: &config.eval.command,
        retries: config.eval.retries,
        timeout_secs: config.eval.timeout,
        format: config.eval.format,
        regex: regex.as_ref(),
        metric_name: &config.metric.name,
        unit: config.metric.unit.as_deref(),
    };
    match run_metric_command_with_retries(&metric_spec, root) {
        Ok(execution) => ValidationCheck {
            name: "eval".to_string(),
            kind: CheckKind::Eval,
            command: config.eval.command.clone(),
            status: CheckStatus::Pass,
            message: metric_message(&execution.metric),
            metric: Some(execution.metric),
            capture: execution.command,
        },
        Err(failure) => failure_check(
            "eval",
            CheckKind::Eval,
            config.eval.command.clone(),
            failure,
        ),
    }
}

fn validate_guardrail(
    root: &Path,
    config: &Config,
    guardrail: &GuardrailConfig,
) -> ValidationCheck {
    match guardrail.kind {
        GuardrailKind::PassFail => validate_pass_fail_guardrail(root, config, guardrail),
        GuardrailKind::Metric => validate_metric_guardrail(root, config, guardrail),
    }
}

fn validate_pass_fail_guardrail(
    root: &Path,
    config: &Config,
    guardrail: &GuardrailConfig,
) -> ValidationCheck {
    if guardrail.command.trim().is_empty() {
        return failed_check(
            &guardrail.name,
            CheckKind::PassFailGuardrail,
            guardrail.command.clone(),
            "Guardrail command is empty.",
        );
    }

    match run_raw_command_capture(&guardrail.command, config.eval.timeout, root) {
        Ok(capture) => {
            let passed = capture.exit_code == Some(0);
            ValidationCheck {
                name: guardrail.name.clone(),
                kind: CheckKind::PassFailGuardrail,
                command: guardrail.command.clone(),
                status: if passed {
                    CheckStatus::Pass
                } else {
                    CheckStatus::Fail
                },
                message: match capture.exit_code {
                    Some(0) => "Guardrail command passed.".to_string(),
                    Some(code) => format!("Guardrail command exited with status {code}."),
                    None => "Guardrail command exited without a status code.".to_string(),
                },
                metric: None,
                capture,
            }
        }
        Err(failure) => failure_check(
            &guardrail.name,
            CheckKind::PassFailGuardrail,
            guardrail.command.clone(),
            failure,
        ),
    }
}

fn validate_metric_guardrail(
    root: &Path,
    config: &Config,
    guardrail: &GuardrailConfig,
) -> ValidationCheck {
    if guardrail.command.trim().is_empty() {
        return failed_check(
            &guardrail.name,
            CheckKind::MetricGuardrail,
            guardrail.command.clone(),
            "Metric guardrail command is empty.",
        );
    }

    let Some(threshold_spec) = guardrail.threshold.as_deref() else {
        return failed_check(
            &guardrail.name,
            CheckKind::MetricGuardrail,
            guardrail.command.clone(),
            "Metric guardrail is missing a threshold.",
        );
    };
    if let Err(error) = parse_threshold(threshold_spec) {
        return failed_check(
            &guardrail.name,
            CheckKind::MetricGuardrail,
            guardrail.command.clone(),
            format!("Metric guardrail threshold is invalid: {error}"),
        );
    }

    let regex = match compile_regex(guardrail.regex.as_deref()) {
        Ok(regex) => regex,
        Err(error) => {
            return failed_check(
                &guardrail.name,
                CheckKind::MetricGuardrail,
                guardrail.command.clone(),
                format!("Metric guardrail regex is invalid: {error}"),
            );
        }
    };

    let metric_spec = MetricCommandSpec {
        command: &guardrail.command,
        retries: 0,
        timeout_secs: config.eval.timeout,
        format: guardrail.format,
        regex: regex.as_ref(),
        metric_name: &guardrail.name,
        unit: None,
    };
    match run_metric_command_with_retries(&metric_spec, root) {
        Ok(execution) => ValidationCheck {
            name: guardrail.name.clone(),
            kind: CheckKind::MetricGuardrail,
            command: guardrail.command.clone(),
            status: CheckStatus::Pass,
            message: format!(
                "{}; threshold {} will be evaluated once baseline data exists.",
                metric_message(&execution.metric),
                threshold_spec
            ),
            metric: Some(execution.metric),
            capture: execution.command,
        },
        Err(failure) => failure_check(
            &guardrail.name,
            CheckKind::MetricGuardrail,
            guardrail.command.clone(),
            failure,
        ),
    }
}

fn failure_check(
    name: &str,
    kind: CheckKind,
    command: String,
    failure: RuntimeFailure,
) -> ValidationCheck {
    ValidationCheck {
        name: name.to_string(),
        kind,
        command,
        status: CheckStatus::Fail,
        message: failure.message,
        metric: None,
        capture: failure.command,
    }
}

fn failed_check(
    name: &str,
    kind: CheckKind,
    command: String,
    message: impl Into<String>,
) -> ValidationCheck {
    ValidationCheck {
        name: name.to_string(),
        kind,
        command: command.clone(),
        status: CheckStatus::Fail,
        message: message.into(),
        metric: None,
        capture: CommandCapture {
            command,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
        },
    }
}

fn metric_message(metric: &MetricSnapshot) -> String {
    match metric.unit.as_deref() {
        Some(unit) => format!("Parsed {}={}{}", metric.name, metric.value, unit),
        None => format!("Parsed {}={}", metric.name, metric.value),
    }
}
