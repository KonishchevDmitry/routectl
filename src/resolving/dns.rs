use std::io::ErrorKind;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use hickory_resolver::{Resolver, TokioResolver};
use hickory_resolver::net::{DnsError, NetError};
use hickory_resolver::proto::ProtoError;
use hickory_resolver::proto::op::ResponseCode;
use hickory_resolver::proto::rr::{DNSClass, RData, RecordType};
use log::debug;
use tokio::sync::Mutex as AsyncMutex;

use crate::ips::IpVersion;
use crate::resolving::TransientError;
use crate::sources::Domain;
use crate::util;

pub struct DnsResolver {
    resolver: AsyncMutex<Option<Arc<TokioResolver>>>,
}

impl DnsResolver {
    pub fn new() -> Self {
        DnsResolver {
            resolver: AsyncMutex::new(None),
        }
    }

    pub async fn resolve(&self, domain: &Domain, version: IpVersion) -> Result<Vec<IpAddr>> {
        debug!("Resolving {domain} ({version})...");

        let mut name = domain.to_owned();
        name.set_fqdn(true); // To not issue additional queries

        let start_time = Instant::now();
        let result = self.resolver().await?.lookup(name, match version {
            IpVersion::V4 => RecordType::A,
            IpVersion::V6 => RecordType::AAAA,
        }).await;
        let duration = Instant::now() - start_time;

        let response = match result {
            Ok(response) => Some(response),
            Err(err) if err.is_nx_domain() => return Err!("invalid domain"),
            Err(err) if err.is_no_records_found() => None,
            Err(err @ (
                NetError::Busy | NetError::Dns(DnsError::ResponseCode(ResponseCode::ServFail)) |
                NetError::Io(_) | NetError::NoConnections | NetError::Timeout
            )) => return Err(err).context(TransientError),
            Err(err) => return Err(err.into()),
        };

        let mut ips = Vec::new();

        if let Some(response) = response {
            for answer in response.answers() {
                if answer.dns_class != DNSClass::IN {
                    continue;
                }

                match answer.data {
                    RData::A(ip) => ips.push(IpAddr::V4(ip.into())),
                    RData::AAAA(ip) => ips.push(IpAddr::V6(ip.into())),
                    _ => {},
                }
            }
        }

        debug!("Got {} {version} for {domain} in {}.", ips.len(), util::format_duration(duration));
        Ok(ips)
    }

    // Resolver builder will fail to build the resolver if network is down and no DNS is configured now, so do the
    // asynchronous initialization.
    async fn resolver(&self) -> Result<Arc<TokioResolver>> {
        let mut configured_resolver = self.resolver.lock().await;
        if let Some(resolver) = configured_resolver.clone() {
            return Ok(resolver);
        }

        let resolver = Resolver::builder_tokio().map_err(|err| {
            match err {
                // On MacOS, ProtoError is returned when Wi-Fi is turned off
                NetError::Proto(ProtoError::Msg(message)) => anyhow!("{message}").context(TransientError),
                NetError::Proto(ProtoError::Message(message)) => anyhow!("{message}").context(TransientError),

                // On Linux, any /etc/resolv.conf issues are translated into io::Error
                NetError::Io(err) if err.kind() == ErrorKind::Other => anyhow!("{err}").context(TransientError),

                _ => err.into(),
            }

        })?.build()?;

        let resolver = Arc::new(resolver);
        configured_resolver.replace(resolver.clone());

        Ok(resolver)
    }
}