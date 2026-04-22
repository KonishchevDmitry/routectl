use std::error::Error;

use anyhow::Result;
use ipnet::IpNet;
use reqwest::{Client, ClientBuilder};
use url::Url;

pub struct Lists {
    client: Client,
}

impl Lists {
    pub fn new() -> Result<Self> {
        // FIXME(konishchev): Add repository
        let user_agent = format!("{name} ({homepage})",
            name=env!("CARGO_PKG_NAME"), homepage=env!("CARGO_PKG_REPOSITORY"));

        let client = ClientBuilder::new()
            .user_agent(user_agent)
            .build()?;

        Ok(Self { client })
    }

    // XXX(konishchev): HERE
    pub async fn fetch(&self, url: &Url) -> Result<Vec<IpNet>> {
        // self.client.get(url).send().await?.text().await
        // if !response.status().is_success() {
        //     return Err!("Server returned an error: {}", response.status());
        // }
        Ok(Vec::new())
    }
}

// XXX(konishchev): HERE
pub fn humanize_reqwest_error(err: reqwest::Error) -> String {
    let err = err.without_url();

    // reqwest/hyper errors hide all details, so extract the underlying error
    let mut err: &dyn Error = &err;
    while let Some(source) = err.source() {
        err = source;
    }

    err.to_string()
}