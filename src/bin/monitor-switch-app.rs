#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::collections::HashMap;
use std::{
    error::Error,
    path::{Path, PathBuf},
    thread,
};

use monitor_switch::{
    app_html::settings_html,
    backends::{get_input_source, set_input_source, ReadRequest, SwitchRequest},
    config::{default_config_path, Config, SourceProfile},
    error::Result,
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
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use wry::{WebView, WebViewBuilder};

#[derive(Debug, Clone)]
enum UserEvent {
    TrayActivated,
    #[cfg(target_os = "windows")]
    PrewarmSettings,
    NativeMenu(MenuEvent),
    Ipc(String),
    SwitchFinished {
        value: u16,
        display: String,
        result: std::result::Result<(), String>,
    },
    RefreshFinished(Box<UiState>),
    CurrentInputsRefreshed(Vec<(String, Option<u16>)>),
}

#[derive(Debug, Clone)]
enum NativeMenuAction {
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
    SaveLabels { items: Vec<LabelUpdate> },
}

struct WindowBundle {
    window: Window,
    webview: WebView,
}

struct AppState {
    config_path: PathBuf,
    tray_icon: Option<TrayIcon>,
    settings: Option<WindowBundle>,
    ui_state: Option<UiState>,
    native_menu_actions: HashMap<String, NativeMenuAction>,
    native_input_items: HashMap<String, CheckMenuItem>,
}

impl AppState {
    fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            tray_icon: None,
            settings: None,
            ui_state: None,
            native_menu_actions: HashMap::new(),
            native_input_items: HashMap::new(),
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
        let state = build_cached_state(&self.config_path);
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
        tray_builder = tray_builder
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(true)
            .with_menu_on_right_click(true);

        let tray_icon = tray_builder.build()?;
        configure_macos_tray_symbol(&tray_icon);
        self.tray_icon = Some(tray_icon);

        self.ui_state = Some(state);
        self.refresh_async(proxy.clone());
        #[cfg(target_os = "windows")]
        thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(2_500));
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
            IpcCommand::CloseSettings => {
                if let Some(settings) = &self.settings {
                    settings.window.set_visible(false);
                }
            }
            IpcCommand::SaveLabels { items } => match self.save_labels(items) {
                Ok(()) => {
                    self.refresh_async(proxy);
                    self.evaluate_settings(
                        "window.monitorSwitchSaved && window.monitorSwitchSaved();",
                    );
                }
                Err(error) => {
                    let message = serde_json::to_string(&error.to_string())
                        .unwrap_or_else(|_| "\"保存失败\"".to_string());
                    self.evaluate_settings(&format!(
                            "window.monitorSwitchSaveError && window.monitorSwitchSaveError({message});"
                        ));
                }
            },
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
        self.render_views();

        let config_path = self.config_path.clone();
        thread::spawn(move || {
            let display_for_event = display.clone().unwrap_or_default();
            let result = switch_input(&config_path, value, display, profile)
                .map(|_| ())
                .map_err(|error| error.to_string());
            let _ = proxy.send_event(UserEvent::SwitchFinished {
                value,
                display: display_for_event,
                result,
            });
        });
    }

    #[cfg(target_os = "windows")]
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

    fn save_labels(&self, items: Vec<LabelUpdate>) -> Result<()> {
        let mut config = Config::load(&self.config_path).unwrap_or_default();
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
            } else if item.show_in_menu || custom_label(item.value, label).is_some() {
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
                    backend: Some(monitor_switch::config::BackendKind::Native),
                    show_in_menu: item.show_in_menu,
                });
            }
        }
        config.save(&self.config_path)?;
        Ok(())
    }

    fn render_views(&mut self) {
        let Some(state) = &self.ui_state else {
            return;
        };

        if let Some(tray_icon) = &self.tray_icon {
            let (menu, actions, input_items) = build_native_menu(state);
            self.native_menu_actions = actions;
            self.native_input_items = input_items;
            tray_icon.set_menu(Some(Box::new(menu)));
        }

        if let Some(settings) = &self.settings {
            refresh_webview(&settings.webview, state);
        }
    }

    fn refresh_async(&self, proxy: EventLoopProxy<UserEvent>) {
        let config_path = self.config_path.clone();
        thread::spawn(move || {
            let _ = proxy.send_event(UserEvent::RefreshFinished(Box::new(build_state(
                &config_path,
            ))));
        });
    }

    fn refresh_current_inputs_async(&self, proxy: EventLoopProxy<UserEvent>) {
        let config_path = self.config_path.clone();
        let displays = self
            .ui_state
            .as_ref()
            .map(|state| {
                state
                    .monitors
                    .iter()
                    .map(|monitor| monitor.selector.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        thread::spawn(move || {
            let config = Config::load(&config_path).unwrap_or_default();
            let values = displays
                .into_iter()
                .map(|display| {
                    let current = get_input_source(ReadRequest {
                        backend: config.default_backend,
                        display: (!display.is_empty()).then_some(display.as_str()),
                    })
                    .ok();
                    (display, current)
                })
                .collect();
            let _ = proxy.send_event(UserEvent::CurrentInputsRefreshed(values));
        });
    }

    fn handle_refresh_finished(&mut self, state: UiState) {
        self.ui_state = Some(state);
        self.render_views();
    }

    fn handle_current_inputs_refreshed(&mut self, values: Vec<(String, Option<u16>)>) {
        let Some(state) = &mut self.ui_state else {
            return;
        };
        for (display, current) in values {
            if let Some(monitor) = state
                .monitors
                .iter_mut()
                .find(|monitor| monitor.selector == display)
            {
                monitor.current_value = current;
                monitor.current_hex = current.map(|value| format!("0x{value:02x}"));
                monitor.connected = current.is_some();
                for input in &mut monitor.available_inputs {
                    input.current = current == Some(input.value);
                }
                monitor.inputs = monitor
                    .available_inputs
                    .iter()
                    .filter(|input| input.show_in_menu || input.current)
                    .cloned()
                    .collect();
            }
        }
        sync_primary_monitor(state);
        self.sync_native_input_items();
        if let (Some(settings), Some(state)) = (&self.settings, &self.ui_state) {
            refresh_webview(&settings.webview, state);
        }
    }

    fn sync_native_input_items(&self) {
        for item in self.native_input_items.values() {
            item.set_checked(false);
        }
        let Some(state) = &self.ui_state else {
            return;
        };
        for (monitor_index, monitor) in state.monitors.iter().enumerate() {
            for input in &monitor.inputs {
                let id = format!("input:{monitor_index}:{:02x}", input.value);
                if let Some(item) = self.native_input_items.get(&id) {
                    item.set_checked(input.current);
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
        value: u16,
        display: String,
        result: std::result::Result<(), String>,
    ) {
        match result {
            Ok(()) => {
                if let Some(state) = &mut self.ui_state {
                    for monitor in &mut state.monitors {
                        if monitor.selector == display {
                            monitor.current_value = Some(value);
                            monitor.current_hex = Some(format!("0x{value:02x}"));
                            monitor.connected = true;
                            for input in &mut monitor.available_inputs {
                                input.current = input.value == value;
                            }
                            monitor.inputs = monitor
                                .available_inputs
                                .iter()
                                .filter(|input| input.show_in_menu || input.current)
                                .cloned()
                                .collect();
                        }
                    }
                    sync_primary_monitor(state);
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
        self.render_views();
    }

    fn handle_close(&self, window_id: WindowId, control_flow: &mut ControlFlow) {
        if let Some(settings) = &self.settings {
            if settings.window.id() == window_id {
                settings.window.set_visible(false);
                return;
            }
        }

        *control_flow = ControlFlow::Exit;
    }
}

fn sync_primary_monitor(state: &mut UiState) {
    if let Some(first) = state.monitors.first().cloned() {
        state.monitor = first.clone();
        state.inputs = first.inputs;
        state.available_inputs = first.available_inputs;
    }
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

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::NativeMenu(event));
    }));
    let tray_proxy = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |event| {
        if is_tray_activation(&event) {
            let _ = tray_proxy.send_event(UserEvent::TrayActivated);
        }
    }));

    let mut state = AppState::new(config_path);

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(StartCause::Init) => {
                if let Err(error) = state.init(target, proxy.clone()) {
                    eprintln!("failed to initialize app: {error}");
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(UserEvent::TrayActivated) => {
                state.refresh_current_inputs_async(proxy.clone());
            }
            #[cfg(target_os = "windows")]
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
                value,
                display,
                result,
            }) => {
                state.handle_switch_finished(value, display, result);
            }
            Event::UserEvent(UserEvent::RefreshFinished(ui_state)) => {
                state.handle_refresh_finished(*ui_state);
            }
            Event::UserEvent(UserEvent::CurrentInputsRefreshed(values)) => {
                state.handle_current_inputs_refreshed(values);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => state.handle_close(window_id, control_flow),
            _ => {}
        }
    });
}

