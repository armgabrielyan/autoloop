use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::{MetricDirection, autoloop_dir};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryRate {
    pub name: String,
    pub kept: usize,
    pub discarded: usize,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreakKind {
    Keep,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreakSummary {
    pub kind: StreakKind,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BestImprovement {
    pub experiment_id: u64,
    pub metric_name: String,
    pub metric_value: f64,
    pub delta_from_baseline: f64,
    #[serde(default)]
    pub percent_from_baseline: Option<f64>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExperimentAnalysis {
    pub experiments_run: usize,
    pub kept: usize,
    pub discarded: usize,
    pub crashed: usize,
    #[serde(default)]
    pub current_streak: Option<StreakSummary>,
    #[serde(default)]
    pub best_improvement: Option<BestImprovement>,
    #[serde(default)]
    pub cumulative_improvement: Option<f64>,
    #[serde(default)]
    pub category_rates: Vec<CategoryRate>,
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
    let records = load_records(root)?;
    let mut summary = ExperimentSummary::default();

    for record in &records {
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
    let records = load_records(root)?;
    let mut observations = Vec::new();

    for record in records {
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

pub fn analyze_records(
    root: &Path,
    session_id: Option<&str>,
    direction: MetricDirection,
) -> Result<ExperimentAnalysis> {
    let records = load_records(root)?;
    let filtered: Vec<&ExperimentRecord> = records
        .iter()
        .filter(|record| match session_id {
            Some(session_id) => record.session_id.as_deref() == Some(session_id),
            None => true,
        })
        .collect();
    let finalized: Vec<&ExperimentRecord> = filtered
        .into_iter()
        .filter(|record| !matches!(record.status, ExperimentStatus::Baseline))
        .collect();

    let mut analysis = ExperimentAnalysis::default();
    analysis.experiments_run = finalized.len();

    for record in &finalized {
        match record.status {
            ExperimentStatus::Kept => analysis.kept += 1,
            ExperimentStatus::Discarded => analysis.discarded += 1,
            ExperimentStatus::Crashed => analysis.crashed += 1,
            ExperimentStatus::Baseline => {}
        }
    }

    analysis.current_streak = current_streak(&finalized);
    analysis.best_improvement = best_improvement(&finalized, direction);
    analysis.cumulative_improvement = analysis
        .best_improvement
        .as_ref()
        .and_then(|best| best.percent_from_baseline);
    analysis.category_rates = category_rates(&finalized);

    Ok(analysis)
}

fn load_records(root: &Path) -> Result<Vec<ExperimentRecord>> {
    let path = experiments_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file =
        fs::File::open(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for line in reader.lines() {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        let record: ExperimentRecord = serde_json::from_str(&line)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        records.push(record);
    }

    Ok(records)
}

fn current_streak(records: &[&ExperimentRecord]) -> Option<StreakSummary> {
    let latest = records.last()?;
    let kind = if matches!(latest.status, ExperimentStatus::Kept) {
        StreakKind::Keep
    } else {
        StreakKind::Failure
    };

    let count = records
        .iter()
        .rev()
        .take_while(|record| match kind {
            StreakKind::Keep => matches!(record.status, ExperimentStatus::Kept),
            StreakKind::Failure => {
                matches!(
                    record.status,
                    ExperimentStatus::Discarded | ExperimentStatus::Crashed
                )
            }
        })
        .count();

    Some(StreakSummary { kind, count })
}

fn best_improvement(
    records: &[&ExperimentRecord],
    direction: MetricDirection,
) -> Option<BestImprovement> {
    records
        .iter()
        .filter(|record| matches!(record.status, ExperimentStatus::Kept))
        .filter_map(|record| {
            let metric = record.metric.as_ref()?;
            let delta = metric.delta_from_baseline?;
            if !is_improvement(direction, delta) {
                return None;
            }

            Some(BestImprovement {
                experiment_id: record.id,
                metric_name: metric.name.clone(),
                metric_value: metric.value,
                delta_from_baseline: delta,
                percent_from_baseline: metric
                    .baseline
                    .filter(|baseline| *baseline != 0.0)
                    .map(|baseline| delta / baseline * 100.0),
                unit: metric.unit.clone(),
                description: record.description.clone(),
            })
        })
        .min_by(|left, right| compare_improvements(direction, left, right))
}

fn compare_improvements(
    direction: MetricDirection,
    left: &BestImprovement,
    right: &BestImprovement,
) -> std::cmp::Ordering {
    match direction {
        MetricDirection::Lower => left
            .delta_from_baseline
            .partial_cmp(&right.delta_from_baseline)
            .unwrap_or(std::cmp::Ordering::Equal),
        MetricDirection::Higher => right
            .delta_from_baseline
            .partial_cmp(&left.delta_from_baseline)
            .unwrap_or(std::cmp::Ordering::Equal),
    }
}

fn is_improvement(direction: MetricDirection, delta: f64) -> bool {
    match direction {
        MetricDirection::Lower => delta < 0.0,
        MetricDirection::Higher => delta > 0.0,
    }
}

fn category_rates(records: &[&ExperimentRecord]) -> Vec<CategoryRate> {
    let mut stats: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for record in records {
        let is_kept = matches!(record.status, ExperimentStatus::Kept);
        let is_discarded = matches!(record.status, ExperimentStatus::Discarded);
        if !is_kept && !is_discarded {
            continue;
        }

        let Some(tags) = &record.tags else {
            continue;
        };
        for category in &tags.auto_categories {
            let entry = stats.entry(category.clone()).or_default();
            if is_kept {
                entry.0 += 1;
            } else if is_discarded {
                entry.1 += 1;
            }
        }
    }

    let mut categories: Vec<CategoryRate> = stats
        .into_iter()
        .map(|(name, (kept, discarded))| {
            let total = kept + discarded;
            let success_rate = if total == 0 {
                0.0
            } else {
                kept as f64 / total as f64
            };
            CategoryRate {
                name,
                kept,
                discarded,
                success_rate,
            }
        })
        .collect();

    categories.sort_by(|left, right| {
        right
            .success_rate
            .partial_cmp(&left.success_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| (right.kept + right.discarded).cmp(&(left.kept + left.discarded)))
            .then_with(|| left.name.cmp(&right.name))
    });
    categories
}
