use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn os() -> String {
    let osr = read_os_release();
    let pretty = osr
        .iter()
        .find_map(|(k, v)| (k == "PRETTY_NAME").then(|| v.clone()))
        .or_else(|| osr.iter().find_map(|(k, v)| (k == "NAME").then(|| v.clone())))
        .unwrap_or_else(|| "Linux".into());
    format!("{} {}", pretty, std::env::consts::ARCH)
}

pub fn host_model() -> String {
    for p in [
        "/sys/devices/virtual/dmi/id/product_name",
        "/sys/devices/virtual/dmi/id/board_name",
        "/sys/firmware/devicetree/base/model",
    ] {
        if let Ok(s) = fs::read_to_string(p) {
            let s = s.trim_end_matches('\0').trim();
            if !s.is_empty() {
                return s.into();
            }
        }
    }
    "Linux Host".into()
}

pub fn packages() -> Vec<String> {
    let mut out = Vec::new();
    if let Some(n) = count_lines("dpkg-query", &["-f", ".\n", "-W"]) {
        out.push(format!("{n} (dpkg)"));
    }
    if let Some(n) = count_lines("rpm", &["-qa"]) {
        out.push(format!("{n} (rpm)"));
    }
    if let Some(n) = count_lines("pacman", &["-Qq"]) {
        out.push(format!("{n} (pacman)"));
    }
    if let Some(n) = count_lines("apk", &["info"]) {
        out.push(format!("{n} (apk)"));
    }
    if let Some(n) = count_lines("flatpak", &["list", "--app", "--columns=application"]) {
        if n > 0 {
            out.push(format!("{n} (flatpak)"));
        }
    }
    if let Some(n) = count_lines("snap", &["list"]).map(|n| n.saturating_sub(1)) {
        if n > 0 {
            out.push(format!("{n} (snap)"));
        }
    }
    if let Some(n) = count_lines("nix-env", &["-qa", "--installed", "*"]) {
        if n > 0 {
            out.push(format!("{n} (nix)"));
        }
    }
    out
}

pub fn desktop_environment() -> Option<String> {
    if let Ok(de) = env::var("XDG_CURRENT_DESKTOP") {
        if !de.is_empty() {
            return Some(de);
        }
    }
    if let Ok(de) = env::var("DESKTOP_SESSION") {
        if !de.is_empty() {
            return Some(de);
        }
    }
    // Process-based fallback — skip on TTY since DE/GDM processes can be
    // running for the login screen even when no user has a desktop session.
    if env::var("XDG_SESSION_TYPE").as_deref() == Ok("tty") {
        return None;
    }
    for (proc_name, label) in [
        ("gnome-shell", "GNOME"),
        ("plasmashell", "KDE Plasma"),
        ("xfce4-session", "XFCE"),
        ("mate-session", "MATE"),
        ("cinnamon-session", "Cinnamon"),
        ("lxqt-session", "LXQt"),
        ("lxsession", "LXDE"),
        ("budgie-panel", "Budgie"),
    ] {
        if pgrep_exists(proc_name) {
            return Some(label.into());
        }
    }
    None
}

pub fn window_manager() -> Option<String> {
    // Wayland: ask compositor when possible.
    if env::var("WAYLAND_DISPLAY").is_ok() {
        if env::var("SWAYSOCK").is_ok() || pgrep_exists("sway") {
            return Some("Sway".into());
        }
        if env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() || pgrep_exists("Hyprland") {
            return Some("Hyprland".into());
        }
        if pgrep_exists("river") {
            return Some("river".into());
        }
        if pgrep_exists("niri") {
            return Some("niri".into());
        }
        if pgrep_exists("gnome-shell") {
            return Some("Mutter (Wayland)".into());
        }
        if pgrep_exists("kwin_wayland") {
            return Some("KWin (Wayland)".into());
        }
        let w = env::var("WAYLAND_DISPLAY").unwrap_or_default();
        return Some(format!("Wayland ({w})"));
    }
    // X11: try _NET_WM_NAME via xprop, then wmctrl, then known processes.
    if env::var("DISPLAY").is_ok() {
        if let Some(out) = run("xprop", &["-root", "-notype", "_NET_SUPPORTING_WM_CHECK"]) {
            if let Some(id) = out.split_whitespace().last() {
                if let Some(name) = run("xprop", &["-id", id, "-notype", "_NET_WM_NAME"]) {
                    if let Some(q1) = name.find('"') {
                        if let Some(q2) = name[q1 + 1..].find('"') {
                            let s = &name[q1 + 1..q1 + 1 + q2];
                            if !s.is_empty() {
                                return Some(s.to_string());
                            }
                        }
                    }
                }
            }
        }
        if let Some(out) = run("wmctrl", &["-m"]) {
            for line in out.lines() {
                if let Some(rest) = line.strip_prefix("Name:") {
                    return Some(rest.trim().to_string());
                }
            }
        }
        for (proc_name, label) in [
            ("i3", "i3"),
            ("bspwm", "bspwm"),
            ("dwm", "dwm"),
            ("openbox", "Openbox"),
            ("xfwm4", "Xfwm4"),
            ("kwin_x11", "KWin"),
            ("mutter", "Mutter"),
            ("awesome", "awesome"),
        ] {
            if pgrep_exists(proc_name) {
                return Some(label.into());
            }
        }
    }
    None
}

