use crate::strategies::{parse_strategy, resolve_strategy};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use sysinfo::System;

const WINWS_EXE: &str = "winws.exe";

/// Internal state of the running winws.exe process.
#[derive(Default)]
pub struct ZapretState {
    /// PID of the child we spawned, if any. Used to kill on Stop.
    pub child_pid: Mutex<Option<u32>>,
}

/// Snapshot of zapret status.
#[derive(serde::Serialize, Clone)]
pub struct Status {
    pub active: bool,
    pub pid: Option<u32>,
}

pub fn current_status(state: &ZapretState) -> Status {
    let owned_pid = *state.child_pid.lock().unwrap();
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    // First check our owned PID.
    if let Some(pid) = owned_pid {
        if sys.process(sysinfo::Pid::from_u32(pid)).is_some() {
            return Status {
                active: true,
                pid: Some(pid),
            };
        }
    }
    // Otherwise scan for any running winws.exe (e.g. left over from a previous session,
    // or installed as a service).
    for (pid, proc) in sys.processes() {
        let name = proc.name().to_string_lossy().to_lowercase();
        if name == WINWS_EXE || name == "winws" {
            return Status {
                active: true,
                pid: Some(pid.as_u32()),
            };
        }
    }
    Status {
        active: false,
        pid: None,
    }
}

/// Start zapret: parse the chosen strategy, spawn winws.exe with substituted args.
pub fn start(
    state: &ZapretState,
    zapret_dir: &Path,
    strategy_file: &str,
    game_filter_mode: &str,
) -> Result<u32, String> {
    if current_status(state).active {
        return Err("zapret уже запущен".into());
    }

    let bat = resolve_strategy(zapret_dir, strategy_file);
    if !bat.exists() {
        return Err(format!("Файл стратегии не найден: {}", bat.display()));
    }

    let parsed = parse_strategy(&bat, zapret_dir, game_filter_mode)?;
    let exe: PathBuf = zapret_dir.join("bin").join(WINWS_EXE);
    if !exe.exists() {
        return Err(format!("winws.exe не найден: {}", exe.display()));
    }

    let pid = spawn_winws(&exe, zapret_dir, &parsed.args)?;
    *state.child_pid.lock().unwrap() = Some(pid);
    Ok(pid)
}

/// Stop zapret: kill the owned PID; fall back to killing all winws.exe.
pub fn stop(state: &ZapretState) -> Result<(), String> {
    let pid = state.child_pid.lock().unwrap().take();
    if let Some(pid) = pid {
        let _ = kill_pid(pid);
    }
    // Defensive: if other winws.exe instances are still running (e.g. service-installed,
    // or a previous stale launch), kill them too so the UI status reflects the truth.
    kill_all_winws();
    Ok(())
}

#[cfg(windows)]
fn spawn_winws(exe: &Path, zapret_dir: &Path, args: &[String]) -> Result<u32, String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    // CREATE_NO_WINDOW = 0x08000000  -- run without a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let bin_dir = zapret_dir.join("bin");
    let child = Command::new(exe)
        .args(args)
        .current_dir(&bin_dir)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Не удалось запустить winws.exe: {}", e))?;
    Ok(child.id())
}

#[cfg(not(windows))]
fn spawn_winws(_exe: &Path, _zapret_dir: &Path, _args: &[String]) -> Result<u32, String> {
    Err("zapret поддерживается только на Windows".into())
}

#[cfg(windows)]
fn kill_pid(pid: u32) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(windows))]
fn kill_pid(_pid: u32) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn kill_all_winws() {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = Command::new("taskkill")
        .args(["/F", "/IM", WINWS_EXE])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

#[cfg(not(windows))]
fn kill_all_winws() {}
