#[cfg(target_os = "macos")]
use std::collections::HashMap;
use std::{
    error::Error,
    path::{Path, PathBuf},
    thread,
};

#[cfg(not(target_os = "macos"))]
use monitor_switch::app_html::popover_html;
use monitor_switch::{
    app_html::settings_html,
    backends::{set_input_source, SwitchRequest},
    config::{default_config_path, Config, SourceProfile},
    error::Result,
    ui::{build_state, OperationStatus, UiState},
};
use serde::Deserialize;
#[cfg(not(target_os = "macos"))]
use tao::dpi::PhysicalPosition;
use tao::{
    dpi::LogicalSize,
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    window::{Window, WindowBuilder, WindowId},
};
#[cfg(target_os = "macos")]
use tray_icon::menu::{
    CheckMenuItem, ContextMenu, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem,
};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
#[cfg(not(target_os = "macos"))]
use tray_icon::{MouseButtonState, Rect, TrayIconEvent};
use wry::{WebView, WebViewBuilder};

#[derive(Debug, Clone)]
enum UserEvent {
    #[cfg(not(target_os = "macos"))]
    Tray(Rect),
    #[cfg(target_os = "macos")]
    NativeMenu(MenuEvent),
    #[cfg(not(target_os = "macos"))]
    HidePopoverIfUnfocused(u64),
    Ipc(String),
    SwitchFinished {
        value: u16,
        display: String,
        result: std::result::Result<UiState, String>,
    },
    RefreshFinished(UiState),
}

#[cfg(target_os = "macos")]
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
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum IpcCommand {
    #[serde(rename = "switchInput")]
    SwitchInput {
        value: Option<u16>,
        display: Option<String>,
        profile: Option<String>,
    },
    #[serde(rename = "openSettings")]
    OpenSettings,
    #[serde(rename = "closeSettings")]
    CloseSettings,
    #[serde(rename = "saveLabels")]
    SaveLabels { items: Vec<LabelUpdate> },
    #[serde(rename = "quit")]
    Quit,
}

struct WindowBundle {
    window: Window,
    webview: WebView,
}

struct AppState {
    config_path: PathBuf,
    tray_icon: Option<TrayIcon>,
    popover: Option<WindowBundle>,
    settings: Option<WindowBundle>,
    ui_state: Option<UiState>,
    #[cfg(not(target_os = "macos"))]
    popover_epoch: u64,
    #[cfg(target_os = "macos")]
    native_menu_actions: HashMap<String, NativeMenuAction>,
}

