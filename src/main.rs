mod config;
mod generator;
mod ips;
mod resolving;
mod rules;
mod sources;

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use clap_verbosity_flag::{Verbosity, InfoLevel};
use easy_logging::LoggingConfig;
use log::{Level, error};

use config::Config;

#[derive(Parser)]
#[command(about, disable_help_subcommand = true)]
struct Args {
    #[arg(short, long, global = true, default_value = "/etc/routectl.yaml", help = "Config path")]
    config: PathBuf,

    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Generates the configuration")]
    Generate {},
}

fn main() -> ExitCode {
    let args = Args::parse();

    let log_level = args.verbose.log_level().unwrap_or(Level::Error);
    if let Err(err) = LoggingConfig::new(module_path!(), log_level).minimal().build() {
        let _ = writeln!(io::stderr(), "Failed to initialize the logging: {err}.");
        return ExitCode::FAILURE;
    }

    let Err(err) = run(&args) else {
        return ExitCode::SUCCESS;
    };

    // FIXME(konishchev): title
    let message = format!("{err:#}");

    if message.contains('\n') || message.ends_with('.') {
        error!("{message}");
    } else {
        error!("{message}.");
    }

    ExitCode::FAILURE
}

fn run(args: &Args) -> Result<()> {
    let config_path = &args.config;
    let config = Config::load(config_path).with_context(|| format!(
        "failed to load configuration file {config_path:?}"))?;

    match &args.command {
        Command::Generate {} => {
            generator::generate(&config)?;
        }
    }

    Ok(())
}