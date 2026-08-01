# Architecture

Monitor Switch has three layers:

1. `monitor_switch` is the shared Rust library for configuration, display
   discovery, input values, and backend dispatch.
2. `monitor-switch` is a diagnostic and scripting CLI.
3. `monitor-switch-app` is the desktop shell: a native `NSMenu` on macOS and a
   native Win32 context menu on Windows, plus a small settings window backed by
   WebKit on macOS and the system WebView2 runtime on Windows.

## Backend policy

`auto` resolves to the built-in backend. The built-in backend uses `ddc-macos`
on macOS and `ddc-winapi` on Windows through `ddc-hi`. External command backends
remain available only for monitors or connection paths that need a compatibility
workaround.

The Windows backend talks to the monitor through the operating system's DXVA2
monitor configuration API. Neither platform needs BetterDisplay or a bundled
third-party command-line utility for normal operation.

Every switch request contains a display selector and VCP `0x60` value. Display
selectors use the native backend's stable identifier. Legacy one-based numeric
selectors remain accepted.

## Product boundaries

- Local only; no account, network service, analytics, or cloud state.
- User initiated; no USB hot-plug listener or background polling loop.
- Idle work is limited to the operating system's tray event loop.
- Configuration is stored in the user's platform configuration directory.
