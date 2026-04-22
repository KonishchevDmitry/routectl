use std::fmt::Write;

use anyhow::Result;
use log::{Level, log_enabled, debug};

use crate::config::Config;
use crate::resolving::Resolver;
use crate::rules::Rule;

// XXX(konishchev): HERE
#[tokio::main]
pub async fn generate(config: &Config) -> Result<()> {
    let resolver = Resolver::new(&config.resolver)?;

    for rule in &config.rules {
        process_rule(&resolver, rule).await?;
    }

    Ok(())
}

// XXX(konishchev): HERE
async fn process_rule(resolver: &Resolver, rule: &Rule) -> Result<()> {
    let targets = resolver.resolve(&rule.targets).await?;
    let excludes = resolver.resolve(&rule.exclude).await?;

    let result = targets.filter(&excludes);

    if log_enabled!(Level::Debug) {
        let mut buf = String::new();

        write!(&mut buf, "Got the following networks:").unwrap();
        for (network, sources) in &result {
            write!(&mut buf, "\n* {network} (source: {sources})").unwrap();
        }

        debug!("{buf}");
    }

    Ok(())
}