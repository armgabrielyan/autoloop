use std::fs;

use assert_cmd::Command;
use git2::{IndexAddOption, Repository, Signature};
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn learn_reports_patterns_across_sessions() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_git_repo(&temp);
    init_workspace(&temp);
    write_config(&temp, &config("echo METRIC latency_p95=50"));

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
    fs::create_dir_all(temp.path().join("src")).expect("src directory should exist");
    fs::write(temp.path().join("src/api.rs"), "pub fn alpha() {}\n")
        .expect("api file should write");
    write_config(&temp, &config("echo METRIC latency_p95=45"));
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["keep", "--description", "api improvement", "--commit"])
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

    for (metric, description) in [
        ("55", "cache regression 1"),
        ("56", "cache regression 2"),
        ("57", "cache regression 3"),
    ] {
        fs::write(
            temp.path().join("src/cache.rs"),
            format!("pub fn cache() -> u32 {{ {metric} }}\n"),
        )
        .expect("cache file should write");
        write_config(&temp, &config(&format!("echo METRIC latency_p95={metric}")));
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
                description,
                "--reason",
                "regressed",
                "--revert",
            ])
            .current_dir(temp.path())
            .assert()
            .success();
    }

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["session", "end"])
        .current_dir(temp.path())
        .assert()
        .success();

    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["learn", "--json", "--all"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("json output should parse");
    assert_eq!(payload["scope"]["all"], true);
    assert_eq!(payload["report"]["summary"]["experiments_run"], 4);
    assert_eq!(payload["report"]["summary"]["kept"], 1);
    assert_eq!(payload["report"]["summary"]["discarded"], 3);
    assert_eq!(
        payload["report"]["best_experiments"][0]["description"],
        "api improvement"
    );
    assert!(
        payload["report"]["dead_end_categories"]
            .as_array()
            .expect("dead_end_categories should be an array")
            .iter()
            .any(|entry| entry["name"] == "cache")
    );
    assert!(
        payload["report"]["file_patterns"]
            .as_array()
            .expect("file_patterns should be an array")
            .iter()
            .any(|entry| entry["path"] == "src/cache.rs" && entry["signal"] == "never_kept")
    );
    assert_eq!(
        payload["report"]["session_trajectory"]
            .as_array()
            .expect("session_trajectory should be an array")
            .len(),
        2
    );
}

#[test]
fn learn_session_flag_scopes_to_latest_completed_session() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_git_repo(&temp);
    init_workspace(&temp);
    write_config(&temp, &config("echo METRIC latency_p95=50"));

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
    fs::create_dir_all(temp.path().join("src")).expect("src directory should exist");
    fs::write(temp.path().join("src/api.rs"), "pub fn alpha() {}\n")
        .expect("api file should write");
    write_config(&temp, &config("echo METRIC latency_p95=45"));
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["keep", "--description", "api improvement", "--commit"])
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
    fs::write(
        temp.path().join("src/cache.rs"),
        "pub fn cache() -> u32 { 55 }\n",
    )
    .expect("cache file should write");
    write_config(&temp, &config("echo METRIC latency_p95=55"));
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
            "cache regression",
            "--reason",
            "regressed",
            "--revert",
        ])
        .current_dir(temp.path())
        .assert()
        .success();
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["session", "end"])
        .current_dir(temp.path())
        .assert()
        .success();

    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["learn", "--json", "--session"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("json output should parse");
    assert_eq!(payload["scope"]["all"], false);
    assert_eq!(payload["report"]["summary"]["experiments_run"], 1);
    assert_eq!(payload["report"]["summary"]["kept"], 0);
    assert_eq!(payload["report"]["summary"]["discarded"], 1);
}

#[test]
fn learn_writes_markdown_summary_to_disk() {
    let temp = TempDir::new().expect("tempdir should exist");
    init_git_repo(&temp);
    init_workspace(&temp);
    write_config(&temp, &config("echo METRIC latency_p95=50"));

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
    fs::create_dir_all(temp.path().join("src")).expect("src directory should exist");
    fs::write(temp.path().join("src/api.rs"), "pub fn alpha() {}\n")
        .expect("api file should write");
    write_config(&temp, &config("echo METRIC latency_p95=45"));
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["keep", "--description", "api improvement", "--commit"])
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
        .args(["learn", "--all"])
        .current_dir(temp.path())
        .assert()
        .success();

    let learnings = fs::read_to_string(temp.path().join(".autoloop/learnings.md"))
        .expect("learnings file should be readable");
    assert!(learnings.contains("Scope: all experiments"));
    assert!(learnings.contains("## What Helped"));
    assert!(learnings.contains("api improvement"));
    assert!(learnings.contains("latency_p95=45ms"));
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

fn config(eval_command: &str) -> String {
    format!(
        r#"strictness = "advisory"

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
