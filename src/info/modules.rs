use sysinfo::{Disks, System};

use super::platform;

#[derive(Clone, Debug)]
pub enum LineKind {
    Field { label: String, value: String },
    Title(String),
    Separator(usize),
    Break,
    Colors,
}

#[derive(Clone, Debug)]
pub struct InfoLine {
    pub kind: LineKind,
}

impl InfoLine {
    pub fn field(label: impl Into<String>, value: impl Into<String>) -> Self {
        InfoLine {
            kind: LineKind::Field {
                label: label.into(),
                value: value.into(),
            },
        }
    }
    pub fn title(s: impl Into<String>) -> Self {
        InfoLine { kind: LineKind::Title(s.into()) }
    }
    pub fn sep(n: usize) -> Self {
        InfoLine { kind: LineKind::Separator(n) }
    }
    pub fn br() -> Self {
        InfoLine { kind: LineKind::Break }
    }
    pub fn colors() -> Self {
        InfoLine { kind: LineKind::Colors }
    }
}

/// Per-collection cache so we don't refresh sysinfo for every module.
pub struct Ctx {
    pub sys: System,
    pub disks: Disks,
}

impl Ctx {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_all();
        sys.refresh_memory();
        Ctx {
            sys,
            disks: Disks::new_with_refreshed_list(),
        }
    }
}

pub fn render(name: &str, ctx: &mut Ctx) -> Option<Vec<InfoLine>> {
    Some(match name {
        "title" => vec![title_line()],
        "separator" => vec![InfoLine::sep(title_width())],
        "os" => vec![InfoLine::field("OS", platform::os())],
        "host" => vec![InfoLine::field("Host", platform::host_model())],
        "kernel" => vec![InfoLine::field(
            "Kernel",
            format!(
                "{} {}",
                System::name().unwrap_or_else(|| "Unknown".into()),
                System::kernel_version().unwrap_or_else(|| "?".into())
            ),
        )],
        "uptime" => vec![InfoLine::field("Uptime", fmt_uptime(System::uptime()))],
        "packages" => {
            let pkgs = platform::packages();
            if pkgs.is_empty() {
                vec![]
            } else {
                vec![InfoLine::field("Packages", pkgs.join(", "))]
            }
        }
        "shell" => vec![InfoLine::field("Shell", platform::shell())],
        "terminal" => vec![InfoLine::field("Terminal", platform::terminal())],
        "de" => match platform::desktop_environment() {
            Some(de) => vec![InfoLine::field("DE", de)],
            None => vec![],
        },
        "wm" => match platform::window_manager() {
            Some(wm) => vec![InfoLine::field("WM", wm)],
            None => vec![],
        },
        "resolution" => match platform::resolution() {
            Some(r) => vec![InfoLine::field("Resolution", r)],
            None => vec![],
        },
        "cpu" => vec![InfoLine::field("CPU", cpu_line(&ctx.sys))],
        "gpu" => {
            let gpus = platform::gpus();
            gpus.into_iter().map(|g| InfoLine::field("GPU", g)).collect()
        }
        "memory" => vec![InfoLine::field(
            "Memory",
            format!(
                "{} / {}",
                fmt_bytes(ctx.sys.used_memory()),
                fmt_bytes(ctx.sys.total_memory())
            ),
        )],
        "swap" => {
            if ctx.sys.total_swap() == 0 {
                vec![]
            } else {
                vec![InfoLine::field(
                    "Swap",
                    format!(
                        "{} / {}",
                        fmt_bytes(ctx.sys.used_swap()),
                        fmt_bytes(ctx.sys.total_swap())
                    ),
                )]
            }
        }
        "disk" => disk_lines(&ctx.disks),
        "battery" => match platform::battery() {
            Some(b) => vec![InfoLine::field("Battery", b)],
            None => vec![],
        },
        "locale" => vec![InfoLine::field("Locale", platform::locale())],
        "init" => match platform::init_system() {
            Some(v) => vec![InfoLine::field("Init", v)],
            None => vec![],
        },
        "loadavg" => match platform::load_average() {
            Some(v) => vec![InfoLine::field("Load", v)],
            None => vec![],
        },
        "cputemp" => match platform::cpu_temperature() {
            Some(v) => vec![InfoLine::field("CPU Temp", v)],
            None => vec![],
        },
        "gpudriver" => platform::gpu_drivers()
            .into_iter()
            .map(|d| InfoLine::field("GPU Driver", d))
            .collect(),
        "audio" => match platform::audio_server() {
            Some(v) => vec![InfoLine::field("Audio", v)],
            None => vec![],
        },
        "session" => match platform::session_type() {
            Some(v) => vec![InfoLine::field("Session", v)],
            None => vec![],
        },
        "localip" => match platform::local_ip() {
            Some(v) => vec![InfoLine::field("Local IP", v)],
            None => vec![],
        },
        "theme" => match platform::theme() {
            Some(v) => vec![InfoLine::field("Theme", v)],
            None => vec![],
        },
        "break" => vec![InfoLine::br()],
        "colors" => vec![InfoLine::colors()],
        _ => return None,
    })
}

fn title_line() -> InfoLine {
    InfoLine::title(format!("{}@{}", whoami::username(), whoami::fallible::hostname().unwrap_or_else(|_| "host".into())))
}

fn title_width() -> usize {
    let n = whoami::username().len() + 1 + whoami::fallible::hostname().unwrap_or_default().len();
    n.max(8)
}

fn cpu_line(sys: &System) -> String {
    let cpu = sys.cpus().first();
    let brand = cpu.map(|c| c.brand().to_string()).unwrap_or_else(|| "Unknown".into());
    let count = sys.cpus().len();
    let freq = cpu.map(|c| c.frequency()).unwrap_or(0);
    if freq > 0 {
        format!("{} ({} cores @ {:.2} GHz)", brand.trim(), count, freq as f32 / 1000.0)
    } else {
        format!("{} ({} cores)", brand.trim(), count)
    }
}

fn disk_lines(disks: &Disks) -> Vec<InfoLine> {
    let mut out = Vec::new();
    for d in disks.iter() {
        // Skip pseudo-FS and snap/loop mounts.
        let mp = d.mount_point().to_string_lossy().to_string();
        let fs = d.file_system().to_string_lossy().to_lowercase();
        if fs.contains("squashfs") || fs.contains("devfs") || fs == "tmpfs" {
            continue;
        }
        if d.total_space() == 0 {
            continue;
        }
        let used = d.total_space() - d.available_space();
        let pct = (used as f64 / d.total_space() as f64 * 100.0) as u32;
        out.push(InfoLine::field(
            format!("Disk ({})", mp),
            format!(
                "{} / {} ({}%)",
                fmt_bytes(used),
                fmt_bytes(d.total_space()),
                pct
            ),
        ));
    }
    out
}

fn fmt_bytes(b: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 || i == 0 {
        format!("{:.0} {}", v, UNITS[i])
    } else {
        format!("{:.2} {}", v, UNITS[i])
    }
}

fn fmt_uptime(secs: u64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3600;
    let m = (secs % 3600) / 60;
    let mut parts = Vec::new();
    if d > 0 {
        parts.push(format!("{}d", d));
    }
    if h > 0 {
        parts.push(format!("{}h", h));
    }
    parts.push(format!("{}m", m));
    parts.join(" ")
}
