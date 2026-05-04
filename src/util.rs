use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
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

pub fn write_config<G, C, A>(path: &Path, generate: G, check: C, apply: A) -> Result<()>
    where
        G: Fn(&mut dyn Write) -> Result<()>,
        C: Fn(&Path) -> Result<()>,
        A: Fn() -> Result<()>,
{
    let mut data: Vec<u8> = Vec::new();
    generate(&mut data)?;

    let up_to_date = match fs::read(path) {
        Ok(current) => data == current,
        Err(err) if err.kind() == ErrorKind::NotFound => false,
        Err(err) => return Err(err.into()),
    };

    if up_to_date {
        debug!("Don't write {path:?} - it's already up-to-date.");
        return Ok(());
    }

    let mut temp_path = path.to_owned();
    if !temp_path.add_extension("new") {
        return Err!("invalid output file path");
    }

    debug!("Writing {temp_path:?}...");

    let mut file = OpenOptions::new()
        .create(true)
        .mode(0o644)
        .write(true)
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temp_path)?;

    file.write_all(&data)?;
    file.flush()?;
    check(&temp_path)?;

    fs::rename(&temp_path, path).with_context(|| format!(
        "rename {temp_path:?} to {path:?}"))?;
    debug!("Wrote {path:?}.");

    apply().inspect_err(|_| {
        // Modify the file contents to trigger apply on next run
        let _ = file.write_all(b"\n");
    })
}

pub fn run(command: &mut Command) -> Result<()> {
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