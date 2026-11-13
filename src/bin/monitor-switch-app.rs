#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use std::collections::HashMap;
use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use monitor_switch::{
    app_html::settings_html,
    backends::{get_input_source, set_input_source, ReadRequest, SwitchRequest},
    config::{default_config_path, Config, SourceProfile},
    desktop::{DesktopEvent, DesktopHooks},
    error::Result,
    history::InputHistory,
    instance::{Acquisition, Instance},
    shortcuts::{ShortcutBinding, Shortcuts},
    ui::{build_cached_state, build_state, OperationStatus, UiState},
};
use serde::Deserialize;
use tao::{
    dpi::LogicalSize,
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    window::{Window, WindowBuilder, WindowId},
};
#[cfg(target_os = "macos")]
use tray_icon::menu::ContextMenu;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, MenuItemKind, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use wry::{WebView, WebViewBuilder};

#[derive(Debug, Clone)]
enum UserEvent {
    Desktop(DesktopEvent),
    ShowSettings,
    PreviousReady {
        epoch: u64,
        display: String,
        current: std::result::Result<u16, String>,
    },
    HotKey(u32),
    PrewarmSettings,
    NativeMenu(MenuEvent),
    Ipc(String),
    SwitchFinished {
        epoch: u64,
        value: u16,
        display: String,
        result: std::result::Result<(), String>,
    },
    RefreshFinished {
        epoch: u64,
        sequence: u64,
        config_revision: u64,
        state: Box<UiState>,
    },
}

#[derive(Debug, Clone)]
enum NativeMenuAction {
    Previous(String),
    Switch {
        value: u16,
        display: String,
        profile: Option<String>,
    },
    Refresh,
    OpenSettings,
    Quit,
}

#[derive(Debug, Deserialize)]
struct LabelUpdate {
    value: u16,
    display: String,
    display_index: usize,
    label: String,
    show_in_menu: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum IpcCommand {
    #[serde(rename = "closeSettings")]
    CloseSettings,
    #[serde(rename = "saveLabels")]
    SaveLabels {
        items: Vec<LabelUpdate>,
        #[serde(default)]
        shortcuts: Option<Vec<ShortcutBinding>>,
    },
    #[serde(rename = "recordShortcut")]
    RecordShortcut { active: bool },
}

struct WindowBundle {
    window: Window,
    webview: WebView,
}

struct AppState {
    desktop_hooks: Option<DesktopHooks>,
    menu_rebuild_pending: bool,
    display_refresh_due: Option<Instant>,
    shortcuts: Option<Shortcuts>,
    history: InputHistory,
    recording_shortcut: bool,
    refresh_pending: bool,
    refresh_in_flight: Option<u64>,
    config_path: PathBuf,
    tray_icon: Option<TrayIcon>,
    settings: Option<WindowBundle>,
    ui_state: Option<UiState>,
    native_menu_actions: HashMap<String, NativeMenuAction>,
    native_menu: Option<Menu>,
    native_input_items: HashMap<String, MenuItem>,
    native_menu_signature: Vec<String>,
    epoch: u64,
    full_refresh_sequence: u64,
    config_revision: u64,
}

impl AppState {
    fn new(config_path: PathBuf) -> Self {
        Self {
            desktop_hooks: None,
            menu_rebuild_pending: false,
            display_refresh_due: None,
            shortcuts: None,
            history: InputHistory::default(),
            recording_shortcut: false,
            refresh_pending: false,
            refresh_in_flight: None,
            config_path,
            tray_icon: None,
            settings: None,
            ui_state: None,
            native_menu_actions: HashMap::new(),
            native_menu: None,
            native_input_items: HashMap::new(),
            native_menu_signature: Vec::new(),
            epoch: 0,
            full_refresh_sequence: 0,
            config_revision: 0,
        }
    }

