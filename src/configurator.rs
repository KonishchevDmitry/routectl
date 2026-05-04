use anyhow::{Context, Result};

use crate::config::Config;
use crate::rules;

pub fn configure(config: &Config) -> Result<()> {
    let rules = rules::resolve(config)?;

    for (path, nftables) in &config.nftables {
        nftables.configure(path, config.ip_stack, &rules).with_context(|| format!(
            "configure {path:?}"))?;
    }

    for (path, dnsmasq) in &config.dnsmasq {
        dnsmasq.configure(path, config.ip_stack, &rules).with_context(|| format!(
            "configure {path:?}"))?;
    }

    Ok(())
}