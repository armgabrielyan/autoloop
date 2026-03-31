use std::fs;
use std::path::Path;

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

#[test]
fn init_infers_python_fixture_commands() {
    let temp = TempDir::new().expect("tempdir should exist");
    copy_dir_all(&fixture_root("examples/smoke-python-search"), temp.path());

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
    assert_eq!(payload["config_inference"]["source"], "inferred");
    assert_eq!(payload["config_inference"]["project_kind"], "python");
    assert_eq!(
        payload["config_inference"]["eval_command"],
        "python3 bench.py"
    );
    assert_eq!(payload["config_inference"]["metric_name"], "latency_p95");
    assert_eq!(
        payload["config_inference"]["guardrail_commands"][0],
        "python3 -m unittest"
    );

    let config = fs::read_to_string(temp.path().join(".autoloop/config.toml"))
        .expect("config should be readable");
    assert!(config.contains("command = \"python3 bench.py\""));
    assert!(config.contains("name = \"latency_p95\""));
    assert!(config.contains("kind = \"pass_fail\""));
    assert!(config.contains("command = \"python3 -m unittest\""));
}

#[test]
fn init_infers_rust_fixture_commands() {
    let temp = TempDir::new().expect("tempdir should exist");
    copy_dir_all(&fixture_root("examples/smoke-rust-cli"), temp.path());

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
    assert_eq!(payload["config_inference"]["source"], "inferred");
    assert_eq!(payload["config_inference"]["project_kind"], "rust");
    assert_eq!(
        payload["config_inference"]["eval_command"],
        "cargo run --quiet --bin bench"
    );
    assert_eq!(payload["config_inference"]["metric_name"], "latency_p95");
    assert_eq!(
        payload["config_inference"]["guardrail_commands"][0],
        "cargo test"
    );

    let config = fs::read_to_string(temp.path().join(".autoloop/config.toml"))
        .expect("config should be readable");
    assert!(config.contains("command = \"cargo run --quiet --bin bench\""));
    assert!(config.contains("name = \"latency_p95\""));
    assert!(config.contains("kind = \"pass_fail\""));
    assert!(config.contains("command = \"cargo test\""));
}

#[test]
fn init_infers_bun_workspace_commands() {
    let temp = TempDir::new().expect("tempdir should exist");
    fs::write(temp.path().join("bun.lockb"), "").expect("lockfile should write");
    fs::write(
        temp.path().join("package.json"),
        r#"{
  "name": "demo",
  "scripts": {
    "bench": "bun bench.ts",
    "test": "bun test"
  }
}
"#,
    )
    .expect("package.json should write");
    fs::write(
        temp.path().join("bench.ts"),
        "console.log('METRIC latency_p95=18.4');\n",
    )
    .expect("bench file should write");

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
    assert_eq!(payload["config_inference"]["source"], "inferred");
    assert_eq!(payload["config_inference"]["project_kind"], "node");
    assert_eq!(payload["config_inference"]["eval_command"], "bun run bench");
    assert_eq!(payload["config_inference"]["metric_name"], "latency_p95");
    assert_eq!(
        payload["config_inference"]["guardrail_commands"][0],
        "bun run test"
    );

    let config = fs::read_to_string(temp.path().join(".autoloop/config.toml"))
        .expect("config should be readable");
    assert!(config.contains("command = \"bun run bench\""));
    assert!(config.contains("name = \"latency_p95\""));
    assert!(config.contains("command = \"bun run test\""));
}

#[test]
fn init_infers_dotnet_workspace_commands() {
    let temp = TempDir::new().expect("tempdir should exist");
    fs::write(
        temp.path().join("Demo.sln"),
        "Microsoft Visual Studio Solution File, Format Version 12.00\n",
    )
    .expect("solution should write");
    fs::create_dir_all(temp.path().join("Benchmarks")).expect("bench dir should exist");
    fs::create_dir_all(temp.path().join("Demo.Tests")).expect("test dir should exist");
    fs::write(
        temp.path().join("Benchmarks/Benchmarks.csproj"),
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
  </PropertyGroup>
</Project>
"#,
    )
    .expect("bench csproj should write");
    fs::write(
        temp.path().join("Benchmarks/Program.cs"),
        "Console.WriteLine(\"METRIC latency_p95=11.7\");\n",
    )
    .expect("program should write");
    fs::write(
        temp.path().join("Demo.Tests/Demo.Tests.csproj"),
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <IsTestProject>true</IsTestProject>
  </PropertyGroup>
</Project>
"#,
    )
    .expect("test csproj should write");

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
    assert_eq!(payload["config_inference"]["source"], "inferred");
    assert_eq!(payload["config_inference"]["project_kind"], "dot_net");
    assert_eq!(
        payload["config_inference"]["eval_command"],
        "dotnet run --project Benchmarks/Benchmarks.csproj"
    );
    assert_eq!(payload["config_inference"]["metric_name"], "latency_p95");
    assert_eq!(
        payload["config_inference"]["guardrail_commands"][0],
        "dotnet test Demo.sln"
    );

    let config = fs::read_to_string(temp.path().join(".autoloop/config.toml"))
        .expect("config should be readable");
    assert!(config.contains("command = \"dotnet run --project Benchmarks/Benchmarks.csproj\""));
    assert!(config.contains("name = \"latency_p95\""));
    assert!(config.contains("command = \"dotnet test Demo.sln\""));
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
