use std::fmt::Write;

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt, TryStreamExt};
use log::{Level, log_enabled, debug};

use crate::config::Config;
use crate::resolving::Resolver;
use crate::rules::Rule;

#[tokio::main]
pub async fn generate(config: &Config) -> Result<()> {
    let resolver = Resolver::new(&config.resolver)?;

    stream::iter(&config.rules)
        .map(|(name, rule)| async {
            let name = name.clone();
            process_rule(&resolver, rule).await.with_context(|| format!(
                "failed to process rule {name:?}"))
        })
        .buffer_unordered(usize::MAX)
        .try_collect::<Vec<_>>()
        .await?;

    Ok(())
}

async fn process_rule(resolver: &Resolver, rule: &Rule) -> Result<()> {
    let (targets, excludes) = tokio::try_join!(
        resolver.resolve(&rule.targets),
        resolver.resolve(&rule.exclude),
    )?;

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