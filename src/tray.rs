use std::time::{Duration, SystemTime};

use crate::usage::CodexUsage;

pub const ICON_ID: u32 = 1;
pub const MENU_REFRESH: usize = 1001;
pub const MENU_QUIT: usize = 1002;

pub fn tooltip(usage: &CodexUsage) -> String {
    let mut text = match (
        usage.session_remaining_percent,
        usage.weekly_remaining_percent,
    ) {
        (Some(session), None) => {
            format!("Codex: {}% remaining", rounded_percent(session))
        }
        (Some(session), Some(weekly)) => format!(
            "Codex: {}% session, {}% weekly remaining",
            rounded_percent(session),
            rounded_percent(weekly)
        ),
        (None, Some(weekly)) => {
            format!("Codex: {}% weekly remaining", rounded_percent(weekly))
        }
        (None, None) => "Codex usage unavailable".to_owned(),
    };

    if let Some(reset_at) = usage.reset_at {
        text.push_str("; session resets ");
        text.push_str(&format_reset(reset_at));
    }

    text
}

fn format_reset(reset_at: SystemTime) -> String {
    match reset_at.duration_since(SystemTime::now()) {
        Ok(remaining) => format!("in {}", format_duration(remaining)),
        Err(_) => "soon".to_owned(),
    }
}

fn format_duration(duration: Duration) -> String {
    let minutes = duration.as_secs().div_ceil(60);
    let hours = minutes / 60;
    let minutes = minutes % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn rounded_percent(value: f32) -> u8 {
    if value.is_finite() {
        value.clamp(0.0, 100.0).round() as u8
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_session_percentage() {
        let usage = CodexUsage {
            session_remaining_percent: Some(73.0),
            weekly_remaining_percent: Some(86.0),
            reset_at: None,
        };

        assert_eq!(tooltip(&usage), "Codex: 73% session, 86% weekly remaining");
    }

    #[test]
    fn formats_reset_duration() {
        assert_eq!(format_duration(Duration::from_secs(7_260)), "2h 1m");
    }
}
