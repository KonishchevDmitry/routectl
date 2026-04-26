use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

use ipnet::IpNet;
use url::Url;

use crate::ips::HumanNetwork;
use crate::resolving::AS_PREFIX;

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
            IpSourceList::As(number) => {
                write!(f, "{AS_PREFIX}{number}[{}]", self.type_)
            },
            IpSourceList::List(url) => {
                write!(f, "{url}#{}", self.type_)
            },
            IpSourceList::Manual => {
                write!(f, "{}", self.type_)
            },
            IpSourceList::Special(name) => {
                write!(f, "{name}[{}]", self.type_)
            },
        }
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
    As(u32),
    List(Url),
    Manual,
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