// The following nftables limitations must be taken into account:
// * You can't do a positive matching against multiple sets in a single rule: `ip daddr {@set1, @set2}` is not supported
//   and nftables has no `or` statement. So you have to either manually combine several sets into a one huge set, or
//   match them via multiple rules.
// * All networks in IP set mustn't intersect with each other, so networks must be compacted before IP set generation.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use anyhow::Result;
use dedent::dedent;
use log::info;
use regex::Regex;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::ips::{HumanNetwork, IpStack, IpVersion};
use crate::rules::{self, Rule};
use crate::util;

#[derive(Deserialize, Serialize, Validate)]
pub struct NftablesConfig {
    #[validate(custom(function = "NftablesIpSet::validate"), nested)]
    #[serde(default)]
    sets: BTreeMap<String, NftablesIpSet>,
}

impl NftablesConfig {
    pub fn configure(&self, path: &Path, ip_stack: IpStack, rules: &HashMap<String, Rule>) -> Result<()> {
        let generate = |file: &mut dyn Write| {
            for (name, set) in &self.sets {
                set.generate(name, ip_stack, rules, file)?;
            }
            Ok(())
        };

        let check = |temp_path: &Path| util::run(
            Command::new("nft")
                .arg("--check")
                .arg("--file").arg(temp_path)
        );

        let apply = || {
            info!("{path:?} has changed. Applying nftables rules...");
            util::run(Command::new("nft").arg("--file").arg(path))
        };

        util::write_config(path, generate, check, apply)
    }
}

#[derive(Deserialize, Serialize, Validate)]
struct NftablesIpSet {
    #[validate(length(min = 1))]
    rules: Vec<String>,

    #[serde(default)]
    with_exclude: bool,
}

impl NftablesIpSet {
    fn validate(sets: &BTreeMap<String, NftablesIpSet>) -> Result<(), ValidationError> {
        for name in sets.keys() {
            validate_set_name(name)?;
        }
        Ok(())
    }

    fn generate(&self, name: &str, ip_stack: IpStack, rules: &HashMap<String, Rule>, file: &mut dyn Write) -> Result<()> {
        let rule = rules::get(rules, &self.rules)?;

        let sources = [
            (&rule.target_networks, "", true),
            (&rule.exclude_networks, "_exclude", self.with_exclude),
        ];

        let mut first_set = true;

        for (networks, name_suffix, enabled) in sources {
            if !enabled {
                continue;
            }

            for ip_version in ip_stack {
                let nft_name = format!("{name}{name_suffix}_ipv{version}", version=ip_version.version());

                let (nft_type, max_width, networks) = match ip_version {
                    IpVersion::V4 => ("ip",  18, networks.iter_compacted(ip_version)),
                    IpVersion::V6 => ("ip6", 43, networks.iter_compacted(ip_version)),
                };

                if first_set {
                    first_set = false;
                } else {
                    writeln!(file)?;
                }

                writeln!(file, dedent!(r#"
                    table inet mangle {{
                        set {name} {{
                            typeof {type} daddr
                            flags interval
                        }}
                    }}
                    flush set inet mangle {name}
                "#), name=nft_name, r#type=nft_type)?;

                let mut empty = true;

                for (network, sources) in networks {
                    if empty {
                        writeln!(file, "add element inet mangle {nft_name} {{")?;
                        empty = false;
                    }

                    // Width formatting works only with strings, so do the stringification explicitly
                    let nft_network = HumanNetwork(network).to_string();
                    writeln!(file, "{nft_network:>max_width$}, # {sources}")?;
                }

                if !empty {
                    writeln!(file, "}}")?;
                }
            }
        }

        Ok(())
    }
}

pub fn validate_set_name(name: &str) -> Result<(), ValidationError> {
    static NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
        r"^[a-z]+(?:_[a-z]+)*$").unwrap());

    if !NAME_RE.is_match(name) {
        return Err(ValidationError::new("invalid IP set name").with_message(format!(
            "invalid IP set name: {name:?} (must match `{}`)", NAME_RE.as_str()).into()));
    }

    Ok(())
}