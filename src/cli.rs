use clap::{Args, Parser, Subcommand};

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
    #[arg(long, global = true)]
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
    Init(InitArgs),
    Baseline(BaselineArgs),
    Session(SessionArgs),
    Eval(EvalArgs),
    Keep(KeepArgs),
    Discard(DiscardArgs),
    Status(StatusArgs),
}

#[derive(Debug, Args, Clone)]
pub struct InitArgs {
    #[arg(long)]
    pub force: bool,

    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args, Clone, Default)]
pub struct BaselineArgs {}

#[derive(Debug, Args, Clone)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub action: SessionAction,
}

#[derive(Debug, Subcommand, Clone)]
pub enum SessionAction {
    Start(SessionStartArgs),
    End(SessionEndArgs),
}

#[derive(Debug, Args, Clone)]
pub struct SessionStartArgs {
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Debug, Args, Clone, Default)]
pub struct SessionEndArgs {}

#[derive(Debug, Args, Clone)]
pub struct EvalArgs {
    #[arg(long)]
    pub command: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct KeepArgs {
    #[arg(long)]
    pub description: String,

    #[arg(long)]
    pub commit: bool,
}

#[derive(Debug, Args, Clone)]
pub struct DiscardArgs {
    #[arg(long)]
    pub description: String,

    #[arg(long)]
    pub reason: String,

    #[arg(long)]
    pub revert: bool,
}

#[derive(Debug, Args, Clone)]
pub struct StatusArgs {
    #[arg(long)]
    pub all: bool,
}
