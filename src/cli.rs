use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Parser)]
#[command(
    name = "autoloop",
    version,
    about = "Agent-agnostic iterative optimization CLI",
    long_about = None
)]
pub struct Cli {
    #[arg(long, global = true, help = "Emit machine-readable JSON output")]
    pub json: bool,

    #[command(subcommand)]
    pub command: CliCommand,
}

impl Cli {
    pub fn output_format(&self) -> OutputFormat {
        if self.json {
            OutputFormat::Json
        } else {
            OutputFormat::Human
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum CliCommand {
    #[command(about = "Initialize AutoLoop in the current repository")]
    Init(InitArgs),
    #[command(about = "Validate the current config and optionally repair it")]
    Doctor(DoctorArgs),
    #[command(about = "Install agent-specific AutoLoop wrappers")]
    Install(InstallArgs),
    #[command(about = "Record the baseline metric and guardrails")]
    Baseline(BaselineArgs),
    #[command(about = "Manage optimization sessions")]
    Session(SessionArgs),
    #[command(about = "Check prior experiment history before trying a change")]
    Pre(PreArgs),
    #[command(about = "Run the evaluation command and record a pending result")]
    Eval(EvalArgs),
    #[command(about = "Keep the current pending experiment result")]
    Keep(KeepArgs),
    #[command(about = "Discard the current pending experiment result")]
    Discard(DiscardArgs),
    #[command(about = "Show AutoLoop status and experiment history")]
    Status(StatusArgs),
    #[command(about = "Refresh learnings from recorded experiment history")]
    Learn(LearnArgs),
    #[command(about = "Create review branches from kept experiments")]
    Finalize(FinalizeArgs),
}

#[derive(Debug, Args, Clone)]
#[command(about = "Initialize .autoloop files and inferred config for this repository")]
pub struct InitArgs {
    #[arg(long, help = "Overwrite existing AutoLoop-managed files")]
    pub force: bool,

    #[arg(
        long,
        help = "Preview the files that would be created without writing them"
    )]
    pub dry_run: bool,

    #[arg(
        long,
        help = "Run config verification immediately after initialization"
    )]
    pub verify: bool,
}

#[derive(Debug, Args, Clone, Default)]
#[command(about = "Validate the current AutoLoop config and detect broken setup")]
pub struct DoctorArgs {
    #[arg(
        long,
        help = "Rewrite the config with a verified inferred candidate when possible"
    )]
    pub fix: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum InstallTool {
    #[value(name = "claude-code")]
    ClaudeCode,
    Codex,
    Cursor,
    Opencode,
    #[value(name = "gemini-cli")]
    GeminiCli,
    Generic,
}

impl InstallTool {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
            Self::Opencode => "opencode",
            Self::GeminiCli => "gemini-cli",
            Self::Generic => "generic",
        }
    }
}

#[derive(Debug, Args, Clone)]
#[command(about = "Install workspace-local wrappers for a supported coding agent")]
pub struct InstallArgs {
    #[arg(help = "Agent runtime to install wrappers for")]
    pub tool: InstallTool,

    #[arg(
        long,
        help = "Install into a specific workspace path instead of the current directory"
    )]
    pub path: Option<PathBuf>,

    #[arg(long, help = "Overwrite existing installed wrapper files")]
    pub force: bool,
}

#[derive(Debug, Args, Clone, Default)]
#[command(about = "Run the configured eval and guardrails to record a baseline")]
pub struct BaselineArgs {}

#[derive(Debug, Args, Clone)]
#[command(about = "Start or end an AutoLoop optimization session")]
pub struct SessionArgs {
    #[command(subcommand)]
    pub action: SessionAction,
}

#[derive(Debug, Subcommand, Clone)]
pub enum SessionAction {
    #[command(about = "Start a new optimization session")]
    Start(SessionStartArgs),
    #[command(about = "End the current optimization session and summarize it")]
    End(SessionEndArgs),
}

#[derive(Debug, Args, Clone)]
#[command(about = "Start a new session for tracking a sequence of experiments")]
pub struct SessionStartArgs {
    #[arg(long, help = "Optional human-readable name for the session")]
    pub name: Option<String>,
}

#[derive(Debug, Args, Clone, Default)]
#[command(about = "End the active session and write a session summary")]
pub struct SessionEndArgs {}

#[derive(Debug, Args, Clone)]
#[command(about = "Check experiment history for similar prior attempts before editing code")]
pub struct PreArgs {
    #[arg(
        long,
        help = "Short description of the experiment you are about to try"
    )]
    pub description: String,
}

#[derive(Debug, Args, Clone)]
#[command(about = "Run the configured eval command and record a pending verdict")]
pub struct EvalArgs {
    #[arg(
        long,
        help = "Temporarily override the configured eval command for this run"
    )]
    pub command: Option<String>,
}

#[derive(Debug, Args, Clone)]
#[command(about = "Accept the current pending experiment result")]
pub struct KeepArgs {
    #[arg(long, help = "Short description of the experiment being kept")]
    pub description: String,

    #[arg(long, help = "Create a git commit for the recorded experiment paths")]
    pub commit: bool,
}

#[derive(Debug, Args, Clone)]
#[command(about = "Reject the current pending experiment result")]
pub struct DiscardArgs {
    #[arg(long, help = "Short description of the experiment being discarded")]
    pub description: String,

    #[arg(long, help = "Reason the experiment is being discarded")]
    pub reason: String,

    #[arg(
        long,
        help = "Revert only the recorded experiment paths in the worktree"
    )]
    pub revert: bool,
}

#[derive(Debug, Args, Clone)]
#[command(about = "Show current status, baseline, pending evals, and experiment summaries")]
pub struct StatusArgs {
    #[arg(
        long,
        help = "Show status across all recorded experiments instead of only the active scope"
    )]
    pub all: bool,
}

#[derive(Debug, Args, Clone, Default)]
#[command(about = "Refresh learnings from recorded experiment outcomes")]
pub struct LearnArgs {
    #[arg(
        long,
        conflicts_with = "all",
        help = "Limit learnings to the latest completed session"
    )]
    pub session: bool,

    #[arg(long, help = "Aggregate learnings across all recorded sessions")]
    pub all: bool,
}

#[derive(Debug, Args, Clone, Default)]
#[command(about = "Create review branches from kept experiment commits")]
pub struct FinalizeArgs {
    #[arg(
        long,
        conflicts_with = "all",
        help = "Finalize only the latest completed session"
    )]
    pub session: bool,

    #[arg(long, help = "Finalize across all recorded kept experiments")]
    pub all: bool,
}
