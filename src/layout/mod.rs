use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::config::Config;
use crate::image::ImageAsset;
use crate::info::{InfoLine, LineKind};
use crate::terminal::{self, cell_pixel_size, renderers, Protocol};

pub fn run(
    cfg: Config,
    info: Vec<InfoLine>,
    image: Option<ImageAsset>,
    proto: Protocol,
) -> Result<()> {
    let cell = cell_pixel_size();
    let info_lines = format_info(&info);

    let image_cols = if image.is_some() && proto != Protocol::None {
        cfg.image_cols.min(cell.cols.saturating_sub(20))
    } else {
        0
    };
    let gutter: u16 = if image_cols > 0 { 2 } else { 0 };
    let info_col = image_cols + gutter;

    let info_rows = info_lines.len() as u16;
    let image_rows = if let Some(asset) = &image {
        compute_image_rows(asset, image_cols, &cell, info_rows)
    } else {
        0
    };
    let total_rows = info_rows.max(image_rows).max(1);

    let mut stdout = io::stdout().lock();

    // Reserve vertical space without querying cursor position:
    //   1. Hide cursor.
    //   2. Print N blank lines (scrolls the viewport if we were near the bottom).
    //   3. Move cursor back up N lines — now we are at the top-left of our region.
    //   4. Save cursor (DECSC). All subsequent paints restore + move relative.
    write!(stdout, "\x1b[?25l")?;
    for _ in 0..total_rows {
        writeln!(stdout)?;
    }
    write!(stdout, "\x1b[{}A", total_rows)?;
    write!(stdout, "\x1b7")?; // DECSC
    stdout.flush()?;

    paint_info(&mut stdout, &info_lines, info_col)?;
    stdout.flush()?;

    let stop = Arc::new(AtomicBool::new(false));
    install_ctrl_c(stop.clone());

    if let Some(asset) = image {
        if image_cols > 0 {
            let mut renderer =
                renderers::build(proto, asset, (image_cols, image_rows), cell);

            if !cfg.animate || renderer.frame_count() <= 1 {
                renderer.render_frame(&mut stdout, 0)?;
                stdout.flush()?;
            } else {
                run_loop(
                    &mut *renderer,
                    &mut stdout,
                    cfg.loop_forever,
                    stop.clone(),
                )?;
            }

            renderer.finish(&mut stdout)?;
        }
    }

    // Drop cursor below the block so the next shell prompt lands cleanly.
    write!(stdout, "\x1b8")?; // DECRC to anchor
    write!(stdout, "\x1b[{}B", total_rows)?; // move down past the block
    write!(stdout, "\r")?; // column 1
    write!(stdout, "\x1b[?25h")?; // show cursor
    stdout.flush()?;
    Ok(())
}

fn run_loop<W: Write>(
    renderer: &mut dyn renderers::Renderer,
    out: &mut W,
    loop_forever: bool,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    let total = renderer.frame_count();
    let mut idx = 0;
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        // All renderers position relative to the saved cursor (DECSC) anchor.
        let delay = renderer.render_frame(out, idx)?;
        out.flush()?;
        sleep_interruptible(delay, &stop);
        idx += 1;
        if idx >= total {
            if !loop_forever {
                break;
            }
            idx = 0;
        }
    }
    Ok(())
}

fn sleep_interruptible(d: Duration, stop: &AtomicBool) {
    let end = Instant::now() + d;
    while Instant::now() < end {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let chunk = (end - Instant::now()).min(Duration::from_millis(50));
        std::thread::sleep(chunk);
    }
}

fn install_ctrl_c(stop: Arc<AtomicBool>) {
    #[cfg(unix)]
    {
        static SIGNALED: AtomicBool = AtomicBool::new(false);
        extern "C" fn handler(_sig: libc::c_int) {
            SIGNALED.store(true, Ordering::Relaxed);
        }
        unsafe {
            libc::signal(libc::SIGINT, handler as *const () as libc::sighandler_t);
        }
        std::thread::spawn(move || loop {
            if SIGNALED.load(Ordering::Relaxed) {
                stop.store(true, Ordering::Relaxed);
                return;
            }
            if stop.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_millis(80));
        });
    }
    #[cfg(not(unix))]
    {
        let _ = stop;
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

fn paint_info<W: Write>(out: &mut W, lines: &[String], col: u16) -> Result<()> {
    for (i, line) in lines.iter().enumerate() {
        // Restore to anchor, move down i rows + right `col` columns.
        write!(out, "\x1b8")?;
        if i > 0 {
            write!(out, "\x1b[{}B", i)?;
        }
        if col > 0 {
            write!(out, "\x1b[{}C", col)?;
        }
        out.write_all(line.as_bytes())?;
    }
    // Leave cursor back at anchor for the next paint.
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
