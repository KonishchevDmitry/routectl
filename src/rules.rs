use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow};
use futures::stream::{self, StreamExt, TryStreamExt};
use log::{Level, log_enabled, debug};
use regex::Regex;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::config::Config;
use crate::ips::{HumanNetwork, IpStack, Networks};
use crate::resolving::{Resolver, Target};
use crate::sources::Domain;

#[derive(Deserialize, Serialize, Validate)]
pub struct RuleConfig {
    pub ip_stack: Option<IpStack>,

    #[serde(deserialize_with = "Target::deserialize_list")]
    pub targets: Vec<Target>,

    #[serde(default)]
    #[serde(deserialize_with = "Target::deserialize_list")]
    pub exclude: Vec<Target>,
}

impl RuleConfig {
    pub fn validate(rules: &BTreeMap<String, RuleConfig>) -> Result<(), ValidationError> {
        static NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
            r"^[a-z]+(?:-[a-z]+)*$").unwrap());

        for name in rules.keys() {
            if !NAME_RE.is_match(name) {
                return Err(ValidationError::new("invalid rule name").with_message(format!(
                    "invalid rule name: {name:?} (must match `{}`)", NAME_RE.as_str()).into()));
            }
        }

        Ok(())
    }

    // FIXME(konishchev): Compact the network lists
    // FIXME(konishchev): Do we need to calculate domains intersection?
    async fn resolve(&self, name: &str, global_ip_stack: IpStack, resolver: &Resolver) -> Result<Rule> {
        let ip_stack = self.ip_stack.unwrap_or(global_ip_stack);

        let (
            (target_domains, target_networks),
            (exclude_domains, exclude_networks),
        ) = tokio::try_join!(
            resolver.resolve(name, ip_stack, &self.targets),
            resolver.resolve(name, ip_stack, &self.exclude),
        )?;

        let target_networks = target_networks.filter(name, &exclude_networks);

        if log_enabled!(Level::Debug) {
            let mut buf = String::new();

            if !target_networks.is_empty() {
                write!(&mut buf, "[{name}] Got the following networks:").unwrap();
                for (network, sources) in &target_networks {
                    write!(&mut buf, "\n* {} (source: {sources})", HumanNetwork(network)).unwrap();
                }
                debug!("{buf}");
            }

            for (domain_type, domains) in [("target", &target_domains), ("exclude", &exclude_domains)] {
                if !domains.is_empty() {
                    buf.truncate(0);

                    write!(&mut buf, "[{name}] Got the following {domain_type} domains:").unwrap();
                    for domain in domains {
                        write!(&mut buf, "\n* {domain}").unwrap();
                    }
                    debug!("{buf}");
                }
            }
        }

        Ok(Rule {
            target_domains,
            exclude_domains,

            target_networks,
            exclude_networks,
        })
    }
}

pub struct Rule {
    pub target_domains: BTreeSet<Domain>,
    pub exclude_domains: BTreeSet<Domain>,

    pub target_networks: Networks,
    pub exclude_networks: Networks,
}

#[tokio::main]
pub async fn resolve(config: &Config) -> Result<HashMap<String, Rule>> {
    let resolver = &Resolver::new(&config.resolver)?;

    Ok(stream::iter(&config.rules)
        .map(|(name, spec)| {
            async move {
                spec.resolve(name, config.ip_stack, resolver).await
                    .with_context(|| format!("failed to process rule {name:?}"))
                    .map(|rule| (name.to_owned(), rule))
            }
        })
        .buffer_unordered(usize::MAX)
        .try_collect()
        .await?)
}

pub fn get<'a>(rules: &'a HashMap<String, Rule>, names: &[String]) -> Result<&'a Rule> {
    if names.is_empty() {
        return Err!("got an empty rule list");
    } else if names.len() != 1 {
        return Err!("multiple rules specification isn't supported yet")
    }

    let name = &names[0];
    rules.get(name).ok_or_else(|| anyhow!("invalid rule {name:?}"))
}