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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedExperiment {
    pub experiment_id: u64,
    #[serde(default)]
    pub session_id: Option<String>,
    pub status: ExperimentStatus,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub metric_name: Option<String>,
    #[serde(default)]
    pub metric_value: Option<f64>,
    #[serde(default)]
    pub delta_from_baseline: Option<f64>,
    #[serde(default)]
    pub percent_from_baseline: Option<f64>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub file_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadEndCategory {
    pub name: String,
    pub attempts: usize,
    pub discarded: usize,
    pub crashed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePattern {
    pub path: String,
    pub attempts: usize,
    pub kept: usize,
    pub discarded: usize,
    pub crashed: usize,
    pub success_rate: f64,
    pub signal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTrajectory {
    #[serde(default)]
    pub session_id: Option<String>,
    pub experiments_run: usize,
    pub kept: usize,
    pub discarded: usize,
    pub crashed: usize,
    #[serde(default)]
    pub best_improvement: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LearnReport {
    pub summary: ExperimentAnalysis,
    pub sessions_seen: usize,
    #[serde(default)]
    pub best_experiments: Vec<RankedExperiment>,
    #[serde(default)]
    pub worst_experiments: Vec<RankedExperiment>,
    #[serde(default)]
    pub dead_end_categories: Vec<DeadEndCategory>,
    #[serde(default)]
    pub file_patterns: Vec<FilePattern>,
    #[serde(default)]
    pub session_trajectory: Vec<SessionTrajectory>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuerySource {
    WorkingTree,
    Description,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorySignal {
    pub name: String,
    pub attempts: usize,
    pub kept: usize,
    pub discarded: usize,
    pub crashed: usize,
    pub success_rate: f64,
    pub sampling_probability: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarExperiment {
    pub experiment_id: u64,
    #[serde(default)]
    pub session_id: Option<String>,
    pub status: ExperimentStatus,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub metric_name: Option<String>,
    #[serde(default)]
    pub metric_value: Option<f64>,
    #[serde(default)]
    pub delta_from_baseline: Option<f64>,
    #[serde(default)]
    pub percent_from_baseline: Option<f64>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub shared_categories: Vec<String>,
    #[serde(default)]
    pub shared_file_paths: Vec<String>,
    pub exact_description_match: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreflightVerdict {
    Proceed,
    Caution,
    Avoid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightReport {
    pub description: String,
    pub source: QuerySource,
    #[serde(default)]
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    pub exact_matches: usize,
    pub similar_experiments: usize,
    pub kept: usize,
    pub discarded: usize,
    pub crashed: usize,
    #[serde(default)]
    pub category_signals: Vec<CategorySignal>,
    #[serde(default)]
    pub matches: Vec<SimilarExperiment>,
    pub verdict: PreflightVerdict,
    pub verdict_reason: String,
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

pub fn latest_session_id(root: &Path) -> Result<Option<String>> {
    let records = load_records(root)?;
    Ok(records
        .iter()
        .filter_map(|record| {
            record
                .session_id
                .as_ref()
                .map(|session_id| (record.timestamp, session_id.clone()))
        })
        .max_by_key(|(timestamp, _)| *timestamp)
        .map(|(_, session_id)| session_id))
}

pub fn analyze_records(
    root: &Path,
    session_id: Option<&str>,
    direction: MetricDirection,
) -> Result<ExperimentAnalysis> {
    let records = load_records(root)?;
    let filtered = filter_records(&records, session_id);
    let finalized = finalized_records(&filtered);
    Ok(analysis_from_finalized(&finalized, direction))
}

pub fn learn_report(
    root: &Path,
    session_id: Option<&str>,
    direction: MetricDirection,
) -> Result<LearnReport> {
    let records = load_records(root)?;
    let filtered = filter_records(&records, session_id);
    let finalized = finalized_records(&filtered);

    Ok(LearnReport {
        summary: analysis_from_finalized(&finalized, direction),
        sessions_seen: unique_sessions(&filtered),
        best_experiments: ranked_best_experiments(&finalized, direction, 3),
        worst_experiments: ranked_worst_experiments(&finalized, direction, 3),
        dead_end_categories: dead_end_categories(&finalized),
        file_patterns: consistent_file_patterns(&finalized),
        session_trajectory: session_trajectory(&finalized, direction),
    })
}

pub fn preflight_report(
    root: &Path,
    description: &str,
    source: QuerySource,
    file_paths: &[String],
    categories: &[String],
) -> Result<PreflightReport> {
    let records = load_records(root)?;
    let finalized: Vec<&ExperimentRecord> = records
        .iter()
        .filter(|record| !matches!(record.status, ExperimentStatus::Baseline))
        .collect();
    let normalized_description = normalize_description(description);
    let mut matches: Vec<SimilarExperiment> = finalized
        .iter()
        .filter_map(|record| {
            let exact_description_match = record
                .description
                .as_deref()
                .map(normalize_description)
                .as_deref()
                == Some(normalized_description.as_str());
            let shared_categories = shared_categories(record, categories);
            let shared_file_paths = shared_file_paths(record, file_paths);
            if !exact_description_match
                && shared_categories.is_empty()
                && shared_file_paths.is_empty()
            {
                return None;
            }

            Some(similar_experiment(
                record,
                exact_description_match,
                shared_categories,
                shared_file_paths,
            ))
        })
        .collect();

    matches.sort_by(|left, right| {
        similarity_score(right)
            .cmp(&similarity_score(left))
            .then_with(|| right.experiment_id.cmp(&left.experiment_id))
    });

    let exact_matches = matches
        .iter()
        .filter(|entry| entry.exact_description_match)
        .count();
    let similar_experiments = matches.len();
    let kept = matches
        .iter()
        .filter(|entry| matches!(entry.status, ExperimentStatus::Kept))
        .count();
    let discarded = matches
        .iter()
        .filter(|entry| matches!(entry.status, ExperimentStatus::Discarded))
        .count();
    let crashed = matches
        .iter()
        .filter(|entry| matches!(entry.status, ExperimentStatus::Crashed))
        .count();
    let category_signals = category_signals(&finalized, categories);
    let (verdict, verdict_reason) = preflight_verdict(
        exact_matches,
        kept,
        discarded,
        crashed,
        similar_experiments,
        &category_signals,
    );

    matches.truncate(5);

    Ok(PreflightReport {
        description: description.to_string(),
        source,
        file_paths: file_paths.to_vec(),
        categories: categories.to_vec(),
        exact_matches,
        similar_experiments,
        kept,
        discarded,
        crashed,
        category_signals,
        matches,
        verdict,
        verdict_reason,
    })
}

pub fn load_records(root: &Path) -> Result<Vec<ExperimentRecord>> {
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

fn filter_records<'a>(
    records: &'a [ExperimentRecord],
    session_id: Option<&str>,
) -> Vec<&'a ExperimentRecord> {
    records
        .iter()
        .filter(|record| match session_id {
            Some(session_id) => record.session_id.as_deref() == Some(session_id),
            None => true,
        })
        .collect()
}

fn finalized_records<'a>(records: &[&'a ExperimentRecord]) -> Vec<&'a ExperimentRecord> {
    records
        .iter()
        .copied()
        .filter(|record| !matches!(record.status, ExperimentStatus::Baseline))
        .collect()
}

fn analysis_from_finalized(
    finalized: &[&ExperimentRecord],
    direction: MetricDirection,
) -> ExperimentAnalysis {
    let mut analysis = ExperimentAnalysis {
        experiments_run: finalized.len(),
        ..ExperimentAnalysis::default()
    };

    for record in finalized {
        match record.status {
            ExperimentStatus::Kept => analysis.kept += 1,
            ExperimentStatus::Discarded => analysis.discarded += 1,
            ExperimentStatus::Crashed => analysis.crashed += 1,
            ExperimentStatus::Baseline => {}
        }
    }

    analysis.current_streak = current_streak(finalized);
    analysis.best_improvement = best_improvement(finalized, direction);
    analysis.cumulative_improvement = analysis
        .best_improvement
        .as_ref()
        .and_then(|best| best.percent_from_baseline);
    analysis.category_rates = category_rates(finalized);
    analysis
}

fn unique_sessions(records: &[&ExperimentRecord]) -> usize {
    records
        .iter()
        .filter_map(|record| record.session_id.as_deref())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
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

fn ranked_best_experiments(
    records: &[&ExperimentRecord],
    direction: MetricDirection,
    limit: usize,
) -> Vec<RankedExperiment> {
    let mut ranked: Vec<RankedExperiment> = records
        .iter()
        .filter(|record| matches!(record.status, ExperimentStatus::Kept))
        .filter_map(|record| {
            let ranked = ranked_experiment(record)?;
            let delta = ranked.delta_from_baseline?;
            is_improvement(direction, delta).then_some(ranked)
        })
        .collect();

    ranked.sort_by(|left, right| {
        normalized_delta(direction, right.delta_from_baseline)
            .partial_cmp(&normalized_delta(direction, left.delta_from_baseline))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.experiment_id.cmp(&right.experiment_id))
    });
    ranked.truncate(limit);
    ranked
}

fn ranked_worst_experiments(
    records: &[&ExperimentRecord],
    direction: MetricDirection,
    limit: usize,
) -> Vec<RankedExperiment> {
    let mut ranked: Vec<RankedExperiment> = records
        .iter()
        .filter_map(|record| {
            let ranked = ranked_experiment(record)?;
            let delta = ranked.delta_from_baseline?;
            (!is_improvement(direction, delta)).then_some(ranked)
        })
        .collect();

    ranked.sort_by(|left, right| {
        normalized_delta(direction, left.delta_from_baseline)
            .partial_cmp(&normalized_delta(direction, right.delta_from_baseline))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.experiment_id.cmp(&right.experiment_id))
    });
    ranked.truncate(limit);
    ranked
}

fn ranked_experiment(record: &ExperimentRecord) -> Option<RankedExperiment> {
    let metric = record.metric.as_ref()?;
    Some(RankedExperiment {
        experiment_id: record.id,
        session_id: record.session_id.clone(),
        status: record.status.clone(),
        description: record.description.clone(),
        reason: record.reason.clone(),
        metric_name: Some(metric.name.clone()),
        metric_value: Some(metric.value),
        delta_from_baseline: metric.delta_from_baseline,
        percent_from_baseline: metric
            .baseline
            .filter(|baseline| *baseline != 0.0)
            .zip(metric.delta_from_baseline)
            .map(|(baseline, delta)| delta / baseline * 100.0),
        unit: metric.unit.clone(),
        categories: record
            .tags
            .as_ref()
            .map(|tags| tags.auto_categories.clone())
            .unwrap_or_default(),
        file_paths: record
            .tags
            .as_ref()
            .map(|tags| tags.file_paths.clone())
            .unwrap_or_default(),
    })
}

fn dead_end_categories(records: &[&ExperimentRecord]) -> Vec<DeadEndCategory> {
    let mut stats: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();

    for record in records {
        let Some(tags) = &record.tags else {
            continue;
        };
        for category in &tags.auto_categories {
            let entry = stats.entry(category.clone()).or_default();
            match record.status {
                ExperimentStatus::Kept => entry.0 += 1,
                ExperimentStatus::Discarded => entry.1 += 1,
                ExperimentStatus::Crashed => entry.2 += 1,
                ExperimentStatus::Baseline => {}
            }
        }
    }

    let mut dead_ends: Vec<DeadEndCategory> = stats
        .into_iter()
        .filter_map(|(name, (kept, discarded, crashed))| {
            let attempts = kept + discarded + crashed;
            (attempts >= 3 && kept == 0).then_some(DeadEndCategory {
                name,
                attempts,
                discarded,
                crashed,
            })
        })
        .collect();

    dead_ends.sort_by(|left, right| {
        right
            .attempts
            .cmp(&left.attempts)
            .then_with(|| left.name.cmp(&right.name))
    });
    dead_ends
}

fn consistent_file_patterns(records: &[&ExperimentRecord]) -> Vec<FilePattern> {
    let mut stats: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();

    for record in records {
        let Some(tags) = &record.tags else {
            continue;
        };
        for path in &tags.file_paths {
            let entry = stats.entry(path.clone()).or_default();
            match record.status {
                ExperimentStatus::Kept => entry.0 += 1,
                ExperimentStatus::Discarded => entry.1 += 1,
                ExperimentStatus::Crashed => entry.2 += 1,
                ExperimentStatus::Baseline => {}
            }
        }
    }

    let mut patterns: Vec<FilePattern> = stats
        .into_iter()
        .filter_map(|(path, (kept, discarded, crashed))| {
            let attempts = kept + discarded + crashed;
            if attempts < 2 {
                return None;
            }

            let signal = if kept == attempts {
                Some("always_kept".to_string())
            } else if kept == 0 {
                Some("never_kept".to_string())
            } else {
                None
            }?;

            let success_rate = kept as f64 / attempts as f64;
            Some(FilePattern {
                path,
                attempts,
                kept,
                discarded,
                crashed,
                success_rate,
                signal,
            })
        })
        .collect();

    patterns.sort_by(|left, right| {
        right
            .attempts
            .cmp(&left.attempts)
            .then_with(|| left.path.cmp(&right.path))
    });
    patterns
}

fn session_trajectory(
    records: &[&ExperimentRecord],
    direction: MetricDirection,
) -> Vec<SessionTrajectory> {
    let mut grouped: BTreeMap<Option<String>, Vec<&ExperimentRecord>> = BTreeMap::new();

    for record in records {
        grouped
            .entry(record.session_id.clone())
            .or_default()
            .push(*record);
    }

    let mut trajectory: Vec<SessionTrajectory> = grouped
        .into_iter()
        .map(|(session_id, records)| {
            let analysis = analysis_from_finalized(&records, direction);
            let best_improvement = analysis
                .best_improvement
                .as_ref()
                .and_then(|best| best.percent_from_baseline);
            SessionTrajectory {
                session_id,
                experiments_run: analysis.experiments_run,
                kept: analysis.kept,
                discarded: analysis.discarded,
                crashed: analysis.crashed,
                best_improvement,
            }
        })
        .collect();

    trajectory.sort_by(|left, right| left.session_id.cmp(&right.session_id));
    trajectory
}

fn normalized_delta(direction: MetricDirection, delta: Option<f64>) -> f64 {
    match (direction, delta) {
        (_, None) => 0.0,
        (MetricDirection::Lower, Some(delta)) => -delta,
        (MetricDirection::Higher, Some(delta)) => delta,
    }
}

fn shared_categories(record: &ExperimentRecord, categories: &[String]) -> Vec<String> {
    let Some(tags) = &record.tags else {
        return Vec::new();
    };

    tags.auto_categories
        .iter()
        .filter(|category| categories.iter().any(|candidate| candidate == *category))
        .cloned()
        .collect()
}

fn shared_file_paths(record: &ExperimentRecord, file_paths: &[String]) -> Vec<String> {
    let Some(tags) = &record.tags else {
        return Vec::new();
    };

    tags.file_paths
        .iter()
        .filter(|path| file_paths.iter().any(|candidate| candidate == *path))
        .cloned()
        .collect()
}

fn similar_experiment(
    record: &ExperimentRecord,
    exact_description_match: bool,
    shared_categories: Vec<String>,
    shared_file_paths: Vec<String>,
) -> SimilarExperiment {
    let (metric_name, metric_value, delta_from_baseline, percent_from_baseline, unit) =
        if let Some(metric) = &record.metric {
            (
                Some(metric.name.clone()),
                Some(metric.value),
                metric.delta_from_baseline,
                metric
                    .baseline
                    .filter(|baseline| *baseline != 0.0)
                    .zip(metric.delta_from_baseline)
                    .map(|(baseline, delta)| delta / baseline * 100.0),
                metric.unit.clone(),
            )
        } else {
            (None, None, None, None, None)
        };

    SimilarExperiment {
        experiment_id: record.id,
        session_id: record.session_id.clone(),
        status: record.status.clone(),
        description: record.description.clone(),
        reason: record.reason.clone(),
        metric_name,
        metric_value,
        delta_from_baseline,
        percent_from_baseline,
        unit,
        shared_categories,
        shared_file_paths,
        exact_description_match,
    }
}

fn similarity_score(experiment: &SimilarExperiment) -> usize {
    let exact = usize::from(experiment.exact_description_match) * 100;
    exact + experiment.shared_file_paths.len() * 10 + experiment.shared_categories.len()
}

fn category_signals(records: &[&ExperimentRecord], categories: &[String]) -> Vec<CategorySignal> {
    let mut signals = Vec::new();

    for category in categories {
        let mut kept = 0;
        let mut discarded = 0;
        let mut crashed = 0;

        for record in records {
            let Some(tags) = &record.tags else {
                continue;
            };
            if !tags
                .auto_categories
                .iter()
                .any(|candidate| candidate == category)
            {
                continue;
            }

            match record.status {
                ExperimentStatus::Kept => kept += 1,
                ExperimentStatus::Discarded => discarded += 1,
                ExperimentStatus::Crashed => crashed += 1,
                ExperimentStatus::Baseline => {}
            }
        }

        let attempts = kept + discarded + crashed;
        if attempts == 0 {
            continue;
        }

        signals.push(CategorySignal {
            name: category.clone(),
            attempts,
            kept,
            discarded,
            crashed,
            success_rate: kept as f64 / attempts as f64,
            sampling_probability: (kept as f64 + 1.0) / (attempts as f64 + 2.0),
        });
    }

    signals.sort_by(|left, right| {
        right
            .sampling_probability
            .partial_cmp(&left.sampling_probability)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.attempts.cmp(&left.attempts))
            .then_with(|| left.name.cmp(&right.name))
    });
    signals
}

fn preflight_verdict(
    exact_matches: usize,
    kept: usize,
    discarded: usize,
    crashed: usize,
    similar_experiments: usize,
    category_signals: &[CategorySignal],
) -> (PreflightVerdict, String) {
    let failures = discarded + crashed;
    if similar_experiments == 0 {
        return (
            PreflightVerdict::Proceed,
            "no similar experiments are recorded yet".to_string(),
        );
    }

    if exact_matches > 0 && kept == 0 {
        return (
            PreflightVerdict::Avoid,
            "this exact description only appears in failed experiments".to_string(),
        );
    }

    if failures >= 2 && kept == 0 {
        return (
            PreflightVerdict::Avoid,
            "similar experiments have repeatedly failed".to_string(),
        );
    }

    if kept > failures {
        return (
            PreflightVerdict::Proceed,
            "similar experiments have a positive history".to_string(),
        );
    }

    if let Some(best_category) = category_signals.first() {
        if best_category.success_rate >= 0.6 {
            return (
                PreflightVerdict::Proceed,
                format!(
                    "category `{}` has worked {}/{} times",
                    best_category.name, best_category.kept, best_category.attempts
                ),
            );
        }
    }

    (
        PreflightVerdict::Caution,
        "history is mixed; proceed carefully and validate quickly".to_string(),
    )
}

fn normalize_description(description: &str) -> String {
    description.trim().to_ascii_lowercase()
}
