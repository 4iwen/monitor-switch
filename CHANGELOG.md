# Changelog

All notable changes to Monitor Switch are documented here. The project follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Native Windows system-tray context menu with current-input checkmarks.
- Windows build and per-user installation scripts, including optional login
  startup and a console-free desktop executable.

### Changed

- Removed the custom Windows flyout in favor of operating-system menu behavior,
  positioning, dismissal, focus, shadow, and accessibility.
- Windows release artifacts now use the same packaging script locally and in CI.

## [0.2.1] - 2026-08-01

### Added

- Per-input `show-in-menu` preference for every display.
- Monitor capability labels that distinguish reported inputs from fallback
  candidates.

### Changed

- The tray menu now contains configured or explicitly enabled inputs only; the
  current input remains visible even when disabled.
- The settings window now uses a compact monitor switcher, source table, native
  style toggles, and a denser macOS layout.
- Existing profiles remain visible after upgrade, while unconfigured candidates
  start hidden.

## [0.2.0] - 2026-08-01

### Added

- Built-in DDC/CI transport on macOS and Windows.
- Native display discovery with stable selectors.
- Multi-display menu and per-display input naming.
- `monitor-switch displays` discovery command.
- Native SF Symbol menu-bar icon on macOS.
- macOS app packaging, login startup, and migration from the old launch agent.
- CI and tagged release workflows for macOS and Windows.

### Changed

- The `auto` backend now selects built-in DDC/CI instead of external tools.
- BetterDisplay, ddcctl, ddcutil, and ControlMyMonitor are compatibility backends.
- The settings window groups inputs by physical display.
- The repository now ships one desktop application instead of two overlapping
  tray binaries.

### Compatibility

- Existing TOML profiles continue to load.
- Numeric display selectors such as `1` continue to map to discovered displays.

## [0.1.0] - 2026-07-07

- Initial CLI, external command backends, named input profiles, and tray UI.
