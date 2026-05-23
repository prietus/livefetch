use std::collections::VecDeque;
use std::time::{Duration, Instant};

use sysinfo::{Disks, Networks, System};

use super::platform;

const HIST_LEN: usize = 20;
const BATTERY_TTL: Duration = Duration::from_secs(10);

/// Catalog of every known module name and a one-line description, in the same
/// order they appear in the default layout. Keep this in sync with the match
/// arms in [`render`] — `--list-modules` reads from here.
pub const ALL_MODULES: &[(&str, &str)] = &[
    ("title", "user@hostname header"),
    ("separator", "divider line under the title"),
    ("os", "OS name + version + arch"),
    ("host", "hardware model (DMI / sysctl hw.model)"),
    ("kernel", "kernel name and version"),
    ("init", "init system (systemd, launchd, openrc…)"),
    ("uptime", "time since boot"),
    ("loadavg", "load averages 1/5/15"),
    ("packages", "installed package count per package manager"),
    ("shell", "current shell ($SHELL)"),
    ("terminal", "terminal program ($TERM_PROGRAM / $TERM)"),
    ("session", "session type (Wayland/X11/TTY) and compositor"),
    ("de", "desktop environment"),
    ("wm", "window manager"),
    ("resolution", "active display resolution per connector"),
    ("cpu", "CPU brand, cores, frequency, live usage % + sparkline"),
    ("cputemp", "CPU temperature from hwmon (Linux)"),
    ("gpu", "GPU model with kernel driver in brackets"),
    ("gpudriver", "GPU kernel driver (separate line)"),
    ("memory", "used/total RAM, percentage, bar"),
    ("swap", "used/total swap (hidden when no swap)"),
    ("network", "rx/tx rates with sparklines (excludes loopback)"),
    ("disk", "per-mount used/total/percent"),
    ("localip", "primary IPv4 address + interface"),
    ("localip6", "primary IPv6 address + interface"),
    ("audio", "audio server (PipeWire/PulseAudio/CoreAudio)"),
    ("theme", "GTK theme + icons (Linux) / Light/Dark (macOS)"),
    ("battery", "charge % and AC/charging state"),
    ("locale", "$LC_ALL / $LANG"),
    ("break", "blank line spacer"),
    ("colors", "16-color palette swatches"),
];

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
///
/// Holds the live state for the dashboard: ring buffers of recent samples so
/// modules can render sparklines, plus the previous-tick timestamps needed to
/// turn cumulative counters (network bytes) into per-second rates.
pub struct Ctx {
    pub sys: System,
    pub disks: Disks,
    pub networks: Networks,
    pub cpu_hist: VecDeque<f32>,   // 0..=100
    pub mem_hist: VecDeque<f32>,   // 0..=100
    pub rx_hist: VecDeque<f64>,    // bytes/sec
    pub tx_hist: VecDeque<f64>,    // bytes/sec
    pub last_rx_bps: f64,
    pub last_tx_bps: f64,
    last_tick: Option<Instant>,
    /// Cached `platform::battery()` result with the timestamp it was sampled.
    /// `pmset -g batt` on macOS forks a subprocess that takes tens of ms;
    /// refreshing it every tick (default 500ms) is wasted work for a value
    /// that changes in minutes. TTL = [`BATTERY_TTL`].
    battery_cache: Option<(Option<String>, Instant)>,
}

impl Ctx {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_all();
        sys.refresh_memory();
        Ctx {
            sys,
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            cpu_hist: VecDeque::with_capacity(HIST_LEN),
            mem_hist: VecDeque::with_capacity(HIST_LEN),
            rx_hist: VecDeque::with_capacity(HIST_LEN),
            tx_hist: VecDeque::with_capacity(HIST_LEN),
            last_rx_bps: 0.0,
            last_tx_bps: 0.0,
            last_tick: None,
            battery_cache: None,
        }
    }

    fn battery(&mut self) -> Option<String> {
        let now = Instant::now();
        let fresh = self
            .battery_cache
            .as_ref()
            .is_some_and(|(_, t)| now.duration_since(*t) < BATTERY_TTL);
        if !fresh {
            self.battery_cache = Some((platform::battery(), now));
        }
        self.battery_cache.as_ref().and_then(|(v, _)| v.clone())
    }

    /// Refresh sysinfo and push a fresh sample into each history ring.
    /// Called once per live-refresh tick from the layout loop.
    pub fn tick(&mut self) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.networks.refresh();

        let cpus = self.sys.cpus();
        let cpu_pct = if cpus.is_empty() {
            0.0
        } else {
            cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32
        };
        push_capped(&mut self.cpu_hist, cpu_pct);

        let total = self.sys.total_memory() as f32;
        let used = self.sys.used_memory() as f32;
        let mem_pct = if total > 0.0 { used / total * 100.0 } else { 0.0 };
        push_capped(&mut self.mem_hist, mem_pct);

        // Skip loopback so something like `curl localhost` doesn't dominate
        // the visible rate. Linux uses `lo`, macOS uses `lo0`, BSDs sometimes
        // have multiple (`lo1`…), so match the prefix.
        let (rx_bytes, tx_bytes) = self
            .networks
            .iter()
            .filter(|(name, _)| !is_loopback(name))
            .fold((0u64, 0u64), |(r, t), (_, d)| (r + d.received(), t + d.transmitted()));
        let now = Instant::now();
        let elapsed = self
            .last_tick
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0);
        self.last_tick = Some(now);
        let (rx_bps, tx_bps) = if elapsed > 0.0 {
            (rx_bytes as f64 / elapsed, tx_bytes as f64 / elapsed)
        } else {
            (0.0, 0.0)
        };
        self.last_rx_bps = rx_bps;
        self.last_tx_bps = tx_bps;
        push_capped(&mut self.rx_hist, rx_bps);
        push_capped(&mut self.tx_hist, tx_bps);
    }
}

