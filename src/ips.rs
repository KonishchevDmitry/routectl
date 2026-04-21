use std::collections::BTreeMap;
use std::fmt::Display;
use std::string::ToString;

use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use iprange::{IpNet as IpNetTrait, IpRange};
use itertools::Itertools;
use log::warn;

use crate::sources::{IpSourceRef, IpSources};

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

    pub fn add(&mut self, network: IpNet, source: IpSourceRef) {
        match network {
            IpNet::V4(network) => Self::add_inner(&mut self.v4, network, source),
            IpNet::V6(network) => Self::add_inner(&mut self.v6, network, source),
        }
    }

    fn add_inner<N: IpNetTrait>(networks: &mut BTreeMap<N, IpSources>, network: N, source: IpSourceRef) {
        networks.entry(network).or_default().add(source);
    }

    // XXX(konishchev): Drop?
    // pub fn iter(&self) -> btree_map::Iter<'_, IpNet, IpSource> {
    //     self.ips.iter()
    // }

    // XXX(konishchev): HERE
    pub fn filter(self, excludes: &Networks) {
        calculate(&self.v4, &excludes.v4);
        calculate(&self.v6, &excludes.v6);
    }
}

// XXX(konishchev): Drop?
// impl IntoIterator for Networks {
//     type Item = (IpNet, IpSource);
//     type IntoIter = btree_map::IntoIter<IpNet, IpSource>;

//     fn into_iter(self) -> Self::IntoIter {
//         self.ips.into_iter()
//     }
// }

// XXX(konishchev): Drop?
// impl<'a> IntoIterator for &'a Networks {
//     type Item = (&'a IpNet, &'a IpSource);
//     type IntoIter = btree_map::Iter<'a, IpNet, IpSource>;

//     fn into_iter(self) -> Self::IntoIter {
//         self.ips.iter()
//     }
// }

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