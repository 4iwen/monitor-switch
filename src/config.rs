use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    #[serde(default = "legacy_schema_version")]
    pub schema_version: u8,

    #[serde(default)]
    pub default_backend: BackendKind,

    #[serde(default)]
    pub default_display: Option<String>,

    #[serde(default)]
    pub profiles: Vec<SourceProfile>,

    #[serde(default)]
    pub shortcuts: Vec<crate::shortcuts::ShortcutBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SourceProfile {
    pub name: String,
    pub value: u16,

    #[serde(default)]
    pub label: Option<String>,

    #[serde(default)]
    pub display: Option<String>,

    #[serde(default)]
    pub backend: Option<BackendKind>,

    #[serde(default = "default_true")]
    pub show_in_menu: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BackendKind {
    #[default]
    Auto,
    Native,
    Ddcutil,
    Ddcctl,
    BetterDisplay,
    ControlMyMonitor,
}

impl BackendKind {
    pub fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "native" | "native-ddc" => Ok(Self::Native),
            "ddcutil" => Ok(Self::Ddcutil),
            "ddcctl" => Ok(Self::Ddcctl),
            "betterdisplay" | "better-display" => Ok(Self::BetterDisplay),
            "controlmymonitor" | "control-my-monitor" | "control_my_monitor" => {
                Ok(Self::ControlMyMonitor)
            }
            other => Err(AppError::UnsupportedBackend(other.to_string())),
        }
    }
}

impl std::fmt::Display for BackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Native => write!(f, "native"),
            Self::Ddcutil => write!(f, "ddcutil"),
            Self::Ddcctl => write!(f, "ddcctl"),
            Self::BetterDisplay => write!(f, "betterdisplay"),
            Self::ControlMyMonitor => write!(f, "control-my-monitor"),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 2,
            default_backend: BackendKind::Auto,
            default_display: None,
            profiles: Vec::new(),
            shortcuts: Vec::new(),
        }
    }
}

impl Config {
    pub fn load_or_default(path: &Path) -> Result<Self> {
        match Self::load(path) {
            Ok(config) => Ok(config),
            Err(AppError::ConfigMissing(_)) => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(AppError::ConfigMissing(path.display().to_string()));
            }
            Err(error) => return Err(error.into()),
        };
        Ok(toml::from_str(&raw)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let raw = toml::to_string_pretty(self)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        if let Ok(metadata) = fs::metadata(path) {
            temporary
                .as_file()
                .set_permissions(metadata.permissions())?;
        }
        temporary.write_all(raw.as_bytes())?;
        temporary.as_file().sync_all()?;
        persist_config(temporary, path)?;
        Ok(())
    }

    pub fn find_profile(&self, name: &str) -> Option<&SourceProfile> {
        self.profiles.iter().find(|profile| profile.name == name)
    }

    pub fn effective_backend(&self, profile: Option<&SourceProfile>) -> BackendKind {
        profile
            .and_then(|profile| profile.backend)
            .unwrap_or(self.default_backend)
    }

    pub fn effective_display<'a>(&'a self, profile: Option<&'a SourceProfile>) -> Option<&'a str> {
        profile
            .and_then(|profile| profile.display.as_deref())
            .or(self.default_display.as_deref())
    }

    pub fn migrate_to_native(&mut self, display_selectors: &[String]) -> bool {
        if self.schema_version >= 2 {
            return false;
        }

        self.schema_version = 2;
        self.default_backend = BackendKind::Auto;
        self.default_display = migrate_selector(self.default_display.take(), display_selectors);
        for profile in &mut self.profiles {
            profile.backend = None;
            profile.display = migrate_selector(profile.display.take(), display_selectors);
        }
        true
    }
}

fn persist_config(temporary: tempfile::NamedTempFile, path: &Path) -> Result<()> {
    #[cfg(not(target_os = "windows"))]
    temporary
        .persist(path)
        .map_err(|error| AppError::Io(error.error))?;
    #[cfg(target_os = "windows")]
    {
        let mut pending = temporary;
        for attempt in 0..8 {
            match pending.persist(path) {
                Ok(_) => return Ok(()),
                // Windows can temporarily deny replacement while a reader or
                // antivirus holds the target. Retry without truncating either file.
                Err(error)
                    if matches!(error.error.raw_os_error(), Some(5 | 32 | 33)) && attempt < 7 =>
                {
                    pending = error.file;
                    std::thread::sleep(std::time::Duration::from_millis(10 * (attempt + 1)));
                }
                Err(error) => return Err(AppError::Io(error.error)),
            }
        }
    }
    Ok(())
}

