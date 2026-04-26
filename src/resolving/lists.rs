use std::error;
use std::io::{self, ErrorKind};
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use ipnet::IpNet;
use log::debug;
use reqwest::{Client, ClientBuilder};
use tokio::io::AsyncBufReadExt;
use tokio_util::io::StreamReader;
use url::Url;

use crate::resolving::TransientError;
use crate::ips::{self, IpStack};
use crate::util;

pub struct Lists {
    client: Client,
}

impl Lists {
    pub fn new() -> Result<Self> {
        let user_agent = format!("{name} ({homepage})",
            name=env!("CARGO_PKG_NAME"), homepage=env!("CARGO_PKG_REPOSITORY"));

        let client = ClientBuilder::new()
            .user_agent(user_agent)
            .build()?;

        Ok(Self { client })
    }

    pub async fn fetch(&self, url: &Url, ip_stack: IpStack) -> Result<Vec<IpNet>> {
        debug!("Fetching {url} ({ip_stack})...");
        let start_time = Instant::now();

        let response = self.client.get(url.to_owned()).send().await
            .map_err(humanize_reqwest_error)
            .context(TransientError)?;

        if let status = response.status() && !status.is_success() {
            let mut err = Err!("server returned an error: {status}");
            if status.is_server_error() {
                err = err.context(TransientError);
            }
            return err;
        }

        let mut lines = StreamReader::new(response.bytes_stream().map(|result| {
            result.map_err(|e| io::Error::new(ErrorKind::Other, humanize_reqwest_error(e)))
        })).lines();

        let mut is_empty = true;
        let mut networks = Vec::new();

        while let Some(line) = lines.next_line().await.context(TransientError)? {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let network = ips::parse_network(line).ok_or_else(|| anyhow!(
                "got an invalid network: {line:?}"))?;

            is_empty = false;
            if ip_stack.matches(network) {
                networks.push(network);
            }
        }

        debug!("Got {} networks from {url} in {}.", networks.len(),
            util::format_duration(start_time.elapsed()));

        if is_empty {
            return Err!("the list is empty");
        } else if networks.is_empty() {
            return Err!("the list has no networks for {ip_stack}");
        }

        Ok(networks)
    }
}

fn humanize_reqwest_error(err: reqwest::Error) -> anyhow::Error {
    let err = err.without_url();

    // reqwest/hyper errors hide all details, so extract the underlying error
    let mut err: &dyn error::Error = &err;
    while let Some(source) = err.source() {
        err = source;
    }

    anyhow!("{err}")
}