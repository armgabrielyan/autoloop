use std::fs;
use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn init_verify_reports_healthy_for_python_fixture() {
    let temp = TempDir::new().expect("tempdir should exist");
    copy_dir_all(&fixture_root("examples/smoke-python-search"), temp.path());

    let output = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["init", "--json", "--verify"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&output).expect("json output should parse");
    assert_eq!(payload["verification"]["healthy"], true);
    assert_eq!(payload["verification"]["eval"]["status"], "pass");
}

#[test]
fn doctor_reports_placeholder_config_as_unhealthy() {
    let temp = TempDir::new().expect("tempdir should exist");

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("init")
        .current_dir(temp.path())
        .assert()
        .success();

    let output = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["doctor", "--json"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&output).expect("json output should parse");
    assert_eq!(payload["healthy"], false);
    assert_eq!(payload["report"]["eval"]["status"], "fail");
    assert!(
        payload["report"]["eval"]["message"]
            .as_str()
            .expect("message should exist")
            .contains("default autoloop placeholder")
    );
}

#[test]
fn doctor_fix_rewrites_broken_config_with_verified_inference() {
    let temp = TempDir::new().expect("tempdir should exist");
    copy_dir_all(&fixture_root("examples/smoke-python-search"), temp.path());

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .arg("init")
        .current_dir(temp.path())
        .assert()
        .success();

    fs::write(
        temp.path().join(".autoloop/config.toml"),
        autoloop::config::default_config_template(),
    )
    .expect("config should overwrite");

    let output = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["doctor", "--json", "--fix"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&output).expect("json output should parse");
    assert_eq!(payload["healthy"], true);
    assert_eq!(payload["fix"]["applied"], true);
    assert_eq!(payload["report"]["eval"]["status"], "pass");

    let config = fs::read_to_string(temp.path().join(".autoloop/config.toml"))
        .expect("config should be readable");
    assert!(config.contains("command = \"python3 bench.py\""));
    assert!(config.contains("command = \"python3 -m unittest\""));
}

fn fixture_root(relative: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn copy_dir_all(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("destination should exist");
    for entry in fs::read_dir(source).expect("source dir should be readable") {
        let entry = entry.expect("dir entry should be readable");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_all(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path).expect("file should copy");
        }
    }
}