fn legacy_schema_version() -> u8 {
    1
}

fn default_true() -> bool {
    true
}

fn migrate_selector(selector: Option<String>, displays: &[String]) -> Option<String> {
    let first = displays.first().cloned();
    match selector
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        None | Some("main") | Some("mouse") | Some("focus") => first,
        Some(value) => value
            .parse::<usize>()
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| displays.get(index).cloned())
            .or(selector),
    }
}

pub fn default_config_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        if let Some(appdata) = env::var_os("APPDATA") {
            return PathBuf::from(appdata)
                .join("monitor-switch")
                .join("config.toml");
        }
    }

    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg)
            .join("monitor-switch")
            .join("config.toml");
    }

    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("monitor-switch")
        .join("config.toml")
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::{BackendKind, Config, SourceProfile};

    #[test]
    fn atomic_save_never_exposes_partial_config_and_cleans_temporary_files() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        Config::default().save(&path).unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let reader_running = running.clone();
        let reader_path = path.clone();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let reader_barrier = barrier.clone();
        let reader = std::thread::spawn(move || {
            Config::load(&reader_path).unwrap();
            let mut reads = 1;
            reader_barrier.wait();
            while reader_running.load(Ordering::Acquire) {
                let config = Config::load(&reader_path).unwrap();
                assert!(config.schema_version == 2);
                reads += 1;
            }
            reads
        });
        barrier.wait();
        for index in 0..25 {
            Config {
                default_display: Some(format!("display-{index}")),
                ..Config::default()
            }
            .save(&path)
            .unwrap();
        }
        running.store(false, Ordering::Release);
        assert!(reader.join().unwrap() > 0);
        assert_eq!(
            Config::load(&path).unwrap().default_display.as_deref(),
            Some("display-24")
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_atomic_replace_keeps_existing_destination() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("existing-directory");
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("keep"), "original").unwrap();
        assert!(Config::default().save(&destination).is_err());
        assert_eq!(
            std::fs::read_to_string(destination.join("keep")).unwrap(),
            "original"
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn profile_backend_overrides_default_backend() {
        let config = Config {
            shortcuts: Vec::new(),
            schema_version: 2,
            default_backend: BackendKind::Ddcutil,
            default_display: None,
            profiles: vec![SourceProfile {
                name: "pc".to_string(),
                value: 0x10,
                label: None,
                display: None,
                backend: Some(BackendKind::Ddcctl),
                show_in_menu: true,
            }],
        };

        let profile = config.find_profile("pc").unwrap();
        assert_eq!(config.effective_backend(Some(profile)), BackendKind::Ddcctl);
    }

    #[test]
    fn profile_display_overrides_default_display() {
        let config = Config {
            shortcuts: Vec::new(),
            schema_version: 2,
            default_backend: BackendKind::Auto,
            default_display: Some("1".to_string()),
            profiles: vec![SourceProfile {
                name: "switch".to_string(),
                value: 0x11,
                label: None,
                display: Some("2".to_string()),
                backend: None,
                show_in_menu: true,
            }],
        };

        let profile = config.find_profile("switch").unwrap();
        assert_eq!(config.effective_display(Some(profile)), Some("2"));
    }

    #[test]
    fn migrates_legacy_external_profiles_to_native_selectors() {
        let mut config: Config = toml::from_str(
            r#"
default-backend = "auto"
[[profiles]]
name = "mac"
value = 17
display = "main"
backend = "better-display"
"#,
        )
        .unwrap();

        assert!(config.migrate_to_native(&["R27q-30".to_string()]));
        assert_eq!(config.schema_version, 2);
        assert_eq!(config.profiles[0].display.as_deref(), Some("R27q-30"));
        assert_eq!(config.profiles[0].backend, None);
        assert!(!config.migrate_to_native(&["other".to_string()]));
    }

    #[test]
    fn existing_profiles_remain_visible_when_visibility_is_missing() {
        let config: Config = toml::from_str(
            r#"
schema-version = 2
[[profiles]]
name = "pc"
value = 17
"#,
        )
        .unwrap();

        assert!(config.profiles[0].show_in_menu);
        assert!(config.shortcuts.is_empty());
    }
}
