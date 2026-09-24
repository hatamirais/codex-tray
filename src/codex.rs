use std::{
    ffi::OsStr,
    fmt,
    io::{self, BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant, UNIX_EPOCH},
};

use serde_json::{Value, json};

use crate::usage::CodexUsage;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn load_usage() -> CodexUsage {
    query_usage().unwrap_or_else(|_| CodexUsage::unavailable())
}

fn query_usage() -> Result<CodexUsage, CodexError> {
    let mut server = AppServer::start()?;

    server.send(&json!({
        "id": 1,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "codex-tray",
                "title": "Codex Tray",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    }))?;
    server.response(1)?;

    server.send(&json!({
        "id": 2,
        "method": "account/rateLimits/read",
        "params": null
    }))?;

    parse_usage(&server.response(2)?)
}

fn parse_usage(response: &Value) -> Result<CodexUsage, CodexError> {
    let result = response
        .get("result")
        .ok_or_else(|| CodexError::new("Codex response did not contain a result"))?;

    let snapshot = result
        .get("rateLimitsByLimitId")
        .and_then(|limits| limits.get("codex"))
        .or_else(|| result.get("rateLimits"))
        .ok_or_else(|| CodexError::new("Codex response did not contain rate limits"))?;

    let mut windows = [snapshot.get("primary"), snapshot.get("secondary")]
        .into_iter()
        .flatten()
        .filter_map(parse_window)
        .collect::<Vec<_>>();
    windows.sort_by_key(|window| window.duration_minutes);

    let session = windows
        .iter()
        .find(|window| window.duration_minutes <= 24 * 60);
    let weekly = windows
        .iter()
        .rev()
        .find(|window| window.duration_minutes > 24 * 60);

    if session.is_none() && weekly.is_none() {
        return Err(CodexError::new(
            "Codex response did not contain recognized rate-limit windows",
        ));
    }

    Ok(CodexUsage {
        session_remaining_percent: session.map(|window| window.remaining_percent),
        weekly_remaining_percent: weekly.map(|window| window.remaining_percent),
        reset_at: session
            .and_then(|window| window.resets_at)
            .or_else(|| weekly.and_then(|window| window.resets_at)),
    })
}

fn parse_window(value: &Value) -> Option<RateLimitWindow> {
    let used_percent = value.get("usedPercent")?.as_f64()? as f32;
    let duration_minutes = value.get("windowDurationMins")?.as_u64()?;
    let resets_at = value
        .get("resetsAt")
        .and_then(Value::as_u64)
        .and_then(|seconds| UNIX_EPOCH.checked_add(Duration::from_secs(seconds)));

    Some(RateLimitWindow {
        remaining_percent: (100.0 - used_percent).clamp(0.0, 100.0),
        duration_minutes,
        resets_at,
    })
}

struct RateLimitWindow {
    remaining_percent: f32,
    duration_minutes: u64,
    resets_at: Option<std::time::SystemTime>,
}

struct AppServer {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<io::Result<String>>,
    reader: Option<JoinHandle<()>>,
}

impl AppServer {
    fn start() -> Result<Self, CodexError> {
        match Self::spawn("codex.exe") {
            Ok(server) => Ok(server),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let local_app_data = std::env::var_os("LOCALAPPDATA")
                    .ok_or_else(|| CodexError::new("LOCALAPPDATA is not set"))?;
                let executable =
                    PathBuf::from(local_app_data).join("Programs/OpenAI/Codex/bin/codex.exe");
                Self::spawn(&executable).map_err(CodexError::from)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn spawn(executable: impl AsRef<OsStr>) -> io::Result<Self> {
        let mut child = Command::new(executable)
            .args(["app-server", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("failed to open Codex stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("failed to open Codex stdout"))?;
        let (sender, lines) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });

        Ok(Self {
            child,
            stdin,
            lines,
            reader: Some(reader),
        })
    }

    fn send(&mut self, request: &Value) -> Result<(), CodexError> {
        serde_json::to_writer(&mut self.stdin, request)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        Ok(())
    }

    fn response(&self, request_id: i64) -> Result<Value, CodexError> {
        let deadline = Instant::now() + RESPONSE_TIMEOUT;
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| CodexError::new("Codex response timed out"))?;
            let line = self
                .lines
                .recv_timeout(remaining)
                .map_err(|_| CodexError::new("Codex response timed out"))??;
            let response: Value = serde_json::from_str(&line)?;

            if response.get("id").and_then(Value::as_i64) != Some(request_id) {
                continue;
            }
            if let Some(error) = response.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex returned an unknown error");
                return Err(CodexError::new(message));
            }
            return Ok(response);
        }
    }
}

impl Drop for AppServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[derive(Debug)]
struct CodexError(String);

impl CodexError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for CodexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CodexError {}

impl From<io::Error> for CodexError {
    fn from(error: io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for CodexError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_five_hour_and_weekly_windows() {
        let response = json!({
            "result": {
                "rateLimits": {
                    "primary": {
                        "usedPercent": 18,
                        "windowDurationMins": 300,
                        "resetsAt": 1_790_234_229_u64
                    },
                    "secondary": {
                        "usedPercent": 81,
                        "windowDurationMins": 10_080,
                        "resetsAt": 1_790_563_878_u64
                    }
                }
            }
        });

        let usage = parse_usage(&response).expect("fixture should parse");

        assert_eq!(usage.session_remaining_percent, Some(82.0));
        assert_eq!(usage.weekly_remaining_percent, Some(19.0));
        assert!(usage.reset_at.is_some());
    }

    #[test]
    #[ignore = "requires an authenticated Codex CLI and network access"]
    fn reads_live_account_rate_limits() {
        let usage = query_usage().expect("live Codex usage should be available");

        assert!(usage.session_remaining_percent.is_some());
        assert!(usage.weekly_remaining_percent.is_some());
    }
}
