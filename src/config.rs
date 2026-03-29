use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::error::ConfigError;
use crate::eval::formats::MetricFormat;

pub const AUTOLOOP_DIR: &str = ".autoloop";
pub const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub metric: MetricConfig,
    pub eval: EvalConfig,
    #[serde(default)]
    pub guardrails: Vec<GuardrailConfig>,
    #[serde(default)]
    pub confidence: ConfidenceConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub strictness: Strictness,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricConfig {
    pub name: String,
    pub direction: MetricDirection,
    #[serde(default)]
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricDirection {
    Lower,
    Higher,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalConfig {
    pub command: String,
    #[serde(default = "default_eval_timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub format: MetricFormat,
    #[serde(default)]
    pub regex: Option<String>,
    #[serde(default = "default_eval_retries")]
    pub retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardrailConfig {
    pub name: String,
    pub command: String,
    #[serde(default, alias = "type")]
    pub kind: GuardrailKind,
    #[serde(default)]
    pub format: MetricFormat,
    #[serde(default)]
    pub regex: Option<String>,
    #[serde(default)]
    pub threshold: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardrailKind {
    #[default]
    Metric,
    PassFail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfidenceConfig {
    #[serde(default = "default_confidence_min_experiments")]
    pub min_experiments: usize,
    #[serde(default = "default_confidence_keep_threshold")]
    pub keep_threshold: f64,
    #[serde(default = "default_confidence_rerun_threshold")]
    pub rerun_threshold: f64,
}

impl Default for ConfidenceConfig {
    fn default() -> Self {
        Self {
            min_experiments: default_confidence_min_experiments(),
            keep_threshold: default_confidence_keep_threshold(),
            rerun_threshold: default_confidence_rerun_threshold(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitConfig {
    #[serde(default = "default_git_enabled")]
    pub enabled: bool,
    #[serde(default = "default_commit_prefix")]
    pub commit_prefix: String,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            enabled: default_git_enabled(),
            commit_prefix: default_commit_prefix(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strictness {
    #[default]
    Advisory,
    Strict,
}

pub fn autoloop_dir(root: &Path) -> PathBuf {
    root.join(AUTOLOOP_DIR)
}

pub fn config_path(root: &Path) -> PathBuf {
    autoloop_dir(root).join(CONFIG_FILE)
}

impl Config {
    pub fn load(root: &Path) -> Result<Self> {
        let path = config_path(root);
        let content = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        let config = toml::from_str(&content).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source,
        })?;
        Ok(config)
    }
}

pub fn default_config_template() -> &'static str {
    r#"# autoloop v0 template
strictness = "advisory"

[metric]
name = "latency_p95"
direction = "lower"
unit = "ms"

[eval]
command = "echo 'METRIC latency_p95=42.3'"
timeout = 300
format = "metric_lines"
retries = 1

[confidence]
min_experiments = 3
keep_threshold = 1.0
rerun_threshold = 2.0

[git]
enabled = true
commit_prefix = "experiment:"
"#
}

fn default_eval_timeout() -> u64 {
    300
}

fn default_eval_retries() -> u32 {
    1
}

fn default_confidence_min_experiments() -> usize {
    3
}

fn default_confidence_keep_threshold() -> f64 {
    1.0
}

fn default_confidence_rerun_threshold() -> f64 {
    2.0
}

fn default_git_enabled() -> bool {
    true
}

fn default_commit_prefix() -> String {
    "experiment:".to_string()
}
