use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct CodexUsage {
    pub session_remaining_percent: Option<f32>,
    pub weekly_remaining_percent: Option<f32>,
    pub reset_at: Option<SystemTime>,
}

impl CodexUsage {
    pub const fn unavailable() -> Self {
        Self {
            session_remaining_percent: None,
            weekly_remaining_percent: None,
            reset_at: None,
        }
    }
}
