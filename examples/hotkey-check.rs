//! Check native registration without performing any monitor I/O.
use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager};
use tao::{
    event::{Event, StartCause},
    event_loop::{ControlFlow, EventLoopBuilder},
};

fn main() {
    let event_loop = EventLoopBuilder::new().build();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if matches!(event, Event::NewEvents(StartCause::Init)) {
            match check() {
                Ok(()) => {
                    println!("Native hotkey registration, conflict detection, and release passed")
                }
                Err(error) => eprintln!("Native hotkey check failed: {error}"),
            }
            *control_flow = ControlFlow::Exit;
        }
    });
}

fn check() -> Result<(), Box<dyn std::error::Error>> {
    let manager = GlobalHotKeyManager::new()?;
    let hotkey: HotKey = "Control+Alt+Shift+F19".parse()?;
    manager.register(hotkey)?;
    if manager.register(hotkey).is_ok() {
        return Err("OS unexpectedly accepted a duplicate shortcut".into());
    }
    manager.unregister(hotkey)?;
    manager.register(hotkey)?;
    manager.unregister(hotkey)?;
    Ok(())
}
