use anyhow::{Result, bail};

use crate::cli::{DiscardArgs, OutputFormat};

pub fn run(_args: DiscardArgs, _output: OutputFormat) -> Result<()> {
    bail!("`discard` is scaffolded but not implemented yet")
}
