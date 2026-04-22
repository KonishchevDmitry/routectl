mod lists;

use std::net::IpAddr;
use std::sync::Mutex;

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use ipnet::IpNet;
use serde::Deserialize;
use serde::de::{Deserializer, Error};
use tokio::sync::Semaphore;
use validator::Validate;
use url::Url;

use crate::ips::Networks;
use crate::sources::{IpSource, IpSourceRef};

use lists::Lists;

pub enum Target {
    Network(IpNet),
    List(Url),
}

impl<'de> Deserialize<'de> for Target {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let target: String = Deserialize::deserialize(deserializer)?;

        if let Ok(network) = target.parse() {
            return Ok(Target::Network(network));
        } else if let Ok(address) = target.parse::<IpAddr>() {
            return Ok(Target::Network(address.into()));
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
    lists: Lists,
    concurrency: usize,
    semaphore: Semaphore,
}

impl Resolver {
    pub fn new(config: &ResolverConfig) -> Result<Self> {
        Ok(Self {
            lists: Lists::new().context("failed to create lists resolver")?,
            concurrency: config.concurrency,
            semaphore: Semaphore::new(config.concurrency),
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

    // XXX(konishchev): HERE
    async fn resolve_target(&self, target: &Target, result: &Mutex<Networks>) -> Result<()> {
        match target {
            &Target::Network(network) => {
                let source = IpSourceRef::new(IpSource::Network(network));
                result.lock().unwrap().add(network, source);
            },
            Target::List(url) => {
                let _permit = self.semaphore.acquire().await.unwrap();
                self.lists.fetch(url).await?;
            },
        }
        Ok(())
    }
}