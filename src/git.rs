use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use git2::{
    BranchType, DiffFormat, DiffOptions, ErrorCode, IndexAddOption, ObjectType, Oid, Repository,
    Signature, Status, StatusOptions, StatusShow, build::CheckoutBuilder,
};

use crate::error::GitError;
use crate::state::{PathState, PreparedExperiment, RecordedWorktree};
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
    pub path_states: Vec<PathState>,
}

#[derive(Debug, Clone)]
pub struct HeadState {
    pub refname: Option<String>,
    pub oid: Oid,
}

#[derive(Debug, Clone)]
pub struct FinalizedBranch {
    pub branch_name: String,
    pub base_commit: String,
    pub head_commit: String,
    pub applied_commits: Vec<String>,
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
    let Some(workdir) = repo.workdir() else {
        return Ok(WorkingTreeSnapshot::default());
    };

    let statuses = capture_statuses(&repo)?;
    let file_paths: Vec<String> = statuses.keys().cloned().collect();
    let untracked_paths: Vec<String> = statuses
        .iter()
        .filter_map(|(path, is_untracked)| is_untracked.then_some(path.clone()))
        .collect();
    let path_states = capture_path_states(workdir, &statuses)?;

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
        path_states,
    })
}

pub fn derive_experiment_worktree(
    prepared: Option<&PreparedExperiment>,
    current: &WorkingTreeSnapshot,
) -> RecordedWorktree {
    match prepared {
        Some(prepared) => diff_recorded_worktree(&prepared.worktree, current),
        None => recorded_worktree_from_snapshot(current),
    }
}

pub fn recorded_worktree_from_snapshot(snapshot: &WorkingTreeSnapshot) -> RecordedWorktree {
    RecordedWorktree {
        file_paths: snapshot.file_paths.clone(),
        untracked_paths: snapshot.untracked_paths.clone(),
        auto_categories: snapshot.auto_categories.clone(),
        diff_summary: snapshot.diff_summary.clone(),
        diff: snapshot.diff.clone(),
        path_states: snapshot.path_states.clone(),
    }
}

pub fn pending_worktree_matches(root: &Path, recorded: &RecordedWorktree) -> Result<bool> {
    for expected in &recorded.path_states {
        let actual = capture_current_path_state(root, expected)?;
        if actual != *expected {
            return Ok(false);
        }
    }

    Ok(true)
}

pub fn ensure_clean_worktree(root: &Path) -> Result<()> {
    let snapshot = capture_working_tree(root)?;
    if snapshot.has_changes {
        bail!("finalize requires a clean working tree; commit or discard local changes first");
    }

    Ok(())
}

pub fn capture_head_state(root: &Path) -> Result<HeadState> {
    let repo = require_repository(root)?;
    let head = repo.head().map_err(|source| GitError::Operation {
        operation: "read HEAD reference",
        source,
    })?;
    let oid = head
        .target()
        .ok_or_else(|| anyhow!("HEAD does not point to a commit"))?;
    Ok(HeadState {
        refname: head.name().map(ToString::to_string),
        oid,
    })
}

pub fn restore_head(root: &Path, state: &HeadState) -> Result<()> {
    let repo = require_repository(root)?;
    match &state.refname {
        Some(refname) => {
            repo.set_head(refname)
                .map_err(|source| GitError::Operation {
                    operation: "restore HEAD reference",
                    source,
                })?;
            let mut checkout = CheckoutBuilder::new();
            checkout.force().recreate_missing(true).update_index(true);
            repo.checkout_head(Some(&mut checkout))
                .map_err(|source| GitError::Operation {
                    operation: "restore working tree",
                    source,
                })?;
        }
        None => {
            repo.set_head_detached(state.oid)
                .map_err(|source| GitError::Operation {
                    operation: "restore detached HEAD",
                    source,
                })?;
            let commit = repo
                .find_commit(state.oid)
                .map_err(|source| GitError::Operation {
                    operation: "resolve detached HEAD commit",
                    source,
                })?;
            let mut checkout = CheckoutBuilder::new();
            checkout.force().recreate_missing(true).update_index(true);
            repo.checkout_tree(commit.as_object(), Some(&mut checkout))
                .map_err(|source| GitError::Operation {
                    operation: "checkout detached HEAD tree",
                    source,
                })?;
        }
    }

    Ok(())
}

