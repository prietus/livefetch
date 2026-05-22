mod ansi;
mod iterm2;
mod kitty;

use std::io::Write;
use std::time::Duration;

use anyhow::Result;

use crate::image::ImageAsset;
use crate::terminal::{CellSize, Protocol};

/// A renderer paints one frame of an animation.
///
/// The caller has already saved the cursor with DECSC (`ESC 7`) at the top-left of
/// the image area. Each `render_frame` MUST start by restoring the cursor
/// (`ESC 8`) so positioning works regardless of where the previous frame left it.
pub trait Renderer {
    fn render_frame(&mut self, out: &mut dyn Write, frame_idx: usize) -> Result<Duration>;

    fn frame_count(&self) -> usize;

    fn finish(&mut self, _out: &mut dyn Write) -> Result<()> {
        Ok(())
    }
}

pub fn build(
    proto: Protocol,
    asset: ImageAsset,
    area_cells: (u16, u16),
    cell: CellSize,
) -> Box<dyn Renderer> {
    match proto {
        Protocol::Kitty => Box::new(kitty::KittyRenderer::new(asset, area_cells, cell)),
        Protocol::Iterm2 => Box::new(iterm2::Iterm2Renderer::new(asset, area_cells)),
        Protocol::Ansi | Protocol::None => {
            Box::new(ansi::AnsiRenderer::new(asset, area_cells, cell))
        }
    }
}
