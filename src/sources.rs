use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

use ipnet::IpNet;
use url::Url;

use crate::ips::HumanNetwork;

#[derive(Clone)]
pub struct IpSource {
    type_: IpSourceType,
    list: IpSourceListRef,
}

impl IpSource {
    pub fn new(type_: IpSourceType, list: IpSourceListRef) -> IpSource {
        IpSource { type_, list }
    }
}

impl Display for IpSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.list.as_ref() {
            IpSourceList::Rule(url) => {
                if let Some(url) = url {
                    write!(f, "{url}#")?;
                }
            },
            IpSourceList::Special(name) => {
                write!(f, "{name}:")?;
            },
        }
        write!(f, "{}", self.type_)
    }
}

#[derive(Clone)]
pub enum IpSourceType {
    Network(IpNet),
}

impl Display for IpSourceType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            &IpSourceType::Network(network) => write!(f, "{}", HumanNetwork(network)),
        }
    }
}

pub enum IpSourceList {
    Rule(Option<Url>),
    Special(&'static str),
}

pub type IpSourceListRef = Arc<IpSourceList>;

#[derive(Default)]
pub struct IpSources {
    sources: Vec<IpSource>,
}

impl IpSources {
    pub fn add(&mut self, source: IpSource) {
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