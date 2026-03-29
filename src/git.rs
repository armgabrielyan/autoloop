use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use git2::{ErrorCode, Repository};

use crate::error::GitError;

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
