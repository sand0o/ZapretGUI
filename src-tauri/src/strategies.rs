use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    /// Display name (e.g. "general", "general (ALT)").
    pub name: String,
    /// Bare file name (e.g. "general.bat", "general (ALT).bat").
    pub file_name: String,
}

/// List `general*.bat` files in the given zapret directory.
pub fn list_strategies<P: AsRef<Path>>(zapret_dir: P) -> Vec<Strategy> {
    let mut out: Vec<Strategy> = Vec::new();
    let dir = zapret_dir.as_ref();
    let read = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for entry in read.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name_os) = path.file_name() else {
            continue;
        };
        let name = name_os.to_string_lossy().to_string();
        let lower = name.to_lowercase();
        if !lower.starts_with("general") || !lower.ends_with(".bat") {
            continue;
        }
        let display = name
            .trim_end_matches(".bat")
            .trim_end_matches(".BAT")
            .to_string();
        out.push(Strategy {
            name: display,
            file_name: name,
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Game-filter values (TCP, UDP, generic).
pub fn game_filter_values(mode: &str) -> (&'static str, &'static str, &'static str) {
    match mode {
        "all" => ("1024-65535", "1024-65535", "1024-65535"),
        "tcp" => ("1024-65535", "12", "1024-65535"),
        "udp" => ("12", "1024-65535", "1024-65535"),
        // "off" or anything else: defaults from service.bat (no game traffic affected).
        _ => ("12", "12", "12"),
    }
}

/// Parsed `winws.exe` invocation extracted from a `general*.bat` file.
#[derive(Debug, Clone)]
pub struct ParsedStrategy {
    pub args: Vec<String>,
}

/// Parse a `general*.bat` file and produce the argument list to pass to
/// `winws.exe`, with all `%BIN%`, `%LISTS%`, `%GameFilter*%` placeholders
/// substituted. Returns an error if no `start "..." winws.exe ...` block is
/// found.
pub fn parse_strategy(
    bat_path: &Path,
    zapret_dir: &Path,
    game_filter_mode: &str,
) -> Result<ParsedStrategy, String> {
    let raw = fs::read_to_string(bat_path).map_err(|e| {
        format!(
            "Не удалось прочитать {}: {}",
            bat_path.display(),
            e
        )
    })?;

    // Stitch line-continuations (`^` at end of line) into one logical line.
    let mut joined: Vec<String> = Vec::new();
    let mut buf = String::new();
    for line in raw.lines() {
        let trimmed = line.trim_end_matches(['\r', ' ', '\t']);
        if let Some(stripped) = trimmed.strip_suffix('^') {
            buf.push_str(stripped);
            buf.push(' ');
        } else {
            buf.push_str(trimmed);
            joined.push(buf.clone());
            buf.clear();
        }
    }
    if !buf.is_empty() {
        joined.push(buf);
    }

    // Find the winws.exe launch line.
    let launch_line = joined
        .iter()
        .find(|l| l.to_lowercase().contains("winws.exe"))
        .ok_or_else(|| "В .bat нет команды запуска winws.exe".to_string())?
        .clone();

    // Tokenize.
    let tokens = tokenize(&launch_line);

    // Drop everything up to and including winws.exe.
    let mut iter = tokens.into_iter().peekable();
    let mut found_winws = false;
    let mut args: Vec<String> = Vec::new();
    while let Some(tok) = iter.next() {
        if !found_winws {
            if tok.to_lowercase().contains("winws.exe") {
                found_winws = true;
            }
            continue;
        }
        args.push(tok);
    }
    if !found_winws {
        return Err("Не удалось найти winws.exe в команде запуска".into());
    }

    // Substitute variables.
    let bin = path_with_trailing_sep(&zapret_dir.join("bin"));
    let lists = path_with_trailing_sep(&zapret_dir.join("lists"));
    let (gf_tcp, gf_udp, gf) = game_filter_values(game_filter_mode);

    let substituted: Vec<String> = args
        .into_iter()
        .map(|a| {
            a.replace("%BIN%", &bin)
                .replace("%LISTS%", &lists)
                .replace("%GameFilterTCP%", gf_tcp)
                .replace("%GameFilterUDP%", gf_udp)
                .replace("%GameFilter%", gf)
        })
        .filter(|a| !a.is_empty())
        .collect();

    Ok(ParsedStrategy { args: substituted })
}

fn path_with_trailing_sep(p: &Path) -> String {
    let mut s = p.to_string_lossy().into_owned();
    if !s.ends_with('\\') && !s.ends_with('/') {
        s.push(std::path::MAIN_SEPARATOR);
    }
    s
}

/// Very small CMD-style tokenizer: splits on whitespace, honors double-quoted
/// strings (preserving inner content, dropping the quotes).
fn tokenize(line: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in line.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if !in_quotes && ch.is_whitespace() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Resolve the absolute path to a strategy `.bat` file inside `zapret_dir`.
pub fn resolve_strategy(zapret_dir: &Path, file_name: &str) -> PathBuf {
    zapret_dir.join(file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_handles_quotes() {
        let toks = tokenize(r#"foo "a b" --x="c d" e"#);
        assert_eq!(toks, vec!["foo", "a b", "--x=c d", "e"]);
    }

    #[test]
    fn substitutes_placeholders() {
        let dir = std::env::temp_dir().join("zapret_test_strategy");
        let _ = fs::create_dir_all(&dir);
        let bat = dir.join("general.bat");
        let content = r#"@echo off
set "BIN=%~dp0bin\"
set "LISTS=%~dp0lists\"
start "zapret: %~n0" /min "%BIN%winws.exe" --wf-tcp=80,%GameFilterTCP% ^
--filter-tcp=80 --hostlist="%LISTS%list-general.txt" --new ^
--filter-udp=%GameFilterUDP% --dpi-desync=fake
"#;
        fs::write(&bat, content).unwrap();
        let parsed = parse_strategy(&bat, &dir, "off").unwrap();
        assert!(parsed.args.iter().any(|a| a == "--wf-tcp=80,12"));
        assert!(parsed.args.iter().any(|a| a == "--filter-udp=12"));
        assert!(parsed.args.iter().any(|a| a.contains("list-general.txt")));
        assert!(parsed.args.iter().any(|a| a == "--dpi-desync=fake"));
    }
}
