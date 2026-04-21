use std::net::IpAddr;

use ipnet::IpNet;
use serde::Deserialize;
use serde::de::{Deserializer, Error};
use url::Url;

#[derive(Deserialize)]
pub struct Rule {
    pub targets: Vec<Target>,
    pub exclude: Vec<Target>,
}

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