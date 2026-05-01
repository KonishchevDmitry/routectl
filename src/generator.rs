use anyhow::Result;

use crate::config::Config;
use crate::rules;

pub fn generate(config: &Config) -> Result<()> {
    rules::resolve(config)?;
    Ok(())
}