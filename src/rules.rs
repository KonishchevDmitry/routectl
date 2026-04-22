use serde::Deserialize;
use validator::Validate;

use crate::resolving::Target;

#[derive(Deserialize, Validate)]
pub struct Rule {
    // FIXME(konishchev): #[validate(length(min = 1))]
    pub targets: Vec<Target>,
    pub exclude: Vec<Target>,
}