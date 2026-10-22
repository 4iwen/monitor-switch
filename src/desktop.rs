use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tray_icon::{menu::Menu, TrayIcon};

#[derive(Clone, Copy, Debug)]
pub enum DesktopEvent {
    MenuClosed,
    DisplaysChanged,
}

pub type Callback = Arc<dyn Fn(DesktopEvent) + Send + Sync>;

pub struct DesktopHooks {
    tracking: Arc<AtomicBool>,
    _native: platform::Hooks,
}

impl DesktopHooks {
    pub fn new(tray: &TrayIcon, menu: &Menu, callback: Callback) -> std::io::Result<Self> {
        let tracking = Arc::new(AtomicBool::new(false));
        let native = platform::Hooks::new(tray, menu, tracking.clone(), callback)?;
        Ok(Self {
            tracking,
            _native: native,
        })
    }

    pub fn menu_is_open(&self) -> bool {
        self.tracking.load(Ordering::Acquire)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2::{
        define_class, msg_send, rc::Retained, runtime::AnyObject, sel, DefinedClass, MainThreadOnly,
    };
    use objc2_foundation::{
        ns_string, MainThreadMarker, NSNotification, NSNotificationCenter, NSObject,
        NSObjectProtocol,
    };
    use tray_icon::menu::ContextMenu;

    struct ObserverState {
        tracking: Arc<AtomicBool>,
        callback: Callback,
    }
    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "MonitorSwitchDesktopObserver"]
        #[thread_kind = MainThreadOnly]
        #[ivars = ObserverState]
        struct Observer;
        unsafe impl NSObjectProtocol for Observer {}
        impl Observer {
            #[unsafe(method(menuOpened:))]
            fn menu_opened(&self, _notification: &NSNotification) {
                self.ivars().tracking.store(true, Ordering::Release);
            }
            #[unsafe(method(menuClosed:))]
            fn menu_closed(&self, _notification: &NSNotification) {
                self.ivars().tracking.store(false, Ordering::Release);
                (self.ivars().callback)(DesktopEvent::MenuClosed);
            }
            #[unsafe(method(displaysChanged:))]
            fn displays_changed(&self, _notification: &NSNotification) {
                (self.ivars().callback)(DesktopEvent::DisplaysChanged);
            }
        }
    );

    pub struct Hooks {
        observer: Retained<Observer>,
        centers: Vec<Retained<NSNotificationCenter>>,
    }
    impl Hooks {
        pub fn new(
            _tray: &TrayIcon,
            menu: &Menu,
            tracking: Arc<AtomicBool>,
            callback: Callback,
        ) -> std::io::Result<Self> {
            let mtm = MainThreadMarker::new().expect("desktop hooks must run on the main thread");
            let allocated = Observer::alloc(mtm).set_ivars(ObserverState { tracking, callback });
            let observer: Retained<Observer> = unsafe { msg_send![super(allocated), init] };
            let center = NSNotificationCenter::defaultCenter();
            let workspace: Retained<AnyObject> =
                unsafe { msg_send![objc2::class!(NSWorkspace), sharedWorkspace] };
            let workspace_center: Retained<NSNotificationCenter> =
                unsafe { msg_send![&*workspace, notificationCenter] };
            let menu_object = unsafe { &*(menu.ns_menu() as *const AnyObject) };
            unsafe {
                center.addObserver_selector_name_object(
                    &observer,
                    sel!(menuOpened:),
                    Some(ns_string!("NSMenuDidBeginTrackingNotification")),
                    Some(menu_object),
                );
                center.addObserver_selector_name_object(
                    &observer,
                    sel!(menuClosed:),
                    Some(ns_string!("NSMenuDidEndTrackingNotification")),
                    Some(menu_object),
                );
                center.addObserver_selector_name_object(
                    &observer,
                    sel!(displaysChanged:),
                    Some(ns_string!(
                        "NSApplicationDidChangeScreenParametersNotification"
                    )),
                    None,
                );
                workspace_center.addObserver_selector_name_object(
                    &observer,
                    sel!(displaysChanged:),
                    Some(ns_string!("NSWorkspaceDidWakeNotification")),
                    None,
                );
            }
            Ok(Self {
                observer,
                centers: vec![center, workspace_center],
            })
        }
    }
    impl Drop for Hooks {
        fn drop(&mut self) {
            for center in &self.centers {
                unsafe {
                    center.removeObserver(&self.observer);
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use winapi::{
        shared::{
            basetsd::{DWORD_PTR, UINT_PTR},
            minwindef::{LPARAM, LRESULT, UINT, WPARAM},
            windef::HWND,
        },
        um::{
            commctrl::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            winuser::{
                WM_DEVICECHANGE, WM_DISPLAYCHANGE, WM_ENTERMENULOOP, WM_EXITMENULOOP,
                WM_POWERBROADCAST,
            },
        },
    };

    struct HookState {
        tracking: Arc<AtomicBool>,
        callback: Callback,
    }
    pub struct Hooks {
        hwnd: HWND,
        state: Box<HookState>,
    }
    const SUBCLASS_ID: UINT_PTR = 0x4d535744;
    unsafe extern "system" fn procedure(
        hwnd: HWND,
        message: UINT,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: UINT_PTR,
        data: DWORD_PTR,
    ) -> LRESULT {
        let state = unsafe { &*(data as *const HookState) };
        match message {
            WM_ENTERMENULOOP => state.tracking.store(true, Ordering::Release),
            WM_EXITMENULOOP => {
                state.tracking.store(false, Ordering::Release);
                (state.callback)(DesktopEvent::MenuClosed);
            }
            WM_DISPLAYCHANGE | WM_DEVICECHANGE => (state.callback)(DesktopEvent::DisplaysChanged),
            WM_POWERBROADCAST if wparam == 7 || wparam == 18 => {
                (state.callback)(DesktopEvent::DisplaysChanged)
            }
            _ => {}
        }
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }
    impl Hooks {
        pub fn new(
            tray: &TrayIcon,
            _menu: &Menu,
            tracking: Arc<AtomicBool>,
            callback: Callback,
        ) -> std::io::Result<Self> {
            let mut state = Box::new(HookState { tracking, callback });
            let hwnd = tray.window_handle() as HWND;
            if unsafe {
                SetWindowSubclass(
                    hwnd,
                    Some(procedure),
                    SUBCLASS_ID,
                    (&mut *state as *mut HookState) as DWORD_PTR,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Self { hwnd, state })
        }
    }
    impl Drop for Hooks {
        fn drop(&mut self) {
            unsafe {
                RemoveWindowSubclass(self.hwnd, Some(procedure), SUBCLASS_ID);
            }
            // Keep callback data alive until the subclass has been removed.
            let _ = &self.state;
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::*;
    pub struct Hooks;
    impl Hooks {
        pub fn new(
            _tray: &TrayIcon,
            _menu: &Menu,
            _tracking: Arc<AtomicBool>,
            _callback: Callback,
        ) -> std::io::Result<Self> {
            Ok(Self)
        }
    }
}
