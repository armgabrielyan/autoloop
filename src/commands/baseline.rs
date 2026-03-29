use anyhow::{Result, bail};

use crate::cli::{BaselineArgs, OutputFormat};

pub fn run(_args: BaselineArgs, _output: OutputFormat) -> Result<()> {
    bail!("`baseline` is scaffolded but not implemented yet")
}
