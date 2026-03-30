use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use git2::{
    DiffFormat, DiffOptions, ErrorCode, IndexAddOption, ObjectType, Oid, Repository, Signature,
    Status, StatusOptions, StatusShow, build::CheckoutBuilder,
};

use crate::error::GitError;
use crate::tags::derive_categories;

#[derive(Debug, Clone, Default)]
pub struct WorkingTreeSnapshot {
    pub fingerprint: Option<String>,
    pub has_changes: bool,
    pub file_paths: Vec<String>,
    pub auto_categories: Vec<String>,
    pub diff_summary: Option<String>,
    pub diff: Option<String>,
    pub untracked_paths: Vec<String>,
}

pub fn ensure_gitignore_contains(root: &Path, entry: &str) -> Result<bool> {
    let path = gitignore_path(root)?;
    let existing = if path.exists() {
        fs::read_to_string(&path).map_err(|source| GitError::Read {
            path: path.clone(),
            source,
        })?
    } else {
        String::new()
    };

    if existing.lines().any(|line| line.trim() == entry) {
        return Ok(false);
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(entry);
    updated.push('\n');

    fs::write(&path, updated).map_err(|source| GitError::Write {
        path: path.clone(),
        source,
    })?;
    Ok(true)
}

pub fn gitignore_path(root: &Path) -> Result<PathBuf> {
    match Repository::discover(root) {
        Ok(repo) => {
            if let Some(workdir) = repo.workdir() {
                Ok(workdir.join(".gitignore"))
            } else {
                Ok(root.join(".gitignore"))
            }
        }
        Err(error) if error.code() == ErrorCode::NotFound => Ok(root.join(".gitignore")),
        Err(source) => Err(GitError::Discover {
            path: root.to_path_buf(),
            source,
        }
        .into()),
    }
}

pub fn capture_working_tree(root: &Path) -> Result<WorkingTreeSnapshot> {
    let Some(repo) = discover_repository(root)? else {
        return Ok(WorkingTreeSnapshot::default());
    };
    if repo.workdir().is_none() {
        return Ok(WorkingTreeSnapshot::default());
    }

    let statuses = capture_statuses(&repo)?;
    let file_paths: Vec<String> = statuses.keys().cloned().collect();
    let untracked_paths: Vec<String> = statuses
        .iter()
        .filter_map(|(path, is_untracked)| is_untracked.then_some(path.clone()))
        .collect();

    let (diff_summary, diff) = capture_diff(&repo)?;
    let fingerprint = diff
        .as_ref()
        .map(|content| {
            Oid::hash_object(ObjectType::Blob, content.as_bytes())
                .map(|oid| oid.to_string())
                .map_err(|source| GitError::Operation {
                    operation: "hash git diff",
                    source,
                })
        })
        .transpose()?;
    let auto_categories = derive_categories(file_paths.iter().map(String::as_str))
        .into_iter()
        .collect();

    Ok(WorkingTreeSnapshot {
        fingerprint,
        has_changes: !file_paths.is_empty(),
        file_paths,
        auto_categories,
        diff_summary,
        diff,
        untracked_paths,
    })
}

pub fn commit_all(root: &Path, message: &str) -> Result<String> {
    let repo = require_repository(root)?;
    let snapshot = capture_working_tree(root)?;
    if !snapshot.has_changes {
        bail!("working tree has no changes to commit");
    }

    let mut index = repo.index().map_err(|source| GitError::Operation {
        operation: "open git index",
        source,
    })?;
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .map_err(|source| GitError::Operation {
            operation: "stage changes",
            source,
        })?;
    index.write().map_err(|source| GitError::Operation {
        operation: "write git index",
        source,
    })?;

    let tree_id = index.write_tree().map_err(|source| GitError::Operation {
        operation: "write git tree",
        source,
    })?;
    let tree = repo
        .find_tree(tree_id)
        .map_err(|source| GitError::Operation {
            operation: "find git tree",
            source,
        })?;
    let signature = signature_for(&repo)?;

    let commit_id = match repo.head() {
        Ok(head) => {
            let parent = head
                .peel_to_commit()
                .map_err(|source| GitError::Operation {
                    operation: "resolve HEAD commit",
                    source,
                })?;
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &[&parent],
            )
            .map_err(|source| GitError::Operation {
                operation: "create git commit",
                source,
            })?
        }
        Err(error)
            if error.code() == ErrorCode::NotFound || error.code() == ErrorCode::UnbornBranch =>
        {
            repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
                .map_err(|source| GitError::Operation {
                    operation: "create initial git commit",
                    source,
                })?
        }
        Err(source) => {
            return Err(GitError::Operation {
                operation: "read HEAD reference",
                source,
            }
            .into());
        }
    };

    Ok(commit_id.to_string())
}

