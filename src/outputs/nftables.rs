use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use anyhow::Result;
use dedent::dedent;
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
        util::write_config(path, |temp_path: &Path, file: &mut dyn Write| {
            for (name, set) in &self.sets {
                set.generate(&name, ip_stack, rules, file)?;
            }
            file.flush()?;

            util::run(
                Command::new("nft")
                    .arg("--check")
                    .arg("--file").arg(temp_path)
            )
        })?;

        util::run(Command::new("nft").arg("--file").arg(path))
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
                    IpVersion::V4 => ("ip",  18, networks.iter(ip_version)),
                    IpVersion::V6 => ("ip6", 43, networks.iter(ip_version)),
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