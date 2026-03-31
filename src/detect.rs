use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use toml::Value as TomlValue;

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

#[derive(Debug, Clone)]
struct PythonWorkspace {
    runner: PythonRunner,
    pyproject: Option<TomlValue>,
    pyproject_text: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum PythonRunner {
    Direct,
    Uv,
    Poetry,
    Pipenv,
    Hatch,
}

pub fn infer_config(root: &Path) -> Result<(Config, ConfigInference)> {
    let project_kind = detect_project_kind(root);
    let mut notes = Vec::new();
    let (eval, guardrail) = match project_kind {
        ProjectKind::Python => {
            let workspace = inspect_python_workspace(root);
            notes.push(workspace.runner.note().to_string());
            (
                detect_python_eval(root, &workspace),
                detect_python_guardrail(root, &workspace),
            )
        }
        _ => (
            detect_eval_candidate(root, project_kind),
            detect_guardrail(root, project_kind),
        ),
    };
    let mut config = default_config();

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
        ProjectKind::Rust => detect_rust_eval(root).or_else(|| {
            let workspace = inspect_python_workspace(root);
            detect_python_eval(root, &workspace)
        }),
        ProjectKind::Python => {
            let workspace = inspect_python_workspace(root);
            detect_python_eval(root, &workspace)
        }
        ProjectKind::Node => detect_node_eval(root).or_else(|| {
            let workspace = inspect_python_workspace(root);
            detect_python_eval(root, &workspace)
        }),
        ProjectKind::Unknown => {
            let workspace = inspect_python_workspace(root);
            detect_python_eval(root, &workspace).or_else(|| detect_rust_eval(root))
        }
    }
}

fn detect_guardrail(root: &Path, project_kind: ProjectKind) -> Option<String> {
    match project_kind {
        ProjectKind::Rust => root
            .join("Cargo.toml")
            .exists()
            .then(|| "cargo test".to_string()),
        ProjectKind::Python => {
            let workspace = inspect_python_workspace(root);
            detect_python_guardrail(root, &workspace)
        }
        ProjectKind::Node => detect_node_test(root),
        ProjectKind::Unknown => {
            let workspace = inspect_python_workspace(root);
            detect_python_guardrail(root, &workspace)
        }
    }
}

fn inspect_python_workspace(root: &Path) -> PythonWorkspace {
    let pyproject_path = root.join("pyproject.toml");
    let pyproject_text = fs::read_to_string(&pyproject_path).ok();
    let pyproject = pyproject_text
        .as_deref()
        .and_then(|text| toml::from_str::<TomlValue>(text).ok());
    let runner = detect_python_runner(root, pyproject.as_ref());
    PythonWorkspace {
        runner,
        pyproject,
        pyproject_text,
    }
}

