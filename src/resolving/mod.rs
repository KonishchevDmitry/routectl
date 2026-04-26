mod r#as;
mod lists;

use std::fmt::{self, Display, Formatter};
use std::future::Future;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use backon::{ExponentialBuilder, Retryable};
use futures::stream::{self, StreamExt, TryStreamExt};
use ipnet::IpNet;
use log::warn;
use serde::{Deserialize, Serialize, Serializer};
use serde::de::{Deserializer, Error as _};
use tokio::sync::Semaphore;
use validator::Validate;
use url::Url;

use crate::ips::{self, HumanNetwork, IpStack, Networks};
use crate::sources::{IpSource, IpSourceType, IpSourceList, IpSourceListRef};
use crate::util;

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

    #[serde(default)]
    #[validate(nested)]
    retry: RetryConfig,
}

pub struct Resolver {
    concurrency: usize,
    semaphore: Semaphore,
    retry: RetryConfig,

    special_networks: Networks,

    r#as: AsResolver,
    lists: Lists,
}

impl Resolver {
    pub fn new(config: &ResolverConfig) -> Result<Self> {
        // FIXME(konishchev): Add owned networks
        let special_networks = ips::reserved_networks().context(
            "failed to get a list of reserved networks")?;

        Ok(Self {
            concurrency: config.concurrency,
            semaphore: Semaphore::new(config.concurrency),
            retry: config.retry,

            special_networks,

            r#as: AsResolver::new(),
            lists: Lists::new().context("failed to create lists resolver")?,
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
                        self.resolve_inner(context, || async {
                            self.r#as.resolve(number, version).await
                                .with_context(|| format!("resolve {AS_PREFIX}{number}"))
                        }).await
                    })
                    .buffer_unordered(self.concurrency)
                    .try_concat()
                    .await?;

                let source_list = IpSourceListRef::new(IpSourceList::As(number));
                self.on_resolved_network_list(context, as_networks, source_list, result);
            },

            Target::List(url) => {
                let list_networks = self.resolve_inner(context, || async {
                    self.lists.fetch(url, ip_stack).await
                        .with_context(|| format!("fetch {url}"))
                }).await?;

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

    async fn resolve_inner<F, Fut, R>(&self, context: &str, resolve: F) -> Result<R>
        where
            F: Fn() -> Fut,
            Fut: Future<Output = Result<R>>,
    {
        let _permit = self.semaphore.acquire().await.unwrap();

        resolve
            .retry(self.retry.backoff_builder())
            .when(anyhow::Error::is::<TransientError>)
            .notify(|err: &anyhow::Error, delay: Duration| {
                warn!("[{context}] [retry in {}] {}", util::format_duration(delay), util::format_error(err));
            })
            .await
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

#[derive(Debug)]
struct TransientError;

impl std::error::Error for TransientError {
}

impl Display for TransientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("transient error")
    }
}

#[derive(Clone, Copy, Default, Deserialize, Validate)]
struct RetryConfig {
    #[serde(default, with = "humantime_serde")]
    min_delay: Option<Duration>,
    #[serde(default, with = "humantime_serde")]
    max_delay: Option<Duration>,

    max_times: Option<usize>,
    #[serde(default, with = "humantime_serde")]
    max_total_delay: Option<Duration>,
}

impl RetryConfig {
    fn backoff_builder(&self) -> ExponentialBuilder {
        let mut builder = ExponentialBuilder::new()
            .with_min_delay(self.min_delay.unwrap_or(Duration::from_secs(1)))
            .with_max_delay(self.max_delay.unwrap_or(Duration::from_mins(1)))
            .with_max_times(self.max_times.unwrap_or(3));

        if self.max_times.is_none() && self.max_total_delay.is_some() {
            builder = builder.without_max_times();
        }

        builder.with_total_delay(self.max_total_delay)
    }
}