fn is_loopback(name: &str) -> bool {
    // Match `lo` or `lo<digits>` — covers Linux (`lo`), macOS/BSD (`lo0`,
    // `lo1`…) without false-matching real interfaces like `loopnet0`.
    name == "lo"
        || name
            .strip_prefix("lo")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

fn push_capped<T>(buf: &mut VecDeque<T>, v: T) {
    if buf.len() == HIST_LEN {
        buf.pop_front();
    }
    buf.push_back(v);
}

fn sparkline<I, F>(values: I, to_f32: F, max: Option<f32>) -> String
where
    I: IntoIterator<Item = f32> + Clone,
    F: Fn(f32) -> f32,
{
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let vals: Vec<f32> = values.into_iter().map(&to_f32).collect();
    if vals.is_empty() {
        return String::new();
    }
    let cap = max.unwrap_or_else(|| vals.iter().cloned().fold(0.0_f32, f32::max));
    if cap <= 0.0 {
        return BARS[0].to_string().repeat(vals.len());
    }
    vals.iter()
        .map(|v| {
            let n = (v / cap * 7.0).round().clamp(0.0, 7.0) as usize;
            BARS[n]
        })
        .collect()
}

fn spark_f32(values: &VecDeque<f32>, max: Option<f32>) -> String {
    sparkline(values.iter().copied(), |v| v, max)
}

fn spark_f64(values: &VecDeque<f64>) -> String {
    sparkline(values.iter().map(|v| *v as f32), |v| v, None)
}

fn bar(pct: f32, width: usize) -> String {
    let filled = ((pct / 100.0 * width as f32).round() as usize).min(width);
    let mut s = String::with_capacity(width);
    for i in 0..width {
        s.push(if i < filled { '█' } else { '░' });
    }
    s
}

fn fmt_rate(bps: f64) -> String {
    const UNITS: &[&str] = &["B/s", "KB/s", "MB/s", "GB/s"];
    let mut v = bps;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 || v >= 100.0 {
        format!("{:>4.0} {}", v, UNITS[i])
    } else {
        format!("{:>4.1} {}", v, UNITS[i])
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
        "cpu" => vec![InfoLine::field("CPU", cpu_line(ctx))],
        "gpu" => {
            let gpus = platform::gpus();
            gpus.into_iter().map(|g| InfoLine::field("GPU", g)).collect()
        }
        "memory" => vec![InfoLine::field("Memory", memory_line(ctx))],
        "swap" => match swap_line(ctx) {
            Some(s) => vec![InfoLine::field("Swap", s)],
            None => vec![],
        },
        "network" => network_lines(ctx),
        "disk" => disk_lines(&ctx.disks),
        "battery" => match ctx.battery() {
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
        "localip6" => match platform::local_ip6() {
            Some(v) => vec![InfoLine::field("Local IPv6", v)],
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

fn cpu_line(ctx: &Ctx) -> String {
    let cpu = ctx.sys.cpus().first();
    let brand = cpu.map(|c| c.brand().to_string()).unwrap_or_else(|| "Unknown".into());
    let count = ctx.sys.cpus().len();
    let freq = cpu.map(|c| c.frequency()).unwrap_or(0);
    let base = if freq > 0 {
        format!("{} ({} cores @ {:.2} GHz)", brand.trim(), count, freq as f32 / 1000.0)
    } else {
        format!("{} ({} cores)", brand.trim(), count)
    };
    match ctx.cpu_hist.back() {
        Some(&pct) => format!(
            "{} · {:>4.1}% {}",
            base,
            pct,
            spark_f32(&ctx.cpu_hist, Some(100.0))
        ),
        None => base,
    }
}

fn memory_line(ctx: &Ctx) -> String {
    let used = ctx.sys.used_memory();
    let total = ctx.sys.total_memory();
    let pct = if total > 0 {
        (used as f64 / total as f64 * 100.0) as f32
    } else {
        0.0
    };
    format!(
        "{} / {} ({:>2.0}%) {}",
        fmt_bytes(used),
        fmt_bytes(total),
        pct,
        bar(pct, 10)
    )
}

fn swap_line(ctx: &Ctx) -> Option<String> {
    let total = ctx.sys.total_swap();
    if total == 0 {
        return None;
    }
    let used = ctx.sys.used_swap();
    let pct = (used as f64 / total as f64 * 100.0) as f32;
    Some(format!(
        "{} / {} ({:>2.0}%) {}",
        fmt_bytes(used),
        fmt_bytes(total),
        pct,
        bar(pct, 10)
    ))
}

fn network_lines(ctx: &Ctx) -> Vec<InfoLine> {
    let down = format!("{} {}", fmt_rate(ctx.last_rx_bps), spark_f64(&ctx.rx_hist));
    let up = format!("{} {}", fmt_rate(ctx.last_tx_bps), spark_f64(&ctx.tx_hist));
    vec![
        InfoLine::field("Net ↓", down),
        InfoLine::field("Net ↑", up),
    ]
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
