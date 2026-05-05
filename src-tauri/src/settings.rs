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

    // Typical user paths on Windows.
    if let Some(home) = dirs::home_dir() {
        for sub in [
            "zapret-discord-youtube",
            "Desktop/zapret-discord-youtube",
            "Documents/zapret-discord-youtube",
            "Downloads/zapret-discord-youtube",
        ] {
            candidates.push(home.join(sub));
        }
    }

    for root in ["C:\\", "D:\\", "C:\\Program Files", "C:\\Program Files (x86)"] {
        for sub in ["zapret-discord-youtube", "zapret"] {
            candidates.push(Path::new(root).join(sub));
        }
    }

    for c in candidates {
        if looks_like_zapret(&c) {
            return Some(c.to_string_lossy().into_owned());
        }
    }
    None
}

pub fn looks_like_zapret<P: AsRef<Path>>(path: P) -> bool {
    let p = path.as_ref();
    let winws = p.join("bin").join("winws.exe");
    let general = p.join("general.bat");
    winws.exists() && general.exists()
}
