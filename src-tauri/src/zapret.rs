use crate::strategies::resolve_strategy;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
#[cfg(windows)]
use std::time::Duration;
use sysinfo::System;

const WINWS_EXE: &str = "winws.exe";

/// Internal state of the running winws.exe process.
#[derive(Default)]
pub struct ZapretState {
    /// PID of the winws.exe we're tracking.
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

    if let Some(pid) = owned_pid {
        if sys.process(sysinfo::Pid::from_u32(pid)).is_some() {
            return Status {
                active: true,
                pid: Some(pid),
            };
        }
    }
    // Scan for any running winws.exe (e.g. left over from a previous session,
    // or installed as a service).
    if let Some(pid) = find_winws_pid(&sys) {
        return Status {
            active: true,
            pid: Some(pid),
        };
    }
    Status {
        active: false,
        pid: None,
    }
}

fn find_winws_pid(sys: &System) -> Option<u32> {
    for (pid, proc) in sys.processes() {
        let name = proc.name().to_string_lossy().to_lowercase();
        if name == WINWS_EXE || name == "winws" {
            return Some(pid.as_u32());
        }
    }
    None
}

/// Start zapret. We invoke the chosen `general*.bat` through cmd.exe so that
/// `service.bat` initialisation runs (creates user list files, sets game-filter
/// env vars, etc.). The bat file uses `start /min` to spawn winws.exe and then
/// returns; we wait for cmd.exe to exit and then locate the freshly-spawned
/// winws.exe in the process list.
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
    if !zapret_dir.join("bin").join(WINWS_EXE).exists() {
        return Err(format!(
            "winws.exe не найден: {}",
            zapret_dir.join("bin").join(WINWS_EXE).display()
        ));
    }

    // Synchronise the on-disk game-filter file with the user's UI choice.
    // service.bat reads `utils/game_filter.enabled` to decide which ports to
    // include; we mirror exactly what its interactive menu does.
    sync_game_filter_file(zapret_dir, game_filter_mode)
        .map_err(|e| format!("Не удалось обновить игровой фильтр: {}", e))?;

    let pid = spawn_via_bat(&bat, zapret_dir)?;
    *state.child_pid.lock().unwrap() = Some(pid);
    Ok(pid)
}

/// Stop zapret: kill the owned PID; fall back to killing all winws.exe.
pub fn stop(state: &ZapretState) -> Result<(), String> {
    let pid = state.child_pid.lock().unwrap().take();
    if let Some(pid) = pid {
        let _ = kill_pid(pid);
    }
    // Defensive: clean up any other winws.exe (e.g. service-installed or stale).
    kill_all_winws();
    Ok(())
}

fn sync_game_filter_file(zapret_dir: &Path, mode: &str) -> Result<(), String> {
    let utils = zapret_dir.join("utils");
    fs::create_dir_all(&utils).map_err(|e| e.to_string())?;
    let flag = utils.join("game_filter.enabled");
    match mode {
        "all" | "tcp" | "udp" => {
            fs::write(&flag, mode).map_err(|e| e.to_string())?;
        }
        _ => {
            // "off" or unknown: remove the file (matches service.bat's behaviour).
            if flag.exists() {
                let _ = fs::remove_file(&flag);
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
fn spawn_via_bat(bat: &Path, zapret_dir: &Path) -> Result<u32, String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    // CREATE_NO_WINDOW = 0x08000000  -- run cmd.exe without a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // CRITICAL: stdio MUST be Stdio::null(), not Stdio::piped().
    //
    // service.bat (called from general.bat) runs
    //   start /b service check_updates soft
    // which spawns a background descendant that INHERITS our stdout/stderr
    // pipe handles. Even after cmd.exe itself exits, that descendant keeps
    // the pipes open, so `Command::output()` (and `wait_with_output()`)
    // would block forever waiting for EOF. spawn() + wait() avoids
    // wait_with_output, and null stdio avoids the inherited-handle issue
    // entirely.
    let mut child = Command::new("cmd.exe")
        .arg("/c")
        .arg(bat)
        .current_dir(zapret_dir)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Не удалось запустить cmd.exe: {}", e))?;

    // Wait for cmd.exe (the bat) to finish. This does NOT wait for
    // grandchildren like the spawned winws.exe or background update-check.
    let _ = child.wait();

    // The bat used `start /min winws.exe ...` to detach winws.exe; give
    // Windows a moment to put it in the process list, then poll.
    let mut sys = System::new();
    let mut found_pid: Option<u32> = None;
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(300));
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        if let Some(pid) = find_winws_pid(&sys) {
            found_pid = Some(pid);
            break;
        }
    }

    // If we saw a winws.exe, verify it actually stays up — when WinDivert
    // can't be loaded (no admin rights / AV interference) winws.exe spawns,
    // logs an error and exits within ~1 s. We want to catch that case
    // explicitly instead of returning a false-positive Ok.
    if let Some(pid) = found_pid {
        std::thread::sleep(Duration::from_millis(1200));
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        if sys.process(sysinfo::Pid::from_u32(pid)).is_some() {
            return Ok(pid);
        }
    }

    let mut msg = if found_pid.is_some() {
        String::from("winws.exe запустился, но сразу же завершился. ")
    } else {
        String::from("winws.exe не запустился. ")
    };
    msg.push_str(
        "Чаще всего это значит:\n\
         1) приложение запущено без прав администратора (драйвер WinDivert не грузится без них) — \
         перезапустите от имени администратора;\n\
         2) winws.exe / WinDivert64.sys заблокирован антивирусом — добавьте папку bin в исключения;\n\
         3) уже запущен другой экземпляр zapret или установлена служба zapret \
         (попробуйте `service.bat` → пункт «Remove Services»).\n\n\
         Если не помогло — запустите выбранный .bat вручную из папки zapret \
         через двойной клик и пришлите скриншот ошибки.",
    );
    Err(msg)
}

#[cfg(not(windows))]
fn spawn_via_bat(_bat: &Path, _zapret_dir: &Path) -> Result<u32, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("zapret-gui-zapret-test-{}", name));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn sync_game_filter_writes_mode() {
        let dir = temp("gf-write");
        sync_game_filter_file(&dir, "all").unwrap();
        let flag = dir.join("utils/game_filter.enabled");
        assert!(flag.exists());
        assert_eq!(fs::read_to_string(&flag).unwrap(), "all");
    }

    #[test]
    fn sync_game_filter_removes_for_off() {
        let dir = temp("gf-remove");
        let utils = dir.join("utils");
        fs::create_dir_all(&utils).unwrap();
        let flag = utils.join("game_filter.enabled");
        fs::write(&flag, "tcp").unwrap();
        sync_game_filter_file(&dir, "off").unwrap();
        assert!(!flag.exists());
    }

    #[test]
    fn sync_game_filter_creates_utils_dir() {
        let dir = temp("gf-mkdir");
        // Note: utils folder doesn't exist yet.
        sync_game_filter_file(&dir, "udp").unwrap();
        assert!(dir.join("utils/game_filter.enabled").exists());
    }
}
