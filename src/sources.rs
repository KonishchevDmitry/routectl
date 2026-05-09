use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};
use std::slice;
use std::sync::Arc;

use ipnet::IpNet;
use url::Url;

use crate::ips::HumanNetwork;
use crate::resolving::AS_PREFIX;

pub use hickory_resolver::proto::rr::Name as Domain;

#[derive(Clone, PartialEq)]
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

#[derive(Clone, PartialEq)]
pub enum IpSourceType {
    Domain(Arc<Domain>),
    Network(IpNet),
}

impl Display for IpSourceType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            IpSourceType::Domain(domain) => write!(f, "{domain}"),
            &IpSourceType::Network(network) => write!(f, "{}", HumanNetwork(network)),
        }
    }
}

#[derive(PartialEq)]
pub enum IpSourceList {
    As(u32),
    List(Url),
    Manual,
    Special(&'static str),
}

pub type IpSourceListRef = Arc<IpSourceList>;

#[derive(Default, Clone)]
pub struct IpSources {
    sources: Vec<IpSource>,
}

impl IpSources {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, source: IpSource) {
        self.sources.push(source);
    }

    pub fn extend<'a, S>(&mut self, other: S)
        where S: IntoIterator<Item = &'a IpSource>
    {
        for source in other {
            if !self.sources.contains(source) {
                self.sources.push(source.clone());
            }
        }
    }
}

impl<'a> IntoIterator for &'a IpSources {
    type Item = &'a IpSource;
    type IntoIter = slice::Iter<'a, IpSource>;

    fn into_iter(self) -> Self::IntoIter {
        self.sources.iter()
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

pub fn parse_domain(domain: &str) -> Option<Domain> {
    let mut domain: Domain = domain.parse().ok()?;

    let mut parent = domain.base_name();
    if parent.is_root() {
        return None;
    }

    loop {
        if parent.is_wildcard() {
            return None;
        }

        parent = parent.base_name();
        if parent.is_root() {
            break;
        }
    }

    domain.set_fqdn(false);
    Some(domain)
}

pub fn trim_wildcard(domain: &Domain) -> Cow<'_, Domain> {
    if domain.is_wildcard() {
        let mut domain = domain.base_name();
        domain.set_fqdn(false); // A side effect of base_name()
        Cow::Owned(domain)
    } else {
        Cow::Borrowed(domain)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use super::*;

    #[rstest(domain, expected,
        case("a",  None),

        case("b.a",  Some("b.a")),
        case("B.A",  Some("b.a")),
        case("р.ф",  Some("р.ф")),
        case("Р.Ф",  Some("р.ф")),
        case("b.a.",  Some("b.a")),

        case("*",  None),
        case("*.a",  Some("*.a")),
        case("a.*",  None),
        case("*.b.a",  Some("*.b.a")),
        case("b.*.a",  None),
        case("*.b.*.a",  None),
        case("*.*.b.a",  None),
    )]
    fn domain_parsing(domain: &str, expected: Option<&str>) {
        let result = parse_domain(domain).map(|domain| domain.to_string());
        let expected = expected.map(ToOwned::to_owned);
        assert_eq!(result, expected, "{domain}");
    }

    #[rstest(domain, expected,
        case("c.b.a",  "c.b.a"),
        case("*.b.a",    "b.a"),
    )]
    fn wildcard_trimming(domain: &str, expected: &str) {
        let domain = parse_domain(domain).unwrap();
        let result = trim_wildcard(&domain).to_string();
        assert_eq!(result, expected, "{domain}");
    }
}