fn detect_python_eval(root: &Path, workspace: &PythonWorkspace) -> Option<EvalCandidate> {
    if let Some((script_name, module)) =
        detect_python_pyproject_script(workspace, &["bench", "benchmark", "eval"])
    {
        return Some(EvalCandidate {
            command: workspace.runner.python_module_command(&module, &[]),
            metric_name: metric_name_from_python_module(root, &module),
            format: MetricFormat::MetricLines,
            note: format!(
                "Detected Python eval entrypoint `{script_name}` from `pyproject.toml` using `{}`",
                workspace.runner.label()
            ),
        });
    }

    find_named_file(
        root,
        &["bench.py", "benchmark.py", "eval.py"],
        3,
        &[
            ".git",
            ".autoloop",
            ".venv",
            "venv",
            "__pycache__",
            "node_modules",
            "target",
        ],
    )
    .map(|path| {
        let relative = path
            .strip_prefix(root)
            .ok()
            .map(|value| value.display().to_string())
            .unwrap_or_else(|| path.display().to_string());
        EvalCandidate {
            command: workspace
                .runner
                .python_script_command(&normalize_relative_path(&relative)),
            metric_name: metric_name_from_file(&path),
            format: MetricFormat::MetricLines,
            note: format!(
                "Detected Python eval command from `{relative}` using `{}`",
                workspace.runner.label()
            ),
        }
    })
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

fn detect_python_guardrail(root: &Path, workspace: &PythonWorkspace) -> Option<String> {
    if root.join("noxfile.py").exists() {
        return Some(
            workspace
                .runner
                .python_module_command("nox", &["-s", "tests"]),
        );
    }
    if root.join("tox.ini").exists() || root.join("tox.toml").exists() {
        return Some(workspace.runner.python_module_command("tox", &["-q"]));
    }
    if detect_pytest(root, workspace) {
        return Some(workspace.runner.python_module_command("pytest", &[]));
    }
    detect_python_tests(root).then(|| workspace.runner.python_module_command("unittest", &[]))
}

fn detect_python_tests(root: &Path) -> bool {
    if root.join("tests").exists() {
        return true;
    }
    has_matching_name(root, |name| {
        name.starts_with("test_") && name.ends_with(".py") || name.ends_with("_test.py")
    })
}

fn detect_pytest(root: &Path, workspace: &PythonWorkspace) -> bool {
    if root.join("pytest.ini").exists()
        || root.join(".pytest.ini").exists()
        || root.join("conftest.py").exists()
    {
        return true;
    }
    if has_toml_path(
        workspace.pyproject.as_ref(),
        &["tool", "pytest", "ini_options"],
    ) {
        return true;
    }
    if pyproject_mentions_dependency(workspace.pyproject_text.as_deref(), "pytest") {
        return true;
    }
    requirements_mention(root, "pytest")
}

fn metric_name_from_file(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let regex = Regex::new(r"METRIC\s+([A-Za-z0-9_]+)=").ok()?;
    regex
        .captures(&content)
        .and_then(|captures| captures.get(1))
        .map(|capture| capture.as_str().to_string())
}

fn metric_name_from_python_module(root: &Path, module: &str) -> Option<String> {
    let relative = module.replace('.', "/");
    for base in [root.to_path_buf(), root.join("src")] {
        let file = base.join(format!("{relative}.py"));
        if file.exists() {
            return metric_name_from_file(&file);
        }
        let main_file = base.join(&relative).join("__main__.py");
        if main_file.exists() {
            return metric_name_from_file(&main_file);
        }
    }
    None
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

impl PythonRunner {
    fn label(self) -> &'static str {
        match self {
            Self::Direct => "python3",
            Self::Uv => "uv run",
            Self::Poetry => "poetry run",
            Self::Pipenv => "pipenv run",
            Self::Hatch => "hatch run",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::Direct => "Detected plain Python workspace; using direct `python3` commands.",
            Self::Uv => "Detected uv-managed Python workspace; using `uv run` commands.",
            Self::Poetry => {
                "Detected Poetry-managed Python workspace; using `poetry run` commands."
            }
            Self::Pipenv => {
                "Detected Pipenv-managed Python workspace; using `pipenv run` commands."
            }
            Self::Hatch => "Detected Hatch-managed Python workspace; using `hatch run` commands.",
        }
    }

    fn python_script_command(self, path: &str) -> String {
        match self {
            Self::Direct => format!("python3 {path}"),
            Self::Uv => format!("uv run python {path}"),
            Self::Poetry => format!("poetry run python {path}"),
            Self::Pipenv => format!("pipenv run python {path}"),
            Self::Hatch => format!("hatch run python {path}"),
        }
    }

    fn python_module_command(self, module: &str, args: &[&str]) -> String {
        let suffix = if args.is_empty() {
            String::new()
        } else {
            format!(" {}", args.join(" "))
        };
        match self {
            Self::Direct => format!("python3 -m {module}{suffix}"),
            Self::Uv => format!("uv run python -m {module}{suffix}"),
            Self::Poetry => format!("poetry run python -m {module}{suffix}"),
            Self::Pipenv => format!("pipenv run python -m {module}{suffix}"),
            Self::Hatch => format!("hatch run python -m {module}{suffix}"),
        }
    }
}

fn detect_python_runner(root: &Path, pyproject: Option<&TomlValue>) -> PythonRunner {
    if root.join("uv.lock").exists() || has_toml_path(pyproject, &["tool", "uv"]) {
        PythonRunner::Uv
    } else if root.join("poetry.lock").exists() || has_toml_path(pyproject, &["tool", "poetry"]) {
        PythonRunner::Poetry
    } else if root.join("Pipfile").exists() || root.join("Pipfile.lock").exists() {
        PythonRunner::Pipenv
    } else if root.join("hatch.toml").exists() || has_toml_path(pyproject, &["tool", "hatch"]) {
        PythonRunner::Hatch
    } else {
        PythonRunner::Direct
    }
}

fn detect_python_pyproject_script(
    workspace: &PythonWorkspace,
    names: &[&str],
) -> Option<(String, String)> {
    for path in [
        &["project", "scripts"][..],
        &["tool", "poetry", "scripts"][..],
    ] {
        let Some(table) = get_toml_table(workspace.pyproject.as_ref(), path) else {
            continue;
        };
        for name in names {
            let Some(value) = table.get(*name).and_then(|value| value.as_str()) else {
                continue;
            };
            if let Some((module, _function)) = value.split_once(':') {
                return Some(((*name).to_string(), module.to_string()));
            }
        }
    }
    None
}

fn get_toml_table<'a>(
    value: Option<&'a TomlValue>,
    path: &[&str],
) -> Option<&'a toml::map::Map<String, TomlValue>> {
    let mut current = value?;
    for part in path {
        current = current.get(*part)?;
    }
    current.as_table()
}

fn has_toml_path(value: Option<&TomlValue>, path: &[&str]) -> bool {
    let mut current = match value {
        Some(value) => value,
        None => return false,
    };
    for part in path {
        current = match current.get(*part) {
            Some(value) => value,
            None => return false,
        };
    }
    true
}

