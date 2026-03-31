mod baseline;
mod discard;
mod doctor;
mod eval;
mod finalize;
mod init;
mod install;
mod keep;
mod learn;
mod pre;
mod session;
mod status;

use anyhow::Result;

use crate::cli::{CliCommand, OutputFormat, SessionAction};

pub fn dispatch(command: CliCommand, output: OutputFormat) -> Result<()> {
    match command {
        CliCommand::Init(args) => init::run(args, output),
        CliCommand::Doctor(args) => doctor::run(args, output),
        CliCommand::Install(args) => install::run(args, output),
        CliCommand::Baseline(args) => baseline::run(args, output),
        CliCommand::Session(args) => match args.action {
            SessionAction::Start(start_args) => session::start(start_args, output),
            SessionAction::End(end_args) => session::end(end_args, output),
        },
        CliCommand::Pre(args) => pre::run(args, output),
        CliCommand::Eval(args) => eval::run(args, output),
        CliCommand::Keep(args) => keep::run(args, output),
        CliCommand::Discard(args) => discard::run(args, output),
        CliCommand::Status(args) => status::run(args, output),
        CliCommand::Learn(args) => learn::run(args, output),
        CliCommand::Finalize(args) => finalize::run(args, output),
    }
}
