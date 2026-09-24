# codex-tray

`codex-tray` is a Windows-only, native Rust system-tray application for displaying
Codex usage. It uses Win32 directly through the `windows` crate: there is no GUI
framework, webview, managed runtime, or polling loop.

On startup and when **Refresh** is selected, the app starts the authenticated Codex
app server long enough to read the five-hour and weekly rate-limit windows, then
shuts it down. The UI only receives `CodexUsage` values and does not know how they
were retrieved.

The Codex app-server protocol is currently experimental and may change between CLI
versions. If Codex is unavailable, logged out, times out, or returns an incompatible
response, the tooltip reports that usage is unavailable instead of exiting.

## Requirements

- Current stable Rust with the `x86_64-pc-windows-msvc` target
- Microsoft C++ Build Tools and a Windows SDK
- The [Codex desktop app/CLI](https://developers.openai.com/learn/developers-codex-plugin#get-started-with-codex)
  signed in with ChatGPT

Verify Codex and authenticate before installing the tray app:

```powershell
codex --version
codex login
codex login status
```

The final command must report `Logged in using ChatGPT`. `codex-tray` uses that
existing login; it does not require an API key and does not read credential files
directly.

Install the target if needed:

```powershell
rustup target add x86_64-pc-windows-msvc
```

## Build and run

From this directory:

```powershell
cargo build
cargo run
cargo build --release
```

The debug executable is written to `target\debug\codex-tray.exe`; the optimized,
stripped executable is written to `target\release\codex-tray.exe`.

## Install

Build the release executable, copy it to a stable per-user location, and start it:

```powershell
cargo build --release

$installDir = Join-Path $env:LOCALAPPDATA "Programs\codex-tray"
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Copy-Item -LiteralPath ".\target\release\codex-tray.exe" `
    -Destination (Join-Path $installDir "codex-tray.exe") -Force

& (Join-Path $installDir "codex-tray.exe")
```

No administrator rights are required. Rust and the MSVC build tools are needed only
to build from source; the installed executable still requires the authenticated
Codex CLI at runtime.

### Start automatically with Windows (optional)

Register the installed executable for the current user:

```powershell
$installDir = Join-Path $env:LOCALAPPDATA "Programs\codex-tray"
$executable = Join-Path $installDir "codex-tray.exe"
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
Set-ItemProperty -Path $runKey -Name "CodexTray" -Value "`"$executable`""
```

### Update

Choose **Quit** from the tray menu, pull or copy the updated source, and repeat the
release build and `Copy-Item` commands above. Then start the installed executable
again.

### Uninstall

Choose **Quit** first. Then remove the optional startup entry and installation
directory:

```powershell
$installDir = Join-Path $env:LOCALAPPDATA "Programs\codex-tray"
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
Remove-ItemProperty -Path $runKey -Name "CodexTray" -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $installDir -Recurse
```

To validate the live provider separately from the UI:

```powershell
cargo test reads_live_account_rate_limits -- --ignored
```

Left- or right-click the notification-area icon to open its native menu. Choose
**Refresh** to retrieve current usage or **Quit** to remove the icon and exit.

## Source layout

- `main.rs` wires the usage provider to the Windows application.
- `usage.rs` defines the UI-independent usage model.
- `codex.rs` queries and parses the authenticated Codex app-server protocol.
- `tray.rs` formats usage for display and defines tray command identifiers.
- `win32.rs` owns the hidden window, message loop, tray icon, native menu, resource
  cleanup, and all unsafe Win32 interaction.
