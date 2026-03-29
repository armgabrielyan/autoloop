use std::fs;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn init_creates_v0_layout_and_updates_gitignore() {
    let temp = TempDir::new().expect("tempdir should exist");
    fs::write(temp.path().join(".gitignore"), "/target\n").expect("gitignore should exist");

    let output = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["init", "--json"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&output).expect("json output should parse");
    assert_eq!(payload["dry_run"], false);
    assert!(temp.path().join(".autoloop/config.toml").exists());
    assert!(temp.path().join(".autoloop/state.json").exists());
    assert!(temp.path().join(".autoloop/last_eval.json").exists());
    assert!(temp.path().join(".autoloop/learnings.md").exists());
    assert!(temp.path().join(".autoloop/session.md").exists());

    let gitignore =
        fs::read_to_string(temp.path().join(".gitignore")).expect("gitignore should be readable");
    assert!(gitignore.contains(".autoloop/"));
}

#[test]
fn session_start_and_end_round_trip() {
    let temp = TempDir::new().expect("tempdir should exist");

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("init")
        .current_dir(temp.path())
        .assert()
        .success();

    let started = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["session", "start", "--json", "--name", "smoke"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let started_payload: Value =
        serde_json::from_slice(&started).expect("session start json should parse");
    assert_eq!(started_payload["name"], "smoke");

    let state_bytes =
        fs::read(temp.path().join(".autoloop/state.json")).expect("state should be readable");
    let state_json: Value = serde_json::from_slice(&state_bytes).expect("state json should parse");
    assert_eq!(state_json["active_session"]["name"], "smoke");

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["session", "end", "--json"])
        .current_dir(temp.path())
        .assert()
        .success();

    let state_bytes =
        fs::read(temp.path().join(".autoloop/state.json")).expect("state should be readable");
    let state_json: Value = serde_json::from_slice(&state_bytes).expect("state json should parse");
    assert!(state_json["active_session"].is_null());
}

#[test]
fn session_start_error_is_styled() {
    let temp = TempDir::new().expect("tempdir should exist");

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("init")
        .current_dir(temp.path())
        .assert()
        .success();

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("session")
        .arg("start")
        .current_dir(temp.path())
        .assert()
        .success();

    let stderr = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .env("NO_COLOR", "1")
        .arg("session")
        .arg("start")
        .current_dir(temp.path())
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();

    let plain = String::from_utf8(stderr).expect("stderr should be utf-8");
    assert!(
        plain.starts_with("error a session is already active; end it before starting a new one")
    );
    assert!(!plain.contains("Error:"));
}
