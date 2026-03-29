use anyhow::Result;
use serde::Serialize;

use crate::cli::OutputFormat;

pub fn emit<T>(format: OutputFormat, human: impl Into<String>, payload: &T) -> Result<()>
where
    T: Serialize,
{
    match format {
        OutputFormat::Human => {
            println!("{}", human.into());
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(payload)?);
        }
    }

    Ok(())
}
