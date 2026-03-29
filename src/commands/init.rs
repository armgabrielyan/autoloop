use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::cli::{InitArgs, OutputFormat};
use crate::config::{autoloop_dir, config_path, default_config_template};
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
        });
        let human = render_summary(
            Tone::Warning,
            "Dry run",
            &root.display().to_string(),
            &display_path(&root, &dir),
            &display_path(&root, &config),
            &display_path(&root, &state_path),
            &display_path(&root, &last_eval_path),
            &created,
            &updated,
        );
        return emit(output, human, &payload);
    }

    let spinner = Spinner::new("Initializing autoloop workspace");
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    fs::write(&config, default_config_template())
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
    });
    let human = render_summary(
        Tone::Success,
        "Initialized autoloop",
        &root.display().to_string(),
        &display_path(&root, &dir),
        &display_path(&root, &config),
        &display_path(&root, &state_path),
        &display_path(&root, &last_eval_path),
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
    created: &[String],
    updated: &[String],
) -> String {
    let table = render_table(&[
        TableRow::new("Workspace", root),
        TableRow::new("Autoloop dir", dir),
        TableRow::new("Config", config),
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
    if let Some(next_block) = render_steps(
        "Next",
        &[
            "Run `autoloop status` to inspect the initialized workspace".to_string(),
            "Run `autoloop session start --name \"first-run\"` before the first experiment"
                .to_string(),
        ],
    ) {
        blocks.push(next_block);
    }

    join_blocks(blocks)
}

fn display_path(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) if relative.as_os_str().is_empty() => ".".to_string(),
        Ok(relative) => relative.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}
