use std::fs;

use assert_cmd::Command;
use git2::{IndexAddOption, Repository, Signature};
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn session_end_reports_summary_and_trigger_learn() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_git_repo(&temp);
    init_workspace(&temp);
    write_config(&temp, &config("advisory", "echo METRIC latency_p95=50"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("baseline")
        .current_dir(temp.path())
        .assert()
        .success();

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["session", "start", "--name", "smoke"])
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(temp.path().join("tracked.txt"), "changed once\n").expect("tracked file should edit");
    write_config(&temp, &config("advisory", "echo METRIC latency_p95=45"));
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["keep", "--description", "first improvement"])
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(temp.path().join("tracked.txt"), "changed twice\n")
        .expect("tracked file should edit");
    write_config(&temp, &config("advisory", "echo METRIC latency_p95=60"));
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args([
            "discard",
            "--description",
            "second try",
            "--reason",
            "regressed",
        ])
        .current_dir(temp.path())
        .assert()
        .success();
    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["session", "end", "--json"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("json output should parse");
    assert_eq!(payload["trigger_learn"], true);
    assert_eq!(payload["summary"]["experiments_run"], 2);
    assert_eq!(payload["summary"]["kept"], 1);
    assert_eq!(payload["summary"]["discarded"], 1);
    assert_eq!(payload["summary"]["best_improvement"]["experiment_id"], 2);
}

#[test]
fn status_scopes_to_active_session_and_all_history() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_git_repo(&temp);
    init_workspace(&temp);
    write_config(&temp, &config("advisory", "echo METRIC latency_p95=50"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("baseline")
        .current_dir(temp.path())
        .assert()
        .success();

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["session", "start", "--name", "alpha"])
        .current_dir(temp.path())
        .assert()
        .success();
    fs::write(temp.path().join("tracked.txt"), "changed once\n").expect("tracked file should edit");
    write_config(&temp, &config("advisory", "echo METRIC latency_p95=45"));
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["keep", "--description", "alpha keep"])
        .current_dir(temp.path())
        .assert()
        .success();
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["session", "end"])
        .current_dir(temp.path())
        .assert()
        .success();

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["session", "start", "--name", "beta"])
        .current_dir(temp.path())
        .assert()
        .success();
    fs::write(temp.path().join("tracked.txt"), "changed twice\n")
        .expect("tracked file should edit");
    write_config(&temp, &config("advisory", "echo METRIC latency_p95=60"));
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args([
            "discard",
            "--description",
            "beta discard",
            "--reason",
            "regressed",
        ])
        .current_dir(temp.path())
        .assert()
        .success();

    let session_stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["status", "--json"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let session_payload: Value =
        serde_json::from_slice(&session_stdout).expect("status json should parse");
    assert_eq!(session_payload["scope"]["all"], false);
    assert_eq!(session_payload["analysis"]["experiments_run"], 1);
    assert_eq!(session_payload["analysis"]["discarded"], 1);
    assert_eq!(session_payload["analysis"]["kept"], 0);
    assert_eq!(
        session_payload["analysis"]["current_streak"]["kind"],
        "failure"
    );

    let all_stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["status", "--json", "--all"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let all_payload: Value = serde_json::from_slice(&all_stdout).expect("status json should parse");
    assert_eq!(all_payload["scope"]["all"], true);
    assert_eq!(all_payload["analysis"]["experiments_run"], 2);
    assert_eq!(all_payload["analysis"]["kept"], 1);
    assert_eq!(all_payload["analysis"]["discarded"], 1);
}

#[test]
fn status_tolerates_future_fields_in_last_eval_state() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_git_repo(&temp);
    init_workspace(&temp);

    fs::write(
        temp.path().join(".autoloop/last_eval.json"),
        r#"{
  "schema_version": 1,
  "future_field": { "mode": "preview" },
  "pending_eval": {
    "metric": {
      "name": "latency_p95",
      "value": 42.0,
      "unit": "ms",
      "recorded_at": "2026-04-01T00:00:00Z",
      "future_metric_note": "ignored"
    },
    "delta_from_baseline": -8.0,
    "confidence": 1.5,
    "verdict": "keep",
    "command": {
      "command": "echo METRIC latency_p95=42",
      "exit_code": 0,
      "stdout": "METRIC latency_p95=42\n",
      "stderr": "",
      "timed_out": false,
      "future_capture": true
    },
    "guardrails": [],
    "worktree": {
      "file_paths": ["tracked.txt"],
      "untracked_paths": [],
      "auto_categories": ["tracked"],
      "path_states": [],
      "future_worktree": "ignored"
    },
    "future_pending": "ignored"
  }
}
"#,
    )
    .expect("last_eval should write");

    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["status", "--json", "--all"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("status json should parse");
    assert_eq!(payload["pending_eval"]["verdict"], "keep");
    assert_eq!(payload["pending_eval"]["metric"]["value"], 42.0);
}

fn init_git_repo(temp: &TempDir) {
    let repo = Repository::init(temp.path()).expect("git repo should initialize");
    let mut config = repo.config().expect("git config should open");
    config
        .set_bool("core.autocrlf", false)
        .expect("git autocrlf should disable");
    config
        .set_str("core.eol", "lf")
        .expect("git eol should pin to lf");
    fs::write(temp.path().join("tracked.txt"), "hello\n").expect("tracked file should write");

    let mut index = repo.index().expect("git index should open");
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .expect("initial files should stage");
    index.write().expect("git index should write");

    let tree_id = index.write_tree().expect("tree should write");
    let tree = repo.find_tree(tree_id).expect("tree should resolve");
    let signature =
        Signature::now("Autoloop Tests", "tests@example.com").expect("git signature should exist");
    repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .expect("initial commit should succeed");
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

fn config(strictness: &str, eval_command: &str) -> String {
    format!(
        r#"strictness = "{strictness}"

[metric]
name = "latency_p95"
direction = "lower"
unit = "ms"

[eval]
command = "{eval_command}"
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
    )
}
