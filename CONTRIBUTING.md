# Contributing

Contributions are welcome. Keep changes focused on fast, local monitor input
switching; USB-device automation and cloud features are intentionally outside
the product scope.

## Development

```sh
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

On macOS, build the application bundle with:

```sh
./scripts/build-macos-app.sh
```

Hardware behavior varies by monitor, cable, dock, and GPU. Include the operating
system, monitor model, connection type, input value, and exact error in bug
reports. Never include a user's full configuration file without removing local
paths and display serial numbers.
