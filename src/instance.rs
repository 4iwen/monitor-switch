use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

pub enum Acquisition {
    Primary(Instance),
    Secondary,
}

pub struct Instance {
    endpoint: platform::Endpoint,
    _lock: File,
    stop: Arc<AtomicBool>,
    listener: Option<JoinHandle<()>>,
}

impl Instance {
    pub fn acquire(
        config: &Path,
        activate: impl Fn() + Send + Sync + 'static,
    ) -> io::Result<Acquisition> {
        let parent = config
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        let parent = parent.canonicalize()?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(parent.join(".app.lock"))?;
        match FileExt::try_lock_exclusive(&lock) {
            Ok(()) => {}
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.raw_os_error() == Some(33) =>
            {
                // The primary may still be creating its notification endpoint.
                for attempt in 0..20 {
                    match platform::notify(&parent) {
                        Ok(()) => return Ok(Acquisition::Secondary),
                        Err(_) if attempt < 19 => {
                            std::thread::sleep(std::time::Duration::from_millis(50))
                        }
                        Err(error) => return Err(error),
                    }
                }
                unreachable!()
            }
            Err(error) => return Err(error),
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (endpoint, listener) = platform::listen(&parent, stop.clone(), activate)?;
        Ok(Acquisition::Primary(Self {
            _lock: lock,
            stop,
            listener: Some(listener),
            endpoint,
        }))
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.endpoint.wake();
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
        // Endpoint drops before the lock, so a new primary cannot lose its socket.
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::{
        os::unix::net::{UnixListener, UnixStream},
        path::PathBuf,
    };

    pub struct Endpoint(PathBuf);
    impl Endpoint {
        pub fn wake(&self) {
            let _ = UnixStream::connect(&self.0);
        }
    }
    impl Drop for Endpoint {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    pub fn notify(parent: &Path) -> io::Result<()> {
        UnixStream::connect(parent.join(".app.sock")).map(|_| ())
    }
    pub fn listen(
        parent: &Path,
        stop: Arc<AtomicBool>,
        activate: impl Fn() + Send + Sync + 'static,
    ) -> io::Result<(Endpoint, JoinHandle<()>)> {
        let path = parent.join(".app.sock");
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let listener = UnixListener::bind(&path)?;
        let handle = std::thread::spawn(move || {
            for connection in listener.incoming() {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                if connection.is_ok() {
                    activate();
                } else {
                    break;
                }
            }
        });
        Ok((Endpoint(path), handle))
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use winapi::um::{
        handleapi::CloseHandle,
        synchapi::{CreateEventW, OpenEventW, SetEvent, WaitForSingleObject},
        winbase::INFINITE,
        winnt::{EVENT_MODIFY_STATE, HANDLE},
    };

    pub struct Endpoint(HANDLE);
    impl Endpoint {
        pub fn wake(&self) {
            unsafe {
                SetEvent(self.0);
            }
        }
    }
    impl Drop for Endpoint {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    fn name(parent: &Path) -> Vec<u16> {
        let hash = parent
            .to_string_lossy()
            .to_lowercase()
            .bytes()
            .fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            });
        format!("Local\\MonitorSwitch.Activate.{hash:016x}")
            .encode_utf16()
            .chain(Some(0))
            .collect()
    }
    pub fn notify(parent: &Path) -> io::Result<()> {
        let event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name(parent).as_ptr()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        let result = unsafe { SetEvent(event) };
        let error = io::Error::last_os_error();
        unsafe {
            CloseHandle(event);
        }
        if result == 0 {
            Err(error)
        } else {
            Ok(())
        }
    }
    pub fn listen(
        parent: &Path,
        stop: Arc<AtomicBool>,
        activate: impl Fn() + Send + Sync + 'static,
    ) -> io::Result<(Endpoint, JoinHandle<()>)> {
        let event = unsafe { CreateEventW(std::ptr::null_mut(), 0, 0, name(parent).as_ptr()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        let address = event as usize;
        let handle = std::thread::spawn(move || loop {
            if unsafe { WaitForSingleObject(address as HANDLE, INFINITE) } != 0 {
                break;
            }
            if stop.load(Ordering::Acquire) {
                break;
            }
            activate();
        });
        Ok((Endpoint(event), handle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_launch_notifies_primary_and_lock_is_reusable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let (sender, receiver) = std::sync::mpsc::channel();
        let Acquisition::Primary(primary) = Instance::acquire(&path, move || {
            let _ = sender.send(());
        })
        .unwrap() else {
            panic!("expected primary");
        };
        assert!(matches!(
            Instance::acquire(&path, || {}).unwrap(),
            Acquisition::Secondary
        ));
        receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "instance::tests::child_instance_reports_secondary",
                "--nocapture",
            ])
            .env("MONITOR_SWITCH_INSTANCE_TEST_PATH", &path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        drop(primary);
        assert!(matches!(
            Instance::acquire(&path, || {}).unwrap(),
            Acquisition::Primary(_)
        ));
        assert!(!path.exists());
    }

    #[test]
    fn child_instance_reports_secondary() {
        let Some(path) = std::env::var_os("MONITOR_SWITCH_INSTANCE_TEST_PATH") else {
            return;
        };
        assert!(matches!(
            Instance::acquire(Path::new(&path), || {}).unwrap(),
            Acquisition::Secondary
        ));
    }
}