pub fn resolution() -> Option<String> {
    // Try xrandr first for active mode + connector name + refresh rate.
    if let Some(out) = run("xrandr", &["--current"]) {
        let parts = parse_xrandr(&out);
        if !parts.is_empty() {
            return Some(parts.join(", "));
        }
    }
    drm_resolutions()
}

fn parse_xrandr(out: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut lines = out.lines().peekable();
    while let Some(line) = lines.next() {
        if !line.contains(" connected") {
            continue;
        }
        let name = line.split_whitespace().next().unwrap_or("").to_string();
        // Active mode for connected output: WxH+x+y after "connected[ primary]"
        let after = line.split(" connected").nth(1).unwrap_or("");
        let active_mode = after
            .split_whitespace()
            .find(|t| {
                t.contains('x')
                    && t.chars().next().is_some_and(|c| c.is_ascii_digit())
            })
            .and_then(|t| t.split('+').next())
            .unwrap_or("")
            .to_string();
        // Refresh: look at following indented mode lines for one with `*`.
        let mut refresh = String::new();
        while let Some(next) = lines.peek() {
            if !next.starts_with(' ') && !next.starts_with('\t') {
                break;
            }
            let next = lines.next().unwrap();
            if next.contains('*') {
                for tok in next.split_whitespace() {
                    let t = tok.trim_end_matches(['*', '+']);
                    if t.parse::<f32>().is_ok() {
                        refresh = format!("{t}Hz");
                        break;
                    }
                }
            }
        }
        if active_mode.is_empty() {
            continue;
        }
        if refresh.is_empty() {
            result.push(format!("{name}: {active_mode}"));
        } else {
            result.push(format!("{name}: {active_mode} @ {refresh}"));
        }
    }
    result
}

fn drm_resolutions() -> Option<String> {
    let dir = Path::new("/sys/class/drm");
    let entries = fs::read_dir(dir).ok()?;
    let mut out = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        let status = fs::read_to_string(p.join("status")).unwrap_or_default();
        if status.trim() != "connected" {
            continue;
        }
        let Ok(modes) = fs::read_to_string(p.join("modes")) else {
            continue;
        };
        let Some(first) = modes.lines().find(|l| !l.is_empty()) else {
            continue;
        };
        // Connector dir name is "cardX-CONN-N"; strip the card prefix.
        let raw = e.file_name().to_string_lossy().into_owned();
        let name = raw
            .split_once('-')
            .map(|(_, rest)| rest.to_string())
            .unwrap_or(raw);
        out.push(format!("{name}: {first}"));
    }
    if out.is_empty() {
        None
    } else {
        Some(out.join(", "))
    }
}

pub fn gpus() -> Vec<String> {
    lspci_records()
        .into_iter()
        .map(|r| match r.driver {
            Some(d) => format!("{} {} [{}]", r.vendor, r.device, d),
            None => format!("{} {}", r.vendor, r.device),
        })
        .collect()
}

struct GpuRecord {
    vendor: String,
    device: String,
    driver: Option<String>,
}

