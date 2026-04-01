use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::experiments::{ExperimentRecord, ExperimentStatus, load_records};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizeCandidate {
    pub experiment_id: u64,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub commit_hash: String,
    #[serde(default)]
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizeSkipped {
    pub experiment_id: u64,
    #[serde(default)]
    pub description: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizeGroup {
    pub index: usize,
    #[serde(default)]
    pub branch_name: Option<String>,
    pub slug: String,
    pub experiment_ids: Vec<u64>,
    pub commit_hashes: Vec<String>,
    #[serde(default)]
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub descriptions: Vec<String>,
    #[serde(default)]
    pub best_improvement: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FinalizePlan {
    #[serde(default)]
    pub groups: Vec<FinalizeGroup>,
    #[serde(default)]
    pub skipped: Vec<FinalizeSkipped>,
}

pub fn build_finalize_plan(root: &Path, session_id: Option<&str>) -> Result<FinalizePlan> {
    let records = load_records(root)?;
    let mut candidates = Vec::new();
    let mut skipped = Vec::new();

    for record in records
        .iter()
        .filter(|record| matches!(record.status, ExperimentStatus::Kept))
    {
        if let Some(session_id) = session_id
            && record.session_id.as_deref() != Some(session_id)
        {
            continue;
        }

        match finalize_candidate(record) {
            Some(candidate) => candidates.push(candidate),
            None => skipped.push(FinalizeSkipped {
                experiment_id: record.id,
                description: record.description.clone(),
                reason: "missing recorded commit hash; rerun `autoloop keep --commit` for finalize support".to_string(),
            }),
        }
    }

    candidates.sort_by_key(|candidate| candidate.experiment_id);
    let groups = finalize_groups(&candidates);

    Ok(FinalizePlan { groups, skipped })
}

fn finalize_candidate(record: &ExperimentRecord) -> Option<FinalizeCandidate> {
    let commit_hash = record.commit_hash.clone()?;
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

    Some(FinalizeCandidate {
        experiment_id: record.id,
        session_id: record.session_id.clone(),
        description: record.description.clone(),
        commit_hash,
        file_paths: record
            .tags
            .as_ref()
            .map(|tags| tags.file_paths.clone())
            .unwrap_or_default(),
        categories: record
            .tags
            .as_ref()
            .map(|tags| tags.auto_categories.clone())
            .unwrap_or_default(),
        metric_name,
        metric_value,
        delta_from_baseline,
        percent_from_baseline,
        unit,
    })
}

fn finalize_groups(candidates: &[FinalizeCandidate]) -> Vec<FinalizeGroup> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let mut parent: Vec<usize> = (0..candidates.len()).collect();
    for i in 0..candidates.len() {
        for j in (i + 1)..candidates.len() {
            if overlaps(&candidates[i].file_paths, &candidates[j].file_paths) {
                union(&mut parent, i, j);
            }
        }
    }

    let mut grouped: BTreeMap<usize, Vec<&FinalizeCandidate>> = BTreeMap::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let root = find(&mut parent, index);
        grouped.entry(root).or_default().push(candidate);
    }

    let mut groups: Vec<FinalizeGroup> = grouped
        .into_values()
        .map(|records| {
            let mut file_paths = BTreeSet::new();
            let mut categories = BTreeSet::new();
            let mut descriptions = Vec::new();
            let mut experiment_ids = Vec::new();
            let mut commit_hashes = Vec::new();
            let mut best_improvement = None::<f64>;

            for candidate in &records {
                experiment_ids.push(candidate.experiment_id);
                commit_hashes.push(candidate.commit_hash.clone());
                file_paths.extend(candidate.file_paths.iter().cloned());
                categories.extend(candidate.categories.iter().cloned());
                if let Some(description) = &candidate.description {
                    descriptions.push(description.clone());
                }
                if let Some(percent) = candidate.percent_from_baseline {
                    best_improvement = Some(match best_improvement {
                        Some(current) if current >= percent => current,
                        _ => percent,
                    });
                }
            }

            FinalizeGroup {
                index: 0,
                branch_name: None,
                slug: branch_slug(&file_paths, &categories, &descriptions, experiment_ids[0]),
                experiment_ids,
                commit_hashes,
                file_paths: file_paths.into_iter().collect(),
                categories: categories.into_iter().collect(),
                descriptions,
                best_improvement,
            }
        })
        .collect();

    groups.sort_by_key(|group| group.experiment_ids.first().copied().unwrap_or(u64::MAX));
    for (index, group) in groups.iter_mut().enumerate() {
        group.index = index + 1;
    }
    groups
}

fn overlaps(left: &[String], right: &[String]) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }

    let right: BTreeSet<&str> = right.iter().map(String::as_str).collect();
    left.iter().any(|path| right.contains(path.as_str()))
}

fn find(parent: &mut [usize], node: usize) -> usize {
    if parent[node] != node {
        let root = find(parent, parent[node]);
        parent[node] = root;
    }
    parent[node]
}

fn union(parent: &mut [usize], left: usize, right: usize) {
    let left_root = find(parent, left);
    let right_root = find(parent, right);
    if left_root != right_root {
        parent[right_root] = left_root;
    }
}

fn branch_slug(
    file_paths: &BTreeSet<String>,
    categories: &BTreeSet<String>,
    descriptions: &[String],
    experiment_id: u64,
) -> String {
    let generic = ["src", "tests", "test", "bin", "lib"];
    let mut parts: Vec<String> = categories
        .iter()
        .filter(|category| !generic.contains(&category.as_str()) && !category.contains('.'))
        .take(2)
        .cloned()
        .collect();

    if parts.is_empty() {
        parts.extend(
            file_paths
                .iter()
                .filter_map(|path| {
                    std::path::Path::new(path)
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .map(ToString::to_string)
                })
                .take(2),
        );
    }

    if parts.is_empty() {
        parts.extend(
            descriptions
                .iter()
                .flat_map(|description| description.split_whitespace())
                .map(sanitize_slug)
                .filter(|token| !token.is_empty())
                .take(2),
        );
    }

    if parts.is_empty() {
        return format!("group-{experiment_id}");
    }

    sanitize_slug(&parts.join("-"))
}

fn sanitize_slug(input: &str) -> String {
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
