# Changelog

All notable changes to Monitor Switch are documented here. The project follows
[Semantic Versioning](https://semver.org/).

## [0.4.1] - 2026-10-22

### Changed

- Input menu entries are plain actions without current-source checkmarks.
- Opening the tray menu and completing a switch no longer trigger input-state
  reads. Previous-input history is retained for the explicit return action.

### Fixed

- Native menu structure changes wait for the menu to close on macOS and Windows.
- Repeated launches notify the existing process to open Settings instead of
  creating another tray icon or competing for shortcuts.
- Display topology changes and wake events trigger debounced, serialized
  rediscovery without background polling.
- Configuration writes use a synced temporary file and atomic replacement;
  loading errors are displayed in Settings instead of silently appearing reset.
- Windows configuration replacement retries transient file-sharing errors
  without truncating the previous configuration.
- Shortcut restoration keeps available combinations active when another
  combination is occupied, including after the recorder loses focus.
- First-run input switching and previous-input reads now use default settings
  when no configuration file exists, without requiring a Settings save first
  (issue #1). Invalid existing configuration files still report errors.

## [0.4.0] - 2026-09-26

### Added

- Optional global shortcuts for each display input and for returning to the
  previous input, using native macOS and Windows hotkey registration.
- Shortcut recording and clearing in Settings, duplicate validation, and
  registration conflict feedback. Existing shortcuts pause while recording.
- Per-display previous-input history for the current app session. Returning
  first reads the actual input so manual switches from another device are handled.
- Offline settings preview and browser interaction/layout regression checks.

### Fixed

- Stale reads cannot overwrite a newer switch. Transient read failures preserve
  the last known input as unverified instead of clearing it.
- Native tray menu identity remains stable during state updates.
- Settings retain drafts on refresh, confirm discarding changes, and show save
  errors inline. Hidden monitor-reported inputs now persist correctly.
- Newly created input profiles inherit the configured DDC backend.

### Changed

- The previous-input shortcut is prominently placed above the input list in
  Settings, with an explicit recording field.
- Supersedes the local, unpublished 0.3.0 development build.

## [0.2.1] - 2026-08-01

### Added

- Per-input `show-in-menu` preference for every display.
- Monitor capability labels that distinguish reported inputs from fallback
  candidates.
- Native Windows system-tray context menu with current-input checkmarks.
- Windows build and per-user installation scripts, including optional login
  startup and a console-free desktop executable.

### Changed

- Removed the custom Windows flyout in favor of operating-system menu behavior,
  positioning, dismissal, focus, shadow, and accessibility.
- Windows release artifacts now use the same packaging script locally and in CI.
- Windows display discovery now exposes one responsive DDC/CI device per logical
  display instead of treating driver-level physical handles as separate screens.
- Inputs reported by monitor capabilities are visible by default; generic
  candidates remain hidden until enabled in Settings.
- The Windows user package now contains one executable named `Monitor Switch.exe`.
- Opening the tray menu now refreshes VCP `0x60` first, so the current-input
  checkmark stays accurate after switching away and returning from another device.
- macOS opens its AppKit menu immediately and applies the refreshed checkmark to
  the live menu instead of waiting for DDC/CI before showing it.
- Windows now creates the tray icon from cached configuration before starting
  DDC/CI discovery, opens the native menu immediately, and refreshes only VCP
  `0x60` on tray clicks instead of re-reading the full capabilities string.
- The Windows settings WebView2 window is prewarmed shortly after startup to
  reduce the first-open delay.
- The tray menu contains configured, monitor-reported, or explicitly enabled
  inputs; the current input remains visible even when disabled.
- The settings window now uses a compact monitor switcher, source table, native
  style toggles, and a denser macOS layout.
- Existing profiles remain visible after upgrade, while generic fallback
  candidates start hidden.
- macOS keeps the status-item menu instance stable while switching and defers
  structural menu changes until native menu tracking has finished.
- Settings refreshes now patch live state in place instead of rebuilding the
  entire WebView document, preserving focus and reducing layout work.
- Settings typography, contrast, and platform colors now follow the macOS and
  Windows system font rendering more closely, with motion limited to lightweight
  control-state transitions.

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