fn lspci_records() -> Vec<GpuRecord> {
    // `-vmmk`: machine-readable, one Key: value per line, records separated
    // by blank lines. Includes Driver: when known.
    let Some(out) = run("lspci", &["-vmmk"]) else {
        return Vec::new();
    };
    let mut records = Vec::new();
    for chunk in out.split("\n\n") {
        let mut class = String::new();
        let mut vendor = String::new();
        let mut device = String::new();
        let mut driver: Option<String> = None;
        for line in chunk.lines() {
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            let v = v.trim();
            match k {
                "Class" => class = v.into(),
                "Vendor" => vendor = v.into(),
                "Device" => device = v.into(),
                "Driver" => driver = Some(v.into()),
                _ => {}
            }
        }
        if !(class.contains("VGA") || class.contains("3D") || class.contains("Display")) {
            continue;
        }
        if vendor.is_empty() && device.is_empty() {
            continue;
        }
        records.push(GpuRecord {
            vendor: short_vendor(&vendor),
            device: clean_device(&device),
            driver,
        });
    }
    records
}

fn short_vendor(s: &str) -> String {
    let u = s.to_uppercase();
    // Intel/NVIDIA first: "Intel Corporation" contains the substring "ATI"
    // inside "CorporATIon", so a naive AMD/ATI check would misclassify it.
    if u.contains("INTEL") {
        "Intel".into()
    } else if u.contains("NVIDIA") {
        "NVIDIA".into()
    } else if u.contains("AMD") || u.contains("[AMD") || u.contains("ATI TECH") {
        "AMD".into()
    } else {
        // Strip common corporate suffixes.
        s.replace(" Corporation", "")
            .replace(" Inc.", "")
            .replace(" Corp.", "")
            .replace(", Inc.", "")
            .trim()
            .to_string()
    }
}

fn clean_device(s: &str) -> String {
    if let (Some(open), Some(close)) = (s.find('['), s.rfind(']')) {
        if close > open {
            return s[open + 1..close].trim().to_string();
        }
    }
    s.to_string()
}

pub fn battery() -> Option<String> {
    let base = Path::new("/sys/class/power_supply");
    let entries = fs::read_dir(base).ok()?;
    for e in entries.flatten() {
        let p = e.path();
        let kind = fs::read_to_string(p.join("type")).ok()?;
        if kind.trim() != "Battery" {
            continue;
        }
        let cap = fs::read_to_string(p.join("capacity")).ok()?;
        let status = fs::read_to_string(p.join("status")).unwrap_or_else(|_| "Unknown".into());
        return Some(format!("{}% ({})", cap.trim(), status.trim()));
    }
    None
}

// --- new modules ---

pub fn init_system() -> Option<String> {
    let comm = fs::read_to_string("/proc/1/comm").ok()?.trim().to_string();
    if comm.is_empty() {
        None
    } else {
        Some(comm)
    }
}

pub fn load_average() -> Option<String> {
    let s = fs::read_to_string("/proc/loadavg").ok()?;
    let mut it = s.split_whitespace();
    let one = it.next()?;
    let five = it.next()?;
    let fifteen = it.next()?;
    Some(format!("{one} {five} {fifteen}"))
}

