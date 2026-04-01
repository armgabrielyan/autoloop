use std::fs;

use assert_cmd::Command;
use git2::{BranchType, IndexAddOption, Repository, Signature};
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn finalize_groups_committed_keeps_into_review_branches() {
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

    fs::write(
        temp.path().join("src/api.rs"),
        "pub fn api() -> u32 { 1 }\n",
    )
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
        .args(["keep", "--description", "api improvement 1", "--commit"])
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(
        temp.path().join("src/api.rs"),
        "pub fn api() -> u32 { 2 }\n",
    )
    .expect("api file should write");
    write_config(&temp, &config("echo METRIC latency_p95=44"));
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["keep", "--description", "api improvement 2", "--commit"])
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(
        temp.path().join("src/cache.rs"),
        "pub fn cache() -> u32 { 3 }\n",
    )
    .expect("cache file should write");
    write_config(&temp, &config("echo METRIC latency_p95=43"));
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["keep", "--description", "cache improvement", "--commit"])
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(temp.path().join("src/docs.rs"), "pub fn docs() {}\n")
        .expect("docs file should write");
    write_config(&temp, &config("echo METRIC latency_p95=42"));
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("eval")
        .current_dir(temp.path())
        .assert()
        .success();
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["keep", "--description", "docs improvement"])
        .current_dir(temp.path())
        .assert()
        .success();
    manual_commit(temp.path(), "clean docs change");

    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["finalize", "--json"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("json output should parse");
    assert_eq!(payload["scope"]["all"], false);
    assert_eq!(
        payload["created_branches"]
            .as_array()
            .expect("created_branches should be an array")
            .len(),
        2
    );
    assert_eq!(
        payload["skipped"]
            .as_array()
            .expect("skipped should be an array")
            .len(),
        1
    );
    assert!(
        payload["created_branches"]
            .as_array()
            .expect("created_branches should be an array")
            .iter()
            .any(|group| {
                group["branch_name"] == "autoloop/alpha/01-api"
                    && group["experiment_ids"]
                        .as_array()
                        .expect("experiment_ids should be an array")
                        .len()
                        == 2
            })
    );
    assert!(
        payload["created_branches"]
            .as_array()
            .expect("created_branches should be an array")
            .iter()
            .any(|group| group["branch_name"] == "autoloop/alpha/02-cache")
    );

    let repo = Repository::discover(temp.path()).expect("git repo should exist");
    assert!(
        repo.find_branch("autoloop/alpha/01-api", BranchType::Local)
            .is_ok()
    );
    assert!(
        repo.find_branch("autoloop/alpha/02-cache", BranchType::Local)
            .is_ok()
    );
}

#[test]
fn finalize_refuses_dirty_worktree() {
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
    fs::write(
        temp.path().join("src/api.rs"),
        "pub fn api() -> u32 { 1 }\n",
    )
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

    fs::write(temp.path().join("tracked.txt"), "dirty\n").expect("tracked file should drift");

    let stderr = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .env("NO_COLOR", "1")
        .args(["finalize"])
        .current_dir(temp.path())
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();

    let plain = String::from_utf8(stderr).expect("stderr should be utf-8");
    assert!(plain.contains("finalize requires a clean working tree"));
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

fn manual_commit(path: &std::path::Path, message: &str) {
    let repo = Repository::discover(path).expect("git repo should exist");
    let mut index = repo.index().expect("git index should open");
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .expect("files should stage");
    index.write().expect("git index should write");

    let tree_id = index.write_tree().expect("tree should write");
    let tree = repo.find_tree(tree_id).expect("tree should resolve");
    let parent = repo
        .head()
        .expect("HEAD should exist")
        .peel_to_commit()
        .expect("HEAD commit should resolve");
    let signature =
        Signature::now("Autoloop Tests", "tests@example.com").expect("git signature should exist");
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &[&parent],
    )
    .expect("manual commit should succeed");
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
