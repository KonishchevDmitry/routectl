use std::fmt::Write;

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt, TryStreamExt};
use log::{Level, log_enabled, debug};

use crate::config::Config;
use crate::ips::{HumanNetwork, IpStack};
use crate::resolving::{Resolver, Target};

#[tokio::main]
pub async fn generate(config: &Config) -> Result<()> {
    let resolver = Resolver::new(&config.resolver)?;

    stream::iter(&config.rules)
        .map(|(name, rule)| {
            let resolver = &resolver;
            async move {
                let ip_stack = rule.ip_stack.unwrap_or(config.ip_stack);
                process_rule(name, ip_stack, &rule.targets, &rule.exclude, resolver).await.with_context(|| format!(
                    "failed to process rule {name:?}"))
            }
        })
        .buffer_unordered(usize::MAX)
        .try_collect::<Vec<_>>()
        .await?;

    Ok(())
}

async fn process_rule(
    context: &str, ip_stack: IpStack, targets: &[Target], excludes: &[Target], resolver: &Resolver,
) -> Result<()> {
    let (targets, excludes) = tokio::try_join!(
        resolver.resolve(context, ip_stack, targets),
        resolver.resolve(context, ip_stack, excludes),
    )?;

    let result = targets.filter(context, &excludes);

    if log_enabled!(Level::Debug) {
        let mut buf = String::new();

        write!(&mut buf, "[{context}] Got the following networks:").unwrap();
        for (network, sources) in &result {
            write!(&mut buf, "\n* {} (source: {sources})", HumanNetwork(network)).unwrap();
        }

        debug!("{buf}");
    }

    Ok(())
}