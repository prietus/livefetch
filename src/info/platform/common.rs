use std::env;
use std::path::Path;

pub fn shell() -> String {
    if cfg!(windows) {
        // Best-effort: COMSPEC for cmd, PSModulePath suggests PowerShell.
        if env::var_os("PSModulePath").is_some() {
            return "PowerShell".into();
        }
        if let Ok(c) = env::var("COMSPEC") {
            return Path::new(&c)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(c);
        }
    }
    let raw = env::var("SHELL").unwrap_or_else(|_| "?".into());
    Path::new(&raw)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or(raw)
}

pub fn terminal() -> String {
    // Most accurate signal first.
    if let Ok(t) = env::var("TERM_PROGRAM") {
        if !t.is_empty() {
            let ver = env::var("TERM_PROGRAM_VERSION").unwrap_or_default();
            return if ver.is_empty() { t } else { format!("{t} {ver}") };
        }
    }
    if env::var_os("KITTY_WINDOW_ID").is_some() {
        return "kitty".into();
    }
    if env::var_os("WT_SESSION").is_some() {
        return "Windows Terminal".into();
    }
    if let Ok(t) = env::var("LC_TERMINAL") {
        if !t.is_empty() {
            return t;
        }
    }
    env::var("TERM").unwrap_or_else(|_| "?".into())
}

pub fn locale() -> String {
    for k in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = env::var(k) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    "C".into()
}
