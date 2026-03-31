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
        let payload = json!({
            "dry_run": true,
            "created": created,
            "updated": updated,
            "root": root.display().to_string(),
            "config_inference": &inference,
        });
        let human = render_summary(
            Tone::Warning,
            "Dry run",
            &root.display().to_string(),
            &display_path(&root, &dir),
            &display_path(&root, &config),
            &display_path(&root, &state_path),
            &display_path(&root, &last_eval_path),
            &inference,
            &created,
            &updated,
        );
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

    let payload = json!({
        "dry_run": false,
        "created": created,
        "updated": updated,
        "root": root.display().to_string(),
        "config_inference": &inference,
    });
    let human = render_summary(
        Tone::Success,
        "Initialized autoloop",
        &root.display().to_string(),
        &display_path(&root, &dir),
        &display_path(&root, &config),
        &display_path(&root, &state_path),
        &display_path(&root, &last_eval_path),
        &inference,
        &created,
        &updated,
    );
    emit(output, human, &payload)
}

fn render_summary(
    tone: Tone,
    title: &str,
    root: &str,
    dir: &str,
    config: &str,
    state_path: &str,
    last_eval_path: &str,
    inference: &ConfigInference,
    created: &[String],
    updated: &[String],
) -> String {
    let guardrails = if inference.guardrail_commands.is_empty() {
        "none detected".to_string()
    } else {
        inference.guardrail_commands.join(", ")
    };
    let table = render_table(&[
        TableRow::new("Workspace", root),
        TableRow::new("Autoloop dir", dir),
        TableRow::new("Config", config),
        TableRow::new("Config source", render_source(inference.source)),
        TableRow::new("Project", render_project_kind(inference.project_kind)),
        TableRow::new(
            "Metric",
            render_metric(
                &inference.metric_name,
                inference.metric_direction,
                inference.metric_unit.as_deref(),
            ),
        ),
        TableRow::new("Eval command", inference.eval_command.clone()),
        TableRow::new("Guardrails", guardrails),
        TableRow::new("State", state_path),
        TableRow::new("Pending eval", last_eval_path),
    ]);

    let mut blocks = vec![banner(tone, title), table];
    if let Some(created_block) = render_list("Created", created) {
        blocks.push(created_block);
    }
    if let Some(updated_block) = render_list("Updated", updated) {
        blocks.push(updated_block);
    }
    if let Some(notes_block) = render_list("Inference", &inference.notes) {
        blocks.push(notes_block);
    }
    if let Some(next_block) = render_steps("Next", &next_steps(inference)) {
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

fn next_steps(inference: &ConfigInference) -> Vec<String> {
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

fn display_path(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) if relative.as_os_str().is_empty() => ".".to_string(),
        Ok(relative) => relative.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}