fn pyproject_mentions_dependency(content: Option<&str>, dependency: &str) -> bool {
    content
        .map(|text| text.to_lowercase().contains(&dependency.to_lowercase()))
        .unwrap_or(false)
}

fn requirements_mention(root: &Path, dependency: &str) -> bool {
    [
        "requirements.txt",
        "requirements-dev.txt",
        "dev-requirements.txt",
        "requirements/test.txt",
    ]
    .iter()
    .any(|relative| {
        fs::read_to_string(root.join(relative))
            .ok()
            .map(|content| content.to_lowercase().contains(&dependency.to_lowercase()))
            .unwrap_or(false)
    })
}

fn find_named_file(
    root: &Path,
    names: &[&str],
    max_depth: usize,
    ignored_dirs: &[&str],
) -> Option<PathBuf> {
    find_named_file_inner(root, root, names, max_depth, ignored_dirs)
}

fn find_named_file_inner(
    base: &Path,
    current: &Path,
    names: &[&str],
    depth_remaining: usize,
    ignored_dirs: &[&str],
) -> Option<PathBuf> {
    let entries = fs::read_dir(current).ok()?;
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if path.is_dir() {
            if depth_remaining == 0 || ignored_dirs.iter().any(|ignored| *ignored == file_name) {
                continue;
            }
            if let Some(found) =
                find_named_file_inner(base, &path, names, depth_remaining - 1, ignored_dirs)
            {
                return Some(found);
            }
            continue;
        }
        if names.iter().any(|candidate| *candidate == file_name) {
            return path
                .strip_prefix(base)
                .ok()
                .map(|relative| base.join(relative));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{ConfigSource, ProjectKind, infer_config};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

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

    #[test]
    fn infers_uv_pytest_workspace() {
        let root = temp_dir("uv-pytest");
        write(&root.join("uv.lock"), "");
        write(
            &root.join("pyproject.toml"),
            r#"[project]
name = "demo"

[tool.pytest.ini_options]
addopts = "-q"
"#,
        );
        fs::create_dir_all(root.join("scripts")).expect("scripts dir should exist");
        write(
            &root.join("scripts/bench.py"),
            "print('METRIC latency_p95=12.3')\n",
        );

        let (_, inference) = infer_config(&root).expect("inference should succeed");
        assert!(matches!(inference.project_kind, ProjectKind::Python));
        assert!(matches!(inference.source, ConfigSource::Inferred));
        assert_eq!(inference.eval_command, "uv run python scripts/bench.py");
        assert_eq!(
            inference.guardrail_commands,
            vec!["uv run python -m pytest"]
        );
        assert!(
            inference
                .notes
                .iter()
                .any(|note| note.contains("uv-managed"))
        );
    }

    #[test]
    fn infers_poetry_pyproject_script_entrypoint() {
        let root = temp_dir("poetry-script");
        write(&root.join("poetry.lock"), "");
        write(
            &root.join("pyproject.toml"),
            r#"[tool.poetry]
name = "demo"
version = "0.1.0"

[tool.poetry.dependencies]
python = "^3.12"
pytest = "^8.0"

[tool.poetry.scripts]
bench = "demo.bench:main"
"#,
        );
        fs::create_dir_all(root.join("demo")).expect("package dir should exist");
        write(
            &root.join("demo/bench.py"),
            "def main():\n    print('METRIC latency_p95=7.1')\n",
        );

        let (_, inference) = infer_config(&root).expect("inference should succeed");
        assert!(matches!(inference.project_kind, ProjectKind::Python));
        assert!(matches!(inference.source, ConfigSource::Inferred));
        assert_eq!(inference.eval_command, "poetry run python -m demo.bench");
        assert_eq!(inference.metric_name, "latency_p95");
        assert_eq!(
            inference.guardrail_commands,
            vec!["poetry run python -m pytest"]
        );
    }

    #[test]
    fn falls_back_to_partial_python_config_when_only_tests_are_detected() {
        let root = temp_dir("partial-python");
        write(&root.join("pyproject.toml"), "[project]\nname = \"demo\"\n");
        fs::create_dir_all(root.join("tests")).expect("tests dir should exist");
        write(
            &root.join("tests/test_sample.py"),
            "def test_ok():\n    assert True\n",
        );

        let (_, inference) = infer_config(&root).expect("inference should succeed");
        assert!(matches!(inference.project_kind, ProjectKind::Python));
        assert!(matches!(inference.source, ConfigSource::Partial));
        assert_eq!(inference.eval_command, "echo 'METRIC latency_p95=42.3'");
        assert_eq!(inference.guardrail_commands, vec!["python3 -m unittest"]);
    }

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should advance")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "autoloop-detect-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp dir should exist");
        path
    }

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent dir should exist");
        }
        fs::write(path, content).expect("file should write");
    }
}
