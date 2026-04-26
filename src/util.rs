use std::time::Duration;

pub fn format_duration(mut duration: Duration) -> String {
    if duration >= Duration::from_secs(10) {
        duration = Duration::from_secs(duration.as_secs());
    } else if duration >= Duration::from_millis(10) {
        duration = Duration::from_millis(duration.as_millis() as u64);
    }
    format!("{duration:?}")
}

pub fn format_multiline(text: &str) -> String {
    let text = text.trim_end();

    if text.find('\n').is_some() {
        format!("\n{text}")
    } else {
        format!(" {text}")
    }
}