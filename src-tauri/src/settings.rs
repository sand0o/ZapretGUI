use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Absolute path to a folder containing zapret-discord-youtube
    /// (the folder with `bin/`, `lists/`, and `general*.bat` files).
    pub zapret_path: Option<String>,
    /// File name of the chosen `general*.bat` strategy
    /// (relative to `zapret_path`). E.g. `general.bat`.
    pub strategy: Option<String>,
    /// Game-filter mode: "off" | "all" | "tcp" | "udp".
    pub game_filter: String,
    /// Hide window to tray on close instead of quitting.
    pub minimize_to_tray: bool,
    /// Theme: "dark" or "light".
    pub theme: String,
    /// Auto-start zapret when the app launches.
    pub autostart_zapret: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            zapret_path: None,
            strategy: None,
            game_filter: "off".to_string(),
            minimize_to_tray: true,
            theme: "dark".to_string(),
            autostart_zapret: false,
        }
    }
}

impl Settings {
    pub fn config_path() -> PathBuf {
        if let Some(dir) = dirs::config_dir() {
            return dir.join("ZapretGUI").join("config.json");
        }
        PathBuf::from("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(s) = serde_json::from_str::<Settings>(&text) {
                return s;
            }
        }
        Settings::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, text).map_err(|e| e.to_string())
    }
}

/// Diagnostic info about a candidate zapret folder. Used both for validation
/// and to show the user *why* a folder was rejected.
#[derive(Debug, Clone, Serialize)]
pub struct PathInspection {
    pub path: String,
    /// True when the folder satisfies the minimum requirements
    /// (winws.exe + at least one .bat strategy).
    pub valid: bool,
    /// `<path>/bin/winws.exe` exists.
    pub has_winws: bool,
    /// At least one `general*.bat` was found in <path>.
    pub has_general_bat: bool,
    /// `<path>/service.bat` exists (informational).
    pub has_service_bat: bool,
    /// `<path>/lists` exists (informational, not strictly required).
    pub has_lists: bool,
    /// If the picked folder isn't valid but a sub-folder is, suggest it here.
    pub suggested_subfolder: Option<String>,
    /// Human-readable explanation for the UI.
    pub message: String,
}

impl PathInspection {
    fn empty(path: String) -> Self {
        Self {
            path,
            valid: false,
            has_winws: false,
            has_general_bat: false,
            has_service_bat: false,
            has_lists: false,
            suggested_subfolder: None,
            message: String::new(),
        }
    }
}

/// Inspect a path: check what's there and (if invalid) try to find the real
/// zapret folder one level deeper. This is what the UI calls when the user
/// picks a folder.
pub fn inspect_path<P: AsRef<Path>>(path: P) -> PathInspection {
    let p = path.as_ref();
    let mut info = PathInspection::empty(p.to_string_lossy().into_owned());

    if !p.exists() {
        info.message = "Указанная папка не существует".into();
        return info;
    }
    if !p.is_dir() {
        info.message = "Это не папка".into();
        return info;
    }

    info.has_winws = p.join("bin").join("winws.exe").exists();
    info.has_general_bat = has_general_bat(p);
    info.has_service_bat = p.join("service.bat").exists();
    info.has_lists = p.join("lists").is_dir();

    info.valid = info.has_winws && (info.has_general_bat || info.has_service_bat);

    if info.valid {
        info.message = "Папка zapret найдена".into();
        return info;
    }

    // Try to find a real zapret folder one level inside the picked folder.
    // Handles cases like "Downloads" → "Downloads/zapret-discord-youtube-main".
    if let Some(sub) = find_zapret_subfolder(p) {
        info.suggested_subfolder = Some(sub.to_string_lossy().into_owned());
        info.message = format!(
            "В выбранной папке нет winws.exe, но похоже что zapret лежит в {}",
            sub.display()
        );
        return info;
    }

    // Build a precise error message.
    let mut missing: Vec<&str> = Vec::new();
    if !info.has_winws {
        missing.push("bin\\winws.exe");
    }
    if !info.has_general_bat && !info.has_service_bat {
        missing.push("general.bat (или service.bat)");
    }
    info.message = format!(
        "В папке не найдены: {}. Укажите папку с распакованным архивом zapret-discord-youtube.",
        missing.join(", ")
    );
    info
}

