mod r#as;
mod lists;

use std::sync::Mutex;

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt, TryStreamExt};
use ipnet::IpNet;
use serde::{Deserialize, Serialize, Serializer};
use serde::de::{Deserializer, Error};
use tokio::sync::Semaphore;
use validator::Validate;
use url::Url;

use crate::ips::{self, HumanNetwork, IpStack, Networks};
use crate::sources::{IpSource, IpSourceType, IpSourceList, IpSourceListRef};

use r#as::AsResolver;
use lists::Lists;

pub use r#as::AS_PREFIX;

pub enum Target {
    AS(u32),
    List(Url),
    Network(IpNet),
}

impl Serialize for Target {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            &Target::AS(number) => format!("{AS_PREFIX}{number}").serialize(serializer),
            Target::List(url) => url.as_str().serialize(serializer),
            &Target::Network(network) => network.to_string().serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Target {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let target: String = Deserialize::deserialize(deserializer)?;

        if let Some(number) = target.strip_prefix(AS_PREFIX) {
            return Ok(Target::AS(number.parse().map_err(|_| D::Error::custom(format!(
                "invalid AS number: {target:?}")))?));
        } else if let Some(network) = ips::parse_network(&target) {
            return Ok(Target::Network(network))
        } else if let Ok(url) = target.parse::<Url>() && (url.scheme() == "https" || url.scheme() == "http") {
            return Ok(Target::List(url));
        }

        Err(D::Error::custom(format!("invalid target: {target:?}")))
    }
}

#[derive(Deserialize, Validate)]
pub struct ResolverConfig {
    #[validate(range(min = 1))]
    concurrency: usize,
}

pub struct Resolver {
    concurrency: usize,
    semaphore: Semaphore,

    r#as: AsResolver,
    lists: Lists,
    special_networks: Networks,
}

impl Resolver {
    pub fn new(config: &ResolverConfig) -> Result<Self> {
        // FIXME(konishchev): Add owned networks
        let special_networks = ips::reserved_networks().context(
            "failed to get a list of reserved networks")?;

        Ok(Self {
            concurrency: config.concurrency,
            semaphore: Semaphore::new(config.concurrency),

            r#as: AsResolver::new(),
            lists: Lists::new().context("failed to create lists resolver")?,
            special_networks,
        })
    }

    pub async fn resolve(&self, context: &str, ip_stack: IpStack, targets: &[Target]) -> Result<Networks> {
        let networks = Mutex::new(Networks::new());

        {
            let mut stream = stream::iter(targets)
                .map(|target| self.resolve_target(context, ip_stack, target, &networks))
                .buffer_unordered(self.concurrency);

            while let Some(result) = stream.next().await {
                result?;
            }
        }

        Ok(networks.into_inner().unwrap())
    }

    async fn resolve_target(&self, context: &str, ip_stack: IpStack, target: &Target, result: &Mutex<Networks>) -> Result<()> {
        match target {
            &Target::AS(number) => {
                let as_networks = stream::iter(ip_stack)
                    .map(|version| async move {
                        let _permit = self.semaphore.acquire().await.unwrap();
                        self.r#as.resolve(number, version).await
                    })
                    .buffer_unordered(self.concurrency)
                    .try_concat().await
                    .with_context(|| format!("resolve {AS_PREFIX}{number}"))?;

                let source_list = IpSourceListRef::new(IpSourceList::As(number));
                self.on_resolved_network_list(context, as_networks, source_list, result);
            },

            Target::List(url) => {
                let list_networks = {
                    let _permit = self.semaphore.acquire().await.unwrap();
                    self.lists.fetch(url, ip_stack).await.with_context(|| format!("fetch {url}"))?
                };

                let source_list = IpSourceListRef::new(IpSourceList::List(url.to_owned()));
                self.on_resolved_network_list(context, list_networks, source_list, result);
            },

            &Target::Network(network) => {
                if !ip_stack.matches(network) {
                    return Err!("{} doesn't belong to {ip_stack}", HumanNetwork(network));
                }

                let source_type = IpSourceType::Network(network);
                let source_list = IpSourceListRef::new(IpSourceList::Manual);
                let source = IpSource::new(source_type, source_list);

                result.lock().unwrap().add(network, source);
            },
        }

        Ok(())
    }

    fn on_resolved_network_list(
        &self, context: &str, list_networks: Vec<IpNet>, source_list: IpSourceListRef, result: &Mutex<Networks>,
    ) {
        for list_network in list_networks {
            let source = IpSource::new(IpSourceType::Network(list_network), source_list.clone());
            for filtered_network in ips::filter(context, list_network, &source, &self.special_networks) {
                result.lock().unwrap().add(filtered_network, source.clone());
            }
        }
    }
}