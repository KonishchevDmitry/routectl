use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::net::IpAddr;
use std::string::ToString;

use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use iprange::{IpNet as IpNetTrait, IpRange};
use itertools::Itertools;
use log::warn;

use crate::sources::{IpSource, IpSources};

pub struct Networks {
    v4: BTreeMap<Ipv4Net, IpSources>,
    v6: BTreeMap<Ipv6Net, IpSources>,
}

impl Networks {
    pub fn new() -> Self {
        Self {
            v4: BTreeMap::new(),
            v6: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, network: IpNet, source: IpSource) {
        match network {
            IpNet::V4(network) => Self::add_inner(&mut self.v4, network, source),
            IpNet::V6(network) => Self::add_inner(&mut self.v6, network, source),
        }
    }

    fn add_inner<N: IpNetTrait>(networks: &mut BTreeMap<N, IpSources>, network: N, source: IpSource) {
        networks.entry(network).or_default().add(source);
    }

    pub fn filter(self, excludes: &Networks) -> Networks {
        Networks {
            v4: calculate(&self.v4, &excludes.v4),
            v6: calculate(&self.v6, &excludes.v6),
        }
    }
}

impl<'a> IntoIterator for &'a Networks {
    type Item = (IpNet, &'a IpSources);
    type IntoIter = Box<dyn Iterator<Item = Self::Item> + 'a>;

    fn into_iter(self) -> Self::IntoIter {
        let v4 = self.v4.iter().map(|(&network, sources)| (IpNet::V4(network), sources));
        let v6 = self.v6.iter().map(|(&network, sources)| (IpNet::V6(network), sources));
        Box::new(v4.chain(v6))
    }
}

pub fn parse_network(network: &str) -> Option<IpNet> {
    if let Ok(network) = network.parse() {
        Some(network)
    } else if let Ok(address) = network.parse::<IpAddr>() {
        Some(address.into())
    } else{
        None
    }
}

pub struct HumanNetwork(pub IpNet);

impl Display for HumanNetwork {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.0.prefix_len() == self.0.max_prefix_len() {
            write!(f, "{}", self.0.addr())
        } else {
            write!(f, "{}", self.0)
        }
    }
}

fn calculate<N>(networks: &BTreeMap<N, IpSources>, excludes: &BTreeMap<N, IpSources>) -> BTreeMap<N, IpSources>
    where N: IpNetTrait + Display
{
    let mut result: BTreeMap<N, IpSources> = BTreeMap::new();

    for (&network, sources) in networks {
        let mut range = IpRange::new();
        range.add(network);

        for (&exclude_network, exclude_sources) in excludes {
            let mut exclude_range = IpRange::new();
            exclude_range.add(exclude_network);

            let intersection = range.intersect(&exclude_range);
            if intersection.is_empty() {
                continue;
            }

            warn!(
                "Excluding {} (source: {exclude_sources}) from {network} (source: {sources}).",
                // FIXME(konishchev): HumanNetwork
                intersection.iter().map(|network| network.to_string()).join(", "),
            );

            range.remove(exclude_network);
        }

        for network in range.iter() {
            result.entry(network).or_default().extend(sources);
        }
    }

    result
}