//! Opt-in "launch at login". Disabled unless the user turns it on in Settings.
//! The OS registration itself is the source of truth; nothing is stored in the config.

use std::{env, io, path::PathBuf};

const LABEL: &str = "app.monitorswitch.desktop";

pub fn is_enabled() -> bool {
    platform::is_enabled()
}

pub fn set_enabled(enabled: bool) -> io::Result<()> {
    if enabled {
        platform::enable(&env::current_exe()?)
    } else {
        platform::disable()
    }
}

#[allow(dead_code)]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[allow(dead_code)]
fn home() -> io::Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
}

fn remove_if_exists(path: &std::path::Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::{fs, path::Path};

    fn plist_path() -> io::Result<PathBuf> {
        Ok(home()?
            .join("Library/LaunchAgents")
            .join(format!("{LABEL}.plist")))
    }

    pub fn is_enabled() -> bool {
        plist_path().is_ok_and(|path| path.exists())
    }

    // Only the plist is written: launchd picks it up at next login, so enabling
    // never starts a second instance and disabling never kills the running one.
    pub fn enable(exe: &Path) -> io::Result<()> {
        let path = plist_path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(
            path,
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n<dict>\n\
    <key>Label</key>\n    <string>{LABEL}</string>\n\
    <key>ProgramArguments</key>\n    <array>\n        <string>{}</string>\n    </array>\n\
    <key>RunAtLoad</key>\n    <true/>\n\
    <key>ProcessType</key>\n    <string>Interactive</string>\n\
</dict>\n</plist>\n",
                xml_escape(&exe.to_string_lossy())
            ),
        )
    }

    pub fn disable() -> io::Result<()> {
        remove_if_exists(&plist_path()?)
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::{os::windows::process::CommandExt, path::Path, process::Command};

    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "Monitor Switch";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn reg(args: &[&str]) -> io::Result<bool> {
        Ok(Command::new("reg")
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()?
            .status
            .success())
    }

    // Older installers created a Startup-folder shortcut; honor and remove it too.
    fn legacy_shortcut() -> Option<PathBuf> {
        env::var_os("APPDATA").map(|appdata| {
            PathBuf::from(appdata)
                .join(r"Microsoft\Windows\Start Menu\Programs\Startup")
                .join("Monitor Switch.lnk")
        })
    }

    pub fn is_enabled() -> bool {
        reg(&["query", RUN_KEY, "/v", VALUE]).unwrap_or(false)
            || legacy_shortcut().is_some_and(|path| path.exists())
    }

    pub fn enable(exe: &Path) -> io::Result<()> {
        let command = format!("\"{}\"", exe.display());
        if reg(&["add", RUN_KEY, "/v", VALUE, "/t", "REG_SZ", "/d", &command, "/f"])? {
            Ok(())
        } else {
            Err(io::Error::other("could not write the Windows Run key"))
        }
    }

    pub fn disable() -> io::Result<()> {
        if reg(&["query", RUN_KEY, "/v", VALUE])? && !reg(&["delete", RUN_KEY, "/v", VALUE, "/f"])? {
            return Err(io::Error::other("could not remove the Windows Run key"));
        }
        if let Some(path) = legacy_shortcut() {
            remove_if_exists(&path)?;
        }
        Ok(())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::*;
    use std::{fs, path::Path};

    fn desktop_path() -> io::Result<PathBuf> {
        let base = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or(home()?.join(".config"));
        Ok(base.join("autostart/monitor-switch.desktop"))
    }

    pub fn is_enabled() -> bool {
        desktop_path().is_ok_and(|path| path.exists())
    }

    pub fn enable(exe: &Path) -> io::Result<()> {
        let path = desktop_path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(
            path,
            format!(
                "[Desktop Entry]\nType=Application\nName=Monitor Switch\nExec=\"{}\"\n",
                exe.display()
            ),
        )
    }

    pub fn disable() -> io::Result<()> {
        remove_if_exists(&desktop_path()?)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn escapes_xml_in_paths() {
        assert_eq!(super::xml_escape("/a&b/<c>"), "/a&amp;b/&lt;c&gt;");
    }
}
