use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;
use validator::{Validate, ValidationError};

use crate::resolving::ResolverConfig;
use crate::rules::Rule;

#[derive(Deserialize, Validate)]
pub struct Config {
    #[validate(nested)]
    pub resolver: ResolverConfig,
    #[validate(length(min = 1), custom(function = "validate_rule_names"), nested)]
    pub rules: BTreeMap<String, Rule>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let mut file = File::open(path)?;

        let config: Config = serde_yaml::from_reader(&mut file)?;
        config.validate()?;

        Ok(config)
    }
}

fn validate_rule_names(rules: &BTreeMap<String, Rule>) -> Result<(), ValidationError> {
    for name in rules.keys() {
        Rule::validate_name(name)?;
    }
    Ok(())
}