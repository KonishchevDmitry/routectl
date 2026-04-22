use std::fs::File;
use std::path::Path;

use anyhow::Result;
use serde::Deserialize;
use validator::Validate;

use crate::resolving::ResolverConfig;
use crate::rules::Rule;

#[derive(Deserialize, Validate)]
pub struct Config {
    #[validate(nested)]
    pub resolver: ResolverConfig,
    // FIXME(konishchev): #[validate(length(min = 1))]
    #[validate(nested)]
    pub rules: Vec<Rule>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let mut file = File::open(path)?;

        let config: Config = serde_yaml::from_reader(&mut file)?;
        config.validate()?;

        Ok(config)
    }
}