use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use dedent::dedent;
use log::debug;
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
        let mut temp_path = path.to_owned();
        if !temp_path.add_extension("new") {
            return Err!("invalid output file path");
        }

        debug!("Writing {temp_path:?}...");

        let mut file = BufWriter::new(OpenOptions::new()
            .create(true)
            .mode(0o644)
            .write(true)
            .truncate(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temp_path)?);

        for (name, set) in &self.sets {
            set.generate(&name, ip_stack, rules, &mut file)?;
        }

        file.flush()?;
        nft(&temp_path, true)?;

        fs::rename(&temp_path, path).with_context(|| format!(
            "rename {temp_path:?} to {path:?}"))?;
        debug!("Wrote {path:?}.");

        nft(path, false)
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
        static NAME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(
            r"^[a-z]+(?:_[a-z]+)*$").unwrap());

        for name in sets.keys() {
            if !NAME_RE.is_match(name) {
                return Err(ValidationError::new("invalid IP set name").with_message(format!(
                    "invalid IP set name: {name:?} (must match `{}`)", NAME_RE.as_str()).into()));
            }
        }

        Ok(())
    }

    fn generate(&self, name: &str, ip_stack: IpStack, rules: &HashMap<String, Rule>, file: &mut BufWriter<File>) -> Result<()> {
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
                let nft_name = format!("{name}{name_suffix}_ip{version}", version=ip_version.version());

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

fn nft(path: &Path, check: bool) -> Result<()> {
    let mut command = Command::new("nft");
    if check {
        command.arg("--check");
    }
    command.arg("--file").arg(path);

    debug!("Running `{command:?}`...");

    let result = command.output().with_context(|| format!(
        "failed to execute `{command:?}`"))?;

    let status = result.status;
    let stderr = String::from_utf8_lossy(&result.stderr);

    if !status.success() {
        return Err!(
            "`{command:?}` returned an error ({status}):{}",
            util::format_multiline(&stderr));
    } else if !stderr.is_empty() {
        debug!("`{command:?}` stderr:{}", util::format_multiline(&stderr));
    }

    Ok(())
}