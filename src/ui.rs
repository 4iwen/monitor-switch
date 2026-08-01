use std::path::Path;

use serde::Serialize;

use crate::{
    backends::{get_input_source, list_displays, DisplayDescriptor, ReadRequest},
    config::{BackendKind, Config, SourceProfile},
    sources::{format_input_value, COMMON_INPUT_CANDIDATES},
};

#[derive(Debug, Clone, Serialize)]
pub struct UiState {
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
        .map(|(index, descriptor)| monitor_view(descriptor, index, &config))
        .collect::<Vec<_>>();
    let monitor = monitors.first().cloned().unwrap_or_else(empty_monitor);

    UiState {
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
) -> MonitorView {
    let selector = descriptor.selector.as_str();
    let profiles = config
        .profiles
        .iter()
        .filter(|profile| profile_matches_display(profile, selector, display_index, config))
        .cloned()
        .collect::<Vec<_>>();
    let current = get_input_source(ReadRequest {
        backend: config.default_backend,
        display: (!selector.is_empty()).then_some(selector),
    })
    .ok();
    let values = merge_values(
        &COMMON_INPUT_CANDIDATES
            .iter()
            .map(|candidate| candidate.value)
            .collect::<Vec<_>>(),
        &profiles,
    );
    let inputs = values
        .iter()
        .enumerate()
        .map(|(index, value)| input_view(index + 1, *value, current, selector, &profiles))
        .collect::<Vec<_>>();

    MonitorView {
        selector: selector.to_string(),
        name: descriptor.name.clone(),
        subtitle: descriptor.detail.clone(),
        current_value: current,
        current_hex: current.map(format_input_value),
        connected: current.is_some(),
        inputs: inputs.clone(),
        available_inputs: inputs,
    }
}

fn profile_matches_display(
    profile: &SourceProfile,
    selector: &str,
    display_index: usize,
    config: &Config,
) -> bool {
    match profile.display.as_deref() {
        Some(display) => display == selector || (display_index + 1).to_string() == display,
        None => display_index == 0 || config.default_display.as_deref() == Some(selector),
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
    use super::{profile_matches_display, sort_weight};
    use crate::config::{BackendKind, Config, SourceProfile};

    #[test]
    fn unassigned_legacy_profile_belongs_to_first_display() {
        let profile = SourceProfile {
            name: "pc".into(),
            value: 0x11,
            label: None,
            display: None,
            backend: None,
        };
        let config = Config::default();
        assert!(profile_matches_display(&profile, "A", 0, &config));
        assert!(!profile_matches_display(&profile, "B", 1, &config));
    }

    #[test]
    fn explicit_profile_matches_only_selected_display() {
        let profile = SourceProfile {
            name: "pc".into(),
            value: 0x11,
            label: None,
            display: Some("display-b".into()),
            backend: Some(BackendKind::Native),
        };
        let config = Config::default();
        assert!(!profile_matches_display(&profile, "display-a", 0, &config));
        assert!(profile_matches_display(&profile, "display-b", 1, &config));
    }

    #[test]
    fn common_ports_have_stable_menu_order() {
        assert!(sort_weight(0x1b) < sort_weight(0x11));
        assert!(sort_weight(0x13) < sort_weight(0x0f));
    }
}
