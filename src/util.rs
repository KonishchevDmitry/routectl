use std::time::Duration;

pub fn format_duration(mut duration: Duration) -> String {
    if duration >= Duration::from_secs(10) {
        duration = Duration::from_secs(duration.as_secs());
    } else if duration >= Duration::from_millis(10) {
        duration = Duration::from_millis(duration.as_millis() as u64);
    }
    format!("{duration:?}")
}