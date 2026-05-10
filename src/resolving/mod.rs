mod r#as;
mod dns;
mod lists;

use std::collections::BTreeSet;
use std::fmt::{self, Display, Formatter};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use backon::{ExponentialBuilder, Retryable};
use futures::stream::{self, StreamExt, TryStreamExt};
use ipnet::IpNet;
use log::warn;
use serde::{Deserialize, Serialize, Serializer};
use serde::de::{Deserializer, Error as _};
use tokio::sync::Semaphore;
use validator::Validate;
use url::Url;

use crate::ips::{self, HumanNetwork, IpStack, IpVersion, Networks};
use crate::sources::{self, Domain, IpSource, IpSourceType, IpSourceList, IpSourceListRef};
use crate::util;

use r#as::AsResolver;
use dns::DnsResolver;
use lists::ListsResolver;

pub use r#as::AS_PREFIX;

pub enum Target {
    AS(u32),
    Domain(Domain),
    List(Url),
    Network(IpNet),
}

impl Target {
    pub fn deserialize_list<'de, D>(deserializer: D) -> Result<Vec<Target>, D::Error>
        where D: Deserializer<'de>
    {
        let values: Vec<String> = Deserialize::deserialize(deserializer)?;
        let mut targets = Vec::new();

        for value in values {
            let mut valid = false;

            for target in value.split_ascii_whitespace() {
                let target = Target::deserialize(target).map_err(|e|
                    D::Error::custom(format!("{e:#}")))?;
                targets.push(target);
                valid = true;
            }

            if !valid {
                return Err(D::Error::custom(format!("invalid target: {value:?}")));
            }
        }

        Ok(targets)
    }

    fn deserialize(target: &str) -> Result<Target> {
        Ok(if let Some(number) = target.strip_prefix(AS_PREFIX) {
            let number = number.parse().map_err(|_| anyhow!(
                "invalid AS number: {target:?}"))?;
            Target::AS(number)
        } else if let Some(network) = ips::parse_network(target) {
            Target::Network(network)
        } else if let Some(domain) = sources::parse_domain(target) {
            Target::Domain(domain)
        } else if let Ok(url) = target.parse::<Url>() && (url.scheme() == "https" || url.scheme() == "http") {
            Target::List(url)
        } else {
            return Err!("invalid target: {target:?}")
        })
    }
}

impl Serialize for Target {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            &Target::AS(number) => format!("{AS_PREFIX}{number}").serialize(serializer),
            Target::Domain(domain) => domain.to_string().serialize(serializer),
            Target::List(url) => url.as_str().serialize(serializer),
            &Target::Network(network) => network.to_string().serialize(serializer),
        }
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

pub struct Resolver<'a> {
    concurrency: usize,
    semaphore: Semaphore,
    retry: RetryConfig,

    special_networks: &'a Networks,

    r#as: AsResolver,
    dns: DnsResolver,
    lists: ListsResolver,
}

impl<'a> Resolver<'a> {
    pub fn new(config: &ResolverConfig, special_networks: &'a Networks) -> Result<Self> {
        Ok(Self {
            concurrency: config.concurrency,
            semaphore: Semaphore::new(config.concurrency),
            retry: config.retry,

            special_networks,

            r#as: AsResolver::new(),
            dns: DnsResolver::new(),
            lists: ListsResolver::new().context("failed to create lists resolver")?,
        })
    }

    pub async fn resolve(&self, context: &str, ip_stack: IpStack, targets: &[Target]) -> Result<(BTreeSet<Domain>, Networks)> {
        let domains = Mutex::new(BTreeSet::new());
        let networks = Mutex::new(Networks::new());

        {
            let manual_list = IpSourceListRef::new(IpSourceList::Manual);

            let mut stream = stream::iter(targets)
                .map(|target| self.resolve_target(context, &manual_list, ip_stack, target, &domains, &networks))
                .buffer_unordered(self.concurrency);

            while let Some(result) = stream.next().await {
                result?;
            }
        }

        Ok((
            domains.into_inner().unwrap(),
            networks.into_inner().unwrap(),
        ))
    }

    async fn resolve_target(
        &self, context: &str, manual_list: &IpSourceListRef, ip_stack: IpStack, target: &Target,
        result_domains: &Mutex<BTreeSet<Domain>>, result_networks: &Mutex<Networks>,
    ) -> Result<()> {
        match target {
            &Target::AS(number) => {
                let name = &format!("{AS_PREFIX}{number}");

                let as_networks = self.resolve_inner_by_ip_version(context, ip_stack, |version| async move {
                    self.r#as.resolve(number, version).await
                        .with_context(|| format!("resolve {name}"))
                }).await?;

                if as_networks.is_empty() {
                    return Err!("invalid autonomous system: {name}");
                }

                let source_list = IpSourceListRef::new(IpSourceList::As(number));
                self.on_resolved_network_list(context, as_networks, source_list, result_networks);
            },

            Target::Domain(domain) if domain.is_wildcard() => {
                result_domains.lock().unwrap().insert(domain.clone());
            },

            Target::Domain(domain) => {
                let domain_ips = self.resolve_inner_by_ip_version(context, ip_stack, |version| async move {
                    self.dns.resolve(domain, version).await
                        .with_context(|| format!("resolve {domain}"))
                }).await?;

                if domain_ips.is_empty() {
                    return Err!("invalid domain: {domain}");
                }

                let source_type = IpSourceType::Domain(Arc::new(domain.to_owned()));
                let source = IpSource::new(source_type, manual_list.clone());

                for domain_ip in domain_ips {
                    // FIXME(konishchev): Should we somehow filter the whole domain here?
                    for filtered_ip in ips::filter(context, domain_ip, &source, self.special_networks) {
                        result_networks.lock().unwrap().add(filtered_ip, source.clone());
                    }
                }

                result_domains.lock().unwrap().insert(domain.clone());
            },

            Target::List(url) => {
                let list_networks = self.resolve_inner(context, || async {
                    self.lists.fetch(url, ip_stack).await
                        .with_context(|| format!("fetch {url}"))
                }).await?;

                let source_list = IpSourceListRef::new(IpSourceList::List(url.to_owned()));
                self.on_resolved_network_list(context, list_networks, source_list, result_networks);
            },

            &Target::Network(network) => {
                if !ip_stack.matches(network) {
                    return Err!("{} doesn't belong to {ip_stack}", HumanNetwork(network));
                }

                let source_type = IpSourceType::Network(network);
                let source = IpSource::new(source_type, manual_list.clone());

                result_networks.lock().unwrap().add(network, source);
            },
        }

        Ok(())
    }

    async fn resolve_inner_by_ip_version<F, Fut, R>(&self, context: &str, ip_stack: IpStack, resolve: F) -> Result<Vec<R>>
        where
            F: Fn(IpVersion) -> Fut + Copy,
            Fut: Future<Output = Result<Vec<R>>>,
    {
        stream::iter(ip_stack)
            .map(|version| async move {
                self.resolve_inner(context, || async {
                    resolve(version).await
                }).await
            })
            .buffer_unordered(self.concurrency)
            .try_concat()
            .await
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
            for filtered_network in ips::filter(context, list_network, &source, self.special_networks) {
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