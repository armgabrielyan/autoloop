use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use serde_json::Value as JsonValue;
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
    Go,
    DotNet,
    Jvm,
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
    regex: Option<String>,
    note: String,
}

#[derive(Debug, Clone)]
struct PythonWorkspace {
    runner: PythonRunner,
    pyproject: Option<TomlValue>,
    pyproject_text: Option<String>,
}

#[derive(Debug, Clone)]
struct NodeWorkspace {
    runner: NodeRunner,
    package_json: Option<JsonValue>,
    package_json_text: Option<String>,
}

#[derive(Debug, Clone)]
struct JvmWorkspace {
    tool: JvmBuildTool,
    build_texts: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
enum PythonRunner {
    Direct,
    Uv,
    Poetry,
    Pipenv,
    Hatch,
}

#[derive(Debug, Clone, Copy)]
enum NodeRunner {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

#[derive(Debug, Clone, Copy)]
enum JvmBuildTool {
    GradleWrapper,
    Gradle,
    MavenWrapper,
    Maven,
}

pub fn infer_config(root: &Path) -> Result<(Config, ConfigInference)> {
    let project_kind = detect_project_kind(root);
    let mut notes = Vec::new();
    let (eval, guardrail) = match project_kind {
        ProjectKind::Go => {
            notes.push("Detected Go module; using `go` CLI commands.".to_string());
            (detect_go_eval(root), detect_go_guardrail(root))
        }
        ProjectKind::Jvm => {
            let workspace = inspect_jvm_workspace(root);
            notes.push(workspace.tool.note().to_string());
            (
                detect_jvm_eval(root, &workspace),
                detect_jvm_guardrail(&workspace),
            )
        }
        ProjectKind::Node => {
            let workspace = inspect_node_workspace(root);
            notes.push(workspace.runner.note().to_string());
            (
                detect_node_eval(root, &workspace),
                detect_node_guardrail(root, &workspace),
            )
        }
        ProjectKind::Python => {
            let workspace = inspect_python_workspace(root);
            notes.push(workspace.runner.note().to_string());
            (
                detect_python_eval(root, &workspace),
                detect_python_guardrail(root, &workspace),
            )
        }
        ProjectKind::DotNet => {
            notes.push("Detected .NET workspace; using `dotnet` CLI commands.".to_string());
            (detect_dotnet_eval(root), detect_dotnet_guardrail(root))
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
        config.eval.regex = candidate.regex.clone();
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
    } else if root.join("go.mod").exists() {
        ProjectKind::Go
    } else if has_matching_file(root, 2, &[".git", ".autoloop"], |name| {
        name.ends_with(".sln") || name.ends_with(".csproj")
    }) {
        ProjectKind::DotNet
    } else if is_jvm_workspace(root) {
        ProjectKind::Jvm
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
        ProjectKind::Go => detect_go_eval(root),
        ProjectKind::DotNet => detect_dotnet_eval(root),
        ProjectKind::Jvm => {
            let workspace = inspect_jvm_workspace(root);
            detect_jvm_eval(root, &workspace)
        }
        ProjectKind::Python => {
            let workspace = inspect_python_workspace(root);
            detect_python_eval(root, &workspace)
        }
        ProjectKind::Node => {
            let workspace = inspect_node_workspace(root);
            detect_node_eval(root, &workspace).or_else(|| {
                let workspace = inspect_python_workspace(root);
                detect_python_eval(root, &workspace)
            })
        }
        ProjectKind::Unknown => {
            let workspace = inspect_python_workspace(root);
            detect_python_eval(root, &workspace)
                .or_else(|| detect_rust_eval(root))
                .or_else(|| detect_go_eval(root))
                .or_else(|| detect_dotnet_eval(root))
                .or_else(|| {
                    if is_jvm_workspace(root) {
                        let workspace = inspect_jvm_workspace(root);
                        detect_jvm_eval(root, &workspace)
                    } else {
                        None
                    }
                })
        }
    }
}

fn detect_guardrail(root: &Path, project_kind: ProjectKind) -> Option<String> {
    match project_kind {
        ProjectKind::Rust => root
            .join("Cargo.toml")
            .exists()
            .then(|| "cargo test".to_string()),
        ProjectKind::Go => detect_go_guardrail(root),
        ProjectKind::DotNet => detect_dotnet_guardrail(root),
        ProjectKind::Jvm => {
            let workspace = inspect_jvm_workspace(root);
            detect_jvm_guardrail(&workspace)
        }
        ProjectKind::Python => {
            let workspace = inspect_python_workspace(root);
            detect_python_guardrail(root, &workspace)
        }
        ProjectKind::Node => {
            let workspace = inspect_node_workspace(root);
            detect_node_guardrail(root, &workspace)
        }
        ProjectKind::Unknown => {
            let workspace = inspect_python_workspace(root);
            detect_python_guardrail(root, &workspace)
                .or_else(|| detect_go_guardrail(root))
                .or_else(|| detect_dotnet_guardrail(root))
                .or_else(|| {
                    if is_jvm_workspace(root) {
                        let workspace = inspect_jvm_workspace(root);
                        detect_jvm_guardrail(&workspace)
                    } else {
                        None
                    }
                })
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
            regex: None,
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
            regex: None,
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
            regex: None,
            note: format!("Detected Rust eval binary from `{relative}`"),
        });
    }
    None
}

fn detect_go_eval(root: &Path) -> Option<EvalCandidate> {
    for relative in [
        "cmd/bench/main.go",
        "cmd/benchmark/main.go",
        "cmd/eval/main.go",
        "cmd/perf/main.go",
        "bench/main.go",
        "benchmark/main.go",
        "eval/main.go",
        "perf/main.go",
    ] {
        let path = root.join(relative);
        if !path.exists() {
            continue;
        }
        let package_path = relative
            .strip_suffix("/main.go")
            .map(normalize_relative_path)
            .unwrap_or_else(|| normalize_relative_path(relative));
        let metric_name = metric_name_from_file(&path).or_else(|| {
            path.parent()
                .and_then(|directory| metric_name_from_directory(directory, &["go"]))
        });
        return Some(EvalCandidate {
            command: format!("go run ./{package_path}"),
            metric_name,
            format: MetricFormat::MetricLines,
            regex: None,
            note: format!("Detected Go eval package `./{package_path}`"),
        });
    }

    for relative in ["bench.go", "benchmark.go", "eval.go", "perf.go"] {
        let path = root.join(relative);
        if !path.exists() {
            continue;
        }
        return Some(EvalCandidate {
            command: format!("go run ./{relative}"),
            metric_name: metric_name_from_file(&path),
            format: MetricFormat::MetricLines,
            regex: None,
            note: format!("Detected Go eval file `{relative}`"),
        });
    }

    if has_go_benchmark_tests(root) {
        return Some(EvalCandidate {
            command: "go test ./... -bench . -run ^$".to_string(),
            metric_name: Some("ns_per_op".to_string()),
            format: MetricFormat::Regex,
            regex: Some(
                r"(?m)^Benchmark\S+\s+\d+\s+([0-9]+(?:\.[0-9]+)?)\s+ns/op(?:\s|$)".to_string(),
            ),
            note: "Detected Go benchmark tests; using `go test -bench` output.".to_string(),
        });
    }

    None
}

fn detect_go_guardrail(root: &Path) -> Option<String> {
    root.join("go.mod")
        .exists()
        .then(|| "go test ./...".to_string())
}

fn has_go_benchmark_tests(root: &Path) -> bool {
    find_matching_file(
        root,
        4,
        &[
            ".git",
            ".autoloop",
            "vendor",
            "node_modules",
            "target",
            "bin",
            "dist",
        ],
        |name| name.ends_with("_test.go"),
    )
    .map(|path| {
        fs::read_to_string(path)
            .ok()
            .map(|content| {
                Regex::new(r"(?m)^func\s+Benchmark[A-Za-z0-9_]+\s*\(")
                    .ok()
                    .map(|pattern| pattern.is_match(&content))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    })
    .unwrap_or(false)
}

fn is_jvm_workspace(root: &Path) -> bool {
    root.join("gradlew").exists()
        || root.join("build.gradle").exists()
        || root.join("build.gradle.kts").exists()
        || root.join("settings.gradle").exists()
        || root.join("settings.gradle.kts").exists()
        || root.join("mvnw").exists()
        || root.join("pom.xml").exists()
}

fn inspect_jvm_workspace(root: &Path) -> JvmWorkspace {
    let tool = if root.join("gradlew").exists() {
        JvmBuildTool::GradleWrapper
    } else if root.join("build.gradle").exists() || root.join("build.gradle.kts").exists() {
        JvmBuildTool::Gradle
    } else if root.join("mvnw").exists() {
        JvmBuildTool::MavenWrapper
    } else {
        JvmBuildTool::Maven
    };
    let build_texts = [
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
        "pom.xml",
    ]
    .iter()
    .filter_map(|relative| fs::read_to_string(root.join(relative)).ok())
    .collect();
    JvmWorkspace { tool, build_texts }
}

fn detect_jvm_eval(root: &Path, workspace: &JvmWorkspace) -> Option<EvalCandidate> {
    if workspace.tool.is_gradle() {
        if let Some(task) = detect_gradle_task(
            &workspace.build_texts,
            &["bench", "benchmark", "eval", "perf"],
        ) {
            let metric_name = metric_name_from_directory(root, &["java", "kt", "kts", "groovy"]);
            return Some(EvalCandidate {
                command: workspace.tool.gradle_task_command(&task),
                format: if metric_name.is_some() {
                    MetricFormat::MetricLines
                } else {
                    MetricFormat::Auto
                },
                metric_name,
                regex: None,
                note: format!("Detected Gradle eval task `{task}`"),
            });
        }
        if jvm_texts_mention(&workspace.build_texts, "jmh") {
            let metric_name = jvm_benchmark_metric_name(&workspace.build_texts);
            return Some(EvalCandidate {
                command: workspace.tool.gradle_task_command("jmh"),
                metric_name: Some(metric_name),
                format: MetricFormat::Regex,
                regex: Some(jvm_benchmark_regex().to_string()),
                note: "Detected Gradle JMH configuration; using `jmh` task output.".to_string(),
            });
        }
        return None;
    }

    if jvm_texts_mention(&workspace.build_texts, "jmh") {
        return Some(EvalCandidate {
            command: workspace.tool.maven_goal_command("jmh:benchmark"),
            metric_name: Some(jvm_benchmark_metric_name(&workspace.build_texts)),
            format: MetricFormat::Regex,
            regex: Some(jvm_benchmark_regex().to_string()),
            note: "Detected Maven JMH configuration; using `jmh:benchmark` output.".to_string(),
        });
    }

    None
}

fn detect_jvm_guardrail(workspace: &JvmWorkspace) -> Option<String> {
    Some(workspace.tool.test_command())
}

fn inspect_node_workspace(root: &Path) -> NodeWorkspace {
    let package_json_path = root.join("package.json");
    let package_json_text = fs::read_to_string(&package_json_path).ok();
    let package_json = package_json_text
        .as_deref()
        .and_then(|text| serde_json::from_str::<JsonValue>(text).ok());
    let runner = detect_node_runner(root, package_json.as_ref());
    NodeWorkspace {
        runner,
        package_json,
        package_json_text,
    }
}

fn detect_node_eval(root: &Path, workspace: &NodeWorkspace) -> Option<EvalCandidate> {
    if let Some(script) = detect_node_script(workspace, &["bench", "benchmark", "eval", "perf"]) {
        return Some(EvalCandidate {
            command: workspace.runner.run_script_command(&script),
            metric_name: metric_name_from_package_script(root, workspace, &script),
            format: MetricFormat::Auto,
            regex: None,
            note: format!(
                "Detected {} eval script `{script}`",
                workspace.runner.label()
            ),
        });
    }

    find_named_file(
        root,
        &[
            "bench.js",
            "benchmark.js",
            "eval.js",
            "bench.mjs",
            "benchmark.mjs",
            "eval.mjs",
            "bench.cjs",
            "benchmark.cjs",
            "eval.cjs",
            "bench.ts",
            "benchmark.ts",
            "eval.ts",
            "bench.tsx",
            "benchmark.tsx",
            "eval.tsx",
        ],
        3,
        &[
            ".git",
            ".autoloop",
            "node_modules",
            "dist",
            "build",
            ".next",
            "coverage",
        ],
    )
    .and_then(|path| {
        let relative = path
            .strip_prefix(root)
            .ok()
            .map(|value| value.display().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let command = workspace.runner.file_command(
            &normalize_relative_path(&relative),
            workspace.package_json.as_ref(),
        )?;
        Some(EvalCandidate {
            command,
            metric_name: metric_name_from_file(&path),
            format: MetricFormat::MetricLines,
            regex: None,
            note: format!(
                "Detected {} eval file `{relative}`",
                workspace.runner.label()
            ),
        })
    })
}

fn detect_node_guardrail(root: &Path, workspace: &NodeWorkspace) -> Option<String> {
    if let Some(script) = detect_node_script(workspace, &["test", "check", "verify"]) {
        return Some(workspace.runner.run_script_command(&script));
    }
    if workspace.runner.is_bun()
        && (root.join("bunfig.toml").exists()
            || has_matching_file(root, 3, &[".git", ".autoloop", "node_modules"], |name| {
                name.ends_with(".test.ts")
                    || name.ends_with(".test.js")
                    || name.ends_with(".spec.ts")
                    || name.ends_with(".spec.js")
            }))
    {
        return Some("bun test".to_string());
    }
    if package_json_mentions_dependency(workspace.package_json_text.as_deref(), "vitest")
        || has_matching_file(root, 2, &[".git", ".autoloop", "node_modules"], |name| {
            name.starts_with("vitest.config.")
        })
    {
        return Some(workspace.runner.exec_command("vitest", &["run"]));
    }
    if package_json_mentions_dependency(workspace.package_json_text.as_deref(), "jest")
        || has_matching_file(root, 2, &[".git", ".autoloop", "node_modules"], |name| {
            name.starts_with("jest.config.")
        })
    {
        return Some(workspace.runner.exec_command("jest", &["--runInBand"]));
    }
    None
}

fn detect_node_script(workspace: &NodeWorkspace, names: &[&str]) -> Option<String> {
    let scripts = workspace
        .package_json
        .as_ref()?
        .get("scripts")?
        .as_object()?;
    names
        .iter()
        .find(|name| scripts.contains_key(**name))
        .map(|name| (*name).to_string())
}

fn detect_dotnet_eval(root: &Path) -> Option<EvalCandidate> {
    find_matching_file(
        root,
        3,
        &[".git", ".autoloop", "bin", "obj", "node_modules"],
        |name| {
            name.ends_with(".csproj")
                && (name.to_ascii_lowercase().contains("bench")
                    || name.to_ascii_lowercase().contains("eval"))
        },
    )
    .map(|path| {
        let relative = path
            .strip_prefix(root)
            .ok()
            .map(|value| normalize_relative_path(&value.display().to_string()))
            .unwrap_or_else(|| normalize_relative_path(&path.display().to_string()));
        let metric_name = path
            .parent()
            .and_then(|directory| metric_name_from_directory(directory, &["cs"]))
            .or_else(|| metric_name_from_directory(root, &["cs"]));
        EvalCandidate {
            command: format!("dotnet run --project {relative}"),
            metric_name,
            format: MetricFormat::MetricLines,
            regex: None,
            note: format!("Detected .NET eval project `{relative}`"),
        }
    })
}

fn detect_dotnet_guardrail(root: &Path) -> Option<String> {
    if let Some(solution) = find_matching_file(root, 2, &[".git", ".autoloop"], |name| {
        name.ends_with(".sln")
    }) {
        let relative = solution
            .strip_prefix(root)
            .ok()
            .map(|value| normalize_relative_path(&value.display().to_string()))
            .unwrap_or_else(|| normalize_relative_path(&solution.display().to_string()));
        return Some(format!("dotnet test {relative}"));
    }

    if let Some(test_project) = find_matching_file(
        root,
        3,
        &[".git", ".autoloop", "bin", "obj", "node_modules"],
        |name| {
            let lower = name.to_ascii_lowercase();
            lower.ends_with(".csproj") && (lower.contains("test") || lower.contains("tests"))
        },
    ) {
        let relative = test_project
            .strip_prefix(root)
            .ok()
            .map(|value| normalize_relative_path(&value.display().to_string()))
            .unwrap_or_else(|| normalize_relative_path(&test_project.display().to_string()));
        return Some(format!("dotnet test {relative}"));
    }

    has_matching_file(
        root,
        2,
        &[".git", ".autoloop", "bin", "obj", "node_modules"],
        |name| name.ends_with(".csproj"),
    )
    .then(|| "dotnet test".to_string())
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
        "latency", "time", "duration", "memory", "size", "error", "fail", "_per_op",
    ];
    if lower.iter().any(|needle| metric_name.contains(needle)) {
        MetricDirection::Lower
    } else {
        MetricDirection::Higher
    }
}

fn infer_unit(metric_name: &str) -> Option<String> {
    if metric_name.contains("ns_per_op") {
        Some("ns/op".to_string())
    } else if metric_name.contains("throughput") {
        Some("ops/s".to_string())
    } else {
        let milliseconds = ["latency", "time", "duration"];
        milliseconds
            .iter()
            .any(|needle| metric_name.contains(needle))
            .then(|| "ms".to_string())
    }
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

impl NodeRunner {
    fn label(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::Npm => "Detected npm-managed Node workspace; using `npm` commands.",
            Self::Pnpm => "Detected pnpm-managed Node workspace; using `pnpm` commands.",
            Self::Yarn => "Detected Yarn-managed Node workspace; using `yarn` commands.",
            Self::Bun => "Detected Bun-managed Node workspace; using `bun` commands.",
        }
    }

    fn run_script_command(self, script: &str) -> String {
        match self {
            Self::Npm => format!("npm run {script}"),
            Self::Pnpm => format!("pnpm run {script}"),
            Self::Yarn => format!("yarn {script}"),
            Self::Bun => format!("bun run {script}"),
        }
    }

    fn exec_command(self, binary: &str, args: &[&str]) -> String {
        let suffix = if args.is_empty() {
            String::new()
        } else {
            format!(" {}", args.join(" "))
        };
        match self {
            Self::Npm => format!("npm exec -- {binary}{suffix}"),
            Self::Pnpm => format!("pnpm exec {binary}{suffix}"),
            Self::Yarn => format!("yarn exec {binary}{suffix}"),
            Self::Bun => format!("bun x {binary}{suffix}"),
        }
    }

    fn file_command(self, path: &str, package_json: Option<&JsonValue>) -> Option<String> {
        let extension = Path::new(path)
            .extension()
            .and_then(|value| value.to_str())?;
        match extension {
            "js" | "mjs" | "cjs" => Some(match self {
                Self::Bun => format!("bun {path}"),
                _ => format!("node {path}"),
            }),
            "ts" | "tsx" => {
                if self.is_bun() {
                    Some(format!("bun {path}"))
                } else if package_json_has_dependency(package_json, "tsx") {
                    Some(self.exec_command("tsx", &[path]))
                } else if package_json_has_dependency(package_json, "ts-node") {
                    Some(self.exec_command("ts-node", &[path]))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn is_bun(self) -> bool {
        matches!(self, Self::Bun)
    }
}

impl JvmBuildTool {
    fn note(self) -> &'static str {
        match self {
            Self::GradleWrapper => {
                "Detected Gradle JVM workspace; using the local `./gradlew` wrapper."
            }
            Self::Gradle => "Detected Gradle JVM workspace; using `gradle` commands.",
            Self::MavenWrapper => "Detected Maven JVM workspace; using the local `./mvnw` wrapper.",
            Self::Maven => "Detected Maven JVM workspace; using `mvn` commands.",
        }
    }

    fn is_gradle(self) -> bool {
        matches!(self, Self::GradleWrapper | Self::Gradle)
    }

    fn gradle_task_command(self, task: &str) -> String {
        match self {
            Self::GradleWrapper => format!("./gradlew {task}"),
            Self::Gradle => format!("gradle {task}"),
            Self::MavenWrapper | Self::Maven => task.to_string(),
        }
    }

    fn maven_goal_command(self, goal: &str) -> String {
        match self {
            Self::MavenWrapper => format!("./mvnw -q {goal}"),
            Self::Maven => format!("mvn -q {goal}"),
            Self::GradleWrapper | Self::Gradle => goal.to_string(),
        }
    }

    fn test_command(self) -> String {
        match self {
            Self::GradleWrapper => "./gradlew test".to_string(),
            Self::Gradle => "gradle test".to_string(),
            Self::MavenWrapper => "./mvnw test".to_string(),
            Self::Maven => "mvn test".to_string(),
        }
    }
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

fn detect_node_runner(root: &Path, package_json: Option<&JsonValue>) -> NodeRunner {
    let package_manager = package_json
        .and_then(|value| value.get("packageManager"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if root.join("bun.lockb").exists()
        || root.join("bun.lock").exists()
        || root.join("bunfig.toml").exists()
        || package_manager.starts_with("bun@")
    {
        NodeRunner::Bun
    } else if root.join("pnpm-lock.yaml").exists() || package_manager.starts_with("pnpm@") {
        NodeRunner::Pnpm
    } else if root.join("yarn.lock").exists() || package_manager.starts_with("yarn@") {
        NodeRunner::Yarn
    } else {
        NodeRunner::Npm
    }
}

fn detect_gradle_task(build_texts: &[String], names: &[&str]) -> Option<String> {
    let joined = build_texts.join("\n");
    for name in names {
        let escaped = regex::escape(name);
        let patterns = [
            format!(r#"tasks\.register\(\s*["']{escaped}["']"#),
            format!(r#"task\s+{escaped}\b"#),
            format!(r#"tasks\.named\(\s*["']{escaped}["']"#),
            format!(r#"["']{escaped}["']\s*\{{"#),
        ];
        if patterns.iter().any(|pattern| {
            Regex::new(pattern)
                .ok()
                .map(|regex| regex.is_match(&joined))
                .unwrap_or(false)
        }) {
            return Some((*name).to_string());
        }
    }
    None
}

fn jvm_texts_mention(build_texts: &[String], needle: &str) -> bool {
    let needle = needle.to_ascii_lowercase();
    build_texts
        .iter()
        .any(|text| text.to_ascii_lowercase().contains(&needle))
}

fn jvm_benchmark_metric_name(build_texts: &[String]) -> String {
    let joined = build_texts.join("\n").to_ascii_lowercase();
    if joined.contains("averagetime") || joined.contains("mode.avgt") || joined.contains("avgt") {
        "time_per_op".to_string()
    } else if joined.contains("throughput")
        || joined.contains("mode.thrpt")
        || joined.contains("thrpt")
    {
        "throughput".to_string()
    } else {
        "benchmark_score".to_string()
    }
}

fn jvm_benchmark_regex() -> &'static str {
    r"(?m)^Benchmark\S*(?:\s+\S+)*\s+(?:thrpt|avgt|sample|ss)\s+\d+\s+([0-9]+(?:\.[0-9]+)?)\s+(?:±\s+[0-9]+(?:\.[0-9]+)?\s+)?\S+"
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

fn package_json_mentions_dependency(content: Option<&str>, dependency: &str) -> bool {
    content
        .map(|text| {
            let needle = format!("\"{}\"", dependency.to_ascii_lowercase());
            text.to_ascii_lowercase().contains(&needle)
        })
        .unwrap_or(false)
}

fn package_json_has_dependency(package_json: Option<&JsonValue>, dependency: &str) -> bool {
    let Some(package_json) = package_json else {
        return false;
    };
    for table_name in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if package_json
            .get(table_name)
            .and_then(|value| value.as_object())
            .map(|table| table.contains_key(dependency))
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

fn metric_name_from_package_script(
    root: &Path,
    workspace: &NodeWorkspace,
    script: &str,
) -> Option<String> {
    let script_body = workspace
        .package_json
        .as_ref()?
        .get("scripts")?
        .as_object()?
        .get(script)?
        .as_str()?;
    for token in script_body.split_whitespace() {
        let token = token.trim_matches(|character| {
            matches!(character, '"' | '\'' | ',' | ';' | '(' | ')' | '[' | ']')
        });
        if token.ends_with(".js")
            || token.ends_with(".mjs")
            || token.ends_with(".cjs")
            || token.ends_with(".ts")
            || token.ends_with(".tsx")
        {
            let candidate = root.join(token);
            if candidate.exists() {
                return metric_name_from_file(&candidate);
            }
        }
    }
    None
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

fn has_matching_file(
    root: &Path,
    max_depth: usize,
    ignored_dirs: &[&str],
    predicate: impl Fn(&str) -> bool + Copy,
) -> bool {
    find_matching_file(root, max_depth, ignored_dirs, predicate).is_some()
}

fn find_matching_file(
    root: &Path,
    max_depth: usize,
    ignored_dirs: &[&str],
    predicate: impl Fn(&str) -> bool + Copy,
) -> Option<PathBuf> {
    find_matching_file_inner(root, root, max_depth, ignored_dirs, predicate)
}

fn find_matching_file_inner(
    base: &Path,
    current: &Path,
    depth_remaining: usize,
    ignored_dirs: &[&str],
    predicate: impl Fn(&str) -> bool + Copy,
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
                find_matching_file_inner(base, &path, depth_remaining - 1, ignored_dirs, predicate)
            {
                return Some(found);
            }
            continue;
        }
        if predicate(&file_name) {
            return path
                .strip_prefix(base)
                .ok()
                .map(|relative| base.join(relative));
        }
    }
    None
}

fn metric_name_from_directory(directory: &Path, extensions: &[&str]) -> Option<String> {
    find_matching_file(
        directory,
        2,
        &[".git", ".autoloop", "node_modules", "target", "bin", "obj"],
        |name| {
            extensions.iter().any(|extension| {
                name.to_ascii_lowercase()
                    .ends_with(&format!(".{}", extension.to_ascii_lowercase()))
            })
        },
    )
    .and_then(|path| metric_name_from_file(&path))
}

#[cfg(test)]
mod tests {
    use super::{ConfigSource, ProjectKind, infer_config, jvm_benchmark_regex};
    use crate::eval::formats::MetricFormat;
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

    #[test]
    fn infers_bun_workspace_with_scripts() {
        let root = temp_dir("bun-scripts");
        write(&root.join("bun.lockb"), "");
        write(
            &root.join("package.json"),
            r#"{
  "name": "demo",
  "scripts": {
    "bench": "bun bench.ts",
    "test": "bun test"
  }
}
"#,
        );
        write(
            &root.join("bench.ts"),
            "console.log('METRIC latency_p95=18.4');\n",
        );

        let (_, inference) = infer_config(&root).expect("inference should succeed");
        assert!(matches!(inference.project_kind, ProjectKind::Node));
        assert!(matches!(inference.source, ConfigSource::Inferred));
        assert_eq!(inference.eval_command, "bun run bench");
        assert_eq!(inference.metric_name, "latency_p95");
        assert_eq!(inference.guardrail_commands, vec!["bun run test"]);
        assert!(
            inference
                .notes
                .iter()
                .any(|note| note.contains("Bun-managed"))
        );
    }

    #[test]
    fn infers_pnpm_workspace_with_vitest_and_tsx() {
        let root = temp_dir("pnpm-vitest");
        write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
        write(
            &root.join("package.json"),
            r#"{
  "name": "demo",
  "packageManager": "pnpm@9.0.0",
  "devDependencies": {
    "tsx": "^4.0.0",
    "vitest": "^2.0.0"
  }
}
"#,
        );
        write(
            &root.join("bench.ts"),
            "console.log('METRIC latency_p95=9.2');\n",
        );

        let (_, inference) = infer_config(&root).expect("inference should succeed");
        assert!(matches!(inference.project_kind, ProjectKind::Node));
        assert!(matches!(inference.source, ConfigSource::Inferred));
        assert_eq!(inference.eval_command, "pnpm exec tsx bench.ts");
        assert_eq!(inference.metric_name, "latency_p95");
        assert_eq!(inference.guardrail_commands, vec!["pnpm exec vitest run"]);
        assert!(
            inference
                .notes
                .iter()
                .any(|note| note.contains("pnpm-managed"))
        );
    }

    #[test]
    fn infers_dotnet_workspace() {
        let root = temp_dir("dotnet");
        write(
            &root.join("Demo.sln"),
            "Microsoft Visual Studio Solution File, Format Version 12.00\n",
        );
        write(
            &root.join("Benchmarks/Benchmarks.csproj"),
            r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
  </PropertyGroup>
</Project>
"#,
        );
        write(
            &root.join("Benchmarks/Program.cs"),
            "Console.WriteLine(\"METRIC latency_p95=11.7\");\n",
        );
        write(
            &root.join("Demo.Tests/Demo.Tests.csproj"),
            r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <IsTestProject>true</IsTestProject>
  </PropertyGroup>
</Project>
"#,
        );

        let (_, inference) = infer_config(&root).expect("inference should succeed");
        assert!(matches!(inference.project_kind, ProjectKind::DotNet));
        assert!(matches!(inference.source, ConfigSource::Inferred));
        assert_eq!(
            inference.eval_command,
            "dotnet run --project Benchmarks/Benchmarks.csproj"
        );
        assert_eq!(inference.metric_name, "latency_p95");
        assert_eq!(inference.guardrail_commands, vec!["dotnet test Demo.sln"]);
        assert!(
            inference
                .notes
                .iter()
                .any(|note| note.contains(".NET workspace"))
        );
    }

    #[test]
    fn infers_go_workspace_from_cmd_bench() {
        let root = temp_dir("go-cmd-bench");
        write(&root.join("go.mod"), "module example.com/demo\n\ngo 1.22\n");
        write(
            &root.join("cmd/bench/main.go"),
            r#"package main

import "fmt"

func main() {
    fmt.Println("METRIC latency_p95=6.4")
}
"#,
        );

        let (_, inference) = infer_config(&root).expect("inference should succeed");
        assert!(matches!(inference.project_kind, ProjectKind::Go));
        assert!(matches!(inference.source, ConfigSource::Inferred));
        assert_eq!(inference.eval_command, "go run ./cmd/bench");
        assert_eq!(inference.metric_name, "latency_p95");
        assert_eq!(inference.guardrail_commands, vec!["go test ./..."]);
    }

    #[test]
    fn infers_go_benchmark_tests_with_regex_fallback() {
        let root = temp_dir("go-bench-tests");
        write(&root.join("go.mod"), "module example.com/demo\n\ngo 1.22\n");
        write(
            &root.join("bench_test.go"),
            r#"package demo

import "testing"

func BenchmarkSearch(b *testing.B) {}
"#,
        );

        let (config, inference) = infer_config(&root).expect("inference should succeed");
        assert!(matches!(inference.project_kind, ProjectKind::Go));
        assert!(matches!(inference.source, ConfigSource::Inferred));
        assert_eq!(inference.eval_command, "go test ./... -bench . -run ^$");
        assert_eq!(inference.metric_name, "ns_per_op");
        assert_eq!(inference.guardrail_commands, vec!["go test ./..."]);
        assert!(matches!(config.eval.format, MetricFormat::Regex));
        assert_eq!(
            config.eval.regex.as_deref(),
            Some(r"(?m)^Benchmark\S+\s+\d+\s+([0-9]+(?:\.[0-9]+)?)\s+ns/op(?:\s|$)")
        );
    }

    #[test]
    fn infers_gradle_jvm_workspace() {
        let root = temp_dir("gradle-jvm");
        write(&root.join("gradlew"), "#!/usr/bin/env sh\n");
        write(
            &root.join("build.gradle.kts"),
            r#"tasks.register("bench") {
    doLast {
        println("bench")
    }
}
"#,
        );
        write(
            &root.join("src/main/kotlin/Bench.kt"),
            r#"fun main() {
    println("METRIC latency_p95=4.2")
}
"#,
        );

        let (_, inference) = infer_config(&root).expect("inference should succeed");
        assert!(matches!(inference.project_kind, ProjectKind::Jvm));
        assert!(matches!(inference.source, ConfigSource::Inferred));
        assert_eq!(inference.eval_command, "./gradlew bench");
        assert_eq!(inference.metric_name, "latency_p95");
        assert_eq!(inference.guardrail_commands, vec!["./gradlew test"]);
    }

    #[test]
    fn infers_maven_jmh_workspace() {
        let root = temp_dir("maven-jmh");
        write(&root.join("mvnw"), "#!/usr/bin/env sh\n");
        write(
            &root.join("pom.xml"),
            r#"<project>
  <build>
    <plugins>
      <plugin>
        <artifactId>jmh-maven-plugin</artifactId>
        <configuration>
          <mode>thrpt</mode>
        </configuration>
      </plugin>
    </plugins>
  </build>
</project>
"#,
        );

        let (config, inference) = infer_config(&root).expect("inference should succeed");
        assert!(matches!(inference.project_kind, ProjectKind::Jvm));
        assert!(matches!(inference.source, ConfigSource::Inferred));
        assert_eq!(inference.eval_command, "./mvnw -q jmh:benchmark");
        assert_eq!(inference.metric_name, "throughput");
        assert_eq!(inference.guardrail_commands, vec!["./mvnw test"]);
        assert!(matches!(config.eval.format, MetricFormat::Regex));
        assert_eq!(config.eval.regex.as_deref(), Some(jvm_benchmark_regex()));
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
