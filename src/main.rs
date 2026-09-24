#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("codex-tray supports Windows only");

mod codex;
mod icon;
mod tray;
mod usage;
mod win32;

fn main() -> windows::core::Result<()> {
    let initial_usage = codex::load_usage();
    win32::run(initial_usage, codex::load_usage)
}
