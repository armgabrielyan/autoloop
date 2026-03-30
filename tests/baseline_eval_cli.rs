use std::fs;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn baseline_records_metric_and_guardrail_baselines() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_workspace(&temp);
    write_config(
        &temp,
        &baseline_config(
            "echo 'METRIC latency_p95=50'",
            Some("echo 'METRIC memory_mb=100'"),
            None,
        ),
    );

    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["baseline", "--json"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("json output should parse");
    assert_eq!(payload["metric"]["name"], "latency_p95");
    assert_eq!(payload["metric"]["value"], 50.0);

    let state = read_json(temp.path().join(".autoloop/state.json"));
    assert_eq!(state["baseline"]["value"], 50.0);
    assert_eq!(state["baseline_guardrails"][0]["name"], "memory_mb");
    assert_eq!(state["baseline_guardrails"][0]["value"], 100.0);

    let records = read_jsonl(temp.path().join(".autoloop/experiments.jsonl"));
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["status"], "baseline");
}

#[test]
fn eval_records_pending_eval_with_rerun_verdict() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_workspace(&temp);
    write_config(
        &temp,
        &baseline_config("echo 'METRIC latency_p95=50'", None, None),
    );

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("baseline")
        .current_dir(temp.path())
        .assert()
        .success();

    write_config(
        &temp,
        &baseline_config("echo 'METRIC latency_p95=45'", None, None),
    );

    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["eval", "--json"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("json output should parse");
    assert_eq!(payload["verdict"], "rerun");
    assert_eq!(payload["delta_from_baseline"], -5.0);
    assert!(payload["confidence"].is_null());

    let last_eval = read_json(temp.path().join(".autoloop/last_eval.json"));
    assert_eq!(last_eval["pending_eval"]["metric"]["value"], 45.0);
    assert_eq!(last_eval["pending_eval"]["verdict"], "rerun");
}

#[test]
fn eval_discards_when_metric_guardrail_fails() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_workspace(&temp);
    write_config(
        &temp,
        &baseline_config(
            "echo 'METRIC latency_p95=50'",
            Some("echo 'METRIC memory_mb=100'"),
            None,
        ),
    );

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("baseline")
        .current_dir(temp.path())
        .assert()
        .success();

    write_config(
        &temp,
        &baseline_config(
            "echo 'METRIC latency_p95=45'",
            Some("echo 'METRIC memory_mb=130'"),
            None,
        ),
    );

    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["eval", "--json"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("json output should parse");
    assert_eq!(payload["verdict"], "discard");
    assert_eq!(payload["guardrails"][0]["passed"], false);
}

#[test]
fn eval_refuses_when_pending_eval_exists() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_workspace(&temp);
    write_config(
        &temp,
        &baseline_config("echo 'METRIC latency_p95=50'", None, None),
    );

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("baseline")
        .current_dir(temp.path())
        .assert()
        .success();

    write_config(
        &temp,
        &baseline_config("echo 'METRIC latency_p95=45'", None, None),
    );

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();

    let stderr = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .env("NO_COLOR", "1")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();

    let plain = String::from_utf8(stderr).expect("stderr should be utf-8");
    assert!(plain.contains("a pending eval already exists"));
    assert!(!plain.contains("Error:"));
}

#[test]
fn eval_crash_is_logged_after_retries() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_workspace(&temp);
    write_config(
        &temp,
        &baseline_config("echo 'METRIC latency_p95=50'", None, None),
    );

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("baseline")
        .current_dir(temp.path())
        .assert()
        .success();

    write_config(&temp, &baseline_config("exit 1", None, Some(1)));

    let stderr = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .env("NO_COLOR", "1")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();

    let plain = String::from_utf8(stderr).expect("stderr should be utf-8");
    assert!(plain.contains("crash logged as experiment 2"));

    let records = read_jsonl(temp.path().join(".autoloop/experiments.jsonl"));
    assert_eq!(records.len(), 2);
    assert_eq!(records[1]["status"], "crashed");
}

fn init_workspace(temp: &TempDir) {
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("init")
        .current_dir(temp.path())
        .assert()
        .success();
}

fn write_config(temp: &TempDir, content: &str) {
    fs::write(temp.path().join(".autoloop/config.toml"), content).expect("config should write");
}

fn baseline_config(
    eval_command: &str,
    memory_command: Option<&str>,
    retries: Option<u32>,
) -> String {
    let retries = retries.unwrap_or(1);
    let mut config = format!(
        r#"strictness = "advisory"

[metric]
name = "latency_p95"
direction = "lower"
unit = "ms"

[eval]
command = "{eval_command}"
timeout = 300
format = "metric_lines"
retries = {retries}

[confidence]
min_experiments = 3
keep_threshold = 1.0
rerun_threshold = 2.0

[git]
enabled = true
commit_prefix = "experiment:"
"#
    );

    if let Some(memory_command) = memory_command {
        config.push_str(&format!(
            r#"

[[guardrails]]
name = "memory_mb"
command = "{memory_command}"
format = "metric_lines"
threshold = "+10%"
"#
        ));
    }

    config
}

fn read_json(path: std::path::PathBuf) -> Value {
    let bytes = fs::read(path).expect("json file should read");
    serde_json::from_slice(&bytes).expect("json should parse")
}

fn read_jsonl(path: std::path::PathBuf) -> Vec<Value> {
    let content = fs::read_to_string(path).expect("jsonl should read");
    content
        .lines()
        .map(|line| serde_json::from_str(line).expect("jsonl line should parse"))
        .collect()
}
