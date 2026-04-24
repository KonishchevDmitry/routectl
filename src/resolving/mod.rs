mod lists;

use std::sync::Mutex;

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use ipnet::IpNet;
use serde::{Deserialize, Serialize, Serializer};
use serde::de::{Deserializer, Error};
use tokio::sync::Semaphore;
use validator::Validate;
use url::Url;

use crate::ips::{self, Networks};
use crate::sources::{IpSource, IpSourceType, IpSourceList, IpSourceListRef};

use lists::Lists;

pub enum Target {
    Network(IpNet),
    List(Url),
}

impl Serialize for Target {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            &Target::Network(network) => network.to_string().serialize(serializer),
            &Target::List(ref url) => url.as_str().serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Target {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let target: String = Deserialize::deserialize(deserializer)?;

        if let Some(network) = ips::parse_network(&target) {
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

    lists: Lists,
    // XXX(konishchev): Apply
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

            lists: Lists::new().context("failed to create lists resolver")?,
            special_networks,
        })
    }

    pub async fn resolve(&self, targets: &[Target]) -> Result<Networks> {
        let networks = Mutex::new(Networks::new());

        {
            let mut stream = stream::iter(targets)
                .map(|target| self.resolve_target(target, &networks))
                .buffer_unordered(self.concurrency);

            while let Some(result) = stream.next().await {
                result?;
            }
        }

        Ok(networks.into_inner().unwrap())
    }

    async fn resolve_target(&self, target: &Target, result: &Mutex<Networks>) -> Result<()> {
        match target {
            &Target::Network(network) => {
                let source_list = IpSourceListRef::new(IpSourceList::Rule(None));
                let source = IpSource::new(IpSourceType::Network(network), source_list);
                result.lock().unwrap().add(network, source);
            },

            Target::List(url) => {
                let networks = {
                    let _permit = self.semaphore.acquire().await.unwrap();
                    self.lists.fetch(url).await.with_context(|| format!("fetch {url}"))?
                };

                let source_list = IpSourceListRef::new(IpSourceList::Rule(Some(url.to_owned())));

                {
                    let mut result = result.lock().unwrap();

                    for network in networks {
                        let source = IpSource::new(IpSourceType::Network(network), source_list.clone());
                        result.add(network, source);
                    }
                }
            },
        }
        Ok(())
    }
}