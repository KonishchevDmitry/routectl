use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use log::debug;

pub fn format_duration(mut duration: Duration) -> String {
    if duration >= Duration::from_secs(10) {
        duration = Duration::from_secs(duration.as_secs());
    } else if duration >= Duration::from_millis(10) {
        duration = Duration::from_millis(duration.as_millis() as u64);
    }
    format!("{duration:?}")
}

pub fn format_error(err: &anyhow::Error) -> String {
    let message = format!("{err:#}");
    let add_dot = !message.ends_with('.') && !message.contains('\n');

    let mut buf = if let mut chars = message.chars() && let Some(first) = chars.next() && first.is_lowercase() {
        let mut buf = String::with_capacity(message.len() + add_dot as usize);
        buf.extend(first.to_uppercase());
        buf.extend(chars);
        buf
    } else {
        message
    };

    if add_dot {
        buf.push('.');
    }

    buf
}

pub fn format_multiline(text: &str) -> String {
    let text = text.trim_end();

    if text.find('\n').is_some() {
        format!("\n{text}")
    } else {
        format!(" {text}")
    }
}

pub fn run(mut command: Command) -> Result<()> {
    debug!("Running `{command:?}`...");

    let result = command.output().with_context(|| format!(
        "failed to execute `{command:?}`"))?;

    let status = result.status;
    let stderr = String::from_utf8_lossy(&result.stderr);

    if !status.success() {
        return Err!(
            "`{command:?}` returned an error ({status}):{}",
            format_multiline(&stderr));
    } else if !stderr.is_empty() {
        debug!("`{command:?}` stderr:{}", format_multiline(&stderr));
    }

    Ok(())
}