impl AppState {
    fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            tray_icon: None,
            popover: None,
            settings: None,
            ui_state: None,
            #[cfg(not(target_os = "macos"))]
            popover_epoch: 0,
            #[cfg(target_os = "macos")]
            native_menu_actions: HashMap::new(),
        }
    }

    fn init(
        &mut self,
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> std::result::Result<(), Box<dyn Error>> {
        #[cfg(target_os = "macos")]
        let _ = (target, &proxy);
        configure_macos_application();
        if let Err(error) = migrate_legacy_config(&self.config_path) {
            eprintln!("failed to migrate legacy configuration: {error}");
        }
        let state = build_state(&self.config_path);
        let mut tray_builder = TrayIconBuilder::new()
            .with_tooltip("Monitor Switch")
            .with_icon(build_icon()?)
            .with_icon_as_template(true);

        #[cfg(target_os = "macos")]
        {
            let (menu, actions) = build_native_menu(&state);
            self.native_menu_actions = actions;
            tray_builder = tray_builder
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(true)
                .with_menu_on_right_click(true);
        }

        let tray_icon = tray_builder.build()?;
        configure_macos_tray_symbol(&tray_icon);
        self.tray_icon = Some(tray_icon);

        #[cfg(not(target_os = "macos"))]
        {
            self.popover = Some(create_popover(target, proxy, &state)?);
        }
        self.ui_state = Some(state);
        Ok(())
    }

    #[cfg(target_os = "macos")]
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

    #[cfg(not(target_os = "macos"))]
    fn handle_tray_event(&mut self, rect: Rect, proxy: EventLoopProxy<UserEvent>) {
        self.popover_epoch = self.popover_epoch.wrapping_add(1);
        let Some(popover) = &self.popover else {
            return;
        };

        let visible = popover.window.is_visible();
        if visible {
            popover.window.set_visible(false);
        } else {
            place_popover_at_rect(&popover.window, rect);
            popover.window.set_visible(true);
            popover.window.set_focus();
            self.refresh_async(proxy);
        }
    }

    fn handle_ipc(
        &mut self,
        body: &str,
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
        control_flow: &mut ControlFlow,
    ) {
        let command = match serde_json::from_str::<IpcCommand>(body) {
            Ok(command) => command,
            Err(error) => {
                eprintln!("invalid IPC command: {error}; body={body}");
                return;
            }
        };

        match command {
            IpcCommand::SwitchInput {
                value,
                display,
                profile,
            } => {
                self.begin_switch(value, display, profile, proxy);
            }
            IpcCommand::OpenSettings => {
                if let Some(popover) = &self.popover {
                    popover.window.set_visible(false);
                }
                if let Err(error) = self.open_settings(target, proxy) {
                    eprintln!("failed to open settings: {error}");
                }
            }
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
            IpcCommand::Quit => *control_flow = ControlFlow::Exit,
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
                .map(|_| {
                    let mut state = build_state(&config_path);
                    for monitor in &mut state.monitors {
                        if monitor.selector == display_for_event {
                            monitor.current_value = Some(value);
                            monitor.current_hex = Some(format!("0x{value:02x}"));
                            for input in &mut monitor.inputs {
                                input.current = input.value == value;
                            }
                            monitor.available_inputs = monitor.inputs.clone();
                        }
                    }
                    if let Some(first) = state.monitors.first().cloned() {
                        state.monitor = first.clone();
                        state.inputs = first.inputs;
                        state.available_inputs = first.available_inputs;
                    }
                    state.operation.status = OperationStatus::Success;
                    state.operation.target_value = Some(value);
                    state.operation.target_display = Some(display_for_event.clone());
                    state
                })
                .map_err(|error| error.to_string());
            let _ = proxy.send_event(UserEvent::SwitchFinished {
                value,
                display: display_for_event,
                result,
            });
        });
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
                profile.label = (!label.is_empty()).then(|| label.to_string());
            } else if !label.is_empty() {
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
                    label: Some(label.to_string()),
                    display: (!item.display.is_empty()).then_some(item.display.clone()),
                    backend: Some(monitor_switch::config::BackendKind::Native),
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

        #[cfg(target_os = "macos")]
        if let Some(tray_icon) = &self.tray_icon {
            let (menu, actions) = build_native_menu(state);
            self.native_menu_actions = actions;
            tray_icon.set_menu(Some(Box::new(menu)));
        }

        #[cfg(not(target_os = "macos"))]
        if let Some(popover) = &self.popover {
            let (width, height) = popover_dimensions(state);
            let current = popover
                .window
                .inner_size()
                .to_logical::<f64>(popover.window.scale_factor());
            if (current.width - width).abs() > 0.5 || (current.height - height).abs() > 0.5 {
                popover
                    .window
                    .set_inner_size(LogicalSize::new(width, height));
            }
            refresh_webview(&popover.webview, state);
        }
        if let Some(settings) = &self.settings {
            refresh_webview(&settings.webview, state);
        }
    }

    fn refresh_async(&self, proxy: EventLoopProxy<UserEvent>) {
        let config_path = self.config_path.clone();
        thread::spawn(move || {
            let _ = proxy.send_event(UserEvent::RefreshFinished(build_state(&config_path)));
        });
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
        result: std::result::Result<UiState, String>,
    ) {
        match result {
            Ok(state) => self.ui_state = Some(state),
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

        if let Some(popover) = &self.popover {
            if popover.window.id() == window_id {
                popover.window.set_visible(false);
                return;
            }
        }

        *control_flow = ControlFlow::Exit;
    }

    #[cfg(not(target_os = "macos"))]
    fn handle_focus(&self, window_id: WindowId, focused: bool, proxy: EventLoopProxy<UserEvent>) {
        if focused {
            return;
        }
        if let Some(popover) = &self.popover {
            if popover.window.id() == window_id {
                let epoch = self.popover_epoch;
                thread::spawn(move || {
                    thread::sleep(std::time::Duration::from_millis(140));
                    let _ = proxy.send_event(UserEvent::HidePopoverIfUnfocused(epoch));
                });
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn hide_popover_if_unfocused(&self, epoch: u64) {
        if epoch != self.popover_epoch {
            return;
        }
        if let Some(popover) = &self.popover {
            if !popover.window.is_focused() {
                popover.window.set_visible(false);
            }
        }
    }
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

    #[cfg(not(target_os = "macos"))]
    let tray_proxy = proxy.clone();
    #[cfg(not(target_os = "macos"))]
    TrayIconEvent::set_event_handler(Some(move |event| {
        if is_tray_activation(&event) {
            let _ = tray_proxy.send_event(UserEvent::Tray(tray_event_rect(&event)));
        }
    }));

    #[cfg(target_os = "macos")]
    let menu_proxy = proxy.clone();
    #[cfg(target_os = "macos")]
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::NativeMenu(event));
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
            #[cfg(not(target_os = "macos"))]
            Event::UserEvent(UserEvent::Tray(rect)) => state.handle_tray_event(rect, proxy.clone()),
            #[cfg(target_os = "macos")]
            Event::UserEvent(UserEvent::NativeMenu(event)) => {
                state.handle_native_menu(&event.id, target, proxy.clone(), control_flow);
            }
            #[cfg(not(target_os = "macos"))]
            Event::UserEvent(UserEvent::HidePopoverIfUnfocused(epoch)) => {
                state.hide_popover_if_unfocused(epoch);
            }
            Event::UserEvent(UserEvent::Ipc(body)) => {
                state.handle_ipc(&body, target, proxy.clone(), control_flow);
            }
            Event::UserEvent(UserEvent::SwitchFinished {
                value,
                display,
                result,
            }) => {
                state.handle_switch_finished(value, display, result);
            }
            Event::UserEvent(UserEvent::RefreshFinished(ui_state)) => {
                state.ui_state = Some(ui_state);
                state.render_views();
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => state.handle_close(window_id, control_flow),
            #[cfg(not(target_os = "macos"))]
            Event::WindowEvent {
                event: WindowEvent::Focused(focused),
                window_id,
                ..
            } => state.handle_focus(window_id, focused, proxy.clone()),
            _ => {}
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn create_popover(
    target: &EventLoopWindowTarget<UserEvent>,
    proxy: EventLoopProxy<UserEvent>,
    state: &UiState,
) -> std::result::Result<WindowBundle, Box<dyn Error>> {
    let (width, height) = popover_dimensions(state);
    let window = WindowBuilder::new()
        .with_title("Monitor Switch")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top(true)
        .with_visible(false)
        .build(target)?;
    let webview = build_webview(&window, proxy, popover_html(state), true)?;
    Ok(WindowBundle { window, webview })
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

#[cfg(target_os = "macos")]
fn build_native_menu(state: &UiState) -> (Menu, HashMap<String, NativeMenuAction>) {
    let menu = Menu::new();
    let mut actions = HashMap::new();
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

    (menu, actions)
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

#[cfg(not(target_os = "macos"))]
fn is_tray_activation(event: &TrayIconEvent) -> bool {
    matches!(
        event,
        TrayIconEvent::Click { button_state, .. } if *button_state == MouseButtonState::Down
    )
}

#[cfg(not(target_os = "macos"))]
fn tray_event_rect(event: &TrayIconEvent) -> Rect {
    match event {
        TrayIconEvent::Click { rect, .. }
        | TrayIconEvent::DoubleClick { rect, .. }
        | TrayIconEvent::Enter { rect, .. }
        | TrayIconEvent::Move { rect, .. }
        | TrayIconEvent::Leave { rect, .. } => *rect,
        _ => Rect::default(),
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
    let webview = build_webview(&window, proxy, settings_html(state), false)?;
    Ok(WindowBundle { window, webview })
}

fn build_webview(
    window: &Window,
    proxy: EventLoopProxy<UserEvent>,
    html: String,
    transparent: bool,
) -> wry::Result<WebView> {
    let builder = WebViewBuilder::new()
        .with_html(html)
        .with_ipc_handler(move |request| {
            let _ = proxy.send_event(UserEvent::Ipc(request.body().clone()));
        });
    let builder = if transparent {
        builder
            .with_transparent(true)
            .with_background_color((0, 0, 0, 0))
    } else {
        builder.with_background_color((243, 243, 243, 255))
    };
    builder.build(window)
}

#[cfg(not(target_os = "macos"))]
fn popover_dimensions(state: &UiState) -> (f64, f64) {
    let row_height = if cfg!(target_os = "windows") {
        56.0
    } else {
        44.0
    };
    let width = if cfg!(target_os = "windows") {
        380.0
    } else {
        292.0
    };
    let error_height = (state.operation.status == OperationStatus::Error) as u8 as f64 * 54.0;
    let base_height = if cfg!(target_os = "windows") {
        178.0
    } else {
        164.0
    };
    let max_height = if cfg!(target_os = "windows") {
        580.0
    } else {
        520.0
    };
    let height = (base_height + row_height * state.inputs.len() as f64 + error_height)
        .clamp(258.0, max_height);
    (width, height)
}

fn settings_dimensions() -> (f64, f64) {
    if cfg!(target_os = "windows") {
        (680.0, 460.0)
    } else {
        (620.0, 430.0)
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

#[cfg(not(target_os = "macos"))]
fn place_popover_at_rect(window: &Window, rect: Rect) {
    const GAP: i32 = 8;
    const SCREEN_MARGIN: i32 = 8;

    let window_size = window.outer_size();
    let width = window_size.width as i32;
    let height = window_size.height as i32;
    let icon_center_x = rect.position.x.round() as i32 + rect.size.width as i32 / 2;
    let icon_center_y = rect.position.y.round() as i32 + rect.size.height as i32 / 2;
    let mut x = icon_center_x - width / 2;
    let mut y = rect.position.y.round() as i32 + rect.size.height as i32 + GAP;

    let monitor = window
        .available_monitors()
        .find(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            icon_center_x >= position.x
                && icon_center_x <= position.x + size.width as i32
                && icon_center_y >= position.y
                && icon_center_y <= position.y + size.height as i32
        })
        .or_else(|| window.primary_monitor());

    if let Some(monitor) = monitor {
        let monitor_position = monitor.position();
        let monitor_size = monitor.size();
        let min_x = monitor_position.x + SCREEN_MARGIN;
        let max_x = monitor_position.x + monitor_size.width as i32 - width - SCREEN_MARGIN;
        let min_y = monitor_position.y + SCREEN_MARGIN;
        let max_y = monitor_position.y + monitor_size.height as i32 - height - SCREEN_MARGIN;

        if cfg!(target_os = "windows")
            && icon_center_y > monitor_position.y + monitor_size.height as i32 / 2
        {
            y = rect.position.y.round() as i32 - height - GAP;
        }

        x = x.clamp(min_x, max_x.max(min_x));
        y = y.clamp(min_y, max_y.max(min_y));
    }

    window.set_outer_position(PhysicalPosition::new(x, y));
}

fn build_icon() -> std::result::Result<Icon, tray_icon::BadIcon> {
    const WIDTH: u32 = 18;
    const HEIGHT: u32 = 18;
    let mut rgba = vec![0; (WIDTH * HEIGHT * 4) as usize];

    for y in 3..12 {
        for x in 2..16 {
            let border = x == 2 || x == 15 || y == 3 || y == 11;
            if border {
                set_pixel(&mut rgba, WIDTH, x, y, 255);
            }
        }
    }
    for y in 13..15 {
        for x in 7..11 {
            set_pixel(&mut rgba, WIDTH, x, y, 255);
        }
    }
    for x in 5..13 {
        set_pixel(&mut rgba, WIDTH, x, 15, 255);
    }

    Icon::from_rgba(rgba, WIDTH, HEIGHT)
}

fn set_pixel(rgba: &mut [u8], width: u32, x: u32, y: u32, alpha: u8) {
    let index = ((y * width + x) * 4) as usize;
    rgba[index + 3] = alpha;
}
