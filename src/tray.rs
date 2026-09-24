use std::time::{Duration, SystemTime};

pub const ICON_ID: u32 = 1;
pub const MENU_REFRESH: usize = 1001;
pub const MENU_QUIT: usize = 1002;

const GREEN: [u8; 3] = [0x63, 0x99, 0x22];
const AMBER: [u8; 3] = [0xba, 0x75, 0x17];
const RED: [u8; 3] = [0xd8, 0x5a, 0x30];

pub fn format_reset_duration(reset_at: SystemTime, now: SystemTime) -> String {
    match reset_at.duration_since(now) {
        Ok(remaining) => format_duration(remaining),
        Err(_) => "soon".to_owned(),
    }
}

fn format_duration(duration: Duration) -> String {
    let minutes = duration.as_secs().div_ceil(60);
    let days = minutes / (24 * 60);
    let hours = minutes / 60;
    let minutes = minutes % 60;

    if days > 0 {
        format!("{days}d {}h", hours % 24)
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

pub fn rounded_percent(value: f32) -> u8 {
    if value.is_finite() {
        value.clamp(0.0, 100.0).round() as u8
    } else {
        0
    }
}

pub const fn usage_color(percent: u8) -> [u8; 3] {
    if percent < 50 {
        GREEN
    } else if percent < 80 {
        AMBER
    } else {
        RED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_reset_duration() {
        assert_eq!(format_duration(Duration::from_secs(7_260)), "2h 1m");
        assert_eq!(format_duration(Duration::from_secs(93_600)), "1d 2h");
    }

    #[test]
    fn selects_metric_colors_at_thresholds() {
        assert_eq!(usage_color(49), GREEN);
        assert_eq!(usage_color(50), AMBER);
        assert_eq!(usage_color(79), AMBER);
        assert_eq!(usage_color(80), RED);
    }
}
