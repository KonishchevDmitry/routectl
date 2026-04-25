use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::net::IpAddr;

use anyhow::{Result, anyhow};
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use iprange::{IpNet as IpNetTrait, IpRange};
use itertools::Itertools;
use log::warn;

use crate::sources::{IpSource, IpSources, IpSourceType, IpSourceList, IpSourceListRef};

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
    // We might pack IPv4 into IPv6 using IPv4-mapped addresses and don't manage these split IP sets, but not sure that
    // it's good idea in terms of code readability.
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

    pub fn filter(self, context: &str, excludes: &Networks) -> Networks {
        Networks {
            v4: filter_networks(context, &self.v4, &excludes.v4),
            v6: filter_networks(context, &self.v6, &excludes.v6),
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
    } else {
        None
    }
}

pub fn reserved_networks() -> Result<Networks> {
    let mut networks = Networks::new();
    let source_list = IpSourceListRef::new(IpSourceList::Special("reserved"));

    for network in RESERVED_NETWORKS {
        let network = parse_network(network).ok_or_else(|| anyhow!(
            "invalid network: {network:?}"))?;

        let source = IpSource::new(IpSourceType::Network(network), source_list.clone());
        networks.add(network, source);
    }

    Ok(networks)
}

pub fn filter(context: &str, network: IpNet, source: &IpSource, excludes: &Networks) -> impl Iterator<Item=IpNet> {
    match network {
        IpNet::V4(network) => filter_network(context, network, source, &excludes.v4)
            .into_iter().map(IpNet::from).collect_vec().into_iter(),

        IpNet::V6(network) => filter_network(context, network, source, &excludes.v6)
            .into_iter().map(IpNet::from).collect_vec().into_iter(),
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

fn filter_networks<Network>(
    context: &str, networks: &BTreeMap<Network, IpSources>, excludes: &BTreeMap<Network, IpSources>,
) -> BTreeMap<Network, IpSources>
    where Network: IpNetTrait + Into<IpNet>
{
    let mut result: BTreeMap<Network, IpSources> = BTreeMap::new();

    for (&network, sources) in networks {
        for filtered_network in &filter_network(context, network, sources, excludes) {
            result.entry(filtered_network).or_default().extend(sources);
        }
    }

    result
}

fn filter_network<Network, NetworkSource>(
    context: &str, network: Network, source: NetworkSource, excludes: &BTreeMap<Network, IpSources>,
) -> IpRange<Network>
    where
        Network: IpNetTrait + Into<IpNet>,
        NetworkSource: Display,
{
    let mut range = IpRange::new();
    range.add(network);

    for (&exclude_network, exclude_sources) in excludes {
        let mut exclude_range = IpRange::new();
        exclude_range.add(exclude_network);

        let intersection = range.intersect(&exclude_range);
        if intersection.is_empty() {
            continue;
        }

        let human_network = HumanNetwork::from(network);
        let mut intersection = intersection.iter().peekable();

        if intersection.next_if_eq(&network).is_some() {
            assert_eq!(intersection.next(), None);
            warn!("[{context}] Excluding {human_network} (source: {source}; exclude: {exclude_sources}).");
        } else {
            warn!(
                "[{context}] Excluding {} from {human_network} (source: {source}; exclude: {exclude_sources}).",
                intersection.map(HumanNetwork::from).join(", "),
            );
        }

        range.remove(exclude_network);
    }

    range
}