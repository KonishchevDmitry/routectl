use anyhow::Result;

use crate::config::Config;
use crate::outputs::nftables;
use crate::rules;

pub fn generate(config: &Config) -> Result<()> {
    let rules = rules::resolve(config)?;
    nftables::generate(&config.nftables, &rules)?;
    Ok(())
}