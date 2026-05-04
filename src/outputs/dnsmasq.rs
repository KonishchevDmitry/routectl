use std::collections::{HashMap, hash_map::Entry};
use std::fmt::{Arguments, Write as _};
use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;

use anyhow::Result;
use log::info;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::ips::IpStack;
use crate::outputs::nftables;
use crate::rules::{self, Rule};
use crate::sources::Domain;
use crate::util;

#[derive(Deserialize, Serialize, Validate)]
pub struct DnsmasqConfig {
    #[serde(default)]
    domains: Vec<DomainSet>,
}

impl DnsmasqConfig {
    pub fn configure(&self, path: &Path, ip_stack: IpStack, rules: &HashMap<String, Rule>) -> Result<()> {
        let generate = |file: &mut dyn Write| {
            Dnsmasq::new().generate(self, ip_stack, rules, file)
        };

        let check = |temp_path: &Path| util::run(
            Command::new("dnsmasq")
                .arg("--test")
                .arg("-C").arg(temp_path)
        );

        let apply = || {
            let unit_name = "dnsmasq.service";
            info!("{path:?} has changed. Restarting {unit_name}...");
            util::run(Command::new("systemctl").args(["try-restart", "--no-block", unit_name]))
        };

        util::write_config(path, generate, check, apply)
    }
}


struct Dnsmasq<'a> {
    servers: HashMap<&'a Domain, SocketAddr>,
    nftsets: HashMap<&'a Domain, Rc<str>>,
}

impl<'a> Dnsmasq<'a> {
    fn new() -> Self {
        Dnsmasq {
            servers: HashMap::new(),
            nftsets: HashMap::new(),
        }
    }

    fn generate(mut self, config: &DnsmasqConfig, ip_stack: IpStack, rules: &'a HashMap<String, Rule>, file: &mut dyn Write) -> Result<()> {
        for domain_set in &config.domains {
            let rule = rules::get(rules, &domain_set.rules)?;

            if let Some(server) = domain_set.server {
                self.generate_server_directives(rule, server, file)?;
            }

            if let Some(name) = domain_set.nftset.as_ref() {
                self.generate_nftset_directives(rule, ip_stack, name, domain_set.with_exclude, file)?;
            }
        }

        Ok(())
    }

    fn generate_server_directives(&mut self, rule: &'a Rule, server: SocketAddr, file: &mut dyn Write) -> Result<()> {
        let server_spec = format!("{ip}#{port}", ip=server.ip(), port=server.port());
        let mut writer = ConfigWriter::new(file, "server=/", &server_spec);

        for domain in &rule.target_domains {
            match self.servers.entry(domain) {
                Entry::Vacant(entry) => {
                    // dnsmasq supports wildcard domain specification, so don't handle it specially
                    writer.write(format_args!("{domain}/"))?;
                    entry.insert(server);
                },
                Entry::Occupied(entry) => {
                    let existing = *entry.get();
                    if server != existing {
                        return Err!("conflicting upstream server configuration for {domain}: {existing} and {server}");
                    }
                },
            }
        }

        writer.finish_line()
    }

    fn generate_nftset_directives(&mut self, rule: &'a Rule, ip_stack: IpStack, name: &str, with_exclude: bool, file: &mut dyn Write) -> Result<()> {
        let sources = [
            (&rule.target_domains, "", true),
            (&rule.exclude_domains, "_exclude", with_exclude),
        ];

        for (domains, name_suffix, enabled) in sources {
            if !enabled {
                continue;
            }

            let mut spec = String::new();

            for ip_version in ip_stack {
                if !spec.is_empty() {
                    write!(&mut spec, ",")?;
                }
                write!(
                    &mut spec, "{version}#inet#mangle#{name}{name_suffix}_dns_ipv{version}",
                    version=ip_version.version(),
                )?;
            }

            let spec: Rc<str> = Rc::from(spec);
            let mut writer = ConfigWriter::new(file, "nftset=/", &spec);

            for domain in domains {
                match self.nftsets.entry(domain) {
                    Entry::Vacant(entry) => {
                        // dnsmasq supports wildcard domain specification, so don't handle it specially
                        writer.write(format_args!("{domain}/"))?;
                        entry.insert(spec.clone());
                    },
                    Entry::Occupied(entry) => {
                        let existing = entry.get();
                        if spec != *existing {
                            return Err!("conflicting nftset configuration for {domain}: {existing} and {spec}");
                        }
                    },
                }
            }

            writer.finish_line()?;
        }

        Ok(())
    }
}

#[derive(Deserialize, Serialize, Validate)]
struct DomainSet {
    #[validate(length(min = 1))]
    rules: Vec<String>,

    #[serde(default)]
    server: Option<SocketAddr>,

    #[validate(custom(function = "nftables::validate_set_name"))]
    #[serde(default)]
    nftset: Option<String>,

    #[serde(default)]
    with_exclude: bool,
}

struct ConfigWriter<'a> {
    file: &'a mut dyn Write,

    directive_prefix: &'a str,
    directive_suffix: &'a str,

    buf: Vec<u8>,
    current_line_len: Option<usize>,
}

impl<'a> ConfigWriter<'a> {
    // dnsmasq fails to load configuration file which contains a line longer than 1024 characters (excluding '\n')
    const MAX_LINE_LENGTH: usize = 1024;

    fn new(file: &'a mut dyn Write, directive_prefix: &'a str, directive_suffix: &'a str) -> Self {
        ConfigWriter {
            file,

            directive_prefix,
            directive_suffix,

            buf: Vec::new(),
            current_line_len: None,
        }
    }

    fn write(&mut self, args: Arguments) -> Result<()> {
        self.buf.truncate(0);
        self.buf.write_fmt(args)?;

        if let Some(current_line_len) = self.current_line_len &&
            current_line_len + self.buf.len() + self.directive_suffix.len() > Self::MAX_LINE_LENGTH {
            self.finish_line()?;
        }

        let current_line_len = match self.current_line_len.as_mut() {
            Some(current_line_len) => current_line_len,
            None => {
                let directive_prefix = self.directive_prefix.as_bytes();
                self.file.write_all(directive_prefix)?;
                self.current_line_len.insert(directive_prefix.len())
            },
        };

        self.file.write_all(&self.buf)?;
        *current_line_len += self.buf.len();

        Ok(())
    }

    fn finish_line(&mut self) -> Result<()> {
        if self.current_line_len.take().is_some() {
            self.file.write_all(self.directive_suffix.as_bytes())?;
            self.file.write_all(b"\n")?;
        }
        Ok(())
    }
}