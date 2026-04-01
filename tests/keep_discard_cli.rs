use std::fs;
use std::io::Write;

use assert_cmd::Command;
use git2::{IndexAddOption, Repository, Signature};
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn keep_records_success_and_clears_pending_eval() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_git_repo(&temp);
    init_workspace(&temp);
    write_config(&temp, &config("advisory", "echo 'METRIC latency_p95=50'"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("baseline")
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(temp.path().join("tracked.txt"), "changed once\n").expect("tracked file should edit");
    write_config(&temp, &config("advisory", "echo 'METRIC latency_p95=45'"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();

    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["keep", "--description", "improved tracked file", "--json"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("json output should parse");
    assert_eq!(payload["status"], "kept");

    let last_eval = read_json(temp.path().join(".autoloop/last_eval.json"));
    assert!(last_eval["pending_eval"].is_null());

    let records = read_jsonl(temp.path().join(".autoloop/experiments.jsonl"));
    assert_eq!(records.len(), 2);
    assert_eq!(records[1]["status"], "kept");
    assert_eq!(records[1]["description"], "improved tracked file");
    assert!(
        records[1]["tags"]["file_paths"]
            .as_array()
            .expect("file paths should be an array")
            .iter()
            .any(|value| value == "tracked.txt")
    );
}

#[test]
fn strict_keep_refuses_non_keep_verdict() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_git_repo(&temp);
    init_workspace(&temp);
    write_config(&temp, &config("strict", "echo 'METRIC latency_p95=50'"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("baseline")
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(temp.path().join("tracked.txt"), "changed once\n").expect("tracked file should edit");
    write_config(&temp, &config("strict", "echo 'METRIC latency_p95=60'"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();

    let stderr = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .env("NO_COLOR", "1")
        .args(["keep", "--description", "should fail"])
        .current_dir(temp.path())
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();

    let plain = String::from_utf8(stderr).expect("stderr should be utf-8");
    assert!(plain.contains("strict mode requires a KEEP verdict"));

    let last_eval = read_json(temp.path().join(".autoloop/last_eval.json"));
    assert!(last_eval["pending_eval"].is_object());
}

#[test]
fn discard_revert_restores_tracked_file() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_git_repo(&temp);
    init_workspace(&temp);
    write_config(&temp, &config("advisory", "echo 'METRIC latency_p95=50'"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("baseline")
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(temp.path().join("tracked.txt"), "changed once\n").expect("tracked file should edit");
    write_config(&temp, &config("advisory", "echo 'METRIC latency_p95=60'"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();

    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args([
            "discard",
            "--description",
            "rejected tracked change",
            "--reason",
            "latency regressed",
            "--revert",
            "--json",
        ])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("json output should parse");
    assert_eq!(payload["status"], "discarded");
    assert_eq!(
        fs::read_to_string(temp.path().join("tracked.txt")).expect("tracked file should read"),
        "hello\n"
    );

    let last_eval = read_json(temp.path().join(".autoloop/last_eval.json"));
    assert!(last_eval["pending_eval"].is_null());

    let records = read_jsonl(temp.path().join(".autoloop/experiments.jsonl"));
    assert_eq!(records[1]["status"], "discarded");
    assert_eq!(records[1]["reason"], "latency regressed");
}

#[test]
fn discard_revert_preserves_preexisting_untracked_setup_files() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_git_repo(&temp);
    init_workspace(&temp);
    write_config(&temp, &config("advisory", "echo 'METRIC latency_p95=50'"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("baseline")
        .current_dir(temp.path())
        .assert()
        .success();

    fs::create_dir_all(temp.path().join(".agents/skills/autoloop-run"))
        .expect("skills directory should exist");
    fs::write(temp.path().join("AGENTS.md"), "# Installed wrapper\n")
        .expect("context file should write");
    fs::write(
        temp.path().join(".agents/skills/autoloop-run/SKILL.md"),
        "# Skill\n",
    )
    .expect("skill file should write");

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args([
            "pre",
            "--description",
            "reject tracked file change without touching installed wrappers",
        ])
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(temp.path().join("tracked.txt"), "changed once\n").expect("tracked file should edit");
    write_config(&temp, &config("advisory", "echo 'METRIC latency_p95=60'"));

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
            "reject tracked change only",
            "--reason",
            "latency regressed",
            "--revert",
        ])
        .current_dir(temp.path())
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(temp.path().join("tracked.txt")).expect("tracked file should read"),
        "hello\n"
    );
    assert!(temp.path().join("AGENTS.md").exists());
    assert!(
        temp.path()
            .join(".agents/skills/autoloop-run/SKILL.md")
            .exists()
    );
}

#[test]
fn keep_survives_git_exclude_changes_when_experiment_paths_match() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_git_repo(&temp);
    init_workspace(&temp);
    write_config(&temp, &config("advisory", "echo 'METRIC latency_p95=50'"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("baseline")
        .current_dir(temp.path())
        .assert()
        .success();

    fs::create_dir_all(temp.path().join(".agents/skills/autoloop-run"))
        .expect("skills directory should exist");
    fs::write(temp.path().join("AGENTS.md"), "# Installed wrapper\n")
        .expect("context file should write");
    fs::write(
        temp.path().join(".agents/skills/autoloop-run/SKILL.md"),
        "# Skill\n",
    )
    .expect("skill file should write");

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args([
            "pre",
            "--description",
            "keep tracked file change after local git metadata tweak",
        ])
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(temp.path().join("tracked.txt"), "changed once\n").expect("tracked file should edit");
    write_config(&temp, &config("advisory", "echo 'METRIC latency_p95=45'"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();

    let mut exclude = fs::OpenOptions::new()
        .append(true)
        .open(temp.path().join(".git/info/exclude"))
        .expect("exclude file should open");
    writeln!(exclude, ".agents/").expect("exclude file should write");
    writeln!(exclude, "AGENTS.md").expect("exclude file should write");

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args([
            "keep",
            "--description",
            "tracked change only after exclude tweak",
            "--json",
        ])
        .current_dir(temp.path())
        .assert()
        .success();

    let last_eval = read_json(temp.path().join(".autoloop/last_eval.json"));
    assert!(last_eval["pending_eval"].is_null());
}

#[test]
fn discard_refuses_when_worktree_drifted() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_git_repo(&temp);
    init_workspace(&temp);
    write_config(&temp, &config("advisory", "echo 'METRIC latency_p95=50'"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("baseline")
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(temp.path().join("tracked.txt"), "changed once\n").expect("tracked file should edit");
    write_config(&temp, &config("advisory", "echo 'METRIC latency_p95=60'"));

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(temp.path().join("tracked.txt"), "changed twice\n")
        .expect("tracked file should drift");

    let stderr = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .env("NO_COLOR", "1")
        .args([
            "discard",
            "--description",
            "rejected tracked change",
            "--reason",
            "latency regressed",
        ])
        .current_dir(temp.path())
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();

    let plain = String::from_utf8(stderr).expect("stderr should be utf-8");
    assert!(plain.contains("working tree no longer matches the recorded pending eval"));

    let last_eval = read_json(temp.path().join(".autoloop/last_eval.json"));
    assert!(last_eval["pending_eval"].is_object());
}

fn init_git_repo(temp: &TempDir) {
    let repo = Repository::init(temp.path()).expect("git repo should initialize");
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
