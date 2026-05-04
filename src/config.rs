use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;
use validator::Validate;

use crate::ips::IpStack;
use crate::outputs::dnsmasq::DnsmasqConfig;
use crate::outputs::nftables::NftablesConfig;
use crate::resolving::ResolverConfig;
use crate::rules::RuleConfig;

#[derive(Deserialize, Validate)]
pub struct Config {
    pub ip_stack: IpStack,

    #[validate(nested)]
    pub resolver: ResolverConfig,

    #[validate(length(min = 1), custom(function = "RuleConfig::validate"), nested)]
    pub rules: BTreeMap<String, RuleConfig>,

    #[validate(nested)]
    #[serde(default)]
    pub dnsmasq: BTreeMap<PathBuf, DnsmasqConfig>,

    #[validate(nested)]
    #[serde(default)]
    pub nftables: BTreeMap<PathBuf, NftablesConfig>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let mut file = File::open(path)?;

        let config: Config = serde_yaml::from_reader(&mut file)?;
        config.validate()?;

        Ok(config)
    }
}