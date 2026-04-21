use std::fmt::Write;

use anyhow::Result;
use log::{Level, log_enabled, debug};

use crate::ips::Networks;
use crate::rules::{Rule, Target};
use crate::sources::{IpSource, IpSourceRef};

pub fn generate(rules: &[Rule]) -> Result<()> {
    for rule in rules {
        process_rule(rule)?;
    }
    Ok(())
}

fn process_rule(rule: &Rule) -> Result<()> {
    let targets = resolve_targets(&rule.targets)?;
    let excludes = resolve_targets(&rule.exclude)?;

    let result = targets.filter(&excludes);

    if log_enabled!(Level::Debug) {
        let mut buf = String::new();

        write!(&mut buf, "Got the following networks:").unwrap();
        for (network, sources) in &result {
            write!(&mut buf, "\n* {network} (source: {sources})").unwrap();
        }

        debug!("{buf}");
    }

    Ok(())
}

fn resolve_targets(targets: &[Target]) -> Result<Networks> {
    let mut networks = Networks::new();

    for target in targets {
        match target {
            &Target::Network(network) => {
                let source = IpSourceRef::new(IpSource::Network(network));
                networks.add(network, source);
            },
        }
    }

    Ok(networks)
}