mod modules;
mod platform;

use crate::config::Config;

pub use modules::{InfoLine, LineKind};

pub fn collect(cfg: &Config) -> Vec<InfoLine> {
    let mut ctx = modules::Ctx::new();
    let mut out = Vec::new();
    for name in &cfg.modules {
        match modules::render(name, &mut ctx) {
            Some(lines) => out.extend(lines),
            None => out.push(InfoLine::field(name, "(unknown module)")),
        }
    }
    out
}
