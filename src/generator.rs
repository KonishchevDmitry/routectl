use anyhow::Result;

use crate::ips::Networks;
use crate::rules::{Rule, Target};
use crate::sources::{IpSource, IpSourceRef};

pub fn generate(rules: &[Rule]) -> Result<()> {
    for rule in rules {
        process_rule(rule)?;
    }
    Ok(())
}

// XXX(konishchev): HERE
fn process_rule(rule: &Rule) -> Result<()> {
    let targets = resolve_targets(&rule.targets)?;
    let excludes = resolve_targets(&rule.exclude)?;

    targets.filter(&excludes);
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