fn has_general_bat(dir: &Path) -> bool {
    let Ok(read) = fs::read_dir(dir) else {
        return false;
    };
    for entry in read.flatten() {
        let name = entry.file_name();
        let lower = name.to_string_lossy().to_ascii_lowercase();
        if lower.starts_with("general") && lower.ends_with(".bat") {
            return true;
        }
    }
    false
}

/// Search direct children of `dir` for a folder that itself looks like zapret.
fn find_zapret_subfolder(dir: &Path) -> Option<PathBuf> {
    let read = fs::read_dir(dir).ok()?;
    for entry in read.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        if looks_like_zapret(&p) {
            return Some(p);
        }
    }
    None
}

/// Best-effort detection of a zapret installation in common locations.
pub fn autodetect_zapret_path() -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // Same folder as the GUI (portable layout: drop the .exe inside zapret-discord-youtube).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd);
    }

    // Common folder names. GitHub "Download ZIP" produces `*-main` / `*-master`,
    // releases produce just `zapret-discord-youtube`, some users rename to `zapret`.
    let folder_names = [
        "zapret-discord-youtube",
        "zapret-discord-youtube-main",
        "zapret-discord-youtube-master",
        "zapret",
    ];

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for sub in ["", "Desktop", "Documents", "Downloads", "OneDrive\\Desktop"] {
            if sub.is_empty() {
                roots.push(home.clone());
            } else {
                roots.push(home.join(sub));
            }
        }
    }
    for d in [
        "C:\\",
        "D:\\",
        "E:\\",
        "C:\\Program Files",
        "C:\\Program Files (x86)",
    ] {
        roots.push(PathBuf::from(d));
    }

    for root in &roots {
        for name in &folder_names {
            candidates.push(root.join(name));
        }
        // Also probe one level deeper: e.g. ~/Downloads/zapret-discord-youtube-main/zapret-discord-youtube-main
        if let Ok(read) = fs::read_dir(root) {
            for entry in read.flatten() {
                let p = entry.path();
                if !p.is_dir() {
                    continue;
                }
                let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                if n.contains("zapret") {
                    candidates.push(p);
                }
            }
        }
    }

    for c in candidates {
        if looks_like_zapret(&c) {
            return Some(c.to_string_lossy().into_owned());
        }
    }
    None
}

/// Lenient check: requires winws.exe and at least one .bat strategy
/// (either `general*.bat` or `service.bat`).
pub fn looks_like_zapret<P: AsRef<Path>>(path: P) -> bool {
    let p = path.as_ref();
    let winws = p.join("bin").join("winws.exe").exists();
    let has_bat = has_general_bat(p) || p.join("service.bat").exists();
    winws && has_bat
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("zapret-gui-test-{}", name));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn inspect_reports_missing_files() {
        let dir = temp("missing");
        let info = inspect_path(&dir);
        assert!(!info.valid);
        assert!(info.message.contains("winws.exe"));
        assert!(info.message.contains("general.bat"));
    }

    #[test]
    fn inspect_accepts_minimal_zapret() {
        let dir = temp("minimal");
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(dir.join("bin").join("winws.exe"), "").unwrap();
        fs::write(dir.join("general.bat"), "").unwrap();
        let info = inspect_path(&dir);
        assert!(info.valid);
        assert!(info.has_winws);
        assert!(info.has_general_bat);
    }

    #[test]
    fn inspect_accepts_service_bat_only() {
        let dir = temp("service-only");
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(dir.join("bin").join("winws.exe"), "").unwrap();
        fs::write(dir.join("service.bat"), "").unwrap();
        let info = inspect_path(&dir);
        assert!(info.valid);
        assert!(info.has_service_bat);
    }

    #[test]
    fn inspect_suggests_subfolder() {
        let outer = temp("outer");
        let inner = outer.join("zapret-discord-youtube-main");
        fs::create_dir_all(inner.join("bin")).unwrap();
        fs::write(inner.join("bin").join("winws.exe"), "").unwrap();
        fs::write(inner.join("general.bat"), "").unwrap();
        let info = inspect_path(&outer);
        assert!(!info.valid);
        assert!(info.suggested_subfolder.is_some());
        assert!(info.suggested_subfolder.unwrap().contains("zapret-discord-youtube-main"));
    }
}
