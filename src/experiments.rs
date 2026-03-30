use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::autoloop_dir;
use crate::state::{CommandCapture, EvalVerdict, GuardrailOutcome};

pub const EXPERIMENTS_FILE: &str = "experiments.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    Baseline,
    Kept,
    Discarded,
    Crashed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentRecord {
    pub id: u64,
    #[serde(default)]
    pub session_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub status: ExperimentStatus,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub metric: Option<MetricRecord>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub verdict: Option<EvalVerdict>,
    #[serde(default)]
    pub guardrails: Vec<GuardrailOutcome>,
    #[serde(default)]
    pub command: Option<CommandCapture>,
    #[serde(default)]
    pub tags: Option<ExperimentTags>,
    #[serde(default)]
    pub diff_summary: Option<String>,
    #[serde(default)]
    pub diff: Option<String>,
    #[serde(default)]
    pub commit_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricRecord {
    pub name: String,
    pub value: f64,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub baseline: Option<f64>,
    #[serde(default)]
    pub delta_from_baseline: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentTags {
    #[serde(default)]
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub auto_categories: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExperimentSummary {
    pub total: usize,
    pub baseline: usize,
    pub kept: usize,
    pub discarded: usize,
    pub crashed: usize,
}

pub fn experiments_path(root: &Path) -> PathBuf {
    autoloop_dir(root).join(EXPERIMENTS_FILE)
}

pub fn append_record(root: &Path, record: &ExperimentRecord) -> Result<()> {
    let path = experiments_path(root);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    let line = serde_json::to_string(record)?;
    writeln!(file, "{line}").with_context(|| format!("failed to append {}", path.display()))
}

pub fn count_records(root: &Path) -> Result<usize> {
    Ok(summarize_records(root)?.total)
}

pub fn summarize_records(root: &Path) -> Result<ExperimentSummary> {
    let path = experiments_path(root);
    if !path.exists() {
        return Ok(ExperimentSummary::default());
    }

    let file =
        fs::File::open(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut summary = ExperimentSummary::default();

    for line in reader.lines() {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        let record: ExperimentRecord = serde_json::from_str(&line)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        summary.total += 1;
        match record.status {
            ExperimentStatus::Baseline => summary.baseline += 1,
            ExperimentStatus::Kept => summary.kept += 1,
            ExperimentStatus::Discarded => summary.discarded += 1,
            ExperimentStatus::Crashed => summary.crashed += 1,
        }
    }

    Ok(summary)
}

pub fn metric_observations(root: &Path, metric_name: &str) -> Result<Vec<f64>> {
    let path = experiments_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file =
        fs::File::open(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut observations = Vec::new();

    for line in reader.lines() {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        let record: ExperimentRecord = serde_json::from_str(&line)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        if matches!(record.status, ExperimentStatus::Crashed) {
            continue;
        }
        if let Some(metric) = record.metric {
            if metric.name == metric_name {
                observations.push(metric.value);
            }
        }
    }

    Ok(observations)
}
