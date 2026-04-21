pub mod lists;

use anyhow::{Context, Result};

use lists::Lists;

pub struct Resolvers {
    pub lists: Lists,
}

impl Resolvers {
    pub fn new() -> Result<Self> {
        Ok(Self {
            lists: Lists::new().context("failed to create lists resolver")?,
        })
    }
}