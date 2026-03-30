use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

#[derive(Debug)]
pub struct CommandOutput {
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl CommandOutput {
    pub fn synthetic_failure(command: &str, stderr: impl Into<String>) -> Self {
        Self {
            command: command.to_string(),
            exit_code: None,
            stdout: String::new(),
            stderr: stderr.into(),
            timed_out: false,
        }
    }

    pub fn succeeded(&self) -> bool {
        !self.timed_out && self.exit_code == Some(0)
    }
}

pub fn run_shell_command(command: &str, cwd: &Path, timeout_secs: u64) -> Result<CommandOutput> {
    let mut child = shell_command(command)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to execute `{command}`"))?;

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let mut timed_out = false;

    loop {
        if child.try_wait()?.is_some() {
            break;
        }

        if Instant::now() >= deadline {
            timed_out = true;
            child
                .kill()
                .with_context(|| format!("failed to stop timed out command `{command}`"))?;
            break;
        }

        thread::sleep(Duration::from_millis(25));
    }

    let output = child
        .wait_with_output()
        .with_context(|| format!("failed to collect output from `{command}`"))?;

    Ok(CommandOutput {
        command: command.to_string(),
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        timed_out,
    })
}

#[cfg(target_os = "windows")]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", command]);
    cmd
}

#[cfg(not(target_os = "windows"))]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.args(["-lc", command]);
    cmd
}
