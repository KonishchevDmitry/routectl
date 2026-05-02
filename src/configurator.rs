use anyhow::Result;

use crate::config::Config;
use crate::rules;

pub fn configure(config: &Config) -> Result<()> {
    rules::resolve(config)?;
    Ok(())
}