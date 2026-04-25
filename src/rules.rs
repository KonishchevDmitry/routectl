use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::ips::IpStack;
use crate::resolving::Target;

#[derive(Deserialize, Serialize, Validate)]
pub struct Rule {
    pub ip_stack: Option<IpStack>,
    #[validate(length(min = 1))]
    pub targets: Vec<Target>,
    pub exclude: Vec<Target>,
}

impl Rule {
    pub fn validate_name(name: &str) -> Result<(), ValidationError> {
        static NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
            r"^[a-z]+(?:-[a-z]+)*$").unwrap());

        if !NAME_RE.is_match(name) {
            return Err(ValidationError::new("invalid rule name").with_message(format!(
                "invalid rule name: {name:?} (must match `{}`)", NAME_RE.as_str()).into()));
        }

        Ok(())
    }
}