fn is_tray_activation(event: &TrayIconEvent) -> bool {
    matches!(
        event,
        TrayIconEvent::Click {
            button: MouseButton::Left | MouseButton::Right,
            button_state: MouseButtonState::Down,
            ..
        }
    )
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
    HashMap<String, CheckMenuItem>,
) {
    let menu = Menu::new();
    let mut actions = HashMap::new();
    let mut input_items = HashMap::new();
    let connected = if state.monitor.connected {
        "DDC/CI 已连接"
    } else {
        "无法读取当前输入源"
    };
    let status_subtitle = match state.operation.status {
        OperationStatus::Error => state
            .operation
            .message
            .as_deref()
            .unwrap_or("切换失败")
            .to_string(),
        _ => format!("{} · {connected}", state.monitor.name),
    };

    let header = MenuItem::new("Monitor Switch", false, None);
    let _ = menu.append(&header);
    decorate_native_menu_item(&menu, 0, Some(&status_subtitle), Some("display"));
    let _ = menu.append(&PredefinedMenuItem::separator());

    for (monitor_index, monitor) in state.monitors.iter().enumerate() {
        if state.monitors.len() > 1 {
            let monitor_item = MenuItem::new(monitor.name.replace('&', "&&"), false, None);
            let _ = menu.append(&monitor_item);
            decorate_native_menu_item(
                &menu,
                menu.items().len() - 1,
                Some(&monitor.subtitle),
                Some("display"),
            );
        }

        for input in &monitor.inputs {
            let id = format!("input:{monitor_index}:{:02x}", input.value);
            let switching = state.operation.status == OperationStatus::Switching
                && state.operation.target_value == Some(input.value)
                && state.operation.target_display.as_deref() == Some(&monitor.selector);
            let item = CheckMenuItem::with_id(
                id.clone(),
                input.name.replace('&', "&&"),
                state.operation.status != OperationStatus::Switching,
                switching || input.current,
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
            decorate_native_menu_item(&menu, index, Some(subtitle), Some("cable.connector"));
            actions.insert(
                id,
                NativeMenuAction::Switch {
                    value: input.value,
                    display: monitor.selector.clone(),
                    profile: input.profile_name.clone(),
                },
            );
        }

        if monitor_index + 1 < state.monitors.len() {
            let _ = menu.append(&PredefinedMenuItem::separator());
        }
    }

    let _ = menu.append(&PredefinedMenuItem::separator());
    let refresh = MenuItem::with_id("refresh", "刷新", true, None);
    let _ = menu.append(&refresh);
    decorate_native_menu_item(&menu, menu.items().len() - 1, None, Some("arrow.clockwise"));
    actions.insert("refresh".to_string(), NativeMenuAction::Refresh);

    let settings = MenuItem::with_id("settings", "设置…", true, None);
    let _ = menu.append(&settings);
    decorate_native_menu_item(&menu, menu.items().len() - 1, None, Some("gearshape"));
    actions.insert("settings".to_string(), NativeMenuAction::OpenSettings);

    let _ = menu.append(&PredefinedMenuItem::separator());
    let quit = MenuItem::with_id("quit", "退出 Monitor Switch", true, None);
    let _ = menu.append(&quit);
    actions.insert("quit".to_string(), NativeMenuAction::Quit);

    (menu, actions, input_items)
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
        .with_min_inner_size(LogicalSize::new(width, height))
        .with_resizable(false)
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
        (236, 236, 236, 255)
    } else {
        (243, 243, 243, 255)
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
    let config = Config::load(config_path)?;
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

    set_input_source(SwitchRequest {
        backend: config.effective_backend(profile),
        display: display
            .as_deref()
            .or_else(|| config.effective_display(profile)),
        value: profile.map_or(value, |profile| profile.value),
        dry_run: false,
    })?;
    Ok(())
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
