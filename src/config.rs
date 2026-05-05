use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Deserializer};
use validator::Validate;

use crate::ips::{IpStack, Networks};
use crate::outputs::dnsmasq::DnsmasqConfig;
use crate::outputs::nftables::NftablesConfig;
use crate::resolving::ResolverConfig;
use crate::rules::RuleConfig;
use crate::sources::{IpSourceList, IpSourceListRef};

#[derive(Deserialize, Validate)]
pub struct Config {
    pub ip_stack: IpStack,

    #[serde(deserialize_with = "deserialize_owned_networks")]
    #[serde(default)]
    pub owned_networks: Networks,

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

fn deserialize_owned_networks<'de, D>(deserializer: D) -> Result<Networks, D::Error>
    where D: Deserializer<'de>
{
    let source_list = IpSourceListRef::new(IpSourceList::Special("owned"));
    Networks::deserialize(deserializer, source_list)
}