use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::config::Config;
use crate::image::ImageAsset;
use crate::info::{self, InfoLine, LineKind};
use crate::terminal::{self, cell_pixel_size, renderers, Protocol};

pub fn run(cfg: Config, image: Option<ImageAsset>, proto: Protocol) -> Result<()> {
    let cell = cell_pixel_size();

    // Cache the static modules and take a first sysinfo sample so the initial
    // paint already shows real values (CPU% will be zero on the very first
    // tick — that's a sysinfo property — but the live loop fills it in).
    let mut builder = info::Builder::new(cfg.clone());
    builder.tick();
    let info_lines = format_info(&builder.render());
    let info_rows = info_lines.len() as u16;

    let image_cols = if image.is_some() && proto != Protocol::None {
        cfg.image_cols.min(cell.cols.saturating_sub(20))
    } else {
        0
    };
    let gutter: u16 = if image_cols > 0 { 2 } else { 0 };
    let info_col = image_cols + gutter;

    let image_rows = if let Some(asset) = &image {
        compute_image_rows(asset, image_cols, &cell, info_rows)
    } else {
        0
    };
    let total_rows = info_rows.max(image_rows).max(1);

    let mut stdout = io::stdout().lock();

    // Reserve vertical space and save the top-left anchor (DECSC). Every
    // subsequent paint restores to it (DECRC) and moves relative.
    write!(stdout, "\x1b[?25l")?;
    for _ in 0..total_rows {
        writeln!(stdout)?;
    }
    write!(stdout, "\x1b[{}A", total_rows)?;
    write!(stdout, "\x1b7")?;
    stdout.flush()?;

    paint_info(&mut stdout, &info_lines, info_col, info_rows)?;
    stdout.flush()?;

    let mut renderer =
        image.map(|asset| renderers::build(proto, asset, (image_cols, image_rows), cell));

    install_signal_handlers();

    let mut next_frame_at: Option<Instant> = None;
    let mut frame_idx = 0usize;
    if let Some(r) = renderer.as_mut() {
        if image_cols > 0 {
            let delay = r.render_frame(&mut stdout, 0)?;
            stdout.flush()?;
            if cfg.live && cfg.animate && r.frame_count() > 1 {
                next_frame_at = Some(Instant::now() + delay);
            }
        }
    }

    if cfg.live {
        let refresh = Duration::from_millis(cfg.refresh_ms);
        let mut next_info_at = Instant::now() + refresh;

        loop {
            if stop_requested() {
                break;
            }
            let now = Instant::now();

            if let Some(deadline) = next_frame_at {
                if now >= deadline {
                    if let Some(r) = renderer.as_mut() {
                        let total = r.frame_count();
                        frame_idx = (frame_idx + 1) % total.max(1);
                        let delay = r.render_frame(&mut stdout, frame_idx)?;
                        next_frame_at = Some(Instant::now() + delay);
                    }
                }
            }

            if now >= next_info_at {
                builder.tick();
                let lines = format_info(&builder.render());
                paint_info(&mut stdout, &lines, info_col, info_rows)?;
                next_info_at = Instant::now() + refresh;
            }

            stdout.flush()?;

            // Wake at the earlier of the two deadlines, capped at a 50ms
            // heartbeat so the stop flag is checked often. std::thread::sleep
            // retries on EINTR internally, so SIGINT/SIGTERM are not handled
            // until the cap elapses — 50ms is fast enough that the user
            // doesn't notice.
            let now = Instant::now();
            let next = match next_frame_at {
                Some(f) => f.min(next_info_at),
                None => next_info_at,
            };
            let wait = next
                .saturating_duration_since(now)
                .min(Duration::from_millis(50));
            if !wait.is_zero() {
                std::thread::sleep(wait);
            }
        }
    }

    if let Some(r) = renderer.as_mut() {
        r.finish(&mut stdout)?;
    }

    write!(stdout, "\x1b8")?;
    write!(stdout, "\x1b[{}B", total_rows)?;
    write!(stdout, "\r")?;
    write!(stdout, "\x1b[?25h")?;
    stdout.flush()?;
    Ok(())
}

/// Shared flag flipped by the signal handler. AtomicBool ops are async-signal
/// safe on every target we care about (lock-free relaxed store / load).
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

fn stop_requested() -> bool {
    STOP_REQUESTED.load(Ordering::Relaxed)
}

fn install_signal_handlers() {
    #[cfg(unix)]
    unsafe {
        extern "C" fn handler(_sig: libc::c_int) {
            STOP_REQUESTED.store(true, Ordering::Relaxed);
        }
        let h = handler as *const () as libc::sighandler_t;
        libc::signal(libc::SIGINT, h);
        libc::signal(libc::SIGTERM, h);
    }
}

fn compute_image_rows(
    asset: &ImageAsset,
    cols: u16,
    cell: &terminal::CellSize,
    info_rows: u16,
) -> u16 {
    let f = &asset.frames[0];
    let aspect = f.width as f32 / f.height as f32;
    let (cw, ch) = cell.cell_px.unwrap_or((8, 16));
    let px_w = cols as f32 * cw as f32;
    let px_h = px_w / aspect;
    let rows = (px_h / ch as f32).round() as u16;
    rows.max(info_rows).max(1)
}

/// Paint `lines` starting at the saved anchor, indented `col` columns.
///
/// Always paints exactly `reserved` rows: shorter content is padded with blank
/// (erased) rows, longer content is truncated. `\x1b[K` erases from the cursor
/// to the end of the line, so stale text from a previous (longer) paint never
/// leaks into the current frame. The image sits to the left of `col`, so
/// erase-to-end-of-line never touches it.
fn paint_info<W: Write>(
    out: &mut W,
    lines: &[String],
    col: u16,
    reserved: u16,
) -> Result<()> {
    for i in 0..reserved {
        write!(out, "\x1b8")?;
        if i > 0 {
            write!(out, "\x1b[{}B", i)?;
        }
        if col > 0 {
            write!(out, "\x1b[{}C", col)?;
        }
        write!(out, "\x1b[K")?;
        if let Some(line) = lines.get(i as usize) {
            out.write_all(line.as_bytes())?;
        }
    }
    write!(out, "\x1b8")?;
    Ok(())
}

fn format_info(info: &[InfoLine]) -> Vec<String> {
    let mut out = Vec::new();
    for line in info {
        match &line.kind {
            LineKind::Title(s) => out.push(format!("\x1b[1;36m{s}\x1b[0m")),
            LineKind::Separator(n) => out.push(format!("\x1b[2m{}\x1b[0m", "─".repeat(*n))),
            LineKind::Field { label, value } => {
                out.push(format!("\x1b[1;36m{label}\x1b[0m: {value}"));
            }
            LineKind::Break => out.push(String::new()),
            LineKind::Colors => {
                let mut s = String::new();
                for code in 0..8 {
                    s.push_str(&format!("\x1b[4{}m   \x1b[0m", code));
                }
                out.push(s);
                let mut s = String::new();
                for code in 0..8 {
                    s.push_str(&format!("\x1b[10{}m   \x1b[0m", code));
                }
                out.push(s);
            }
        }
    }
    out
}
