use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::cli::{Args, ProtocolArg};
use crate::image::ChromaKey;

#[derive(Debug, Clone)]
pub struct Config {
    pub image_path: Option<PathBuf>,
    pub protocol: ProtocolPref,
    pub image_cols: u16,
    pub animate: bool,
    pub live: bool,
    pub refresh_ms: u64,
    pub modules: Vec<String>,
    pub chroma: ChromaKey,
    pub chroma_tolerance: u8,
}

#[derive(Debug, Clone, Copy)]
pub enum ProtocolPref {
    Auto,
    Kitty,
    Iterm2,
    Ansi,
    None,
}

impl From<&ProtocolArg> for ProtocolPref {
    fn from(p: &ProtocolArg) -> Self {
        match p {
            ProtocolArg::Auto => ProtocolPref::Auto,
            ProtocolArg::Kitty => ProtocolPref::Kitty,
            ProtocolArg::Iterm2 => ProtocolPref::Iterm2,
            ProtocolArg::Ansi => ProtocolPref::Ansi,
            ProtocolArg::None => ProtocolPref::None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    image: Option<PathBuf>,
    protocol: Option<String>,
    image_cols: Option<u16>,
    animate: Option<bool>,
    live: Option<bool>,
    refresh_ms: Option<u64>,
    modules: Option<Vec<String>>,
    chroma: Option<String>,
    chroma_tolerance: Option<u8>,
}

pub fn load(path: Option<&Path>) -> Result<Config> {
    let file_cfg = match resolve_config_path(path) {
        Some(p) if p.exists() => {
            let txt = fs::read_to_string(&p)
                .with_context(|| format!("reading config {}", p.display()))?;
            serde_json::from_str::<FileConfig>(&txt)
                .with_context(|| format!("parsing config {}", p.display()))?
        }
        _ => FileConfig::default(),
    };

    let protocol = match file_cfg.protocol.as_deref() {
        Some("kitty") => ProtocolPref::Kitty,
        Some("iterm2") => ProtocolPref::Iterm2,
        Some("ansi") => ProtocolPref::Ansi,
        Some("none") => ProtocolPref::None,
        _ => ProtocolPref::Auto,
    };

    let chroma = file_cfg
        .chroma
        .as_deref()
        .map(ChromaKey::parse)
        .transpose()?
        .unwrap_or(ChromaKey::None);

    Ok(Config {
        image_path: file_cfg.image,
        protocol,
        image_cols: file_cfg.image_cols.unwrap_or(34),
        animate: file_cfg.animate.unwrap_or(true),
        live: file_cfg.live.unwrap_or(true),
        refresh_ms: file_cfg.refresh_ms.unwrap_or(500).max(50),
        modules: file_cfg.modules.unwrap_or_else(default_modules),
        chroma,
        chroma_tolerance: file_cfg.chroma_tolerance.unwrap_or(24),
    })
}

impl Config {
    pub fn merge_cli(mut self, args: &Args) -> Self {
        if let Some(p) = &args.image {
            self.image_path = Some(p.clone());
        }
        if let Some(p) = &args.protocol {
            self.protocol = p.into();
        }
        if let Some(c) = args.image_cols {
            self.image_cols = c;
        }
        if args.no_animate {
            self.animate = false;
        }
        if args.once {
            self.live = false;
        }
        if let Some(ms) = args.refresh {
            self.refresh_ms = ms.max(50);
        }
        if let Some(m) = &args.modules {
            self.modules = m.clone();
        }
        if let Some(c) = &args.chroma {
            if let Ok(parsed) = ChromaKey::parse(c) {
                self.chroma = parsed;
            }
        }
        if let Some(t) = args.chroma_tolerance {
            self.chroma_tolerance = t;
        }
        self
    }
}

fn resolve_config_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join("livefetch").join("config.json"))
}

fn default_modules() -> Vec<String> {
    [
        "title", "separator", "os", "host", "kernel", "init", "uptime", "loadavg", "packages",
        "shell", "terminal", "session", "de", "wm", "resolution", "cpu", "cputemp", "gpu",
        "gpudriver", "memory", "swap", "network", "disk", "localip", "audio", "theme", "battery",
        "locale", "break", "colors",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}
