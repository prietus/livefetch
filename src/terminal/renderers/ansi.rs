use std::io::Write;
use std::time::Duration;

use anyhow::Result;
use image::imageops::FilterType;
use image::{ImageBuffer, Rgba, RgbaImage};

use crate::image::{Frame, ImageAsset};
use crate::terminal::CellSize;

use super::Renderer;

/// Renders each frame using ▀ (UPPER HALF BLOCK) cells:
/// foreground color = top pixel, background color = bottom pixel.
/// Each terminal cell covers 2 pixels vertically.
pub struct AnsiRenderer {
    frames: Vec<PreparedFrame>,
}

struct PreparedFrame {
    /// Resized RGBA, width = cols, height = rows * 2.
    img: RgbaImage,
    delay: Duration,
    rows: u16,
}

impl AnsiRenderer {
    pub fn new(asset: ImageAsset, area_cells: (u16, u16), _cell: CellSize) -> Self {
        let (cols, max_rows) = area_cells;
        let target_w = cols as u32;
        let max_h_px = (max_rows as u32) * 2;

        let frames = asset
            .frames
            .iter()
            .map(|f| prepare(f, target_w, max_h_px))
            .collect();
        AnsiRenderer { frames }
    }
}

fn prepare(frame: &Frame, target_w: u32, max_h_px: u32) -> PreparedFrame {
    let src: RgbaImage = ImageBuffer::from_raw(frame.width, frame.height, frame.rgba.clone())
        .expect("rgba buffer matches dimensions");

    let aspect = frame.width as f32 / frame.height as f32;
    let mut new_w = target_w;
    let mut new_h = ((new_w as f32) / aspect).round() as u32;
    if new_h % 2 != 0 {
        new_h += 1;
    }
    if new_h > max_h_px {
        new_h = max_h_px - (max_h_px % 2);
        new_w = ((new_h as f32) * aspect).round() as u32;
        if new_w > target_w {
            new_w = target_w;
        }
    }
    let resized = image::imageops::resize(&src, new_w.max(1), new_h.max(2), FilterType::Triangle);
    let rows = (resized.height() / 2) as u16;
    PreparedFrame { img: resized, delay: frame.delay, rows }
}

impl Renderer for AnsiRenderer {
    fn render_frame(&mut self, out: &mut dyn Write, frame_idx: usize) -> Result<Duration> {
        let frame = &self.frames[frame_idx];
        let w = frame.img.width();
        for row in 0..frame.rows {
            // Restore to anchor (top-left of image area), then move down `row` lines.
            write!(out, "\x1b8")?;
            if row > 0 {
                write!(out, "\x1b[{}B", row)?;
            }
            // Reset attributes so transparent cells don't inherit colors.
            write!(out, "\x1b[0m")?;
            let y_top = (row as u32) * 2;
            let y_bot = y_top + 1;
            for x in 0..w {
                let top = pixel(&frame.img, x, y_top);
                let bot = pixel(&frame.img, x, y_bot);
                emit_cell(out, top, bot)?;
            }
            write!(out, "\x1b[0m")?;
        }
        Ok(frame.delay)
    }

    fn frame_count(&self) -> usize {
        self.frames.len()
    }
}

/// (r, g, b, opaque?)
type Pixel = (u8, u8, u8, bool);

fn emit_cell(out: &mut dyn Write, top: Pixel, bot: Pixel) -> std::io::Result<()> {
    match (top.3, bot.3) {
        (false, false) => {
            // Both transparent: blank cell, default bg, no foreground glyph color needed.
            write!(out, "\x1b[0m ")
        }
        (true, false) => {
            // Top opaque, bottom transparent → use ▀ with fg=top and reset bg to default.
            write!(
                out,
                "\x1b[49m\x1b[38;2;{};{};{}m▀",
                top.0, top.1, top.2
            )
        }
        (false, true) => {
            // Top transparent, bottom opaque → use ▄ with fg=bot and reset bg.
            write!(
                out,
                "\x1b[49m\x1b[38;2;{};{};{}m▄",
                bot.0, bot.1, bot.2
            )
        }
        (true, true) => write!(
            out,
            "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m▀",
            top.0, top.1, top.2, bot.0, bot.1, bot.2
        ),
    }
}

fn pixel(img: &RgbaImage, x: u32, y: u32) -> Pixel {
    if y >= img.height() {
        return (0, 0, 0, false);
    }
    let Rgba([r, g, b, a]) = *img.get_pixel(x, y);
    // Treat anything below half-alpha as transparent (terminals are binary).
    (r, g, b, a >= 128)
}