    fn init(
        &mut self,
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> std::result::Result<(), Box<dyn Error>> {
        let _ = target;
        configure_macos_application();
        if let Err(error) = migrate_legacy_config(&self.config_path) {
            eprintln!("failed to migrate legacy configuration: {error}");
        }
        let mut state = build_cached_state(&self.config_path);
        match Shortcuts::new() {
            Ok(mut shortcuts) => {
                state.shortcut_error = shortcuts.restore_available(&state.shortcuts).err();
                self.shortcuts = Some(shortcuts);
            }
            Err(error) => state.shortcut_error = Some(error),
        }
        let mut tray_builder = TrayIconBuilder::new()
            .with_tooltip("Monitor Switch")
            .with_icon(build_icon()?);
        #[cfg(target_os = "macos")]
        {
            tray_builder = tray_builder.with_icon_as_template(true);
        }

        let (menu, actions, input_items) = build_native_menu(&state);
        self.native_menu_actions = actions;
        self.native_input_items = input_items;
        self.native_menu_signature = menu_signature(&state);
        self.native_menu = Some(menu.clone());
        tray_builder = tray_builder
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(true)
            .with_menu_on_right_click(true);

        let tray_icon = tray_builder.build()?;
        configure_macos_tray_symbol(&tray_icon);
        let desktop_proxy = proxy.clone();
        self.desktop_hooks = Some(DesktopHooks::new(
            &tray_icon,
            self.native_menu.as_ref().unwrap(),
            Arc::new(move |event| {
                let _ = desktop_proxy.send_event(UserEvent::Desktop(event));
            }),
        )?);
        self.tray_icon = Some(tray_icon);

        self.ui_state = Some(state);
        self.refresh_async(proxy.clone());
        thread::spawn(move || {
            let delay = if cfg!(target_os = "windows") {
                2_500
            } else {
                1_000
            };
            thread::sleep(std::time::Duration::from_millis(delay));
            let _ = proxy.send_event(UserEvent::PrewarmSettings);
        });
        Ok(())
    }

    fn handle_native_menu(
        &mut self,
        id: &MenuId,
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
        control_flow: &mut ControlFlow,
    ) {
        let Some(action) = self.native_menu_actions.get(id.as_ref()).cloned() else {
            return;
        };

        match action {
            NativeMenuAction::Previous(display) => self.switch_previous(&display, proxy),
            NativeMenuAction::Switch {
                value,
                display,
                profile,
            } => {
                self.begin_switch(Some(value), Some(display), profile, proxy);
            }
            NativeMenuAction::Refresh => self.refresh_async(proxy),
            NativeMenuAction::OpenSettings => {
                if let Err(error) = self.open_settings(target, proxy) {
                    eprintln!("failed to open settings: {error}");
                }
            }
            NativeMenuAction::Quit => *control_flow = ControlFlow::Exit,
        }
    }

    fn handle_ipc(&mut self, body: &str, proxy: EventLoopProxy<UserEvent>) {
        let command = match serde_json::from_str::<IpcCommand>(body) {
            Ok(command) => command,
            Err(error) => {
                eprintln!("invalid IPC command: {error}; body={body}");
                return;
            }
        };

        match command {
            IpcCommand::RecordShortcut { active } => {
                self.set_shortcut_recording(active);
                self.refresh_settings();
            }
            IpcCommand::CloseSettings => {
                self.set_shortcut_recording(false);
                if let Some(settings) = &self.settings {
                    settings.window.set_visible(false);
                }
            }
            IpcCommand::SaveLabels { items, shortcuts } => match self.save_labels(items, shortcuts)
            {
                Ok(()) => {
                    self.refresh_async(proxy);
                    self.evaluate_settings(
                        "window.monitorSwitchSaved && window.monitorSwitchSaved();",
                    );
                }
                Err(error) => {
                    let message = serde_json::to_string(&error.to_string())
                        .unwrap_or_else(|_| "\"Save failed\"".to_string());
                    self.evaluate_settings(&format!(
                            "window.monitorSwitchSaveError && window.monitorSwitchSaveError({message});"
                        ));
                }
            },
        }
    }

    fn set_shortcut_recording(&mut self, active: bool) {
        self.recording_shortcut = active;
        if let (Some(shortcuts), Some(state)) = (&mut self.shortcuts, &mut self.ui_state) {
            state.shortcut_error = if active {
                shortcuts.replace(&[]).err()
            } else {
                shortcuts.restore_available(&state.shortcuts).err()
            };
        }
    }

    fn switch_previous(&mut self, display: &str, proxy: EventLoopProxy<UserEvent>) {
        let Some(state) = &mut self.ui_state else {
            return;
        };
        if state.operation.status == OperationStatus::Switching {
            return;
        }
        state.operation.status = OperationStatus::Switching;
        state.operation.target_display = Some(display.to_owned());
        state.operation.target_value = state
            .monitors
            .iter()
            .find(|monitor| monitor.selector == display)
            .and_then(|monitor| monitor.current_value);
        self.epoch += 1;
        let epoch = self.epoch;
        let config_path = self.config_path.clone();
        let display = display.to_owned();
        self.sync_native_input_items();
        thread::spawn(move || {
            let current = Config::load_or_default(&config_path)
                .and_then(|config| {
                    get_input_source(ReadRequest {
                        backend: config.default_backend,
                        display: (!display.is_empty()).then_some(display.as_str()),
                    })
                })
                .map_err(|error| error.to_string());
            let _ = proxy.send_event(UserEvent::PreviousReady {
                epoch,
                display,
                current,
            });
        });
    }

    fn handle_previous_ready(
        &mut self,
        epoch: u64,
        display: String,
        current: std::result::Result<u16, String>,
        proxy: EventLoopProxy<UserEvent>,
    ) {
        if epoch != self.epoch {
            return;
        }
        let Some(state) = &mut self.ui_state else {
            return;
        };
        state.operation.status = OperationStatus::Idle;
        let message = match current {
            Ok(value) => {
                self.history.observe(&display, value);
                state.previous_inputs = self.history.previous();
                state.update_current_input(&display, Some(value));
                if let Some(previous) = state.previous_inputs.get(&display).copied() {
                    self.begin_switch(Some(previous), Some(display), None, proxy);
                    return;
                }
                "No previous input to return to yet, switch once first".to_owned()
            }
            Err(error) => {
                state.update_current_input(&display, None);
                format!("Cannot confirm the current input, switch back not performed: {error}")
            }
        };
        state.operation.status = OperationStatus::Error;
        state.operation.message = Some(message);
        self.rebuild_native_menu();
        self.refresh_settings();
        if self.refresh_pending {
            self.refresh_async(proxy);
        }
    }

    fn handle_hotkey(&mut self, id: u32, proxy: EventLoopProxy<UserEvent>) {
        if self.recording_shortcut {
            return;
        }
        let Some(binding) = self
            .shortcuts
            .as_ref()
            .and_then(|shortcuts| shortcuts.action(id))
        else {
            return;
        };
        let display = self
            .ui_state
            .as_ref()
            .and_then(|state| {
                if binding.display.is_empty() {
                    state.monitors.first()
                } else {
                    state
                        .monitors
                        .iter()
                        .find(|monitor| monitor.selector == binding.display)
                }
            })
            .map(|monitor| monitor.selector.clone());
        let Some(display) = display else {
            return;
        };
        match binding.value {
            Some(value) => self.begin_switch(Some(value), Some(display), None, proxy),
            None => self.switch_previous(&display, proxy),
        }
    }

    fn begin_switch(
        &mut self,
        value: Option<u16>,
        display: Option<String>,
        profile: Option<String>,
        proxy: EventLoopProxy<UserEvent>,
    ) {
        let Some(value) = value else {
            return;
        };
        if self
            .ui_state
            .as_ref()
            .is_some_and(|state| state.operation.status == OperationStatus::Switching)
        {
            return;
        }

        if let Some(state) = &mut self.ui_state {
            state.operation.status = OperationStatus::Switching;
            state.operation.target_value = Some(value);
            state.operation.target_display = display.clone();
            state.operation.message = None;
        }
        self.epoch += 1;
        let epoch = self.epoch;
        self.sync_native_input_items();
        self.refresh_settings();

        let config_path = self.config_path.clone();
        thread::spawn(move || {
            let display_for_event = display.clone().unwrap_or_default();
            let result = switch_input(&config_path, value, display, profile)
                .map(|_| ())
                .map_err(|error| error.to_string());
            let _ = proxy.send_event(UserEvent::SwitchFinished {
                epoch,
                value,
                display: display_for_event,
                result,
            });
        });
    }

    fn prewarm_settings(
        &mut self,
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
    ) {
        if self.settings.is_some() {
            return;
        }
        let Some(state) = self.ui_state.as_ref() else {
            return;
        };
        match create_settings(target, proxy, state) {
            Ok(settings) => self.settings = Some(settings),
            Err(error) => eprintln!("failed to prewarm settings: {error}"),
        }
    }

    fn open_settings(
        &mut self,
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> std::result::Result<(), Box<dyn Error>> {
        if self.settings.is_none() {
            let state = self
                .ui_state
                .clone()
                .unwrap_or_else(|| build_state(&self.config_path));
            self.settings = Some(create_settings(target, proxy, &state)?);
        }

        if let Some(settings) = &self.settings {
            activate_macos_application();
            settings.window.set_visible(true);
            settings.window.set_focus();
            if let Some(state) = &self.ui_state {
                refresh_webview(&settings.webview, state);
            }
        }

        Ok(())
    }

    fn save_labels(
        &mut self,
        items: Vec<LabelUpdate>,
        bindings: Option<Vec<ShortcutBinding>>,
    ) -> Result<()> {
        let mut config = Config::load_or_default(&self.config_path)?;
        let old_bindings = config.shortcuts.clone();
        if let Some(bindings) = bindings {
            monitor_switch::shortcuts::validate(&bindings)
                .map_err(monitor_switch::error::AppError::Shortcut)?;
            config.shortcuts = bindings;
        }
        for item in items {
            let label = item.label.trim();
            if let Some(profile) = config.profiles.iter_mut().find(|profile| {
                profile.value == item.value
                    && match profile.display.as_deref() {
                        Some(display) => {
                            display == item.display
                                || display == (item.display_index + 1).to_string()
                        }
                        None => item.display_index == 0,
                    }
            }) {
                profile.label = custom_label(item.value, label);
                profile.show_in_menu = item.show_in_menu;
                profile.display = (!item.display.is_empty()).then_some(item.display.clone());
            } else {
                config.profiles.push(SourceProfile {
                    name: if item.display_index == 0 {
                        format!("input-{:02x}", item.value)
                    } else {
                        format!(
                            "display-{}-input-{:02x}",
                            item.display_index + 1,
                            item.value
                        )
                    },
                    value: item.value,
                    label: custom_label(item.value, label),
                    display: (!item.display.is_empty()).then_some(item.display.clone()),
                    backend: None,
                    show_in_menu: item.show_in_menu,
                });
            }
        }
        if let Some(shortcuts) = &mut self.shortcuts {
            shortcuts
                .replace(&config.shortcuts)
                .map_err(monitor_switch::error::AppError::Shortcut)?;
        } else if !config.shortcuts.is_empty() {
            return Err(monitor_switch::error::AppError::Shortcut(
                "System shortcut service unavailable".into(),
            ));
        }
        if let Err(error) = config.save(&self.config_path) {
            if let Some(shortcuts) = &mut self.shortcuts {
                if let Err(restore_error) = shortcuts.restore_available(&old_bindings) {
                    if let Some(state) = &mut self.ui_state {
                        state.shortcut_error = Some(restore_error);
                    }
                }
            }
            return Err(error);
        }
        if let Some(state) = &mut self.ui_state {
            state.shortcuts = config.shortcuts;
            state.shortcut_error = None;
        }
        self.config_revision += 1;
        Ok(())
    }

    fn rebuild_native_menu(&mut self) {
        let Some(state) = &self.ui_state else {
            return;
        };

        let signature = menu_signature(state);
        if signature != self.native_menu_signature {
            if self
                .desktop_hooks
                .as_ref()
                .is_some_and(DesktopHooks::menu_is_open)
            {
                self.menu_rebuild_pending = true;
                return;
            }
            let Some(menu) = &self.native_menu else {
                return;
            };
            while !menu.items().is_empty() {
                menu.remove_at(0);
            }
            let (actions, input_items) = populate_native_menu(menu, state);
            self.native_menu_actions = actions;
            self.native_input_items = input_items;
            self.native_menu_signature = signature;
        }
        self.menu_rebuild_pending = false;
        if let Some(menu) = &self.native_menu {
            decorate_native_menu_item(menu, 0, Some(&menu_status_subtitle(state)), None);
        }
        self.sync_native_input_items();
    }

    fn refresh_settings(&self) {
        let Some(state) = &self.ui_state else {
            return;
        };
        if let Some(settings) = &self.settings {
            refresh_webview(&settings.webview, state);
        }
    }

    fn refresh_async(&mut self, proxy: EventLoopProxy<UserEvent>) {
        if self.refresh_in_flight.is_some()
            || self
                .ui_state
                .as_ref()
                .is_some_and(|state| state.operation.status == OperationStatus::Switching)
        {
            self.refresh_pending = true;
            return;
        }
        self.refresh_pending = false;
        self.full_refresh_sequence += 1;
        let sequence = self.full_refresh_sequence;
        self.refresh_in_flight = Some(sequence);
        let config_path = self.config_path.clone();
        let epoch = self.epoch;
        let config_revision = self.config_revision;
        thread::spawn(move || {
            let _ = proxy.send_event(UserEvent::RefreshFinished {
                epoch,
                sequence,
                config_revision,
                state: Box::new(build_state(&config_path)),
            });
        });
    }

    fn handle_refresh_finished(
        &mut self,
        mut state: UiState,
        epoch: u64,
        sequence: u64,
        config_revision: u64,
    ) {
        if sequence != self.full_refresh_sequence {
            return;
        }
        self.refresh_in_flight = None;
        if epoch != self.epoch || config_revision != self.config_revision {
            self.refresh_pending = true;
            return;
        }
        for monitor in &state.monitors {
            if monitor.connected {
                if let Some(value) = monitor.current_value {
                    self.history.observe(&monitor.selector, value);
                }
            }
        }
        state.previous_inputs = self.history.previous();
        if let Some(previous) = &self.ui_state {
            state.shortcut_error = previous.shortcut_error.clone();
            let remembered = state
                .monitors
                .iter()
                .filter(|monitor| monitor.current_value.is_none())
                .filter_map(|monitor| {
                    previous
                        .monitors
                        .iter()
                        .find(|old| old.selector == monitor.selector)
                        .and_then(|old| {
                            old.current_value
                                .map(|value| (monitor.selector.clone(), value))
                        })
                })
                .collect::<Vec<_>>();
            for (selector, value) in remembered {
                state.mark_switch_sent(&selector, value);
            }
        }
        self.ui_state = Some(state);
        self.rebuild_native_menu();
        self.refresh_settings();
    }

    fn sync_native_input_items(&self) {
        for item in self.native_input_items.values() {
            item.set_enabled(true);
        }
        let Some(state) = &self.ui_state else {
            return;
        };
        if let Some(menu) = &self.native_menu {
            for item in menu.items() {
                if let MenuItemKind::MenuItem(item) = item {
                    if let Some(index) = item
                        .id()
                        .as_ref()
                        .strip_prefix("previous:")
                        .and_then(|value| value.parse::<usize>().ok())
                    {
                        item.set_enabled(
                            state.operation.status != OperationStatus::Switching
                                && state.monitors.get(index).is_some_and(|monitor| {
                                    state.previous_inputs.contains_key(&monitor.selector)
                                }),
                        );
                    }
                }
            }
        }
        if let Some(tray) = &self.tray_icon {
            let _ = tray.set_tooltip(Some(
                state
                    .operation
                    .message
                    .as_deref()
                    .unwrap_or("Monitor Switch"),
            ));
        }
        for (monitor_index, monitor) in state.monitors.iter().enumerate() {
            for input in &monitor.inputs {
                let id = format!("input:{monitor_index}:{:02x}", input.value);
                if let Some(item) = self.native_input_items.get(&id) {
                    item.set_enabled(state.operation.status != OperationStatus::Switching);
                }
            }
        }
    }

    fn evaluate_settings(&self, script: &str) {
        if let Some(settings) = &self.settings {
            let _ = settings.webview.evaluate_script(script);
        }
    }

    fn handle_switch_finished(
        &mut self,
        epoch: u64,
        value: u16,
        display: String,
        result: std::result::Result<(), String>,
        proxy: EventLoopProxy<UserEvent>,
    ) {
        if epoch != self.epoch {
            return;
        }
        match result {
            Ok(()) => {
                self.history.observe(&display, value);
                if let Some(state) = &mut self.ui_state {
                    state.previous_inputs = self.history.previous();
                    state.mark_switch_sent(&display, value);
                    state.operation.status = OperationStatus::Success;
                    state.operation.target_value = Some(value);
                    state.operation.target_display = Some(display);
                }
            }
            Err(message) => {
                if let Some(state) = &mut self.ui_state {
                    state.operation.status = OperationStatus::Error;
                    state.operation.target_value = Some(value);
                    state.operation.target_display = Some(display);
                    state.operation.message = Some(message);
                }
            }
        }
        self.rebuild_native_menu();
        self.refresh_settings();
        if self.refresh_pending {
            self.refresh_async(proxy);
        }
    }

    fn handle_close(&self, window_id: WindowId, control_flow: &mut ControlFlow) {
        if let Some(settings) = &self.settings {
            if settings.window.id() == window_id {
                let _ = settings.webview.evaluate_script(
                    "window.monitorSwitchRequestClose && window.monitorSwitchRequestClose();",
                );
                return;
            }
        }

        *control_flow = ControlFlow::Exit;
    }
}

fn menu_signature(state: &UiState) -> Vec<String> {
    let operation = if state.operation.status == OperationStatus::Error {
        format!(
            "error:{}",
            state.operation.message.as_deref().unwrap_or("Switch failed")
        )
    } else {
        "ready".to_string()
    };
    std::iter::once(operation)
        .chain(
            state
                .monitors
                .iter()
                .enumerate()
                .flat_map(|(monitor_index, monitor)| {
                    let mut entries = vec![format!(
                        "monitor:{monitor_index}:{}:{}",
                        monitor.selector, monitor.name
                    )];
                    entries.push(format!(
                        "previous:{:?}",
                        state.previous_inputs.get(&monitor.selector)
                    ));
                    entries.push(format!("shortcuts:{:?}", state.shortcuts));
                    entries.extend(monitor.inputs.iter().map(|input| {
                        format!(
                            "input:{monitor_index}:{:02x}:{}:{}",
                            input.value, input.name, input.subtitle
                        )
                    }));
                    entries
                }),
        )
        .collect()
}

fn custom_label(value: u16, label: &str) -> Option<String> {
    let label = label.trim();
    (!label.is_empty() && label != monitor_switch::ui::physical_name(value))
        .then(|| label.to_string())
}

fn migrate_legacy_config(config_path: &Path) -> Result<()> {
    let Ok(mut config) = Config::load(config_path) else {
        return Ok(());
    };
    let selectors = monitor_switch::backends::list_displays()
        .into_iter()
        .map(|display| display.selector)
        .collect::<Vec<_>>();
    if config.migrate_to_native(&selectors) {
        let backup = config_path.with_extension("v1.toml.bak");
        if !backup.exists() {
            std::fs::copy(config_path, &backup)?;
        }
        config.save(config_path)?;
    }
    Ok(())
}

fn main() {
    let config_path = default_config_path();
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let activation_proxy = proxy.clone();
    let instance = match Instance::acquire(&config_path, move || {
        let _ = activation_proxy.send_event(UserEvent::ShowSettings);
    }) {
        Ok(Acquisition::Primary(instance)) => instance,
        Ok(Acquisition::Secondary) => return,
        Err(error) => {
            eprintln!("failed to acquire application instance: {error}");
            return;
        }
    };
    let hotkey_proxy = proxy.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state == HotKeyState::Released {
            let _ = hotkey_proxy.send_event(UserEvent::HotKey(event.id));
        }
    }));

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::NativeMenu(event));
    }));

    let mut state = AppState::new(config_path);

    event_loop.run(move |event, target, control_flow| {
        let _keep_instance_alive = &instance;
        *control_flow = ControlFlow::Wait;

        match event {
            Event::UserEvent(UserEvent::ShowSettings) => {
                if let Err(error) = state.open_settings(target, proxy.clone()) {
                    eprintln!("failed to open settings: {error}");
                }
            }
            Event::UserEvent(UserEvent::Desktop(DesktopEvent::MenuClosed)) => {
                if state.menu_rebuild_pending {
                    state.rebuild_native_menu();
                }
            }
            Event::UserEvent(UserEvent::Desktop(DesktopEvent::DisplaysChanged)) => {
                state.display_refresh_due = Some(Instant::now() + Duration::from_millis(700));
            }
            Event::UserEvent(UserEvent::PreviousReady {
                epoch,
                display,
                current,
            }) => state.handle_previous_ready(epoch, display, current, proxy.clone()),
            Event::UserEvent(UserEvent::HotKey(id)) => state.handle_hotkey(id, proxy.clone()),
            Event::NewEvents(StartCause::Init) => {
                if let Err(error) = state.init(target, proxy.clone()) {
                    eprintln!("failed to initialize app: {error}");
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(UserEvent::PrewarmSettings) => {
                state.prewarm_settings(target, proxy.clone());
            }
            Event::UserEvent(UserEvent::NativeMenu(event)) => {
                state.handle_native_menu(&event.id, target, proxy.clone(), control_flow);
            }
            Event::UserEvent(UserEvent::Ipc(body)) => {
                state.handle_ipc(&body, proxy.clone());
            }
            Event::UserEvent(UserEvent::SwitchFinished {
                epoch,
                value,
                display,
                result,
            }) => {
                state.handle_switch_finished(epoch, value, display, result, proxy.clone());
            }
            Event::UserEvent(UserEvent::RefreshFinished {
                epoch,
                sequence,
                config_revision,
                state: ui_state,
            }) => {
                state.handle_refresh_finished(*ui_state, epoch, sequence, config_revision);
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(false),
                window_id,
                ..
            } => {
                if state
                    .settings
                    .as_ref()
                    .is_some_and(|settings| settings.window.id() == window_id)
                {
                    state.set_shortcut_recording(false);
                    state.refresh_settings();
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => state.handle_close(window_id, control_flow),
            _ => {}
        }
        if *control_flow != ControlFlow::Exit {
            if state.refresh_pending
                && state.refresh_in_flight.is_none()
                && !state
                    .ui_state
                    .as_ref()
                    .is_some_and(|state| state.operation.status == OperationStatus::Switching)
            {
                state.refresh_async(proxy.clone());
            }
            if state
                .display_refresh_due
                .is_some_and(|due| due <= Instant::now())
            {
                state.display_refresh_due = None;
                state.refresh_async(proxy.clone());
            }
            if let Some(due) = state.display_refresh_due {
                *control_flow = ControlFlow::WaitUntil(due);
            }
        }
    });
}

#[cfg(target_os = "macos")]
fn configure_macos_application() {
    use objc2::{class, msg_send, runtime::AnyObject};

    let app: *mut AnyObject = unsafe { msg_send![class!(NSApplication), sharedApplication] };
    if !app.is_null() {
        let _: bool = unsafe { msg_send![app, setActivationPolicy: 1isize] };
    }
}

#[cfg(not(target_os = "macos"))]
fn configure_macos_application() {}

#[cfg(target_os = "macos")]
fn activate_macos_application() {
    use objc2::{msg_send, runtime::AnyObject};

    let app: *mut AnyObject = unsafe { msg_send![objc2::class!(NSApplication), sharedApplication] };
    if !app.is_null() {
        let _: () = unsafe { msg_send![app, activateIgnoringOtherApps: true] };
    }
}

#[cfg(not(target_os = "macos"))]
fn activate_macos_application() {}

#[cfg(target_os = "macos")]
fn configure_macos_tray_symbol(tray_icon: &TrayIcon) {
    use std::ffi::CString;

    use objc2::{class, msg_send, runtime::AnyObject};

    let Some(status_item) = tray_icon.ns_status_item() else {
        return;
    };
    let status_item = (&**status_item as *const _) as *mut AnyObject;
    let button: *mut AnyObject = unsafe { msg_send![status_item, button] };
    if button.is_null() {
        return;
    }

    for symbol in ["rectangle.2.swap", "display.2", "display"] {
        let Ok(symbol) = CString::new(symbol) else {
            continue;
        };
        let name: *mut AnyObject =
            unsafe { msg_send![class!(NSString), stringWithUTF8String: symbol.as_ptr()] };
        let image: *mut AnyObject = unsafe {
            msg_send![class!(NSImage), imageWithSystemSymbolName: name, accessibilityDescription: std::ptr::null::<AnyObject>()]
        };
        if !image.is_null() {
            let _: () = unsafe { msg_send![image, setTemplate: true] };
            let _: () = unsafe { msg_send![button, setImage: image] };
            break;
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn configure_macos_tray_symbol(_tray_icon: &TrayIcon) {}

fn build_native_menu(
    state: &UiState,
) -> (
    Menu,
    HashMap<String, NativeMenuAction>,
    HashMap<String, MenuItem>,
) {
    let menu = Menu::new();
    let (actions, input_items) = populate_native_menu(&menu, state);
    (menu, actions, input_items)
}

fn populate_native_menu(
    menu: &Menu,
    state: &UiState,
) -> (HashMap<String, NativeMenuAction>, HashMap<String, MenuItem>) {
    let mut actions = HashMap::new();
    let mut input_items = HashMap::new();
    let status_subtitle = menu_status_subtitle(state);

    let header = MenuItem::new("Monitor Switch", false, None);
    let _ = menu.append(&header);
    decorate_native_menu_item(menu, 0, Some(&status_subtitle), Some("display"));
    let _ = menu.append(&PredefinedMenuItem::separator());

    for (monitor_index, monitor) in state.monitors.iter().enumerate() {
        if state.monitors.len() > 1 {
            let monitor_item = MenuItem::new(monitor.name.replace('&', "&&"), false, None);
            let _ = menu.append(&monitor_item);
            decorate_native_menu_item(
                menu,
                menu.items().len() - 1,
                Some(&monitor.subtitle),
                Some("display"),
            );
        }

        for input in &monitor.inputs {
            let id = format!("input:{monitor_index}:{:02x}", input.value);
            let binding = state.shortcuts.iter().find(|binding| {
                binding.display == monitor.selector && binding.value == Some(input.value)
            });
            let title = if cfg!(target_os = "windows") {
                binding
                    .map(|binding| {
                        format!(
                            "{}    {}",
                            input.name,
                            monitor_switch::shortcuts::label(&binding.shortcut)
                        )
                    })
                    .unwrap_or_else(|| input.name.clone())
            } else {
                input.name.clone()
            };
            let item = MenuItem::with_id(
                id.clone(),
                title.replace('&', "&&"),
                state.operation.status != OperationStatus::Switching,
                None,
            );
            let _ = menu.append(&item);
            input_items.insert(id.clone(), item.clone());
            let index = menu.items().len() - 1;
            let subtitle = if input.subtitle == "Not configured" {
                input.hex.as_str()
            } else {
                input.subtitle.as_str()
            };
            let hint = state
                .shortcuts
                .iter()
                .find(|binding| {
                    binding.display == monitor.selector && binding.value == Some(input.value)
                })
                .map(|binding| {
                    format!(
                        "{subtitle} · {}",
                        monitor_switch::shortcuts::label(&binding.shortcut)
                    )
                })
                .unwrap_or_else(|| subtitle.to_owned());
            decorate_native_menu_item(menu, index, Some(&hint), Some("cable.connector"));
            actions.insert(
                id,
                NativeMenuAction::Switch {
                    value: input.value,
                    display: monitor.selector.clone(),
                    profile: input.profile_name.clone(),
                },
            );
        }

        let previous = state.previous_inputs.get(&monitor.selector);
        let label = previous
            .map(|value| {
                let name = monitor
                    .available_inputs
                    .iter()
                    .find(|input| input.value == *value)
                    .map(|input| input.name.clone())
                    .unwrap_or_else(|| monitor_switch::ui::physical_name(*value).to_owned());
                format!("Switch back to {name}")
            })
            .unwrap_or_else(|| "Switch back to previous input".into());
        let previous_id = format!("previous:{monitor_index}");
        let shortcut = state
            .shortcuts
            .iter()
            .find(|binding| binding.display == monitor.selector && binding.value.is_none())
            .map(|binding| monitor_switch::shortcuts::label(&binding.shortcut));
        let label = if cfg!(target_os = "windows") {
            shortcut
                .as_ref()
                .map(|shortcut| format!("{label}    {shortcut}"))
                .unwrap_or(label)
        } else {
            label
        };
        let item = MenuItem::with_id(
            previous_id.clone(),
            label.replace('&', "&&"),
            previous.is_some(),
            None,
        );
        let _ = menu.append(&item);
        decorate_native_menu_item(
            menu,
            menu.items().len() - 1,
            shortcut.as_deref(),
            Some("arrow.uturn.backward"),
        );
        actions.insert(
            previous_id,
            NativeMenuAction::Previous(monitor.selector.clone()),
        );

        if monitor_index + 1 < state.monitors.len() {
            let _ = menu.append(&PredefinedMenuItem::separator());
        }
    }

    let _ = menu.append(&PredefinedMenuItem::separator());
    let refresh = MenuItem::with_id("refresh", "Refresh", true, None);
    let _ = menu.append(&refresh);
    decorate_native_menu_item(menu, menu.items().len() - 1, None, Some("arrow.clockwise"));
    actions.insert("refresh".to_string(), NativeMenuAction::Refresh);

    let settings = MenuItem::with_id("settings", "Settings…", true, None);
    let _ = menu.append(&settings);
    decorate_native_menu_item(menu, menu.items().len() - 1, None, Some("gearshape"));
    actions.insert("settings".to_string(), NativeMenuAction::OpenSettings);

    let _ = menu.append(&PredefinedMenuItem::separator());
    let quit = MenuItem::with_id("quit", "Quit Monitor Switch", true, None);
    let _ = menu.append(&quit);
    actions.insert("quit".to_string(), NativeMenuAction::Quit);

    (actions, input_items)
}

fn menu_status_subtitle(state: &UiState) -> String {
    if let Some(error) = &state.config_error {
        return error.clone();
    }
    match state.operation.status {
        OperationStatus::Error => state
            .operation
            .message
            .as_deref()
            .unwrap_or("Switch failed")
            .to_string(),
        OperationStatus::Switching => format!("{} · Sending switch command", state.monitor.name),
        _ => state.monitor.name.clone(),
    }
}

#[cfg(target_os = "windows")]
fn decorate_native_menu_item(
    _menu: &Menu,
    _index: usize,
    _subtitle: Option<&str>,
    _system_symbol: Option<&str>,
) {
}

#[cfg(target_os = "macos")]
fn decorate_native_menu_item(
    menu: &Menu,
    index: usize,
    subtitle: Option<&str>,
    system_symbol: Option<&str>,
) {
    use std::ffi::CString;

    use objc2::{class, msg_send, runtime::AnyObject, sel};

    let ns_menu = menu.ns_menu() as *mut AnyObject;
    if ns_menu.is_null() {
        return;
    }
    let item: *mut AnyObject = unsafe { msg_send![ns_menu, itemAtIndex: index as isize] };
    if item.is_null() {
        return;
    }

    if let Some(subtitle) = subtitle.and_then(|value| CString::new(value).ok()) {
        let supports_subtitle: bool =
            unsafe { msg_send![item, respondsToSelector: sel!(setSubtitle:)] };
        if supports_subtitle {
            let value: *mut AnyObject =
                unsafe { msg_send![class!(NSString), stringWithUTF8String: subtitle.as_ptr()] };
            let _: () = unsafe { msg_send![item, setSubtitle: value] };
        }
    }

    if let Some(symbol) = system_symbol.and_then(|value| CString::new(value).ok()) {
        let name: *mut AnyObject =
            unsafe { msg_send![class!(NSString), stringWithUTF8String: symbol.as_ptr()] };
        let image: *mut AnyObject = unsafe {
            msg_send![class!(NSImage), imageWithSystemSymbolName: name, accessibilityDescription: std::ptr::null::<AnyObject>()]
        };
        if !image.is_null() {
            let _: () = unsafe { msg_send![image, setTemplate: true] };
            let _: () = unsafe { msg_send![item, setImage: image] };
        }
    }
}

fn create_settings(
    target: &EventLoopWindowTarget<UserEvent>,
    proxy: EventLoopProxy<UserEvent>,
    state: &UiState,
) -> std::result::Result<WindowBundle, Box<dyn Error>> {
    let (width, height) = settings_dimensions();
    let window = WindowBuilder::new()
        .with_title("Monitor Switch Settings")
        .with_inner_size(LogicalSize::new(width, height))
        .with_min_inner_size(LogicalSize::new(560.0, 400.0))
        .with_resizable(true)
        .with_visible(false)
        .build(target)?;
    let webview = build_webview(&window, proxy, settings_html(state))?;
    Ok(WindowBundle { window, webview })
}

fn build_webview(
    window: &Window,
    proxy: EventLoopProxy<UserEvent>,
    html: String,
) -> wry::Result<WebView> {
    let background = if cfg!(target_os = "macos") {
        (242, 242, 244, 255)
    } else {
        (247, 247, 247, 255)
    };
    WebViewBuilder::new()
        .with_html(html)
        .with_ipc_handler(move |request| {
            let _ = proxy.send_event(UserEvent::Ipc(request.body().clone()));
        })
        .with_background_color(background)
        .build(window)
}

fn settings_dimensions() -> (f64, f64) {
    if cfg!(target_os = "windows") {
        (680.0, 460.0)
    } else {
        (660.0, 500.0)
    }
}

fn refresh_webview(webview: &WebView, state: &UiState) {
    if let Ok(json) = serde_json::to_string(state) {
        let script = format!("window.monitorSwitchRender && window.monitorSwitchRender({json});");
        let _ = webview.evaluate_script(&script);
    }
}

fn switch_input(
    config_path: &Path,
    value: u16,
    display: Option<String>,
    profile_name: Option<String>,
) -> Result<()> {
    switch_input_with(config_path, value, display, profile_name, |request| {
        set_input_source(request).map(|_| ())
    })
}

fn switch_input_with(
    config_path: &Path,
    value: u16,
    display: Option<String>,
    profile_name: Option<String>,
    send: impl FnOnce(SwitchRequest<'_>) -> Result<()>,
) -> Result<()> {
    let config = Config::load_or_default(config_path)?;
    let profile = profile_name
        .as_deref()
        .and_then(|name| config.find_profile(name))
        .or_else(|| {
            config.profiles.iter().find(|profile| {
                profile.value == value
                    && display.as_deref().is_none_or(|selector| {
                        profile.display.as_deref().unwrap_or(selector) == selector
                    })
            })
        });

    send(SwitchRequest {
        backend: config.effective_backend(profile),
        display: display
            .as_deref()
            .or_else(|| config.effective_display(profile)),
        value: profile.map_or(value, |profile| profile.value),
        dry_run: false,
    })
}

#[cfg(target_os = "windows")]
fn build_icon() -> std::result::Result<Icon, tray_icon::BadIcon> {
    Icon::from_rgba(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/packaging/windows/TrayIcon32.rgba"
        ))
        .to_vec(),
        32,
        32,
    )
}

#[cfg(not(target_os = "windows"))]
fn build_icon() -> std::result::Result<Icon, tray_icon::BadIcon> {
    const WIDTH: u32 = 18;
    const HEIGHT: u32 = 18;
    let mut rgba = vec![0; (WIDTH * HEIGHT * 4) as usize];
    let color = if cfg!(target_os = "windows") {
        (36, 122, 218)
    } else {
        (0, 0, 0)
    };

    for y in 3..12 {
        for x in 2..16 {
            let border = x == 2 || x == 15 || y == 3 || y == 11;
            if border {
                set_pixel(&mut rgba, WIDTH, x, y, color, 255);
            }
        }
    }
    for y in 13..15 {
        for x in 7..11 {
            set_pixel(&mut rgba, WIDTH, x, y, color, 255);
        }
    }
    for x in 5..13 {
        set_pixel(&mut rgba, WIDTH, x, 15, color, 255);
    }

    Icon::from_rgba(rgba, WIDTH, HEIGHT)
}

#[cfg(not(target_os = "windows"))]
fn set_pixel(rgba: &mut [u8], width: u32, x: u32, y: u32, color: (u8, u8, u8), alpha: u8) {
    let index = ((y * width + x) * 4) as usize;
    rgba[index] = color.0;
    rgba[index + 1] = color.1;
    rgba[index + 2] = color.2;
    rgba[index + 3] = alpha;
}

#[cfg(test)]
mod tests {
    use super::{AppState, LabelUpdate};
    use monitor_switch::config::Config;

    #[test]
    fn refresh_from_before_save_is_discarded_and_queued_again() {
        let path = std::path::PathBuf::from("/nonexistent/monitor-switch-test.toml");
        let stale = monitor_switch::ui::build_cached_state(&path);
        let mut app = AppState::new(path);
        app.ui_state = Some(stale.clone());
        app.config_revision = 1;
        app.full_refresh_sequence = 1;
        app.refresh_in_flight = Some(1);
        app.handle_refresh_finished(stale, 0, 1, 0);
        assert!(app.refresh_in_flight.is_none());
        assert!(app.refresh_pending);
    }

    #[test]
    fn refresh_interrupted_by_switch_does_not_lose_rediscovery() {
        let path = std::path::PathBuf::from("/nonexistent/monitor-switch-test.toml");
        let stale = monitor_switch::ui::build_cached_state(&path);
        let mut app = AppState::new(path);
        app.epoch = 1;
        app.full_refresh_sequence = 1;
        app.refresh_in_flight = Some(1);
        app.handle_refresh_finished(stale, 0, 1, 0);
        assert!(app.refresh_pending);
        assert!(app.refresh_in_flight.is_none());
    }

    #[test]
    fn first_switch_without_configuration_reaches_backend() {
        let path = std::env::temp_dir().join(format!(
            "monitor-switch-first-run-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut sent = false;
        super::switch_input_with(&path, 0x11, Some("display-a".into()), None, |request| {
            sent = true;
            assert_eq!(request.value, 0x11);
            assert_eq!(request.display, Some("display-a"));
            assert_eq!(request.backend, monitor_switch::config::BackendKind::Auto);
            assert!(!request.dry_run);
            Ok(())
        })
        .unwrap();
        assert!(sent);
        assert!(!path.exists());
    }

    #[test]
    fn malformed_configuration_does_not_send_switch_or_get_overwritten() {
        let path = std::env::temp_dir().join(format!(
            "monitor-switch-invalid-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let invalid = "default-backend = [";
        std::fs::write(&path, invalid).unwrap();
        let result = super::switch_input_with(&path, 0x11, None, None, |_| {
            panic!("invalid config must not reach the backend")
        });
        assert!(result.is_err());
        assert!(Config::load_or_default(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), invalid);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn stale_full_refresh_cannot_replace_new_shortcut_settings() {
        let path = std::path::PathBuf::from("/nonexistent/monitor-switch-test.toml");
        let stale = monitor_switch::ui::build_cached_state(&path);
        let mut current = stale.clone();
        current
            .shortcuts
            .push(monitor_switch::shortcuts::ShortcutBinding {
                display: String::new(),
                value: Some(17),
                shortcut: "Control+Alt+Digit1".into(),
            });
        let mut app = AppState::new(path);
        app.ui_state = Some(current);
        app.full_refresh_sequence = 2;
        app.handle_refresh_finished(stale, 0, 1, 0);
        assert_eq!(app.ui_state.as_ref().unwrap().shortcuts.len(), 1);
    }

    #[test]
    fn hiding_reported_input_creates_profile_without_forcing_backend() {
        let path = std::env::temp_dir().join(format!(
            "monitor-switch-save-{}-{}.toml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = AppState::new(path.clone());
        app.save_labels(
            vec![LabelUpdate {
                value: 0x11,
                display: String::new(),
                display_index: 0,
                label: monitor_switch::ui::physical_name(0x11).to_string(),
                show_in_menu: false,
            }],
            None,
        )
        .unwrap();

        let config = Config::load(&path).unwrap();
        assert_eq!(config.profiles.len(), 1);
        assert!(!config.profiles[0].show_in_menu);
        assert_eq!(config.profiles[0].backend, None);
        std::fs::remove_file(path).unwrap();
    }
}
