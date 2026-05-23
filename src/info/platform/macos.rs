use std::process::Command;
use std::sync::OnceLock;

pub fn os() -> String {
    let name = run("sw_vers", &["-productName"]).unwrap_or_else(|| "macOS".into());
    let ver = run("sw_vers", &["-productVersion"]).unwrap_or_default();
    let build = run("sw_vers", &["-buildVersion"]).unwrap_or_default();
    let arch = std::env::consts::ARCH;
    let mut s = format!("{name} {ver}");
    if !build.is_empty() {
        s.push_str(&format!(" ({build})"));
    }
    s.push_str(&format!(" {arch}"));
    s
}

pub fn host_model() -> String {
    sysctl("hw.model").unwrap_or_else(|| "Mac".into())
}

pub fn packages() -> Vec<String> {
    let mut out = Vec::new();
    if let Some(n) = count_lines("brew", &["list", "--formula", "-1"]) {
        out.push(format!("{n} (brew)"));
    }
    if let Some(n) = count_lines("brew", &["list", "--cask", "-1"]) {
        if n > 0 {
            out.push(format!("{n} (brew-cask)"));
        }
    }
    if let Some(n) = count_lines("port", &["installed"]).map(|n| n.saturating_sub(1)) {
        if n > 0 {
            out.push(format!("{n} (port)"));
        }
    }
    if let Some(n) = count_lines("mas", &["list"]) {
        if n > 0 {
            out.push(format!("{n} (mas)"));
        }
    }
    out
}

pub fn desktop_environment() -> Option<String> {
    Some("Aqua".into())
}

pub fn window_manager() -> Option<String> {
    Some("Quartz Compositor".into())
}

pub fn resolution() -> Option<String> {
    let out = displays_data()?;
    // Lines look like "          Resolution: 3024 x 1964 Retina"
    let parts: Vec<String> = out
        .lines()
        .filter_map(|l| l.trim().strip_prefix("Resolution:"))
        .map(|s| s.trim().to_string())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

pub fn gpus() -> Vec<String> {
    let Some(out) = displays_data() else {
        return Vec::new();
    };
    let mut gpus = Vec::new();
    for line in out.lines() {
        let line = line.trim();
        if let Some(model) = line.strip_prefix("Chipset Model:") {
            gpus.push(model.trim().to_string());
        }
    }
    gpus
}

/// `system_profiler SPDisplaysDataType` takes 1-3 seconds; both `resolution()`
/// and `gpus()` need it, so cache the first invocation for the process lifetime.
fn displays_data() -> Option<&'static str> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| run("system_profiler", &["SPDisplaysDataType"]))
        .as_deref()
}

pub fn battery() -> Option<String> {
    let out = run("pmset", &["-g", "batt"])?;
    // Look for a line like "...   75%; discharging; 4:32 remaining..."
    let line = out.lines().find(|l| l.contains('%'))?;
    let pct_pos = line.find('%')?;
    let start = line[..pct_pos].rfind(|c: char| !c.is_ascii_digit())? + 1;
    let pct = &line[start..pct_pos];
    let status = if line.contains("charging") {
        "charging"
    } else if line.contains("discharging") {
        "discharging"
    } else if line.contains("AC") || line.contains("charged") {
        "AC"
    } else {
        ""
    };
    Some(if status.is_empty() {
        format!("{pct}%")
    } else {
        format!("{pct}% ({status})")
    })
}

pub fn init_system() -> Option<String> { Some("launchd".into()) }

pub fn load_average() -> Option<String> {
    let out = run("sysctl", &["-n", "vm.loadavg"])?;
    // Output looks like "{ 1.23 4.56 7.89 }"
    let s: String = out.chars().filter(|c| *c != '{' && *c != '}').collect();
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() >= 3 {
        Some(format!("{} {} {}", parts[0], parts[1], parts[2]))
    } else {
        None
    }
}

pub fn cpu_temperature() -> Option<String> { None }
pub fn gpu_drivers() -> Vec<String> { Vec::new() }
pub fn audio_server() -> Option<String> { Some("CoreAudio".into()) }
pub fn session_type() -> Option<String> { Some("Aqua (Quartz)".into()) }

pub fn local_ip() -> Option<String> {
    let iface = default_iface("-inet")?;
    let addrs = run("ipconfig", &["getifaddr", &iface])?;
    let ip = addrs.trim();
    if ip.is_empty() {
        None
    } else {
        Some(format!("{ip} ({iface})"))
    }
}

pub fn local_ip6() -> Option<String> {
    let iface = default_iface("-inet6")?;
    // `ipconfig` has no IPv6 equivalent of `getifaddr`, so parse `ifconfig`
    // output and grab the first non-link-local inet6 entry.
    let out = run("ifconfig", &[&iface])?;
    for line in out.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("inet6 ") else { continue };
        let addr = rest.split_whitespace().next()?;
        let clean = addr.split('%').next().unwrap_or(addr);
        if clean.to_lowercase().starts_with("fe80") {
            continue;
        }
        return Some(format!("{clean} ({iface})"));
    }
    None
}

fn default_iface(family: &str) -> Option<String> {
    let out = run("route", &["-n", "get", family, "default"])?;
    out.lines()
        .find_map(|l| l.trim().strip_prefix("interface:"))
        .map(|s| s.trim().to_string())
}

pub fn theme() -> Option<String> {
    let out = run("defaults", &["read", "-g", "AppleInterfaceStyle"]).unwrap_or_default();
    if out.trim().eq_ignore_ascii_case("dark") {
        Some("Dark".into())
    } else {
        Some("Light".into())
    }
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn count_lines(cmd: &str, args: &[&str]) -> Option<usize> {
    let s = run(cmd, args)?;
    if s.is_empty() {
        Some(0)
    } else {
        Some(s.lines().count())
    }
}

fn sysctl(key: &str) -> Option<String> {
    run("sysctl", &["-n", key])
}
