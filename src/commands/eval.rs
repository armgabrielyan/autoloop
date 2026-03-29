use anyhow::{Result, bail};

use crate::cli::{EvalArgs, OutputFormat};

pub fn run(_args: EvalArgs, _output: OutputFormat) -> Result<()> {
    bail!("`eval` is scaffolded but not implemented yet")
}