pub fn revert_paths(root: &Path, file_paths: &[String], untracked_paths: &[String]) -> Result<()> {
    if file_paths.is_empty() && untracked_paths.is_empty() {
        bail!("working tree has no changes to revert");
    }

    let repo = require_repository(root)?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow!("git operations require a non-bare repository"))?
        .to_path_buf();
    let untracked: BTreeSet<&str> = untracked_paths.iter().map(String::as_str).collect();
    let tracked_paths: Vec<&str> = file_paths
        .iter()
        .filter_map(|path| (!untracked.contains(path.as_str())).then_some(path.as_str()))
        .collect();

    if !tracked_paths.is_empty() {
        match repo.head() {
            Ok(head) => {
                let head_object =
                    head.peel(ObjectType::Any)
                        .map_err(|source| GitError::Operation {
                            operation: "resolve HEAD object",
                            source,
                        })?;
                repo.reset_default(Some(&head_object), tracked_paths.iter().copied())
                    .map_err(|source| GitError::Operation {
                        operation: "reset tracked paths",
                        source,
                    })?;

                let mut checkout = CheckoutBuilder::new();
                checkout
                    .force()
                    .update_index(true)
                    .recreate_missing(true)
                    .disable_pathspec_match(true);
                for path in &tracked_paths {
                    checkout.path(*path);
                }
                repo.checkout_head(Some(&mut checkout))
                    .map_err(|source| GitError::Operation {
                        operation: "checkout tracked paths",
                        source,
                    })?;
            }
            Err(error)
                if error.code() == ErrorCode::NotFound
                    || error.code() == ErrorCode::UnbornBranch =>
            {
                repo.reset_default::<&str, _>(None, tracked_paths.iter().copied())
                    .map_err(|source| GitError::Operation {
                        operation: "reset index entries",
                        source,
                    })?;
                for path in &tracked_paths {
                    remove_relative_path(&workdir, path)?;
                }
            }
            Err(source) => {
                return Err(GitError::Operation {
                    operation: "read HEAD reference",
                    source,
                }
                .into());
            }
        }
    }

    for path in untracked_paths {
        remove_relative_path(&workdir, path)?;
    }

    Ok(())
}

fn discover_repository(root: &Path) -> Result<Option<Repository>> {
    match Repository::discover(root) {
        Ok(repo) => Ok(Some(repo)),
        Err(error) if error.code() == ErrorCode::NotFound => Ok(None),
        Err(source) => Err(GitError::Discover {
            path: root.to_path_buf(),
            source,
        }
        .into()),
    }
}

fn require_repository(root: &Path) -> Result<Repository> {
    discover_repository(root)?.ok_or_else(|| anyhow!("git operations require a git repository"))
}

fn capture_statuses(repo: &Repository) -> Result<BTreeMap<String, bool>> {
    let mut options = StatusOptions::new();
    options
        .show(StatusShow::IndexAndWorkdir)
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|source| GitError::Operation {
            operation: "read git status",
            source,
        })?;
    let mut entries = BTreeMap::new();

    for entry in statuses.iter() {
        let status = entry.status();
        if status == Status::CURRENT {
            continue;
        }
        let Some(path) = entry.path() else {
            continue;
        };
        entries.insert(path.to_string(), status == Status::WT_NEW);
    }

    Ok(entries)
}

fn capture_diff(repo: &Repository) -> Result<(Option<String>, Option<String>)> {
    let head_tree = head_tree(repo)?;
    let mut options = DiffOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .include_ignored(false);

    let diff = repo
        .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut options))
        .map_err(|source| GitError::Operation {
            operation: "capture git diff",
            source,
        })?;
    let stats = diff.stats().map_err(|source| GitError::Operation {
        operation: "read git diff stats",
        source,
    })?;

    let diff_summary = if stats.files_changed() == 0 {
        None
    } else {
        Some(format!(
            "{} files changed, {} insertions(+), {} deletions(-)",
            stats.files_changed(),
            stats.insertions(),
            stats.deletions(),
        ))
    };

    let mut patch = Vec::new();
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        patch.extend_from_slice(line.content());
        true
    })
    .map_err(|source| GitError::Operation {
        operation: "render git diff",
        source,
    })?;

    let diff = if patch.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&patch).into_owned())
    };

    Ok((diff_summary, diff))
}

fn head_tree<'repo>(repo: &'repo Repository) -> Result<Option<git2::Tree<'repo>>> {
    match repo.head() {
        Ok(head) => head.peel_to_tree().map(Some).map_err(|source| {
            GitError::Operation {
                operation: "resolve HEAD tree",
                source,
            }
            .into()
        }),
        Err(error)
            if error.code() == ErrorCode::NotFound || error.code() == ErrorCode::UnbornBranch =>
        {
            Ok(None)
        }
        Err(source) => Err(GitError::Operation {
            operation: "read HEAD reference",
            source,
        }
        .into()),
    }
}

fn signature_for(repo: &Repository) -> Result<Signature<'static>> {
    repo.signature()
        .or_else(|_| Signature::now("autoloop", "autoloop@local"))
        .map_err(|source| {
            GitError::Operation {
                operation: "build git signature",
                source,
            }
            .into()
        })
}

fn remove_relative_path(workdir: &Path, relative_path: &str) -> Result<()> {
    let path = workdir.join(relative_path);
    if !path.exists() {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(&path).map_err(|source| GitError::Read {
        path: path.clone(),
        source,
    })?;
    if metadata.is_dir() {
        fs::remove_dir_all(&path).map_err(|source| GitError::Write { path, source })?;
    } else {
        fs::remove_file(&path).map_err(|source| GitError::Write { path, source })?;
    }

    Ok(())
}
