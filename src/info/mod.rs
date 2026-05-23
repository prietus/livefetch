mod modules;
mod platform;

use std::collections::HashMap;

use crate::config::Config;

pub use modules::{Ctx, InfoLine, LineKind, ALL_MODULES};

/// Owns the long-lived state needed to refresh and re-render the info column.
///
/// Modules that are expensive but stable (OS name, package counts, host model,
/// resolution, GPU list, …) are computed once at construction and cached so the
/// live-refresh loop only pays for the modules that actually change tick to tick
/// (CPU%, memory, network rate, battery, uptime…).
pub struct Builder {
    cfg: Config,
    ctx: Ctx,
    static_cache: HashMap<String, Vec<InfoLine>>,
}

impl Builder {
    pub fn new(cfg: Config) -> Self {
        let mut ctx = Ctx::new();
        let mut static_cache = HashMap::new();
        for name in &cfg.modules {
            if is_static(name) {
                if let Some(lines) = modules::render(name, &mut ctx) {
                    static_cache.insert(name.clone(), lines);
                }
            }
        }
        Builder { cfg, ctx, static_cache }
    }

    /// Refresh underlying sysinfo + advance history rings.
    pub fn tick(&mut self) {
        self.ctx.tick();
    }

    pub fn render(&mut self) -> Vec<InfoLine> {
        let mut out = Vec::new();
        for name in &self.cfg.modules {
            if let Some(cached) = self.static_cache.get(name) {
                out.extend(cached.iter().cloned());
                continue;
            }
            match modules::render(name, &mut self.ctx) {
                Some(lines) => out.extend(lines),
                None => out.push(InfoLine::field(name, "(unknown module)")),
            }
        }
        out
    }
}

fn is_static(name: &str) -> bool {
    matches!(
        name,
        "title"
            | "separator"
            | "os"
            | "host"
            | "kernel"
            | "packages"
            | "shell"
            | "terminal"
            | "de"
            | "wm"
            | "resolution"
            | "gpu"
            | "gpudriver"
            | "locale"
            | "init"
            | "audio"
            | "theme"
            | "session"
            | "localip"
            | "localip6"
            | "colors"
            | "break"
    )
}
