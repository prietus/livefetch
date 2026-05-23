use std::process::Command;

pub fn os() -> String {
    let info = os_info::get();
    format!(
        "{} {} {}",
        info.os_type(),
        info.version(),
        std::env::consts::ARCH
    )
}

pub fn host_model() -> String {
    if let Some(out) = ps(
        "(Get-CimInstance Win32_ComputerSystem | Select-Object -ExpandProperty Model)",
    ) {
        let s = out.trim();
        if !s.is_empty() {
            return s.to_string();
        }
    }
    "PC".into()
}

pub fn packages() -> Vec<String> {
    let mut out = Vec::new();
    if let Some(n) = count_lines("winget", &["list"]).map(|n| n.saturating_sub(2)) {
        if n > 0 {
            out.push(format!("{n} (winget)"));
        }
    }
    if let Some(n) = count_lines("scoop", &["list"]).map(|n| n.saturating_sub(2)) {
        if n > 0 {
            out.push(format!("{n} (scoop)"));
        }
    }
    if let Some(n) = count_lines("choco", &["list", "--local-only", "--limit-output"]) {
        if n > 0 {
            out.push(format!("{n} (choco)"));
        }
    }
    out
}

pub fn desktop_environment() -> Option<String> {
    Some("Aero".into())
}

pub fn window_manager() -> Option<String> {
    Some("DWM".into())
}

pub fn resolution() -> Option<String> {
    let out = ps("(Get-CimInstance Win32_VideoController | ForEach-Object { \"$($_.CurrentHorizontalResolution)x$($_.CurrentVerticalResolution)\" }) -join ', '")?;
    let s = out.trim();
    if s.is_empty() || s.starts_with("x") {
        None
    } else {
        Some(s.to_string())
    }
}

pub fn gpus() -> Vec<String> {
    let Some(out) = ps("(Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name) -join \"`n\"") else {
        return Vec::new();
    };
    out.lines()
        .map(|l| l.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn battery() -> Option<String> {
    let out = ps("(Get-CimInstance Win32_Battery | Select-Object -First 1 | ForEach-Object { \"$($_.EstimatedChargeRemaining):$($_.BatteryStatus)\" })")?;
    let s = out.trim();
    if s.is_empty() {
        return None;
    }
    let (pct, status) = s.split_once(':')?;
    let status = match status {
        "1" => "discharging",
        "2" => "AC",
        "3" => "fully charged",
        "4" => "low",
        "5" => "critical",
        "6" => "charging",
        _ => "unknown",
    };
    Some(format!("{pct}% ({status})"))
}

pub fn init_system() -> Option<String> { None }
pub fn load_average() -> Option<String> { None }
pub fn cpu_temperature() -> Option<String> { None }
pub fn gpu_drivers() -> Vec<String> { Vec::new() }
pub fn audio_server() -> Option<String> { None }
pub fn session_type() -> Option<String> { None }
pub fn local_ip() -> Option<String> { None }
pub fn local_ip6() -> Option<String> { None }
pub fn theme() -> Option<String> { None }

fn ps(script: &str) -> Option<String> {
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
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
