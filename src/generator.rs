use std::fmt::Write;

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt, TryStreamExt};
use log::{Level, log_enabled, debug};

use crate::config::Config;
use crate::ips::HumanNetwork;
use crate::resolving::Resolver;
use crate::rules::Rule;

#[tokio::main]
pub async fn generate(config: &Config) -> Result<()> {
    let resolver = Resolver::new(&config.resolver)?;

    stream::iter(&config.rules)
        .map(|(name, rule)| async {
            let name = name.clone();
            process_rule(&name, rule, &resolver).await.with_context(|| format!(
                "failed to process rule {name:?}"))
        })
        .buffer_unordered(usize::MAX)
        .try_collect::<Vec<_>>()
        .await?;

    Ok(())
}

async fn process_rule(name: &str, rule: &Rule, resolver: &Resolver) -> Result<()> {
    let (targets, excludes) = tokio::try_join!(
        resolver.resolve(name, &rule.targets),
        resolver.resolve(name, &rule.exclude),
    )?;

    let result = targets.filter(name, &excludes);

    if log_enabled!(Level::Debug) {
        let mut buf = String::new();

        write!(&mut buf, "[{name}] Got the following networks:").unwrap();
        for (network, sources) in &result {
            write!(&mut buf, "\n* {} (source: {sources})", HumanNetwork(network)).unwrap();
        }

        debug!("{buf}");
    }

    Ok(())
}