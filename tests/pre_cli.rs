use std::fs;

use assert_cmd::Command;
use git2::{IndexAddOption, Repository, Signature};
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn pre_uses_description_history_and_flags_failed_exact_matches() {
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
        .args(["session", "start", "--name", "search"])
        .current_dir(temp.path())
        .assert()
        .success();

    fs::create_dir_all(temp.path().join("src")).expect("src directory should exist");
    fs::write(temp.path().join("src/api.rs"), "pub fn api() {}\n").expect("api file should write");
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

    for (metric, description) in [("55", "cache regression"), ("56", "cache retry")] {
        fs::write(
            temp.path().join("src/cache.rs"),
            format!("pub fn cache() -> u32 {{ {metric} }}\n"),
        )
        .expect("cache file should write");
        write_config(
            &temp,
            &config(&format!("echo METRIC latency_p95={metric}")),
        );
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

    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["pre", "--json", "--description", "cache regression"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("json output should parse");
    assert_eq!(payload["query"]["source"], "description");
    assert_eq!(payload["report"]["exact_matches"], 1);
    assert_eq!(payload["report"]["similar_experiments"], 2);
    assert_eq!(payload["report"]["kept"], 0);
    assert_eq!(payload["report"]["discarded"], 2);
    assert_eq!(payload["report"]["verdict"], "avoid");
    assert!(
        payload["report"]["category_signals"]
            .as_array()
            .expect("category_signals should be an array")
            .iter()
            .any(|entry| entry["name"] == "cache")
    );
}

#[test]
fn pre_prefers_working_tree_tags_when_changes_are_present() {
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
        .args(["session", "start", "--name", "search"])
        .current_dir(temp.path())
        .assert()
        .success();

    fs::create_dir_all(temp.path().join("src")).expect("src directory should exist");
    fs::write(temp.path().join("src/api.rs"), "pub fn api() {}\n").expect("api file should write");
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

    fs::write(
        temp.path().join("src/api.rs"),
        "pub fn api() { println!(\"again\"); }\n",
    )
    .expect("api file should change");

    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["pre", "--json", "--description", "unrelated words"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("json output should parse");
    assert_eq!(payload["query"]["source"], "working_tree");
    assert!(
        payload["query"]["file_paths"]
            .as_array()
            .expect("file_paths should be an array")
            .iter()
            .any(|entry| entry == "src/api.rs")
    );
    assert_eq!(payload["report"]["similar_experiments"], 1);
    assert_eq!(
        payload["report"]["matches"][0]["description"],
        "api improvement"
    );
    assert!(
        payload["report"]["matches"][0]["shared_file_paths"]
            .as_array()
            .expect("shared_file_paths should be an array")
            .iter()
            .any(|entry| entry == "src/api.rs")
    );
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
