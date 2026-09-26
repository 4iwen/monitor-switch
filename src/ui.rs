use std::path::Path;

use serde::Serialize;

use crate::{
    backends::{
        get_input_source, get_supported_input_sources, list_displays, DisplayDescriptor,
        ReadRequest,
    },
    config::{BackendKind, Config, SourceProfile},
    sources::{format_input_value, COMMON_INPUT_CANDIDATES},
};

#[derive(Debug, Clone, Serialize)]
pub struct UiState {
    pub shortcuts: Vec<crate::shortcuts::ShortcutBinding>,
    pub shortcut_error: Option<String>,
    pub previous_inputs: std::collections::HashMap<String, u16>,
    pub monitor: MonitorView,
    pub monitors: Vec<MonitorView>,
    pub inputs: Vec<InputView>,
    pub available_inputs: Vec<InputView>,
    pub profiles: Vec<ProfileView>,
    pub config_path: String,
    pub backend: String,
    pub operation: OperationView,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct OperationView {
    pub status: OperationStatus,
    pub target_value: Option<u16>,
    pub target_display: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OperationStatus {
    #[default]
    Idle,
    Switching,
    Success,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonitorView {
    pub selector: String,
    pub name: String,
    pub subtitle: String,
    pub current_value: Option<u16>,
    pub current_hex: Option<String>,
    pub connected: bool,
    pub inputs: Vec<InputView>,
    pub available_inputs: Vec<InputView>,
}

impl UiState {
    pub fn update_current_input(&mut self, selector: &str, current: Option<u16>) {
        let Some(monitor) = self
            .monitors
            .iter_mut()
            .find(|monitor| monitor.selector == selector)
        else {
            return;
        };

        monitor.connected = current.is_some();
        if let Some(value) = current {
            monitor.current_value = Some(value);
            monitor.current_hex = Some(format_input_value(value));
            for input in &mut monitor.available_inputs {
                input.current = input.value == value;
            }
            monitor.inputs = menu_inputs(&monitor.available_inputs);
        }
        self.sync_primary_monitor();
    }

    pub fn mark_switch_sent(&mut self, selector: &str, value: u16) {
        self.update_current_input(selector, Some(value));
        if let Some(monitor) = self
            .monitors
            .iter_mut()
            .find(|monitor| monitor.selector == selector)
        {
            monitor.connected = false;
        }
        self.sync_primary_monitor();
    }

    fn sync_primary_monitor(&mut self) {
        if let Some(first) = self.monitors.first().cloned() {
            self.monitor = first.clone();
            self.inputs = first.inputs;
            self.available_inputs = first.available_inputs;
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InputView {
    pub index: usize,
    pub name: String,
    pub subtitle: String,
    pub value: u16,
    pub hex: String,
    pub profile_name: Option<String>,
    pub display_selector: String,
    pub icon: String,
    pub current: bool,
    pub show_in_menu: bool,
    pub reported_supported: bool,
    pub shortcut: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileView {
    pub name: String,
    pub label: String,
    pub value: u16,
    pub hex: String,
    pub display: String,
    pub backend: String,
}

pub fn build_state(config_path: &Path) -> UiState {
    let config = Config::load(config_path).unwrap_or_default();
    let descriptors = discovered_or_configured_displays(&config);
    let monitors = descriptors
        .iter()
        .enumerate()
        .map(|(index, descriptor)| monitor_view(descriptor, index, &config, true))
        .collect::<Vec<_>>();
    assemble_state(config_path, config, monitors)
}

pub fn build_cached_state(config_path: &Path) -> UiState {
    let config = Config::load(config_path).unwrap_or_default();
    let descriptors = configured_displays(&config);
    let monitors = descriptors
        .iter()
        .enumerate()
        .map(|(index, descriptor)| monitor_view(descriptor, index, &config, false))
        .collect::<Vec<_>>();
    assemble_state(config_path, config, monitors)
}

fn assemble_state(config_path: &Path, config: Config, monitors: Vec<MonitorView>) -> UiState {
    let monitor = monitors.first().cloned().unwrap_or_else(empty_monitor);

    UiState {
        shortcuts: config.shortcuts.clone(),
        shortcut_error: None,
        previous_inputs: std::collections::HashMap::new(),
        inputs: monitor.inputs.clone(),
        available_inputs: monitor.available_inputs.clone(),
        monitor,
        monitors,
        profiles: config
            .profiles
            .iter()
            .map(|profile| profile_view(profile, config.default_backend))
            .collect(),
        config_path: config_path.display().to_string(),
        backend: backend_label(config.default_backend).to_string(),
        operation: OperationView::default(),
    }
}

fn configured_displays(config: &Config) -> Vec<DisplayDescriptor> {
    let mut selectors = Vec::new();
    if let Some(selector) = config.default_display.as_deref() {
        if !selector.trim().is_empty() {
            selectors.push(selector.to_string());
        }
    }
    for selector in config
        .profiles
        .iter()
        .filter_map(|profile| profile.display.as_deref())
    {
        if !selector.trim().is_empty() && !selectors.iter().any(|value| value == selector) {
            selectors.push(selector.to_string());
        }
    }
    if selectors.is_empty() {
        selectors.push(String::new());
    }

    selectors
        .into_iter()
        .map(|selector| DisplayDescriptor {
            name: if selector.is_empty() {
                "External Display".to_string()
            } else {
                selector.clone()
            },
            selector,
            detail: "Detecting DDC/CI display…".to_string(),
        })
        .collect()
}

fn discovered_or_configured_displays(config: &Config) -> Vec<DisplayDescriptor> {
    let discovered = list_displays();
    if !discovered.is_empty() {
        return discovered;
    }

    let selector = config
        .default_display
        .clone()
        .or_else(|| {
            config
                .profiles
                .iter()
                .find_map(|profile| profile.display.clone())
        })
        .unwrap_or_default();
    vec![DisplayDescriptor {
        selector,
        name: "External Display".to_string(),
        detail: "No DDC/CI display detected".to_string(),
    }]
}

fn monitor_view(
    descriptor: &DisplayDescriptor,
    display_index: usize,
    config: &Config,
    probe_hardware: bool,
) -> MonitorView {
    let selector = descriptor.selector.as_str();
    let profiles = config
        .profiles
        .iter()
        .filter(|profile| {
            profile_matches_display(profile, selector, &descriptor.name, display_index, config)
        })
        .cloned()
        .collect::<Vec<_>>();
    let (current, supported) = if probe_hardware {
        (
            get_input_source(ReadRequest {
                backend: config.default_backend,
                display: (!selector.is_empty()).then_some(selector),
            })
            .ok(),
            get_supported_input_sources(ReadRequest {
                backend: config.default_backend,
                display: (!selector.is_empty()).then_some(selector),
            })
            .unwrap_or_default(),
        )
    } else {
        (None, Vec::new())
    };
    let available_values = merge_values(
        &COMMON_INPUT_CANDIDATES
            .iter()
            .map(|candidate| candidate.value)
            .collect::<Vec<_>>(),
        &profiles,
    );
    let mut available_inputs = available_values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            input_view(
                index + 1,
                *value,
                current,
                selector,
                &profiles,
                supported.contains(value),
            )
        })
        .collect::<Vec<_>>();
    available_inputs.sort_by_key(|input| {
        let group = if input.profile_name.is_some() {
            0
        } else if input.reported_supported {
            1
        } else {
            2
        };
        (group, sort_weight(input.value))
    });
    let inputs = menu_inputs(&available_inputs);

    MonitorView {
        selector: selector.to_string(),
        name: descriptor.name.clone(),
        subtitle: descriptor.detail.clone(),
        current_value: current,
        current_hex: current.map(format_input_value),
        connected: current.is_some(),
        inputs,
        available_inputs,
    }
}

fn menu_inputs(available_inputs: &[InputView]) -> Vec<InputView> {
    available_inputs
        .iter()
        .filter(|input| input.show_in_menu || input.current)
        .cloned()
        .collect()
}

fn profile_matches_display(
    profile: &SourceProfile,
    selector: &str,
    display_name: &str,
    display_index: usize,
    config: &Config,
) -> bool {
    match profile.display.as_deref() {
        Some(display) => {
            display == selector
                || display.eq_ignore_ascii_case(display_name)
                || (display_index + 1).to_string() == display
        }
        None => {
            display_index == 0
                || config.default_display.as_deref() == Some(selector)
                || config
                    .default_display
                    .as_deref()
                    .is_some_and(|display| display.eq_ignore_ascii_case(display_name))
        }
    }
}

fn empty_monitor() -> MonitorView {
    MonitorView {
        selector: String::new(),
        name: "External Display".to_string(),
        subtitle: "No DDC/CI display detected".to_string(),
        current_value: None,
        current_hex: None,
        connected: false,
        inputs: Vec::new(),
        available_inputs: Vec::new(),
    }
}

fn merge_values(supported: &[u16], profiles: &[SourceProfile]) -> Vec<u16> {
    let mut values = supported.to_vec();
    for profile in profiles {
        if !values.contains(&profile.value) {
            values.push(profile.value);
        }
    }
    values.sort_by_key(|value| sort_weight(*value));
    values
}

fn sort_weight(value: u16) -> u16 {
    match value {
        0x1b => 1,
        0x11 => 2,
        0x12 => 3,
        0x13 => 4,
        0x0f => 5,
        0x10 => 6,
        _ => 100 + value,
    }
}

fn input_view(
    index: usize,
    value: u16,
    current: Option<u16>,
    display_selector: &str,
    profiles: &[SourceProfile],
    reported_supported: bool,
) -> InputView {
    let profile = profiles.iter().find(|profile| profile.value == value);
    let physical_name = physical_name(value);
    let label = profile
        .and_then(|profile| profile.label.as_deref())
        .filter(|label| !label.trim().is_empty())
        .unwrap_or(physical_name);

    InputView {
        index,
        name: label.to_string(),
        subtitle: physical_name.to_string(),
        value,
        hex: format_input_value(value),
        profile_name: profile.map(|profile| profile.name.clone()),
        display_selector: display_selector.to_string(),
        icon: icon_name(value).to_string(),
        current: current == Some(value),
        show_in_menu: profile
            .map(|profile| profile.show_in_menu)
            .unwrap_or(reported_supported),
        reported_supported,
        shortcut: if index <= 9 {
            format!("⌘{index}")
        } else {
            String::new()
        },
    }
}

fn profile_view(profile: &SourceProfile, default_backend: BackendKind) -> ProfileView {
    let label = profile
        .label
        .as_deref()
        .filter(|label| !label.trim().is_empty())
        .unwrap_or(&profile.name);

    ProfileView {
        name: profile.name.clone(),
        label: label.to_string(),
        value: profile.value,
        hex: format_input_value(profile.value),
        display: profile.display.clone().unwrap_or_else(|| "-".to_string()),
        backend: profile.backend.unwrap_or(default_backend).to_string(),
    }
}

pub fn physical_name(value: u16) -> &'static str {
    COMMON_INPUT_CANDIDATES
        .iter()
        .find(|candidate| candidate.value == value)
        .map(|candidate| candidate.name)
        .unwrap_or("Input Source")
}

fn icon_name(value: u16) -> &'static str {
    match value {
        0x11..=0x14 => "hdmi",
        0x0f | 0x10 => "displayport",
        0x1b => "usb-c",
        _ => "monitor",
    }
}

fn backend_label(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::Auto | BackendKind::Native => "Built-in DDC/CI",
        BackendKind::BetterDisplay => "BetterDisplay compatibility",
        BackendKind::Ddcutil => "ddcutil compatibility",
        BackendKind::Ddcctl => "ddcctl compatibility",
        BackendKind::ControlMyMonitor => "ControlMyMonitor compatibility",
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        build_cached_state, input_view, menu_inputs, profile_matches_display, sort_weight,
        InputView,
    };
    use crate::config::{BackendKind, Config, SourceProfile};

    #[test]
    fn unassigned_legacy_profile_belongs_to_first_display() {
        let profile = SourceProfile {
            name: "pc".into(),
            value: 0x11,
            label: None,
            display: None,
            backend: None,
            show_in_menu: true,
        };
        let config = Config::default();
        assert!(profile_matches_display(
            &profile,
            "A",
            "Monitor A",
            0,
            &config
        ));
        assert!(!profile_matches_display(
            &profile,
            "B",
            "Monitor B",
            1,
            &config
        ));
    }

    #[test]
    fn explicit_profile_matches_only_selected_display() {
        let profile = SourceProfile {
            name: "pc".into(),
            value: 0x11,
            label: None,
            display: Some("display-b".into()),
            backend: Some(BackendKind::Native),
            show_in_menu: true,
        };
        let config = Config::default();
        assert!(!profile_matches_display(
            &profile,
            "display-a",
            "Monitor A",
            0,
            &config
        ));
        assert!(profile_matches_display(
            &profile,
            "display-b",
            "Monitor B",
            1,
            &config
        ));
    }

    #[test]
    fn legacy_windows_description_still_matches_display_name() {
        let profile = SourceProfile {
            name: "pc".into(),
            value: 0x11,
            label: None,
            display: Some("R27q-30".into()),
            backend: None,
            show_in_menu: true,
        };
        assert!(profile_matches_display(
            &profile,
            r"\\.\DISPLAY1",
            "R27q-30",
            0,
            &Config::default()
        ));
    }

    #[test]
    fn common_ports_have_stable_menu_order() {
        assert!(sort_weight(0x1b) < sort_weight(0x11));
        assert!(sort_weight(0x13) < sort_weight(0x0f));
    }

    #[test]
    fn menu_hides_candidates_but_keeps_the_current_input() {
        let input = |value, show_in_menu, current| InputView {
            index: 1,
            name: "Input".into(),
            subtitle: "HDMI".into(),
            value,
            hex: format!("0x{value:02x}"),
            profile_name: None,
            display_selector: "display".into(),
            icon: "hdmi".into(),
            current,
            show_in_menu,
            reported_supported: true,
            shortcut: String::new(),
        };
        let available = vec![
            input(0x11, true, false),
            input(0x12, false, false),
            input(0x0f, false, true),
        ];

        let visible = menu_inputs(&available);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].value, 0x11);
        assert_eq!(visible[1].value, 0x0f);
    }