pub fn cpu_temperature() -> Option<String> {
    let dir = Path::new("/sys/class/hwmon");
    let entries = fs::read_dir(dir).ok()?;
    // First pass: prefer coretemp / k10temp / zenpower, "Package" label.
    let mut candidates: Vec<(u8, i64)> = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        let name = fs::read_to_string(p.join("name"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let priority: u8 = match name.as_str() {
            "coretemp" | "k10temp" | "zenpower" => 0,
            "cpu_thermal" => 1,
            "acpitz" => 3,
            _ => 2,
        };
        let Ok(rd) = fs::read_dir(&p) else { continue };
        for f in rd.flatten() {
            let fname = f.file_name().to_string_lossy().into_owned();
            let Some(idx) = fname
                .strip_prefix("temp")
                .and_then(|t| t.strip_suffix("_input"))
            else {
                continue;
            };
            let label = fs::read_to_string(p.join(format!("temp{idx}_label")))
                .unwrap_or_default();
            let label_priority = if label.to_lowercase().contains("package") {
                0
            } else if label.to_lowercase().contains("tctl")
                || label.to_lowercase().contains("tdie")
            {
                0
            } else {
                1
            };
            let Ok(raw) = fs::read_to_string(f.path()) else { continue };
            let Ok(v) = raw.trim().parse::<i64>() else { continue };
            if v <= 0 {
                continue;
            }
            candidates.push((priority * 2 + label_priority, v));
        }
    }
    candidates.sort_by_key(|c| c.0);
    let (_, milli) = candidates.first()?;
    Some(format!("{:.1}°C", *milli as f32 / 1000.0))
}

pub fn gpu_drivers() -> Vec<String> {
    lspci_records()
        .into_iter()
        .filter_map(|r| r.driver)
        .collect()
}

pub fn audio_server() -> Option<String> {
    let runtime = env::var("XDG_RUNTIME_DIR").ok()?;
    let base = Path::new(&runtime);
    if base.join("pipewire-0").exists() || pgrep_exists("pipewire") {
        return Some("PipeWire".into());
    }
    if base.join("pulse/native").exists() || pgrep_exists("pulseaudio") {
        return Some("PulseAudio".into());
    }
    if Path::new("/proc/asound/cards").exists() {
        return Some("ALSA".into());
    }
    None
}

pub fn session_type() -> Option<String> {
    let kind = env::var("XDG_SESSION_TYPE").ok().filter(|s| !s.is_empty());
    let compositor = window_manager();
    match (kind.as_deref(), compositor) {
        (Some("tty"), _) => Some("TTY".into()),
        (Some(k), Some(c)) => Some(format!("{k} ({c})")),
        (Some(k), None) => Some(k.to_string()),
        (None, Some(c)) => Some(c),
        (None, None) => None,
    }
}

pub fn local_ip() -> Option<String> {
    // `ip -4 route get 1.1.1.1` consults the routing table without DNS.
    if let Some(out) = run("ip", &["-4", "route", "get", "1.1.1.1"]) {
        let toks: Vec<&str> = out.split_whitespace().collect();
        let mut ip: Option<&str> = None;
        let mut iface: Option<&str> = None;
        for w in toks.windows(2) {
            match w[0] {
                "src" if ip.is_none() => ip = Some(w[1]),
                "dev" if iface.is_none() => iface = Some(w[1]),
                _ => {}
            }
        }
        if let Some(ip) = ip {
            return Some(match iface {
                Some(i) => format!("{ip} ({i})"),
                None => ip.to_string(),
            });
        }
    }
    // Fallback: first non-loopback IPv4 from `hostname -I`.
    if let Some(out) = run("hostname", &["-I"]) {
        if let Some(ip) = out.split_whitespace().next() {
            if !ip.is_empty() {
                return Some(ip.to_string());
            }
        }
    }
    None
}

pub fn theme() -> Option<String> {
    let mut parts = Vec::new();
    let gtk = read_gtk_setting("gtk-theme-name")
        .or_else(|| gsettings_value("org.gnome.desktop.interface", "gtk-theme"));
    if let Some(t) = gtk {
        parts.push(format!("GTK: {t}"));
    }
    let icons = read_gtk_setting("gtk-icon-theme-name")
        .or_else(|| gsettings_value("org.gnome.desktop.interface", "icon-theme"));
    if let Some(t) = icons {
        parts.push(format!("Icons: {t}"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

fn gsettings_value(schema: &str, key: &str) -> Option<String> {
    let out = run("gsettings", &["get", schema, key])?;
    let s = out.trim().trim_matches('\'').to_string();
    if s.is_empty() || s == "''" {
        None
    } else {
        Some(s)
    }
}

fn read_gtk_setting(key: &str) -> Option<String> {
    let home = env::var("HOME").ok()?;
    for rel in ["/.config/gtk-3.0/settings.ini", "/.config/gtk-4.0/settings.ini"] {
        let p = format!("{home}{rel}");
        if let Ok(txt) = fs::read_to_string(&p) {
            for line in txt.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix(key) {
                    let rest = rest.trim_start();
                    if let Some(rest) = rest.strip_prefix('=') {
                        let v = rest.trim().trim_matches('"').to_string();
                        if !v.is_empty() {
                            return Some(v);
                        }
                    }
                }
            }
        }
    }
    None
}

// --- helpers ---

fn read_os_release() -> Vec<(String, String)> {
    let txt = fs::read_to_string("/etc/os-release").unwrap_or_default();
    txt.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.to_string(), v.trim_matches('"').to_string()))
        })
        .collect()
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn count_lines(cmd: &str, args: &[&str]) -> Option<usize> {
    let s = run(cmd, args)?;
    Some(s.lines().filter(|l| !l.trim().is_empty()).count())
}

fn pgrep_exists(name: &str) -> bool {
    Command::new("pgrep")
        .args(["-x", name])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}
