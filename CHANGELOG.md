# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- Replaced the left-click native popup menu with a fixed, owner-drawn Windows 11
  usage card showing session and weekly progress and separate reset countdowns.
- Made the usage card replace the native hover tooltip and dismiss when the pointer
  leaves the tray icon and card.
- Kept Refresh and Quit in the tray icon's right-click command menu.

## [0.1.0] - 2026-09-24

### Added

- Native Win32 notification-area application with a hidden owner window.
- Native Refresh and Quit menu with clean tray, menu, window-class, and handle
  cleanup.
- Live five-hour and weekly Codex usage retrieval through the authenticated Codex
  app-server protocol.
- Remaining-usage tooltip with a session reset countdown.
- Event-driven Win32 message loop with no background polling.
- Optimized single-file `x86_64-pc-windows-msvc` release build.
- Authenticated live-provider integration test and deterministic parser tests.
- Per-user Windows installation, startup, update, and uninstall instructions.

[Unreleased]: https://github.com/hatamirais/codex-tray/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/hatamirais/codex-tray/releases/tag/v0.1.0
