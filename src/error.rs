use std::io;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("config file already exists at {0}")]
    ConfigAlreadyExists(String),

    #[error("config file was not found at {0}; run `monitor-switch init` first")]
    ConfigMissing(String),

    #[error("profile `{0}` was not found")]
    ProfileMissing(String),

    #[error("profile `{0}` already exists")]
    ProfileExists(String),

    #[error("invalid input source value `{0}`; use decimal or hex like 0x11")]
    InvalidInputValue(String),

    #[error("unsupported backend `{0}`")]
    UnsupportedBackend(String),

    #[error("no DDC/CI-capable external displays were detected")]
    NoDisplays,

    #[error("display `{0}` was not found; refresh displays and select it again")]
    DisplayNotFound(String),

    #[error("native DDC/CI operation failed for `{display}`: {message}")]
    NativeDdc { display: String, message: String },

    #[error("no compatible DDC backend was found")]
    BackendNotFound,

    #[error("backend command failed: {program} {args:?}\nstatus: {status}\nstdout: {stdout}\nstderr: {stderr}")]
    BackendCommandFailed {
        program: String,
        args: Vec<String>,
        status: String,
        stdout: String,
        stderr: String,
    },

    #[error("backend command timed out after {seconds}s: {program} {args:?}")]
    BackendCommandTimedOut {
        program: String,
        args: Vec<String>,
        seconds: u64,
    },

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("config parse error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("config serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
}

pub type Result<T> = std::result::Result<T, AppError>;
