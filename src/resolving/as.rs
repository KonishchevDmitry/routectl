use std::time::Instant;

use anyhow::{Result, anyhow};
use ipnet::IpNet;
use log::debug;
use tokio::process::Command;

use crate::ips::IpVersion;
use crate::resolving::TransientError;
use crate::util;

pub const AS_PREFIX: &str = "AS";

pub struct AsResolver;

impl AsResolver {
    pub fn new() -> AsResolver {
        AsResolver
    }

    pub async fn resolve(&self, number: u32, version: IpVersion) -> Result<Vec<IpNet>> {
        let name = format!("{AS_PREFIX}{number}");

        let mut command = Command::new("bgpq4");
        command.arg(match version {
            IpVersion::V4 => "-4",
            IpVersion::V6 => "-6",
        }).arg("-F").arg(r"%n/%l\n").arg(&name);

        debug!("Resolving {name} ({version}) via `{:?}`...", command.as_std());
        let start_time = Instant::now();

        let result = command.output().await.map_err(|e| anyhow!(
            "failed to execute `{:?}`: {e}", command.as_std()))?;
        let finish_time = Instant::now();

        let status = result.status;
        let stderr = String::from_utf8_lossy(&result.stderr);

        if !status.success() {
            let mut err = anyhow!(
                "`{:?}` returned an error ({status}):{}",
                command.as_std(), util::format_multiline(&stderr));

            if status.code().is_some() {
                err = err.context(TransientError);
            }

            return Err(err);
        } else if !stderr.is_empty() {
            debug!("`{:?}` stderr:{}", command.as_std(), util::format_multiline(&stderr));
        }

        let stdout = String::from_utf8(result.stdout).map_err(|_| anyhow!(
            "`{:?}` returned a non-UTF-8 output", command.as_std()))?;

        let mut networks = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let network: IpNet = line.parse().ok()
                .filter(|&network| version.matches(network))
                .ok_or_else(|| anyhow!("got an invalid {version} network: {line:?}"))?;

            networks.push(network);
        }

        debug!("Got {} {version} networks for {name} in {}.",
            networks.len(), util::format_duration(finish_time - start_time));

        if networks.is_empty() {
            return Err!("invalid autonomous system");
        }

        Ok(networks)
    }
}