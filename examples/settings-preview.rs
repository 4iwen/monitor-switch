//! Generate an offline settings fixture without accessing display hardware.
use monitor_switch::{app_html::settings_html, ui::build_cached_state};

fn main() {
    let mut state = build_cached_state(std::path::Path::new(
        "/nonexistent/monitor-switch-preview.toml",
    ));
    state.monitors[0].name = "R27q-30".into();
    state.monitors[0].selector = "preview-display".into();
    state.monitors[0]
        .available_inputs
        .retain(|input| [0x11, 0x12, 0x0f].contains(&input.value));
    state.monitors[0].available_inputs[0].name = "Mac mini".into();
    state.update_current_input("preview-display", Some(0x11));
    let path = std::env::args().nth(1).expect("output HTML path");
    std::fs::write(path, settings_html(&state)).unwrap();
}
