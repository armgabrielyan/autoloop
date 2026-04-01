use std::fs;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::json;

use crate::cli::{DoctorArgs, OutputFormat};
use crate::config::{Config, config_path, default_config, render_config};
use crate::detect::{ConfigInference, ConfigSource, infer_config};
use crate::output::emit;
use crate::ui::{
    Spinner, TableRow, Tone, banner, join_blocks, render_list, render_steps, render_table,
};
use crate::validation::{CheckStatus, ValidationCheck, ValidationReport, validate_config};

#[derive(Debug, Clone, Serialize)]
struct InferredCandidate {
    inference: ConfigInference,
    report: ValidationReport,
}

#[derive(Debug, Clone, Serialize)]
struct FixOutcome {
    available: bool,
    applied: bool,
    #[serde(default)]
    reason: Option<String>,
}

pub fn run(args: DoctorArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    let path = config_path(&root);
    if !path.exists() {
        bail!("autoloop is not initialized in this directory; run `autoloop init` first");
    }

    let config = Config::load(&root)?;
    let current_rendered = render_config(&config)?;
    let spinner = Spinner::new("Verifying autoloop config");
    let current_report = validate_config(&root, &config);
    let mut report = current_report.clone();
    let mut candidate = None;
    let mut fix = FixOutcome {
        available: false,
        applied: false,
        reason: None,
    };

    if !current_report.healthy || args.fix {
        let (inferred_config, inference) = infer_config(&root)?;
        let inferred_report = validate_config(&root, &inferred_config);
        let inferred_rendered = render_config(&inferred_config)?;
        let available = should_offer_fix(
            &config,
            &current_report,
            &current_rendered,
            &inference,
            &inferred_report,
            &inferred_rendered,
        );
        let reason = available.then(|| {
            "A verified inferred config is available and differs from the current config."
                .to_string()
        });
        if args.fix && available {
            fs::write(&path, &inferred_rendered)
                .with_context(|| format!("failed to write {}", path.display()))?;
            report = inferred_report.clone();
            fix.applied = true;
        }
        fix.available = available;
        fix.reason = reason;
        candidate = Some(InferredCandidate {
            inference,
            report: inferred_report,
        });
    }
    spinner.finish();

    let title = if fix.applied {
        "Config repaired and verified"
    } else if report.healthy {
        "Config verification passed"
    } else {
        "Config verification needs attention"
    };
    let tone = if report.healthy {
        Tone::Success
    } else {
        Tone::Warning
    };

    let payload = json!({
        "workspace": root.display().to_string(),
        "config": path.display().to_string(),
        "healthy": report.healthy,
        "report": &report,
        "inferred_candidate": &candidate,
        "fix": &fix,
    });
    let human = render_summary(
        tone,
        title,
        &root.display().to_string(),
        &path.display().to_string(),
        &report,
        candidate.as_ref(),
        &fix,
    );
    emit(output, human, &payload)
}

fn should_offer_fix(
    current_config: &Config,
    current_report: &ValidationReport,
    current_rendered: &str,
    inference: &ConfigInference,
    inferred_report: &ValidationReport,
    inferred_rendered: &str,
) -> bool {
    if matches!(inference.source, ConfigSource::Template) || !inferred_report.healthy {
        return false;
    }
    if current_rendered == inferred_rendered {
        return false;
    }
    !current_report.healthy
        || current_config.eval.command.trim() == default_config().eval.command.trim()
}

fn render_summary(
    tone: Tone,
    title: &str,
    root: &str,
    config_path: &str,
    report: &ValidationReport,
    candidate: Option<&InferredCandidate>,
    fix: &FixOutcome,
) -> String {
    let guardrail_status = if report.guardrails.is_empty() {
        "none configured".to_string()
    } else {
        format!(
            "{}/{} passing",
            report
                .guardrails
                .iter()
                .filter(|check| check.is_pass())
                .count(),
            report.guardrails.len()
        )
    };
    let repair_status = if fix.applied {
        "applied".to_string()
    } else if fix.available {
        "available".to_string()
    } else {
        "none".to_string()
    };
    let table = render_table(&[
        TableRow::new("Workspace", root),
        TableRow::new("Config", config_path),
        TableRow::new("Healthy", if report.healthy { "yes" } else { "no" }),
        TableRow::new("Eval", render_status(&report.eval)),
        TableRow::new("Guardrails", guardrail_status),
        TableRow::new("Repair", repair_status),
    ]);

    let mut blocks = vec![banner(tone, title), table];
    let checks = render_checks(report);
    if let Some(block) = render_list("Checks", &checks) {
        blocks.push(block);
    }
    if let Some(block) = render_list("Warnings", &render_warnings(report, candidate, fix)) {
        blocks.push(block);
    }
    if let Some(block) = render_steps("Next", &next_steps(report, candidate, fix)) {
        blocks.push(block);
    }
    join_blocks(blocks)
}

fn render_checks(report: &ValidationReport) -> Vec<String> {
    std::iter::once(&report.eval)
        .chain(report.guardrails.iter())
        .map(render_check_line)
        .collect()
}

fn render_check_line(check: &ValidationCheck) -> String {
    let prefix = match check.status {
        CheckStatus::Pass => "PASS",
        CheckStatus::Fail => "FAIL",
    };
    format!("{}: {} via `{}`", prefix, check.message, check.command)
}

fn render_warnings(
    report: &ValidationReport,
    candidate: Option<&InferredCandidate>,
    fix: &FixOutcome,
) -> Vec<String> {
    let mut warnings = report.warnings.clone();
    if fix.available && !fix.applied {
        warnings.push(
            "A verified inferred config is available; run `autoloop doctor --fix` to apply it."
                .to_string(),
        );
    } else if fix.applied {
        warnings.push("Applied a verified inferred config.".to_string());
    } else if let Some(candidate) = candidate
        && !candidate.report.healthy
        && !report.healthy
    {
        warnings.push(
            "The inferred config was also unhealthy; this repo likely needs manual config edits."
                .to_string(),
        );
    }
    warnings
}

fn next_steps(
    report: &ValidationReport,
    candidate: Option<&InferredCandidate>,
    fix: &FixOutcome,
) -> Vec<String> {
    if report.healthy {
        return vec![
            "Run `autoloop baseline` to record the verified benchmark.".to_string(),
            "Run `autoloop session start --name \"first-run\"` when you are ready to iterate."
                .to_string(),
        ];
    }

    if fix.available && !fix.applied {
        return vec![
            "Run `autoloop doctor --fix` to replace the current config with the verified inferred config.".to_string(),
            "Rerun `autoloop doctor` after the repair to confirm the final config.".to_string(),
        ];
    }

    let mut steps = vec![
        "Open `.autoloop/config.toml` and replace the failing commands or parsing settings."
            .to_string(),
        "Rerun `autoloop doctor` after editing the config.".to_string(),
    ];
    if let Some(candidate) = candidate
        && !matches!(candidate.inference.source, ConfigSource::Template)
    {
        steps.push(format!(
            "Compare your config against the inferred eval command `{}`.",
            candidate.inference.eval_command
        ));
    }
    steps
}

fn render_status(check: &ValidationCheck) -> String {
    match check.status {
        CheckStatus::Pass => "pass".to_string(),
        CheckStatus::Fail => "fail".to_string(),
    }
}
