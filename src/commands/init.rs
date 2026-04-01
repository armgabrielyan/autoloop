use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::cli::{InitArgs, OutputFormat};
use crate::config::{autoloop_dir, config_path, render_config};
use crate::detect::{ConfigInference, ConfigSource, ProjectKind, infer_config};
use crate::git::{ensure_gitignore_contains, gitignore_path};
use crate::output::emit;
use crate::state::{LastEvalState, State, write_learnings_stub, write_session_markdown};
use crate::ui::{
    Spinner, TableRow, Tone, banner, can_prompt, confirm, join_blocks, render_list, render_steps,
    render_table,
};
use crate::validation::{ValidationReport, validate_config};

pub fn run(args: InitArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    let dir = autoloop_dir(&root);
    let config = config_path(&root);
    let state_path = crate::state::state_path(&root);
    let last_eval_path = crate::state::last_eval_path(&root);
    let learnings_path = crate::state::learnings_path(&root);
    let session_md_path = crate::state::session_markdown_path(&root);
    let gitignore_path = gitignore_path(&root)?;
    let (inferred_config, inference) = infer_config(&root)?;
    let rendered_config = render_config(&inferred_config)?;

    let overwrite_existing = if config.exists() && !args.force {
        if can_prompt() {
            confirm("An autoloop config already exists. Overwrite it?", false)?
        } else {
            bail!(
                "{} already exists; rerun with --force to overwrite it",
                config.display()
            );
        }
    } else {
        args.force
    };

    if config.exists() && !overwrite_existing {
        bail!("initialization aborted");
    }

    let mut created = Vec::new();
    let mut updated = Vec::new();

    if !dir.exists() {
        created.push(display_path(&root, &dir));
    }

    for path in [
        &config,
        &state_path,
        &last_eval_path,
        &learnings_path,
        &session_md_path,
    ] {
        if !path.exists() {
            created.push(display_path(&root, path));
        }
    }

    if config.exists() && overwrite_existing {
        updated.push(display_path(&root, &config));
    }

    if !gitignore_path.exists()
        || !std::fs::read_to_string(&gitignore_path)
            .unwrap_or_default()
            .lines()
            .any(|line| line.trim() == ".autoloop/")
    {
        updated.push(display_path(&root, &gitignore_path));
    }

    if args.dry_run {
        let root_display = root.display().to_string();
        let dir_display = display_path(&root, &dir);
        let config_display = display_path(&root, &config);
        let state_display = display_path(&root, &state_path);
        let last_eval_display = display_path(&root, &last_eval_path);
        let payload = json!({
            "dry_run": true,
            "created": created,
            "updated": updated,
            "root": root_display,
            "config_inference": &inference,
            "verification": serde_json::Value::Null,
            "verification_skipped": args.verify,
        });
        let summary = InitSummary {
            tone: Tone::Warning,
            title: "Dry run",
            root: &root_display,
            dir: &dir_display,
            config: &config_display,
            state_path: &state_display,
            last_eval_path: &last_eval_display,
            inference: &inference,
            verification: None,
            verification_skipped: args.verify,
            created: &created,
            updated: &updated,
        };
        let human = render_summary(&summary);
        return emit(output, human, &payload);
    }

    let spinner = Spinner::new("Initializing autoloop workspace");
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    fs::write(&config, rendered_config)
        .with_context(|| format!("failed to write {}", config.display()))?;

    let state = State::default();
    state.save(&root)?;
    LastEvalState::default().save(&root)?;
    write_learnings_stub(&root)?;
    write_session_markdown(&root, &state)?;

    if ensure_gitignore_contains(&root, ".autoloop/")? {
        let gitignore = display_path(&root, &gitignore_path);
        if !updated.contains(&gitignore) {
            updated.push(gitignore);
        }
    }
    spinner.finish();

    let verification = if args.verify {
        let spinner = Spinner::new("Verifying inferred config");
        let report = validate_config(&root, &inferred_config);
        spinner.finish();
        Some(report)
    } else {
        None
    };
    let verification_healthy = verification
        .as_ref()
        .map(|report| report.healthy)
        .unwrap_or(true);
    let root_display = root.display().to_string();
    let dir_display = display_path(&root, &dir);
    let config_display = display_path(&root, &config);
    let state_display = display_path(&root, &state_path);
    let last_eval_display = display_path(&root, &last_eval_path);

    let payload = json!({
        "dry_run": false,
        "created": created,
        "updated": updated,
        "root": root_display,
        "config_inference": &inference,
        "verification": &verification,
        "verification_skipped": false,
    });
    let summary = InitSummary {
        tone: if verification_healthy {
            Tone::Success
        } else {
            Tone::Warning
        },
        title: if verification_healthy {
            "Initialized autoloop"
        } else {
            "Initialized autoloop (verification needs attention)"
        },
        root: &root_display,
        dir: &dir_display,
        config: &config_display,
        state_path: &state_display,
        last_eval_path: &last_eval_display,
        inference: &inference,
        verification: verification.as_ref(),
        verification_skipped: false,
        created: &created,
        updated: &updated,
    };
    let human = render_summary(&summary);
    emit(output, human, &payload)
}

struct InitSummary<'a> {
    tone: Tone,
    title: &'a str,
    root: &'a str,
    dir: &'a str,
    config: &'a str,
    state_path: &'a str,
    last_eval_path: &'a str,
    inference: &'a ConfigInference,
    verification: Option<&'a ValidationReport>,
    verification_skipped: bool,
    created: &'a [String],
    updated: &'a [String],
}

