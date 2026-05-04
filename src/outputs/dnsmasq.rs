use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Arguments, Write as _};
use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::ips::IpStack;
use crate::outputs::nftables;
use crate::rules::{self, Rule};
use crate::util;

#[derive(Deserialize, Serialize, Validate)]
pub struct DnsmasqConfig {
    #[serde(default)]
    domains: Vec<DomainSet>,
}

impl DnsmasqConfig {
    pub fn configure(&self, path: &Path, ip_stack: IpStack, rules: &HashMap<String, Rule>) -> Result<()> {
        util::write_config(path, |temp_path: &Path, file: &mut dyn Write| {
            for set in &self.domains {
                set.generate(ip_stack, rules, file)?;
            }
            file.flush()?;

            util::run(
                Command::new("dnsmasq")
                    .arg("--test")
                    .arg("-C").arg(temp_path)
            )
        })?;

        // FIXME(konishchev): Implement
        // if reload:
        //     sh.systemctl("try-restart", "--no-block", "dnsmasq.service", _defer=False)

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

impl DomainSet {
    fn generate(&self, ip_stack: IpStack, rules: &HashMap<String, Rule>, file: &mut dyn Write) -> Result<()> {
        let rule = rules::get(rules, &self.rules)?;

        if let Some(server) = self.server {
            generate_server_directive(rule, server, file)?;
        }

        if let Some(name) = self.nftset.as_ref() {
            generate_nftset_directive(name, rule, ip_stack, self.with_exclude, file)?;
        }

        Ok(())
    }
}

fn generate_server_directive(rule: &Rule, server: SocketAddr, file: &mut dyn Write) -> Result<()> {
    let server_spec = format!("{}#{}", server.ip(), server.port());
    let mut writer = ConfigWriter::new(file, "server=/", &server_spec);

    for domain in &rule.target_domains {
        let mut domain = Cow::Borrowed(domain);
        if domain.is_wildcard() {
            domain = Cow::Owned(domain.base_name());
        }
        writer.write(format_args!("{domain}/"))?;
    }

    writer.finish_line()
}

fn generate_nftset_directive(name: &str, rule: &Rule, ip_stack: IpStack, with_exclude: bool, file: &mut dyn Write) -> Result<()> {
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

        let mut writer = ConfigWriter::new(file, "nftset=/", &spec);

        for domain in domains {
            let mut domain = Cow::Borrowed(domain);
            if domain.is_wildcard() {
                domain = Cow::Owned(domain.base_name());
            }
            writer.write(format_args!("{domain}/"))?;
        }

        writer.finish_line()?;
    }

    Ok(())
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