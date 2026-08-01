# Changelog

All notable changes to Monitor Switch are documented here. The project follows
[Semantic Versioning](https://semver.org/).

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