pub fn create_review_branch(
    root: &Path,
    branch_name: &str,
    commit_hashes: &[String],
) -> Result<FinalizedBranch> {
    if commit_hashes.is_empty() {
        bail!("cannot create a review branch without experiment commits");
    }

    let repo = require_repository(root)?;
    let first_commit_oid = parse_oid(&commit_hashes[0])?;
    let first_commit =
        repo.find_commit(first_commit_oid)
            .map_err(|source| GitError::Operation {
                operation: "resolve experiment commit",
                source,
            })?;
    let base_commit = first_commit
        .parent(0)
        .map_err(|source| GitError::Operation {
            operation: "resolve experiment parent commit",
            source,
        })?;

    match repo.find_branch(branch_name, BranchType::Local) {
        Ok(_) => bail!("branch `{branch_name}` already exists"),
        Err(error) if error.code() == ErrorCode::NotFound => {}
        Err(source) => {
            return Err(GitError::Operation {
                operation: "inspect existing branch",
                source,
            }
            .into());
        }
    }

    let branch = repo
        .branch(branch_name, &base_commit, false)
        .map_err(|source| GitError::Operation {
            operation: "create review branch",
            source,
        })?;
    let branch_ref = branch
        .get()
        .name()
        .ok_or_else(|| anyhow!("review branch name is not valid UTF-8"))?
        .to_string();

    repo.set_head(&branch_ref)
        .map_err(|source| GitError::Operation {
            operation: "checkout review branch HEAD",
            source,
        })?;
    let mut checkout = CheckoutBuilder::new();
    checkout.force().recreate_missing(true).update_index(true);
    repo.checkout_head(Some(&mut checkout))
        .map_err(|source| GitError::Operation {
            operation: "checkout review branch",
            source,
        })?;

    let mut applied_commits = Vec::new();
    for commit_hash in commit_hashes {
        applied_commits.push(cherry_pick_commit_to_head(&repo, commit_hash)?);
    }

    let head_commit = repo
        .head()
        .map_err(|source| GitError::Operation {
            operation: "read review branch HEAD",
            source,
        })?
        .target()
        .ok_or_else(|| anyhow!("review branch HEAD does not point to a commit"))?
        .to_string();

    Ok(FinalizedBranch {
        branch_name: branch_name.to_string(),
        base_commit: base_commit.id().to_string(),
        head_commit,
        applied_commits,
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

fn parse_oid(value: &str) -> Result<Oid> {
    Oid::from_str(value).map_err(|source| {
        GitError::Operation {
            operation: "parse git object id",
            source,
        }
        .into()
    })
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

fn capture_path_states(
    workdir: &Path,
    statuses: &BTreeMap<String, bool>,
) -> Result<Vec<PathState>> {
    statuses
        .iter()
        .map(|(path, untracked)| {
            let absolute = workdir.join(path);
            Ok(PathState {
                path: path.clone(),
                untracked: *untracked,
                exists: absolute.exists(),
                content_hash: hash_path_contents(&absolute)?,
            })
        })
        .collect()
}

fn capture_current_path_state(root: &Path, expected: &PathState) -> Result<PathState> {
    let Some(repo) = discover_repository(root)? else {
        return Ok(PathState {
            path: expected.path.clone(),
            untracked: expected.untracked,
            exists: false,
            content_hash: None,
        });
    };
    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow!("git operations require a non-bare repository"))?;
    let absolute = workdir.join(&expected.path);
    Ok(PathState {
        path: expected.path.clone(),
        untracked: expected.untracked,
        exists: absolute.exists(),
        content_hash: hash_path_contents(&absolute)?,
    })
}

fn diff_recorded_worktree(
    base: &RecordedWorktree,
    current: &WorkingTreeSnapshot,
) -> RecordedWorktree {
    let base_states: BTreeMap<&str, &PathState> = base
        .path_states
        .iter()
        .map(|state| (state.path.as_str(), state))
        .collect();
    let current_states: BTreeMap<&str, &PathState> = current
        .path_states
        .iter()
        .map(|state| (state.path.as_str(), state))
        .collect();
    let mut all_paths = BTreeSet::new();
    all_paths.extend(base_states.keys().copied());
    all_paths.extend(current_states.keys().copied());

    let mut path_states = Vec::new();
    let mut file_paths = Vec::new();
    let mut untracked_paths = Vec::new();
    for path in all_paths {
        let current_state = current_states
            .get(path)
            .map(|state| (*state).clone())
            .unwrap_or_else(|| PathState {
                path: path.to_string(),
                untracked: base_states
                    .get(path)
                    .map(|state| state.untracked)
                    .unwrap_or(false),
                exists: false,
                content_hash: None,
            });
        if base_states.get(path).map(|state| (*state).clone()) == Some(current_state.clone()) {
            continue;
        }
        if current_state.untracked {
            untracked_paths.push(current_state.path.clone());
        }
        file_paths.push(current_state.path.clone());
        path_states.push(current_state);
    }

    let auto_categories = derive_categories(file_paths.iter().map(String::as_str))
        .into_iter()
        .collect();

    RecordedWorktree {
        file_paths,
        untracked_paths,
        auto_categories,
        diff_summary: None,
        diff: None,
        path_states,
    }
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

fn hash_path_contents(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }

    let metadata = fs::symlink_metadata(path).map_err(|source| GitError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.is_dir() {
        return Ok(Some(hash_directory(path)?));
    }

    let bytes = fs::read(path).map_err(|source| GitError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Oid::hash_object(ObjectType::Blob, &bytes)
        .map(|oid| Some(oid.to_string()))
        .map_err(|source| {
            GitError::Operation {
                operation: "hash path contents",
                source,
            }
            .into()
        })
}

fn hash_directory(path: &Path) -> Result<String> {
    let mut entries = fs::read_dir(path)
        .map_err(|source| GitError::Read {
            path: path.to_path_buf(),
            source,
        })?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|source| GitError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut manifest = String::new();
    for entry in entries {
        let entry_path = entry.path();
        let entry_name = entry.file_name().to_string_lossy().into_owned();
        let child_hash = hash_path_contents(&entry_path)?.unwrap_or_else(|| "missing".to_string());
        manifest.push_str(&entry_name);
        manifest.push('\t');
        manifest.push_str(&child_hash);
        manifest.push('\n');
    }

    Oid::hash_object(ObjectType::Blob, manifest.as_bytes())
        .map(|oid| oid.to_string())
        .map_err(|source| {
            GitError::Operation {
                operation: "hash directory contents",
                source,
            }
            .into()
        })
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

fn cherry_pick_commit_to_head(repo: &Repository, commit_hash: &str) -> Result<String> {
    let oid = parse_oid(commit_hash)?;
    let commit = repo
        .find_commit(oid)
        .map_err(|source| GitError::Operation {
            operation: "resolve cherry-pick commit",
            source,
        })?;
    repo.cherrypick(&commit, None)
        .map_err(|source| GitError::Operation {
            operation: "cherry-pick commit",
            source,
        })?;

    let mut index = repo.index().map_err(|source| GitError::Operation {
        operation: "open git index",
        source,
    })?;
    if index.has_conflicts() {
        let _ = repo.cleanup_state();
        bail!("cherry-pick for commit `{commit_hash}` produced conflicts");
    }

    let tree_id = index
        .write_tree_to(repo)
        .map_err(|source| GitError::Operation {
            operation: "write cherry-pick tree",
            source,
        })?;
    let tree = repo
        .find_tree(tree_id)
        .map_err(|source| GitError::Operation {
            operation: "find cherry-pick tree",
            source,
        })?;
    let head_commit = repo
        .head()
        .map_err(|source| GitError::Operation {
            operation: "read HEAD reference",
            source,
        })?
        .peel_to_commit()
        .map_err(|source| GitError::Operation {
            operation: "resolve HEAD commit",
            source,
        })?;
    let author = commit.author();
    let committer = signature_for(repo)?;
    let message = commit.message().unwrap_or("autoloop finalize");

    let new_commit = repo
        .commit(
            Some("HEAD"),
            &author,
            &committer,
            message,
            &tree,
            &[&head_commit],
        )
        .map_err(|source| GitError::Operation {
            operation: "commit cherry-pick result",
            source,
        })?;
    repo.cleanup_state().map_err(|source| GitError::Operation {
        operation: "cleanup cherry-pick state",
        source,
    })?;

    Ok(new_commit.to_string())
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