fn render_summary(summary: &InitSummary<'_>) -> String {
    let guardrails = if summary.inference.guardrail_commands.is_empty() {
        "none detected".to_string()
    } else {
        summary.inference.guardrail_commands.join(", ")
    };
    let table = render_table(&[
        TableRow::new("Workspace", summary.root),
        TableRow::new("Autoloop dir", summary.dir),
        TableRow::new("Config", summary.config),
        TableRow::new("Config source", render_source(summary.inference.source)),
        TableRow::new(
            "Project",
            render_project_kind(summary.inference.project_kind),
        ),
        TableRow::new(
            "Metric",
            render_metric(
                &summary.inference.metric_name,
                summary.inference.metric_direction,
                summary.inference.metric_unit.as_deref(),
            ),
        ),
        TableRow::new("Eval command", summary.inference.eval_command.clone()),
        TableRow::new("Guardrails", guardrails),
        TableRow::new(
            "Verified",
            render_verification_status(summary.verification, summary.verification_skipped),
        ),
        TableRow::new("State", summary.state_path),
        TableRow::new("Pending eval", summary.last_eval_path),
    ]);

    let mut blocks = vec![banner(summary.tone, summary.title), table];
    if let Some(created_block) = render_list("Created", summary.created) {
        blocks.push(created_block);
    }
    if let Some(updated_block) = render_list("Updated", summary.updated) {
        blocks.push(updated_block);
    }
    if let Some(notes_block) = render_list("Inference", &summary.inference.notes) {
        blocks.push(notes_block);
    }
    if let Some(verification_block) = render_list(
        "Verification",
        &render_verification_lines(summary.verification, summary.verification_skipped),
    ) {
        blocks.push(verification_block);
    }
    if let Some(next_block) =
        render_steps("Next", &next_steps(summary.inference, summary.verification))
    {
        blocks.push(next_block);
    }

    join_blocks(blocks)
}

fn render_source(source: ConfigSource) -> &'static str {
    match source {
        ConfigSource::Inferred => "inferred",
        ConfigSource::Partial => "partial",
        ConfigSource::Template => "template",
    }
}

fn render_project_kind(kind: ProjectKind) -> &'static str {
    match kind {
        ProjectKind::Rust => "rust",
        ProjectKind::Go => "go",
        ProjectKind::DotNet => ".net",
        ProjectKind::Jvm => "jvm",
        ProjectKind::Python => "python",
        ProjectKind::Node => "node",
        ProjectKind::Unknown => "unknown",
    }
}

fn render_metric(
    name: &str,
    direction: crate::config::MetricDirection,
    unit: Option<&str>,
) -> String {
    let direction = match direction {
        crate::config::MetricDirection::Lower => "lower",
        crate::config::MetricDirection::Higher => "higher",
    };
    match unit {
        Some(unit) => format!("{name} ({direction}, {unit})"),
        None => format!("{name} ({direction})"),
    }
}

fn next_steps(inference: &ConfigInference, verification: Option<&ValidationReport>) -> Vec<String> {
    if matches!(verification, Some(report) if !report.healthy) {
        return vec![
            "Run `autoloop doctor --fix` to apply a verified inferred config when available."
                .to_string(),
            "If repair is not available, edit `.autoloop/config.toml` and rerun `autoloop doctor`."
                .to_string(),
        ];
    }

    let mut steps = vec!["Run `autoloop status` to inspect the initialized workspace".to_string()];
    match inference.source {
        ConfigSource::Inferred => {
            steps.push("Run `autoloop baseline` to record the inferred benchmark".to_string());
            steps.push(
                "Run `autoloop session start --name \"first-run\"` after baseline succeeds"
                    .to_string(),
            );
        }
        ConfigSource::Partial | ConfigSource::Template => {
            steps.push("Open `.autoloop/config.toml` and replace the placeholder eval command with a repo-specific metric command".to_string());
            steps.push("Run `autoloop baseline` only after the config is executable".to_string());
        }
    }
    steps
}

fn render_verification_status(
    verification: Option<&ValidationReport>,
    verification_skipped: bool,
) -> String {
    if verification_skipped {
        "skipped (dry-run)".to_string()
    } else {
        match verification {
            Some(report) if report.healthy => "yes".to_string(),
            Some(_) => "no".to_string(),
            None => "not run".to_string(),
        }
    }
}

fn render_verification_lines(
    verification: Option<&ValidationReport>,
    verification_skipped: bool,
) -> Vec<String> {
    if verification_skipped {
        return vec!["Verification is skipped during `--dry-run`.".to_string()];
    }
    let Some(verification) = verification else {
        return Vec::new();
    };

    let mut lines = vec![format!(
        "eval: {}",
        if verification.eval.is_pass() {
            verification.eval.message.clone()
        } else {
            format!("FAIL {}", verification.eval.message)
        }
    )];
    lines.extend(verification.guardrails.iter().map(|guardrail| {
        format!(
            "{}: {}",
            guardrail.name,
            if guardrail.is_pass() {
                guardrail.message.clone()
            } else {
                format!("FAIL {}", guardrail.message)
            }
        )
    }));
    lines.extend(verification.warnings.clone());
    if !verification.healthy {
        lines.push(
            "Run `autoloop doctor --fix` or edit `.autoloop/config.toml` before baselining."
                .to_string(),
        );
    }
    lines
}

fn display_path(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) if relative.as_os_str().is_empty() => ".".to_string(),
        Ok(relative) => relative.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}
