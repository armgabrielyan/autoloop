pub mod cli;
pub mod commands;
pub mod config;
pub mod detect;
pub mod error;
pub mod eval;
pub mod experiments;
pub mod finalize;
pub mod git;
pub mod integrations;
pub mod output;
pub mod shell;
pub mod state;
pub mod tags;
pub mod ui;
pub mod validation;

use anyhow::Result;
use clap::Parser;

pub fn run() -> Result<()> {
    let cli = cli::Cli::parse();
    let output = cli.output_format();
    commands::dispatch(cli.command, output)
}
