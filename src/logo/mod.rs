// ASCII distro logos. Adapted from neofetch (https://github.com/dylanaraps/neofetch),
// Copyright (c) 2015-2024 Dylan Araps, MIT licensed. See THIRD-PARTY-LICENSES at the
// repository root for the full notice.
//
// Neofetch stores each logo as a shell heredoc with `${c1}` / `${c2}` placeholders
// and a separate `set_colors` call. Here every line is already concrete: ANSI escapes
// inline, reset at end of line, `width` precomputed (printable chars only). Lookup is
// by os-release ID (or platform on macOS / Windows); explicit names are matched
// case-insensitively. `auto()` picks the best match for the running system; an
// unrecognised distro falls back to the generic `tux` penguin.

mod data;

use data::*;

pub struct Logo {
    pub name: &'static str,
    pub width: u16,
    pub lines: &'static [&'static str],
}

impl Logo {
    pub fn height(&self) -> u16 {
        self.lines.len() as u16
    }
}

pub static ALL: &[&Logo] = &[
    &ARCH, &UBUNTU, &DEBIAN, &FEDORA, &MINT, &POP, &MANJARO, &OPENSUSE, &ALPINE,
    &NIXOS, &ENDEAVOUROS, &ARTIX, &GENTOO, &VOID, &KALI, &MACOS, &WINDOWS, &TUX,
];

pub fn lookup(name: &str) -> Option<&'static Logo> {
    let n = name.trim().to_lowercase();
    if n.is_empty() || n == "auto" {
        return auto();
    }
    if n == "none" {
        return None;
    }
    for &logo in ALL {
        if logo.name == n {
            return Some(logo);
        }
    }
    // Common aliases.
    let aliased = match n.as_str() {
        "arch_linux" | "archlinux" => "arch",
        "ubuntu_linux" => "ubuntu",
        "linuxmint" => "mint",
        "pop_os" | "pop!_os" | "popos" => "pop",
        "opensuse-tumbleweed" | "opensuse-leap" | "suse" | "tumbleweed" => "opensuse",
        "manjarolinux" => "manjaro",
        "endeavour" => "endeavouros",
        "darwin" | "mac" | "osx" => "macos",
        "win" | "win32" | "windows10" | "windows11" => "windows",
        "linux" | "penguin" => "tux",
        _ => return None,
    };
    ALL.iter().find(|l| l.name == aliased).copied()
}

pub fn auto() -> Option<&'static Logo> {
    #[cfg(target_os = "macos")]
    {
        return Some(&MACOS);
    }
    #[cfg(target_os = "windows")]
    {
        return Some(&WINDOWS);
    }
    #[cfg(target_os = "linux")]
    {
        let ids = linux_distro_ids();
        for id in &ids {
            if let Some(l) = lookup_strict(id) {
                return Some(l);
            }
        }
        return Some(&TUX);
    }
    #[allow(unreachable_code)]
    {
        Some(&TUX)
    }
}

#[cfg(target_os = "linux")]
fn lookup_strict(name: &str) -> Option<&'static Logo> {
    let n = name.trim().to_lowercase();
    ALL.iter().find(|l| l.name == n).copied()
}

#[cfg(target_os = "linux")]
fn linux_distro_ids() -> Vec<String> {
    let txt = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let mut ids: Vec<String> = Vec::new();
    for line in txt.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim_matches('"').to_string();
        if k == "ID" {
            ids.insert(0, v);
        } else if k == "ID_LIKE" {
            for piece in v.split_whitespace() {
                ids.push(piece.to_string());
            }
        }
    }
    ids
}
