use std::fs;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn install_codex_creates_agents_and_skills() {
    let temp = TempDir::new().expect("tempdir should exist");

    let stdout = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["install", "codex", "--json"])
        .current_dir(temp.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: Value = serde_json::from_slice(&stdout).expect("json output should parse");
    assert_eq!(payload["tool"], "codex");
    assert_eq!(payload["context_path"], "AGENTS.md");
    assert!(temp.path().join("AGENTS.md").exists());
    assert!(
        temp.path()
            .join(".agents/skills/autoloop-run/SKILL.md")
            .exists()
    );
    assert!(
        temp.path()
            .join(".agents/skills/autoloop-run/agents/openai.yaml")
            .exists()
    );
}

#[test]
fn install_claude_and_generic_create_expected_files() {
    let temp = TempDir::new().expect("tempdir should exist");

    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args(["install", "claude-code"])
        .current_dir(temp.path())
        .assert()
        .success();
    assert!(temp.path().join("CLAUDE.md").exists());
    assert!(
        temp.path()
            .join(".claude/commands/autoloop-run.md")
            .exists()
    );

    let generic_dir = temp.path().join("generic");
    fs::create_dir_all(&generic_dir).expect("generic dir should exist");
    Command::cargo_bin("autoloop")
        .expect("binary should build")
        .args([
            "install",
            "generic",
            "--path",
            generic_dir.to_str().expect("path should be utf-8"),
            "--json",
        ])
        .current_dir(temp.path())
        .assert()
        .success();
    assert!(generic_dir.join("program.md").exists());
}

#[test]
fn install_refuses_to_overwrite_without_force() {
    let temp = TempDir::new().expect("tempdir should exist");
    fs::write(temp.path().join("AGENTS.md"), "manual\n").expect("agents file should write");

    let stderr = Command::cargo_bin("autoloop")
        .expect("binary should build")
        .env("NO_COLOR", "1")
        .args(["install", "codex"])
        .current_dir(temp.path())
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();

    let plain = String::from_utf8(stderr).expect("stderr should be utf-8");
    assert!(plain.contains("rerun with --force to overwrite"));
}
