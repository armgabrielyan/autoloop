use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::cli::{InstallArgs, OutputFormat};
use crate::integrations::{GeneratedFile, context_path_for_tool, generate};
use crate::output::emit;
use crate::ui::{
    TableRow, Tone, banner, can_prompt, confirm, join_blocks, render_list, render_steps,
    render_table,
};

pub fn run(args: InstallArgs, output: OutputFormat) -> Result<()> {
    let root = match args.path {
        Some(path) => path,
        None => std::env::current_dir().context("failed to resolve current directory")?,
    };
    let generated = generate(&root, args.tool)?;
    let changes = classify_changes(&root, &generated)?;

    if changes.has_overwrites && !args.force {
        if can_prompt() {
            if !confirm(
                &format!(
                    "Autoloop {} integration files already exist. Overwrite them?",
                    args.tool.as_str()
                ),
                false,
            )? {
                bail!("installation aborted");
            }
        } else {
            bail!(
                "integration files already exist under {}; rerun with --force to overwrite them",
                root.display()
            );
        }
    }

    write_generated_files(&root, &generated)?;
    let payload = json!({
        "tool": args.tool.as_str(),
        "root": root.display().to_string(),
        "context_path": context_path_for_tool(args.tool),
        "created": changes.created,
        "updated": changes.updated,
    });
    let human = render_summary(args.tool.as_str(), &root, &changes);
    emit(output, human, &payload)
}

struct InstallChanges {
    created: Vec<String>,
    updated: Vec<String>,
    has_overwrites: bool,
}

fn classify_changes(root: &Path, files: &[GeneratedFile]) -> Result<InstallChanges> {
    let mut created = Vec::new();
    let mut updated = Vec::new();
    let mut has_overwrites = false;

    for file in files {
        let path = root.join(&file.relative_path);
        if !path.exists() {
            created.push(display_path(root, &path));
            continue;
        }

        let existing = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if existing != file.contents {
            has_overwrites = true;
            updated.push(display_path(root, &path));
        }
    }

    Ok(InstallChanges {
        created,
        updated,
        has_overwrites,
    })
}

fn write_generated_files(root: &Path, files: &[GeneratedFile]) -> Result<()> {
    for file in files {
        let path = root.join(&file.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, &file.contents)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    Ok(())
}

fn render_summary(tool: &str, root: &Path, changes: &InstallChanges) -> String {
    let mut blocks = vec![
        banner(Tone::Success, "Installed autoloop integration"),
        render_table(&[
            TableRow::new("Workspace", root.display().to_string()),
            TableRow::new("Tool", tool),
            TableRow::new("Context", context_path_for_tool_name(tool)),
        ]),
    ];
    if let Some(created) = render_list("Created", &changes.created) {
        blocks.push(created);
    }
    if let Some(updated) = render_list("Updated", &changes.updated) {
        blocks.push(updated);
    }
    if let Some(next) = render_steps(
        "Next",
        &[
            format!(
                "Open `{}` in your agent workspace",
                context_path_for_tool_name(tool)
            ),
            "Invoke `autoloop-run` to start an autonomous bounded loop".to_string(),
        ],
    ) {
        blocks.push(next);
    }
    join_blocks(blocks)
}

fn context_path_for_tool_name(tool: &str) -> &'static str {
    match tool {
        "claude-code" => "CLAUDE.md",
        "gemini-cli" => "GEMINI.md",
        "generic" => "program.md",
        _ => "AGENTS.md",
    }
}

fn display_path(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) if relative.as_os_str().is_empty() => ".".to_string(),
        Ok(relative) => relative.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}
