use std::env;

use crate::config::{Config, ProtocolPref};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Kitty,
    Iterm2,
    Ansi,
    None,
}

pub fn detect_protocol(cfg: &Config) -> Protocol {
    match cfg.protocol {
        ProtocolPref::Kitty => Protocol::Kitty,
        ProtocolPref::Iterm2 => Protocol::Iterm2,
        ProtocolPref::Ansi => Protocol::Ansi,
        ProtocolPref::None => Protocol::None,
        ProtocolPref::Auto => auto_detect(),
    }
}

fn auto_detect() -> Protocol {
    if env::var_os("KITTY_WINDOW_ID").is_some()
        || env::var("TERM").map(|t| t == "xterm-kitty").unwrap_or(false)
    {
        return Protocol::Kitty;
    }
    let term_program = env::var("TERM_PROGRAM").unwrap_or_default();
    if term_program == "iTerm.app" || term_program == "WezTerm" {
        return Protocol::Iterm2;
    }
    if env::var("LC_TERMINAL").map(|t| t == "iTerm2").unwrap_or(false) {
        return Protocol::Iterm2;
    }
    // ghostty supports Kitty protocol too.
    if term_program == "ghostty" {
        return Protocol::Kitty;
    }
    Protocol::Ansi
}
