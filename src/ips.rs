use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::net::IpAddr;

use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use iprange::{IpNet as IpNetTrait, IpRange};
use itertools::Itertools;
use log::warn;

use crate::sources::{IpSource, IpSources};

// https://en.wikipedia.org/wiki/List_of_reserved_IP_addresses
static RESERVED_NETWORKS: &'static [&'static str] = &[
    "0.0.0.0/8",       // [Software] This host on this network
    "10.0.0.0/8",      // [Private network] Used for local communications within a private network
    "100.64.0.0/10",   // [Private network] Shared address space for communications between a service provider and its subscribers when using a carrier-grade NAT
    "127.0.0.0/8",     // [Host] Loopback addresses
    "169.254.0.0/16",  // [Link] Used for link-local addresses between two hosts on a single link when no IP address is otherwise specified
    "172.16.0.0/12",   // [Private network] Used for local communications within a private network
    "192.0.0.0/24",    // IETF Protocol Assignments
    "192.0.2.0/24",    // [Documentation] Assigned as TEST-NET-1, documentation and examples
    "192.88.99.0/24",  // Formerly used for IPv6 to IPv4 relay
    "192.168.0.0/16",  // [Private network] Used for local communications within a private network
    "198.18.0.0/15",   // [Private network] Used for benchmark testing of inter-network communications between two separate subnets
    "198.51.100.0/24", // [Documentation] Assigned as TEST-NET-2, documentation and examples
    "203.0.113.0/24",  // [Documentation] Assigned as TEST-NET-3, documentation and examples
    "224.0.0.0/4",     // Multicast
    "240.0.0.0/4",     // Reserved for future use
    "255.255.255.255", // Limited Broadcast

    "::",             // [Software] Unspecified address
    "::1",            // [Host] Loopback address
    "::ffff:0:0/96",  // [Software] IPv4-mapped addresses
    "64:ff9b::/96",   // NAT64
    "64:ff9b:1::/48", // NAT64
    "100::/64",       // [Routing] Discard prefix
    "2001::/32",      // Teredo tunneling (non-static IP)
    "2001:20::/28",   // [Software] ORCHIDv2
    "2001:db8::/32",  // [Documentation] Addresses used in documentation and example source code
    "2002::/16",      // Deprecated 6to4 addressing scheme
    "3fff::/20",      // [Documentation] Addresses used in documentation and example source code
    "5f00::/16",      // [Routing] IPv6 Segment Routing
    "fc00::/7",       // [Private network] Unique local addresses
    "fe80::/64",      // [Link] Link-local addresses
    "ff00::/8",       // Multicast
];

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

impl<N: Into<IpNet>> From<N> for HumanNetwork {
    fn from(network: N) -> Self {
        HumanNetwork(network.into())
    }
}

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
    where N: IpNetTrait + Into<IpNet>
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
                "Excluding {} (source: {exclude_sources}) from {} (source: {sources}).",
                intersection.iter().map(HumanNetwork::from).join(", "), HumanNetwork::from(network)
            );

            range.remove(exclude_network);
        }

        for network in range.iter() {
            result.entry(network).or_default().extend(sources);
        }
    }

    result
}