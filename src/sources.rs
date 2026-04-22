use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

use ipnet::IpNet;
use url::Url;

use crate::ips::HumanNetwork;

#[derive(Clone)]
pub struct IpSource {
    type_: IpSourceType,
    list: Option<ListIpSourceRef>,
}

impl IpSource {
    pub fn new(type_: IpSourceType, list: Option<ListIpSourceRef>) -> IpSource {
        IpSource { type_, list }
    }
}

impl Display for IpSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(list) = self.list.as_ref() {
            write!(f, "{}#", list.url)?;
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
            &IpSourceType::Network(network) => write!(f, "{}", HumanNetwork(network))
        }
    }
}

pub struct ListIpSource {
    url: Url,
}

pub type ListIpSourceRef = Arc<ListIpSource>;

impl ListIpSource {
    pub fn new(url: &Url) -> ListIpSourceRef {
        ListIpSourceRef::new(ListIpSource {
            url: url.to_owned(),
        })
    }
}

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