# Monitor Switch

A lightweight macOS and Windows utility for switching monitor inputs from the
menu bar or system tray. It sends DDC/CI VCP `0x60` commands directly, so normal
use does not require BetterDisplay, ddcctl, ddcutil, or ControlMyMonitor.

Monitor Switch is deliberately small and user initiated: it does nothing until
you open the menu and choose an input. There is no USB listener, cloud service,
account, analytics, or background polling.

## Features

- Built-in DDC/CI on macOS and Windows
- Native macOS menu and native Windows system-tray menu
- One-click switching between named inputs
- Multiple monitors with independent input names
- Per-input control over which sources appear in the tray menu
- Automatic current-input checkmark when the display supports reading VCP `0x60`
- CLI for discovery, diagnostics, and scripts
- Compatibility backends for unusual monitor, dock, and GPU combinations
- Starts at login on macOS after installation
- Optional per-user startup shortcut on Windows

## Requirements

- A monitor with DDC/CI enabled in its on-screen settings
- macOS 12 or later, or Windows 10/11
- A connection path that carries DDC/CI; some docks, adapters, and KVMs block it

Nintendo Switch and other source devices do not need any software. The Mac or PC
running Monitor Switch sends the command to the monitor.

## macOS installation

Until signed release artifacts are published, build and install locally:

```sh
./scripts/install-macos-app.sh
```

This installs `Monitor Switch.app` in `~/Applications`, registers a per-user
login item, and starts it without a Terminal window. Open it later from Finder,
Launchpad, or Spotlight like any other application.

Build without installing:

```sh
./scripts/build-macos-app.sh
open "dist/Monitor Switch.app"
```

## Windows installation

On Windows 10/11, open PowerShell in the repository and run:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-windows.ps1
```

This installs the app in `%LOCALAPPDATA%\Programs\Monitor Switch`, creates a
per-user Startup shortcut, and opens it directly in the system tray without a
console window. The settings window uses the Windows WebView2 runtime included
with current Windows 10 and Windows 11 installations.

Build a portable ZIP without installing:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-windows.ps1
```

The archive is written to `dist\Monitor-Switch-Windows.zip` and contains one
user-facing executable, `Monitor Switch.exe`. The diagnostic CLI remains
available to developers through Cargo but is not included in the user package.

## First setup

1. Enable DDC/CI in the monitor's physical on-screen menu.
2. Open Monitor Switch from the menu bar.
3. Open Settings.
4. Give the inputs you use recognizable names, such as `Mac mini`, `Gaming PC`,
   or `Nintendo Switch`.
5. Save, then select an input from the menu.

Monitor input values are not perfectly consistent between manufacturers. The
settings window includes the common MCCS values and keeps custom values already
present in the configuration.

DDC/CI can report which inputs a monitor model supports and which input is
currently active, but it usually cannot report whether a cable is physically
connected to every inactive port. Monitor Switch shows inputs reported by the
monitor by default, keeps generic fallback candidates hidden, and lets every
input be shown or hidden in Settings.

## CLI

```sh
# Discover displays and stable selectors
monitor-switch displays

# List common input values
monitor-switch candidates

# Read a display's current input
monitor-switch current --display 1

# Try a value without saving it
monitor-switch raw 0x11 --display 1

# Save and use a named profile
monitor-switch add pc 0x11 --label "Gaming PC" --display 1
monitor-switch switch pc
```

The configuration path can be printed with `monitor-switch config-path`.
Existing 0.1 configurations remain compatible with 0.2.

The macOS application bundle includes the CLI at
`Monitor Switch.app/Contents/Resources/monitor-switch`. Developers can also run
it through `cargo run --bin monitor-switch -- <command>`.

## Input values

Common values include:

| Value | Input |
| --- | --- |
| `0x0f` | DisplayPort 1 |
| `0x10` | DisplayPort 2 |
| `0x11` | HDMI 1 |
| `0x12` | HDMI 2 |
| `0x13` | HDMI 3 |
| `0x1b` | USB-C / DP Alt Mode |

These are common MCCS values, not a guarantee for every monitor model.

## Compatibility backends

The default `auto` backend is the built-in native backend. These explicit values
remain available for troubleshooting older setups:

- `betterdisplay`
- `ddcctl`
- `ddcutil`
- `control-my-monitor`

For example:

```sh
monitor-switch raw 0x11 --display 1 --backend ddcctl
```

## Development

```sh
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

See [Architecture](docs/ARCHITECTURE.md), [Changelog](CHANGELOG.md), and
[Contributing](CONTRIBUTING.md).

## Acknowledgements

The native DDC layer builds on the Rust DDC ecosystem used by
[haimgel/display-switch](https://github.com/haimgel/display-switch), especially
`ddc-hi`, `ddc-macos`, and `ddc-winapi`. Monitor Switch intentionally focuses on
an on-demand menu experience rather than USB-triggered automation.

## License

[MIT](LICENSE)
