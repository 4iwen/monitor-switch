//! Exercise native notification hooks without switching any monitor input.
use monitor_switch::desktop::{DesktopEvent, DesktopHooks};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tao::{
    event::{Event, StartCause},
    event_loop::{ControlFlow, EventLoopBuilder},
};
use tray_icon::{menu::Menu, Icon, TrayIcon, TrayIconBuilder};

fn main() {
    let event_loop = EventLoopBuilder::new().build();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if matches!(event, Event::NewEvents(StartCause::Init)) {
            if let Err(error) = check() {
                eprintln!("Native desktop checks failed: {error}");
                std::process::exit(1);
            }
            println!("Native menu lifecycle and display notification checks passed");
            *control_flow = ControlFlow::Exit;
        }
    });
}

fn check() -> Result<(), Box<dyn std::error::Error>> {
    let menu = Menu::new();
    let icon = Icon::from_rgba(vec![0; 16 * 16 * 4], 16, 16)?;
    let tray = TrayIconBuilder::new()
        .with_icon(icon)
        .with_menu(Box::new(menu.clone()))
        .build()?;
    let closes = Arc::new(AtomicUsize::new(0));
    let displays = Arc::new(AtomicUsize::new(0));
    let (closed, changed) = (closes.clone(), displays.clone());
    let hooks = DesktopHooks::new(
        &tray,
        &menu,
        Arc::new(move |event| match event {
            DesktopEvent::MenuClosed => {
                closed.fetch_add(1, Ordering::Relaxed);
            }
            DesktopEvent::DisplaysChanged => {
                changed.fetch_add(1, Ordering::Relaxed);
            }
        }),
    )?;
    notify(&tray, &menu, true);
    if !hooks.menu_is_open() {
        return Err("menu-open event was not observed".into());
    }
    notify(&tray, &menu, false);
    if hooks.menu_is_open()
        || closes.load(Ordering::Relaxed) != 1
        || displays.load(Ordering::Relaxed) < 1
    {
        return Err("menu-close or display-change event was not observed".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn notify(_tray: &TrayIcon, menu: &Menu, opened: bool) {
    use objc2::runtime::AnyObject;
    use objc2_foundation::{ns_string, NSNotificationCenter};
    use tray_icon::menu::ContextMenu;
    let center = NSNotificationCenter::defaultCenter();
    let object = unsafe { &*(menu.ns_menu() as *const AnyObject) };
    unsafe {
        center.postNotificationName_object(
            if opened {
                ns_string!("NSMenuDidBeginTrackingNotification")
            } else {
                ns_string!("NSMenuDidEndTrackingNotification")
            },
            Some(object),
        );
        if !opened {
            center.postNotificationName_object(
                ns_string!("NSApplicationDidChangeScreenParametersNotification"),
                None,
            );
        }
    }
}

#[cfg(target_os = "windows")]
fn notify(tray: &TrayIcon, _menu: &Menu, opened: bool) {
    use winapi::{
        shared::windef::HWND,
        um::winuser::{SendMessageW, WM_DISPLAYCHANGE, WM_ENTERMENULOOP, WM_EXITMENULOOP},
    };
    unsafe {
        SendMessageW(
            tray.window_handle() as HWND,
            if opened {
                WM_ENTERMENULOOP
            } else {
                WM_EXITMENULOOP
            },
            0,
            0,
        );
        if !opened {
            SendMessageW(tray.window_handle() as HWND, WM_DISPLAYCHANGE, 0, 0);
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn notify(_tray: &TrayIcon, _menu: &Menu, _opened: bool) {}
