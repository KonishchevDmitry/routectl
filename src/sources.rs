use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

use ipnet::IpNet;


#[derive(strum::Display)]
pub enum IpSource {
    #[strum(to_string = "{0}")]
    Network(IpNet),
}

pub type IpSourceRef = Arc<IpSource>;

#[derive(Default)]
pub struct IpSources {
    sources: Vec<IpSourceRef>,
}

impl IpSources {
    pub fn add(&mut self, source: IpSourceRef) {
        self.sources.push(source);
    }

    pub fn extend(&mut self, other: &IpSources) {
        self.sources.extend(other.sources.iter().cloned());
    }
}

impl Display for IpSources {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for (index, source) in self.sources.iter().enumerate() {
            if index != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{source}")?;
        }
        Ok(())
    }
}