    #[test]
    fn reported_inputs_are_visible_until_the_user_overrides_them() {
        let discovered = input_view(1, 0x11, None, "display", &[], true);
        assert!(discovered.show_in_menu);

        let hidden_profile = SourceProfile {
            name: "hdmi-1".into(),
            value: 0x11,
            label: None,
            display: Some("display".into()),
            backend: None,
            show_in_menu: false,
        };
        let hidden = input_view(1, 0x11, None, "display", &[hidden_profile], true);
        assert!(!hidden.show_in_menu);

        let candidate = input_view(1, 0x12, None, "display", &[], false);
        assert!(!candidate.show_in_menu);
    }

    #[test]
    fn cached_state_is_available_without_display_io() {
        let state = build_cached_state(Path::new("/path/that/does/not/exist/config.toml"));
        assert_eq!(state.monitors.len(), 1);
        assert!(!state.monitor.connected);
        assert!(!state.available_inputs.is_empty());
    }

    #[test]
    fn failed_read_keeps_last_known_input() {
        let mut state = build_cached_state(Path::new("/path/that/does/not/exist/config.toml"));
        state.update_current_input("", Some(0x11));
        state.update_current_input("", None);

        assert_eq!(state.monitor.current_value, Some(0x11));
        assert!(!state.monitor.connected);
        assert_eq!(
            state
                .monitor
                .inputs
                .iter()
                .filter(|input| input.current)
                .count(),
            1
        );
        assert!(state
            .monitor
            .inputs
            .iter()
            .any(|input| input.value == 0x11 && input.current));
    }

    #[test]
    fn newly_detected_hidden_input_appears_in_menu() {
        let mut state = build_cached_state(Path::new("/path/that/does/not/exist/config.toml"));
        let hidden = state.monitors[0]
            .available_inputs
            .iter()
            .find(|input| !input.show_in_menu)
            .unwrap()
            .value;
        state.update_current_input("", Some(hidden));

        assert!(state
            .monitor
            .inputs
            .iter()
            .any(|input| input.value == hidden && input.current));
    }

    #[test]
    fn sent_switch_is_shown_but_not_claimed_verified() {
        let mut state = build_cached_state(Path::new("/path/that/does/not/exist/config.toml"));
        state.update_current_input("", Some(0x11));
        state.mark_switch_sent("", 0x12);

        assert_eq!(state.monitor.current_value, Some(0x12));
        assert!(!state.monitor.connected);
        assert_eq!(
            state
                .monitor
                .inputs
                .iter()
                .filter(|input| input.current)
                .count(),
            1
        );
    }
}
