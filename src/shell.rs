use std::path::Path;
use std::process::{Command, ExitStatus};

use anyhow::{Context, Result};

#[derive(Debug)]
pub struct CommandOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_shell_command(command: &str, cwd: &Path) -> Result<CommandOutput> {
    #[cfg(target_os = "windows")]
    let output = Command::new("cmd")
        .args(["/C", command])
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to execute `{command}`"))?;

    #[cfg(not(target_os = "windows"))]
    let output = Command::new("sh")
        .args(["-lc", command])
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to execute `{command}`"))?;

    Ok(CommandOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
