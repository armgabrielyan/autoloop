use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use regex::Regex;
use serde::Serialize;

use crate::config::{Config, GuardrailConfig, GuardrailKind, MetricDirection, default_config};
use crate::eval::formats::MetricFormat;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    Inferred,
    Partial,
    Template,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectKind {
    Rust,
    Python,
    Node,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigInference {
    pub source: ConfigSource,
    pub project_kind: ProjectKind,
    pub metric_name: String,
    pub metric_direction: MetricDirection,
    pub metric_unit: Option<String>,
    pub eval_command: String,
    pub eval_format: MetricFormat,
    pub guardrail_commands: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
struct EvalCandidate {
    command: String,
    metric_name: Option<String>,
    format: MetricFormat,
    note: String,
}

pub fn infer_config(root: &Path) -> Result<(Config, ConfigInference)> {
    let project_kind = detect_project_kind(root);
    let eval = detect_eval_candidate(root, project_kind);
    let guardrail = detect_guardrail(root, project_kind);
    let mut config = default_config();
    let mut notes = Vec::new();

    if let Some(candidate) = &eval {
        config.eval.command = candidate.command.clone();
        config.eval.format = candidate.format;
        config.metric.name = candidate
            .metric_name
            .clone()
            .unwrap_or_else(|| config.metric.name.clone());
        notes.push(candidate.note.clone());
    } else {
        notes.push("No repo-specific eval command detected; leaving the default template command in place.".to_string());
    }

    if let Some(command) = &guardrail {
        config.guardrails = vec![GuardrailConfig {
            name: "tests_pass".to_string(),
            command: command.clone(),
            kind: GuardrailKind::PassFail,
            format: MetricFormat::Auto,
            regex: None,
            threshold: None,
        }];
        notes.push(format!("Detected pass/fail guardrail: `{command}`"));
    } else {
        notes.push("No obvious pass/fail guardrail command detected.".to_string());
    }

    config.metric.direction = infer_direction(&config.metric.name);
    config.metric.unit = infer_unit(&config.metric.name);

    let source = match (eval.is_some(), guardrail.is_some()) {
        (true, _) => ConfigSource::Inferred,
        (false, true) => ConfigSource::Partial,
        (false, false) => ConfigSource::Template,
    };

    let inference = ConfigInference {
        source,
        project_kind,
        metric_name: config.metric.name.clone(),
        metric_direction: config.metric.direction,
        metric_unit: config.metric.unit.clone(),
        eval_command: config.eval.command.clone(),
        eval_format: config.eval.format,
        guardrail_commands: config
            .guardrails
            .iter()
            .map(|guardrail| guardrail.command.clone())
            .collect(),
        notes,
    };

    Ok((config, inference))
}

fn detect_project_kind(root: &Path) -> ProjectKind {
    if root.join("Cargo.toml").exists() {
        ProjectKind::Rust
    } else if root.join("package.json").exists() {
        ProjectKind::Node
    } else if root.join("pyproject.toml").exists() || has_extension(root, "py") {
        ProjectKind::Python
    } else {
        ProjectKind::Unknown
    }
}

fn detect_eval_candidate(root: &Path, project_kind: ProjectKind) -> Option<EvalCandidate> {
    match project_kind {
        ProjectKind::Rust => detect_rust_eval(root).or_else(|| detect_python_eval(root)),
        ProjectKind::Python => detect_python_eval(root),
        ProjectKind::Node => detect_node_eval(root).or_else(|| detect_python_eval(root)),
        ProjectKind::Unknown => detect_python_eval(root).or_else(|| detect_rust_eval(root)),
    }
}

fn detect_guardrail(root: &Path, project_kind: ProjectKind) -> Option<String> {
    match project_kind {
        ProjectKind::Rust => root
            .join("Cargo.toml")
            .exists()
            .then(|| "cargo test".to_string()),
        ProjectKind::Python => detect_python_tests(root).then(|| "python3 -m unittest".to_string()),
        ProjectKind::Node => detect_node_test(root),
        ProjectKind::Unknown => {
            detect_python_tests(root).then(|| "python3 -m unittest".to_string())
        }
    }
}

fn detect_python_eval(root: &Path) -> Option<EvalCandidate> {
    for relative in [
        "bench.py",
        "benchmark.py",
        "eval.py",
        "scripts/bench.py",
        "scripts/benchmark.py",
        "scripts/eval.py",
    ] {
        let path = root.join(relative);
        if !path.exists() {
            continue;
        }
        let metric_name = metric_name_from_file(&path);
        return Some(EvalCandidate {
            command: format!("python3 {}", normalize_relative_path(relative)),
            metric_name,
            format: MetricFormat::MetricLines,
            note: format!("Detected Python eval command from `{relative}`"),
        });
    }
    None
}

fn detect_rust_eval(root: &Path) -> Option<EvalCandidate> {
    for (relative, bin_name) in [
        ("src/bin/bench.rs", "bench"),
        ("src/bin/benchmark.rs", "benchmark"),
        ("src/bin/eval.rs", "eval"),
    ] {
        let path = root.join(relative);
        if !path.exists() {
            continue;
        }
        let metric_name = metric_name_from_file(&path);
        return Some(EvalCandidate {
            command: format!("cargo run --quiet --bin {bin_name}"),
            metric_name,
            format: MetricFormat::MetricLines,
            note: format!("Detected Rust eval binary from `{relative}`"),
        });
    }
    None
}

fn detect_node_eval(root: &Path) -> Option<EvalCandidate> {
    detect_node_script(root, &["bench", "benchmark", "eval"]).map(|script| EvalCandidate {
        command: format!("npm run {script}"),
        metric_name: None,
        format: MetricFormat::Auto,
        note: format!("Detected npm eval script `{script}`"),
    })
}

fn detect_node_test(root: &Path) -> Option<String> {
    detect_node_script(root, &["test"]).map(|script| format!("npm run {script}"))
}

fn detect_node_script(root: &Path, names: &[&str]) -> Option<String> {
    let path = root.join("package.json");
    let content = fs::read_to_string(path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    let scripts = parsed.get("scripts")?.as_object()?;
    names
        .iter()
        .find(|name| scripts.contains_key(**name))
        .map(|name| (*name).to_string())
}

fn detect_python_tests(root: &Path) -> bool {
    if root.join("tests").exists() {
        return true;
    }
    has_matching_name(root, |name| {
        name.starts_with("test_") && name.ends_with(".py") || name.ends_with("_test.py")
    })
}

fn metric_name_from_file(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let regex = Regex::new(r"METRIC\s+([A-Za-z0-9_]+)=").ok()?;
    regex
        .captures(&content)
        .and_then(|captures| captures.get(1))
        .map(|capture| capture.as_str().to_string())
}

fn infer_direction(metric_name: &str) -> MetricDirection {
    let lower = [
        "latency", "time", "duration", "memory", "size", "error", "fail",
    ];
    if lower.iter().any(|needle| metric_name.contains(needle)) {
        MetricDirection::Lower
    } else {
        MetricDirection::Higher
    }
}

fn infer_unit(metric_name: &str) -> Option<String> {
    let milliseconds = ["latency", "time", "duration"];
    milliseconds
        .iter()
        .any(|needle| metric_name.contains(needle))
        .then(|| "ms".to_string())
}

fn has_extension(root: &Path, extension: &str) -> bool {
    shallow_entries(root)
        .into_iter()
        .filter_map(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_string())
        })
        .any(|value| value == extension)
}

fn has_matching_name(root: &Path, predicate: impl Fn(&str) -> bool) -> bool {
    shallow_entries(root)
        .into_iter()
        .filter_map(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.to_string())
        })
        .any(|value| predicate(&value))
}

fn shallow_entries(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .ok()
        .into_iter()
        .flat_map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
        })
        .collect()
}

fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::{ConfigSource, ProjectKind, infer_config};
    use std::path::Path;

    #[test]
    fn infers_python_fixture() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/smoke-python-search");
        let (_, inference) = infer_config(&root).expect("inference should succeed");
        assert!(matches!(inference.project_kind, ProjectKind::Python));
        assert!(matches!(inference.source, ConfigSource::Inferred));
        assert_eq!(inference.eval_command, "python3 bench.py");
        assert_eq!(inference.metric_name, "latency_p95");
        assert_eq!(inference.guardrail_commands, vec!["python3 -m unittest"]);
    }

    #[test]
    fn infers_rust_fixture() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/smoke-rust-cli");
        let (_, inference) = infer_config(&root).expect("inference should succeed");
        assert!(matches!(inference.project_kind, ProjectKind::Rust));
        assert!(matches!(inference.source, ConfigSource::Inferred));
        assert_eq!(inference.eval_command, "cargo run --quiet --bin bench");
        assert_eq!(inference.metric_name, "latency_p95");
        assert_eq!(inference.guardrail_commands, vec!["cargo test"]);
    }
}
