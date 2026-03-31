use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::{GuardrailKind, autoloop_dir};

pub const STATE_FILE: &str = "state.json";
pub const LAST_EVAL_FILE: &str = "last_eval.json";
pub const LEARNINGS_FILE: &str = "learnings.md";
pub const SESSION_FILE: &str = "session.md";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub schema_version: u32,
    pub active_session: Option<SessionState>,
    pub baseline: Option<MetricSnapshot>,
    #[serde(default)]
    pub baseline_guardrails: Vec<GuardrailBaseline>,
    pub next_experiment_id: u64,
    #[serde(default = "default_next_session_id")]
    pub next_session_id: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            active_session: None,
            baseline: None,
            baseline_guardrails: Vec::new(),
            next_experiment_id: 1,
            next_session_id: default_next_session_id(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionState {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricSnapshot {
    pub name: String,
    pub value: f64,
    #[serde(default)]
    pub unit: Option<String>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LastEvalState {
    pub schema_version: u32,
    pub pending_eval: Option<PendingEval>,
}

impl Default for LastEvalState {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            pending_eval: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingEval {
    pub metric: MetricSnapshot,
    pub delta_from_baseline: f64,
    #[serde(default)]
    pub confidence: Option<f64>,
    pub verdict: EvalVerdict,
    pub command: CommandCapture,
    #[serde(default)]
    pub guardrails: Vec<GuardrailOutcome>,
    #[serde(default)]
    pub diff_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardrailBaseline {
    pub name: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardrailOutcome {
    pub name: String,
    pub kind: GuardrailKind,
    pub passed: bool,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub baseline: Option<f64>,
    #[serde(default)]
    pub threshold: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    pub command: CommandCapture,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandCapture {
    pub command: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalVerdict {
    Keep,
    Discard,
    Rerun,
}

fn default_next_session_id() -> u64 {
    1
}

pub fn state_path(root: &Path) -> PathBuf {
    autoloop_dir(root).join(STATE_FILE)
}

pub fn last_eval_path(root: &Path) -> PathBuf {
    autoloop_dir(root).join(LAST_EVAL_FILE)
}

pub fn learnings_path(root: &Path) -> PathBuf {
    autoloop_dir(root).join(LEARNINGS_FILE)
}

pub fn session_markdown_path(root: &Path) -> PathBuf {
    autoloop_dir(root).join(SESSION_FILE)
}

impl State {
    pub fn load(root: &Path) -> Result<Self> {
        let path = state_path(root);
        read_json(&path)
    }

    pub fn load_optional(root: &Path) -> Result<Option<Self>> {
        let path = state_path(root);
        if !path.exists() {
            return Ok(None);
        }

        Ok(Some(read_json(&path)?))
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        write_json(&state_path(root), self)
    }
}

impl LastEvalState {
    pub fn load_or_default(root: &Path) -> Result<Self> {
        let path = last_eval_path(root);
        if !path.exists() {
            return Ok(Self::default());
        }

        read_json(&path)
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        write_json(&last_eval_path(root), self)
    }
}

pub fn write_learnings_stub(root: &Path) -> Result<()> {
    let path = learnings_path(root);
    if path.exists() {
        return Ok(());
    }

    fs::write(
        &path,
        "# Learnings\n\n## What Helped\n\n## What Failed\n\n## Watchouts\n\n## Next Ideas\n",
    )
    .with_context(|| format!("failed to write {}", path.display()))
}

pub fn write_session_markdown(root: &Path, state: &State) -> Result<()> {
    let path = session_markdown_path(root);
    let body = match &state.active_session {
        Some(session) => format!(
            "# Current Session: {}\nStarted: {}\nBaseline: {}\nNext Experiment ID: {}\n",
            session.name.as_deref().unwrap_or(session.id.as_str()),
            session.started_at.to_rfc3339(),
            render_baseline(&state.baseline),
            state.next_experiment_id,
        ),
        None => format!(
            "# Current Session: none\nBaseline: {}\nNext Experiment ID: {}\n",
            render_baseline(&state.baseline),
            state.next_experiment_id,
        ),
    };

    fs::write(&path, body).with_context(|| format!("failed to write {}", path.display()))
}

fn render_baseline(baseline: &Option<MetricSnapshot>) -> String {
    match baseline {
        Some(metric) => match &metric.unit {
            Some(unit) => format!("{}={}{}", metric.name, metric.value, unit),
            None => format!("{}={}", metric.name, metric.value),
        },
        None => "not recorded".to_string(),
    }
}

fn read_json<T>(path: &Path) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

fn write_json<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    let bytes = serde_json::to_vec_pretty(value)?;
    fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
}
