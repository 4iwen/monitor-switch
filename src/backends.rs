use std::{
    env,
    ffi::OsString,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{
    config::BackendKind,
    error::{AppError, Result},
    sources::{format_input_value, INPUT_SOURCE_VCP_CODE},
};
#[cfg(target_os = "windows")]
use ddc_hi::{Backend, DisplayInfo, Handle};
use ddc_hi::{Ddc, Display};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayDescriptor {
    pub selector: String,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct SwitchRequest<'a> {
    pub backend: BackendKind,
    pub display: Option<&'a str>,
    pub value: u16,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct SwitchAction {
    pub backend: BackendKind,
    pub program: String,
    pub args: Vec<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct ReadRequest<'a> {
    pub backend: BackendKind,
    pub display: Option<&'a str>,
}

pub fn set_input_source(request: SwitchRequest<'_>) -> Result<SwitchAction> {
    let backend = resolve_backend(request.backend, request.dry_run)?;
    let action = build_action(backend, request.display, request.value)?;

    if request.dry_run {
        return Ok(SwitchAction {
            dry_run: true,
            ..action
        });
    }

    if backend == BackendKind::Native {
        native_set_input_source(request.display, request.value)?;
        return Ok(action);
    }

    run_action(&action)?;
    Ok(action)
}

pub fn get_input_source(request: ReadRequest<'_>) -> Result<u16> {
    let backend = resolve_backend(request.backend, false)?;

    match backend {
        BackendKind::Auto => unreachable!("auto backend should be resolved before reading"),
        BackendKind::Native => native_get_input_source(request.display),
        BackendKind::BetterDisplay => get_betterdisplay_input_source(request.display),
        BackendKind::Ddcutil => get_ddcutil_input_source(request.display),
        BackendKind::Ddcctl => Err(AppError::BackendCommandFailed {
            program: "ddcctl".to_string(),
            args: vec!["-i".to_string(), "?".to_string()],
            status: "unsupported".to_string(),
            stdout: String::new(),
            stderr: "ddcctl backend does not support reading input source in monitor-switch"
                .to_string(),
        }),
        BackendKind::ControlMyMonitor => get_control_my_monitor_input_source(request.display),
    }
}

pub fn get_supported_input_sources(request: ReadRequest<'_>) -> Result<Vec<u16>> {
    let backend = resolve_backend(request.backend, false)?;

    match backend {
        BackendKind::Auto => unreachable!("auto backend should be resolved before reading"),
        BackendKind::Native => native_get_supported_input_sources(request.display),
        BackendKind::BetterDisplay => get_betterdisplay_supported_input_sources(request.display),
        _ => Ok(Vec::new()),
    }
}

fn resolve_backend(kind: BackendKind, dry_run: bool) -> Result<BackendKind> {
    if kind != BackendKind::Auto {
        return Ok(kind);
    }

    let _ = dry_run;
    Ok(BackendKind::Native)
}

fn build_action(backend: BackendKind, display: Option<&str>, value: u16) -> Result<SwitchAction> {
    match backend {
        BackendKind::Auto => unreachable!("auto backend should be resolved before building action"),
        BackendKind::Native => Ok(SwitchAction {
            backend,
            program: "native-ddc".to_string(),
            args: vec![
                display.unwrap_or("first-display").to_string(),
                format_input_value(value),
            ],
            dry_run: false,
        }),
        BackendKind::Ddcutil => {
            let mut args = Vec::new();
            if let Some(display) = display {
                args.push("--display".to_string());
                args.push(display.to_string());
            }
            args.push("setvcp".to_string());
            args.push(format_input_value(INPUT_SOURCE_VCP_CODE.into()));
            args.push(format_input_value(value));

            Ok(SwitchAction {
                backend,
                program: "ddcutil".to_string(),
                args,
                dry_run: false,
            })
        }
        BackendKind::Ddcctl => {
            let mut args = Vec::new();
            if let Some(display) = display {
                args.push("-d".to_string());
                args.push(display.to_string());
            }
            args.push("-i".to_string());
            args.push(value.to_string());

            Ok(SwitchAction {
                backend,
                program: "ddcctl".to_string(),
                args,
                dry_run: false,
            })
        }
        BackendKind::BetterDisplay => {
            let mut args = vec!["set".to_string()];
            append_betterdisplay_selector(&mut args, display);
            args.push("-feature=ddc".to_string());
            args.push(format!(
                "-vcp={}",
                format_input_value(INPUT_SOURCE_VCP_CODE.into())
            ));
            args.push(format!("-value={}", format_input_value(value)));

            Ok(SwitchAction {
                backend,
                program: betterdisplay_program().unwrap_or_else(|| "betterdisplaycli".to_string()),
                args,
                dry_run: false,
            })
        }
        BackendKind::ControlMyMonitor => {
            let mut args = Vec::new();
            if let Some(display) = display {
                args.push("/SetValue".to_string());
                args.push(display.to_string());
            } else {
                args.push("/SetValueIfNeeded".to_string());
                args.push("Primary".to_string());
            }
            args.push(format!("{INPUT_SOURCE_VCP_CODE:02X}"));
            args.push(value.to_string());

            Ok(SwitchAction {
                backend,
                program: "ControlMyMonitor.exe".to_string(),
                args,
                dry_run: false,
            })
        }
    }
}

pub fn list_displays() -> Vec<DisplayDescriptor> {
    enumerate_native_displays()
        .into_iter()
        .map(|display| descriptor(&display))
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn enumerate_native_displays() -> Vec<Display> {
    Display::enumerate()
}

#[cfg(target_os = "windows")]
fn enumerate_native_displays() -> Vec<Display> {
    let Ok(logical_monitors) = ddc_winapi::enumerate_monitors() else {
        return Vec::new();
    };

    logical_monitors
        .into_iter()
        .enumerate()
        .filter_map(|(index, logical_monitor)| {
            let selector = windows_monitor_selector(logical_monitor, index);
            let physical_monitors =
                ddc_winapi::get_physical_monitors_from_hmonitor(logical_monitor).ok()?;
            let monitors = physical_monitors
                .into_iter()
                .map(|monitor| unsafe { ddc_winapi::Monitor::new(monitor) });

            for monitor in monitors {
                let description = monitor.description();
                let info = DisplayInfo::new(Backend::WinApi, selector.clone());
                let mut display = Display::new(Handle::WinApi(monitor), info);
                let reads_input = display
                    .handle
                    .get_vcp_feature(INPUT_SOURCE_VCP_CODE)
                    .is_ok();
                let reads_capabilities = display.update_capabilities().is_ok();

                if reads_input || reads_capabilities {
                    if display.info.model_name.is_none() && !description.trim().is_empty() {
                        display.info.model_name = Some(description);
                    }
                    return Some(display);
                }
            }

            None
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn windows_monitor_selector(
    monitor: winapi::shared::windef::HMONITOR,
    fallback_index: usize,
) -> String {
    use std::mem::{size_of, zeroed};
    use winapi::shared::minwindef::TRUE;
    use winapi::um::winuser::{GetMonitorInfoW, LPMONITORINFO, MONITORINFOEXW};

    let mut info = unsafe { zeroed::<MONITORINFOEXW>() };
    info.cbSize = size_of::<MONITORINFOEXW>() as u32;
    let read = unsafe { GetMonitorInfoW(monitor, &mut info as *mut _ as LPMONITORINFO) };
    if read == TRUE {
        let length = info
            .szDevice
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(info.szDevice.len());
        let device = String::from_utf16_lossy(&info.szDevice[..length]);
        if !device.trim().is_empty() {
            return device;
        }
    }

    format!("windows-display-{}", fallback_index + 1)
}

fn descriptor(display: &Display) -> DisplayDescriptor {
    let info = &display.info;
    let name = info
        .model_name
        .clone()
        .or_else(|| info.manufacturer_id.clone())
        .unwrap_or_else(|| {
            if info.id.trim().is_empty() {
                "External Display".to_string()
            } else {
                info.id.clone()
            }
        });
    let detail = match (&info.manufacturer_id, &info.serial_number) {
        (Some(manufacturer), Some(serial)) => format!("{manufacturer} · {serial}"),
        (Some(manufacturer), None) => manufacturer.clone(),
        _ => info.id.clone(),
    };
    DisplayDescriptor {
        selector: info.id.clone(),
        name,
        detail,
    }
}

fn with_native_display<T>(
    selector: Option<&str>,
    operation: impl FnOnce(&mut Display) -> Result<T>,
) -> Result<T> {
    let mut displays = enumerate_native_displays();
    if displays.is_empty() {
        return Err(AppError::NoDisplays);
    }

    let index = match selector.map(str::trim).filter(|value| !value.is_empty()) {
        None => 0,
        Some(selector) => find_display_index(&displays, selector)
            .ok_or_else(|| AppError::DisplayNotFound(selector.to_string()))?,
    };
    operation(&mut displays[index])
}

fn find_display_index(displays: &[Display], selector: &str) -> Option<usize> {
    if let Ok(index) = selector.parse::<usize>() {
        if index > 0 && index <= displays.len() {
            return Some(index - 1);
        }
    }

    displays.iter().position(|display| {
        let info = &display.info;
        info.id == selector
            || info
                .model_name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(selector))
            || info.serial_number.as_deref() == Some(selector)
    })
}

fn native_set_input_source(selector: Option<&str>, value: u16) -> Result<()> {
    with_native_display(selector, |display| {
        let name = descriptor(display).name;
        display
            .handle
            .set_vcp_feature(INPUT_SOURCE_VCP_CODE, value)
            .map_err(|error| AppError::NativeDdc {
                display: name,
                message: error.to_string(),
            })
    })
}

fn native_get_input_source(selector: Option<&str>) -> Result<u16> {
    with_native_display(selector, |display| {
        let name = descriptor(display).name;
        display
            .handle
            .get_vcp_feature(INPUT_SOURCE_VCP_CODE)
            .map(|value| value.value())
            .map_err(|error| AppError::NativeDdc {
                display: name,
                message: error.to_string(),
            })
    })
}

fn native_get_supported_input_sources(selector: Option<&str>) -> Result<Vec<u16>> {
    with_native_display(selector, |display| {
        let name = descriptor(display).name;
        let capabilities =
            display
                .handle
                .capabilities_string()
                .map_err(|error| AppError::NativeDdc {
                    display: name,
                    message: error.to_string(),
                })?;
        Ok(parse_vcp_60_values(&String::from_utf8_lossy(&capabilities)))
    })
}

fn run_action(action: &SwitchAction) -> Result<()> {
    let output = command_output(&action.program, &action.args)?;
    ensure_success(&action.program, &action.args, output)
}

fn command_output(program: &str, args: &[String]) -> Result<Output> {
    const TIMEOUT: Duration = Duration::from_secs(8);
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let started = Instant::now();

    loop {
        if child.try_wait()?.is_some() {
            return Ok(child.wait_with_output()?);
        }
        if started.elapsed() >= TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(mut stdout) = child.stdout.take() {
                let mut sink = Vec::new();
                let _ = stdout.read_to_end(&mut sink);
            }
            if let Some(mut stderr) = child.stderr.take() {
                let mut sink = Vec::new();
                let _ = stderr.read_to_end(&mut sink);
            }
            return Err(AppError::BackendCommandTimedOut {
                program: program.to_string(),
                args: args.to_vec(),
                seconds: TIMEOUT.as_secs(),
            });
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn ensure_success(program: &str, args: &[String], output: Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    Err(command_failed(program, args, output))
}

fn command_failed(program: &str, args: &[String], output: Output) -> AppError {
    AppError::BackendCommandFailed {
        program: program.to_string(),
        args: args.to_vec(),
        status: output.status.to_string(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn get_betterdisplay_input_source(display: Option<&str>) -> Result<u16> {
    let program = betterdisplay_program().unwrap_or_else(|| "betterdisplaycli".to_string());
    let mut args = vec!["get".to_string()];
    append_betterdisplay_selector(&mut args, display);
    args.push("-feature=ddc".to_string());
    args.push(format!(
        "-vcp={}",
        format_input_value(INPUT_SOURCE_VCP_CODE.into())
    ));
    args.push("-value".to_string());

    let output = command_output(&program, &args)?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return parse_backend_number(stdout.trim());
    }

    Err(command_failed(&program, &args, output))
}

fn get_ddcutil_input_source(display: Option<&str>) -> Result<u16> {
    let mut args = Vec::new();
    if let Some(display) = display {
        args.push("--display".to_string());
        args.push(display.to_string());
    }
    args.push("getvcp".to_string());
    args.push(format_input_value(INPUT_SOURCE_VCP_CODE.into()));

    let output = command_output("ddcutil", &args)?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return parse_first_hex_or_decimal(&stdout);
    }

    Err(command_failed("ddcutil", &args, output))
}

fn get_control_my_monitor_input_source(display: Option<&str>) -> Result<u16> {
    let args = vec![
        "/GetValue".to_string(),
        display.unwrap_or("Primary").to_string(),
        format!("{INPUT_SOURCE_VCP_CODE:02X}"),
    ];
    let output = command_output("ControlMyMonitor.exe", &args)?;

    if let Some(code) = output.status.code() {
        if let Ok(value) = u16::try_from(code) {
            return Ok(value);
        }
    }

    Err(command_failed("ControlMyMonitor.exe", &args, output))
}

fn get_betterdisplay_supported_input_sources(display: Option<&str>) -> Result<Vec<u16>> {
    let program = betterdisplay_program().unwrap_or_else(|| "betterdisplaycli".to_string());
    let mut args = vec!["get".to_string()];
    append_betterdisplay_selector(&mut args, display);
    args.push("-ddcCapabilitiesString".to_string());

    let output = command_output(&program, &args)?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Ok(parse_vcp_60_values(&stdout));
    }

    Err(command_failed(&program, &args, output))
}

fn parse_backend_number(raw: &str) -> Result<u16> {
    let trimmed = raw.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return u16::from_str_radix(hex, 16)
            .map_err(|_| AppError::InvalidInputValue(raw.to_string()));
    }

    trimmed
        .parse::<u16>()
        .map_err(|_| AppError::InvalidInputValue(raw.to_string()))
}

fn parse_first_hex_or_decimal(raw: &str) -> Result<u16> {
    for token in raw.split(|char: char| !char.is_ascii_hexdigit() && char != 'x' && char != 'X') {
        if token.is_empty() {
            continue;
        }

        if let Ok(value) = parse_backend_number(token) {
            return Ok(value);
        }
    }

    Err(AppError::InvalidInputValue(raw.to_string()))
}

fn parse_vcp_60_values(raw: &str) -> Vec<u16> {
    let Some(start) = raw.find("60(") else {
        return Vec::new();
    };
    let tail = &raw[start + 3..];
    let Some(end) = tail.find(')') else {
        return Vec::new();
    };

    tail[..end]
        .split_whitespace()
        .filter_map(|item| u16::from_str_radix(item, 16).ok())
        .collect()
}

fn betterdisplay_program() -> Option<String> {
    find_executable("betterdisplaycli")
        .or_else(|| {
            let app_binary =
                PathBuf::from("/Applications/BetterDisplay.app/Contents/MacOS/BetterDisplay");
            is_executable_file(&app_binary).then_some(app_binary)
        })
        .map(|path| path.to_string_lossy().into_owned())
}

fn append_betterdisplay_selector(args: &mut Vec<String>, display: Option<&str>) {
    let display = display
        .map(str::trim)
        .filter(|display| !display.is_empty())
        .unwrap_or("main");

    if display.eq_ignore_ascii_case("main") {
        args.push("-displayWithMainStatus".to_string());
    } else if display.eq_ignore_ascii_case("mouse") {
        args.push("-displayWithMouse".to_string());
    } else if display.eq_ignore_ascii_case("focus") {
        args.push("-displayWithFocus".to_string());
    } else if let Some(name) = display.strip_prefix("name:") {
        args.push(format!("-namelike={name}"));
    } else if let Some(uuid) = display.strip_prefix("uuid:") {
        args.push(format!("-UUID={uuid}"));
    } else if let Some(display_id) = display.strip_prefix("display-id:") {
        args.push(format!("-displayID={display_id}"));
    } else if let Some(tag_id) = display.strip_prefix("tag-id:") {
        args.push(format!("-tagID={tag_id}"));
    } else if !display.chars().all(|char| char.is_ascii_digit()) {
        args.push(format!("-namelike={display}"));
    }
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;

    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }

        if cfg!(target_os = "windows") && Path::new(name).extension().is_none() {
            for ext in windows_path_exts() {
                let candidate = dir.join(format!("{name}{ext}"));
                if is_executable_file(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn windows_path_exts() -> Vec<String> {
    env::var_os("PATHEXT")
        .map(split_path_exts)
        .filter(|exts| !exts.is_empty())
        .unwrap_or_else(|| vec![".exe".to_string(), ".cmd".to_string(), ".bat".to_string()])
}

fn split_path_exts(raw: OsString) -> Vec<String> {
    raw.to_string_lossy()
        .split(';')
        .filter(|item| !item.trim().is_empty())
        .map(|item| item.trim().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{build_action, parse_vcp_60_values, split_path_exts};
    use crate::config::BackendKind;

    #[test]
    fn builds_ddcutil_setvcp_command() {
        let action = build_action(BackendKind::Ddcutil, Some("1"), 0x11).unwrap();
        assert_eq!(action.program, "ddcutil");
        assert_eq!(
            action.args,
            vec![
                "--display".to_string(),
                "1".to_string(),
                "setvcp".to_string(),
                "0x60".to_string(),
                "0x11".to_string()
            ]
        );
    }

    #[test]
    fn builds_ddcctl_input_command() {
        let action = build_action(BackendKind::Ddcctl, Some("1"), 0x0f).unwrap();
        assert_eq!(action.program, "ddcctl");
        assert_eq!(
            action.args,
            vec![
                "-d".to_string(),
                "1".to_string(),
                "-i".to_string(),
                "15".to_string()
            ]
        );
    }

    #[test]
    fn builds_betterdisplay_ddc_command() {
        let action = build_action(BackendKind::BetterDisplay, Some("name:LG"), 0x11).unwrap();
        assert_eq!(
            action.args,
            vec![
                "set".to_string(),
                "-namelike=LG".to_string(),
                "-feature=ddc".to_string(),
                "-vcp=0x60".to_string(),
                "-value=0x11".to_string()
            ]
        );
    }

    #[test]
    fn defaults_betterdisplay_to_main_display() {
        let action = build_action(BackendKind::BetterDisplay, None, 0x11).unwrap();
        assert_eq!(action.args[1], "-displayWithMainStatus");
    }

    #[test]
    fn defaults_control_my_monitor_to_primary_display() {
        let action = build_action(BackendKind::ControlMyMonitor, None, 0x11).unwrap();
        assert_eq!(
            action.args,
            vec![
                "/SetValueIfNeeded".to_string(),
                "Primary".to_string(),
                "60".to_string(),
                "17".to_string(),
            ]
        );
    }

    #[test]
    fn skips_numeric_betterdisplay_selector() {
        let action = build_action(BackendKind::BetterDisplay, Some("1"), 0x11).unwrap();
        assert_eq!(
            action.args,
            vec![
                "set".to_string(),
                "-feature=ddc".to_string(),
                "-vcp=0x60".to_string(),
                "-value=0x11".to_string()
            ]
        );
    }

    #[test]
    fn splits_windows_path_exts() {
        let exts = split_path_exts(".EXE;.CMD;".into());
        assert_eq!(exts, vec![".EXE".to_string(), ".CMD".to_string()]);
    }

    #[test]
    fn parses_supported_vcp_60_values() {
        let values = parse_vcp_60_values("vcp(10 60(11 12 0F) 72)");
        assert_eq!(values, vec![0x11, 0x12, 0x0f]);
    }
}
