use std::io;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse TOML in {path}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error("failed to discover a git repository from {path}")]
    Discover {
        path: PathBuf,
        #[source]
        source: git2::Error,
    },
    #[error("failed to read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("git operation failed: {operation}")]
    Operation {
        operation: &'static str,
        #[source]
        source: git2::Error,
    },
}

#[derive(Debug, Error)]
pub enum PromptError {
    #[error("interactive prompt failed")]
    Dialoguer(#[from] dialoguer::Error),
}
