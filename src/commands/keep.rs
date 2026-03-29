use anyhow::{Result, bail};

use crate::cli::{KeepArgs, OutputFormat};

pub fn run(_args: KeepArgs, _output: OutputFormat) -> Result<()> {
    bail!("`keep` is scaffolded but not implemented yet")
}
