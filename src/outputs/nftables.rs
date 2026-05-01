use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;
use validator::{Validate, ValidationError};

use crate::ips::IpStack;
use crate::resolving::ResolverConfig;
use crate::rules::Rule;

#[derive(Deserialize, Validate)]
pub struct NftablesConfig {
    #[validate(nested)]
    sets: BTreeMap<String, NftablesIpSet>,
}

#[derive(Deserialize, Validate)]
struct NftablesIpSet {
    #[validate(length(min = 1, max = 1))]
    pub rules: Vec<String>,
}

pub fn generate(configs: &BTreeMap<PathBuf, NftablesConfig>, rules: &HashMap<String, Rule>) -> Result<()> {
    Ok(())
}