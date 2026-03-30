use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::cli::{FinalizeArgs, OutputFormat};
use crate::finalize::{FinalizeGroup, FinalizePlan, build_finalize_plan};
use crate::git::{
    FinalizedBranch, capture_head_state, create_review_branch, ensure_clean_worktree, restore_head,
};
use crate::output::emit;
use crate::state::State;
use crate::ui::{TableRow, Tone, banner, join_blocks, render_list, render_steps, render_table};

pub fn run(args: FinalizeArgs, output: OutputFormat) -> Result<()> {
    let root = std::env::current_dir().context("failed to resolve current directory")?;
    let Some(state) = State::load_optional(&root)? else {
        bail!("autoloop is not initialized in this directory; run `autoloop init` first");
    };

    ensure_clean_worktree(&root)?;
    let scope = resolve_scope(&root, &state, &args)?;
    let mut plan = build_finalize_plan(&root, scope.session_id.as_deref())?;
    if plan.groups.is_empty() {
        bail!("no committed kept experiments are available to finalize in this scope");
    }

    let original_head = capture_head_state(&root)?;
    let finalize_result = execute_finalize(&root, &mut plan, &scope.scope_slug);
    let restore_result = restore_head(&root, &original_head);

    if let Err(error) = finalize_result {
        if let Err(restore_error) = restore_result {
            return Err(error.context(format!(
                "failed to restore the original HEAD after finalize: {restore_error}"
            )));
        }
        return Err(error);
    }
    restore_result?;

    let payload = json!({
        "scope": {
            "all": scope.all,
            "session_id": scope.session_id,
            "label": scope.label,
        },
        "created_branches": plan.groups,
        "skipped": plan.skipped,
    });

    let mut blocks = vec![
        banner(Tone::Success, "Autoloop finalize"),
        render_table(&[
            TableRow::new("Workspace", root.display().to_string()),
            TableRow::new("Scope", scope.label.clone()),
            TableRow::new("Branches created", plan.groups.len().to_string()),
            TableRow::new("Skipped keeps", plan.skipped.len().to_string()),
        ]),
    ];
    if let Some(branch_block) = render_list(
        "Review Branches",
        &plan.groups.iter().map(render_group).collect::<Vec<_>>(),
    ) {
        blocks.push(branch_block);
    }
    if let Some(skipped_block) = render_list(
        "Skipped Keeps",
        &plan
            .skipped
            .iter()
            .map(|entry| format!("#{} {}", entry.experiment_id, entry.reason))
            .collect::<Vec<_>>(),
    ) {
        blocks.push(skipped_block);
    }
    if let Some(next_block) = render_steps(
        "Next",
        &[
            "Inspect the generated branches with `git branch --list 'autoloop/*'`".to_string(),
            "Open PRs from the review branches once they look clean".to_string(),
        ],
    ) {
        blocks.push(next_block);
    }

    emit(output, join_blocks(blocks), &payload)
}

#[derive(Debug, Clone)]
struct FinalizeScope {
    all: bool,
    session_id: Option<String>,
    label: String,
    scope_slug: String,
}

fn resolve_scope(
    root: &std::path::Path,
    state: &State,
    args: &FinalizeArgs,
) -> Result<FinalizeScope> {
    if args.all {
        return Ok(FinalizeScope {
            all: true,
            session_id: None,
            label: "all experiments".to_string(),
            scope_slug: "history".to_string(),
        });
    }

    if let Some(session) = &state.active_session {
        if args.session || !args.all {
            let label = format!(
                "session {}",
                session.name.as_deref().unwrap_or(session.id.as_str())
            );
            let scope_slug = session
                .name
                .as_deref()
                .map(slugify)
                .filter(|slug| !slug.is_empty())
                .unwrap_or_else(|| slugify(&session.id));
            return Ok(FinalizeScope {
                all: false,
                session_id: Some(session.id.clone()),
                label,
                scope_slug,
            });
        }
    }

    if args.session {
        let Some(session_id) = crate::experiments::latest_session_id(root)? else {
            bail!("no recorded sessions are available to finalize yet");
        };
        return Ok(FinalizeScope {
            all: false,
            label: format!("latest session {session_id}"),
            scope_slug: slugify(&session_id),
            session_id: Some(session_id),
        });
    }

    Ok(FinalizeScope {
        all: true,
        session_id: None,
        label: "all experiments".to_string(),
        scope_slug: "history".to_string(),
    })
}

fn execute_finalize(
    root: &std::path::Path,
    plan: &mut FinalizePlan,
    scope_slug: &str,
) -> Result<()> {
    for group in &mut plan.groups {
        let branch_name = format!("autoloop/{scope_slug}/{:02}-{}", group.index, group.slug);
        let created = create_review_branch(root, &branch_name, &group.commit_hashes)?;
        apply_branch_metadata(group, &created);
    }

    Ok(())
}

fn apply_branch_metadata(group: &mut FinalizeGroup, created: &FinalizedBranch) {
    group.branch_name = Some(created.branch_name.clone());
}

fn render_group(group: &FinalizeGroup) -> String {
    let branch_name = group.branch_name.as_deref().unwrap_or("branch not created");
    let best = group
        .best_improvement
        .map(|value| format!("{value:+.1}% best"))
        .unwrap_or_else(|| "no metric delta".to_string());
    format!(
        "{}: experiments {:?} [{}; {} files]",
        branch_name,
        group.experiment_ids,
        best,
        group.file_paths.len()
    )
}

fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for ch in input.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}
