use std::io::Write;
use std::time::Duration;

use anyhow::Result;
use base64::Engine;
use image::imageops::FilterType;
use image::{ImageBuffer, RgbaImage};

use crate::image::{Frame, ImageAsset};
use crate::terminal::CellSize;

use super::Renderer;

/// Kitty graphics protocol renderer.
///
/// Strategy: upload every frame ONCE (each with its own image ID) on the first
/// render call, then the animation loop only emits lightweight placement
/// commands (~40 bytes each) reusing a single placement ID. Kitty overwrites
/// an existing placement atomically when (image_id, placement_id) reuses the
/// placement_id slot, so frames swap without the brief clear that you get when
/// re-transmitting image data under the same image ID.
pub struct KittyRenderer {
    frames: Vec<PreparedFrame>,
    /// Image IDs are allocated sequentially: frame `i` uses `base_id + i`.
    base_id: u32,
    placement_id: u32,
    cols: u16,
    /// Have we sent all frames to the terminal yet?
    uploaded: bool,
    /// The image_id of the frame we placed last tick, so we can remove its
    /// placement after the next frame is in place.
    prev_placed: Option<u32>,
}

struct PreparedFrame {
    rgba: Vec<u8>,
    w: u32,
    h: u32,
    delay: Duration,
}

impl KittyRenderer {
    pub fn new(asset: ImageAsset, area_cells: (u16, u16), cell: CellSize) -> Self {
        let (cols, rows) = area_cells;
        let (cell_w, cell_h) = cell.cell_px.unwrap_or((8, 16));
        let max_w_px = cols as u32 * cell_w as u32;
        let max_h_px = rows as u32 * cell_h as u32;

        let frames: Vec<_> = asset
            .frames
            .iter()
            .map(|f| prepare(f, max_w_px, max_h_px))
            .collect();

        KittyRenderer {
            base_id: alloc_id_range(frames.len() as u32),
            frames,
            placement_id: 1,
            cols,
            uploaded: false,
            prev_placed: None,
        }
    }

    fn upload_all(&mut self, out: &mut dyn Write) -> Result<()> {
        for (i, frame) in self.frames.iter().enumerate() {
            let id = self.frame_id(i);
            transmit_data(out, id, frame)?;
        }
        Ok(())
    }

    fn frame_id(&self, i: usize) -> u32 {
        self.base_id.wrapping_add(i as u32)
    }
}

fn prepare(frame: &Frame, max_w_px: u32, max_h_px: u32) -> PreparedFrame {
    let src: RgbaImage = ImageBuffer::from_raw(frame.width, frame.height, frame.rgba.clone())
        .expect("rgba buffer matches dimensions");
    let aspect = frame.width as f32 / frame.height as f32;
    let mut new_w = max_w_px;
    let mut new_h = (new_w as f32 / aspect).round() as u32;
    if new_h > max_h_px {
        new_h = max_h_px;
        new_w = (new_h as f32 * aspect).round() as u32;
    }
    let resized = image::imageops::resize(&src, new_w.max(1), new_h.max(1), FilterType::Triangle);
    PreparedFrame {
        rgba: resized.clone().into_raw(),
        w: resized.width(),
        h: resized.height(),
        delay: frame.delay,
    }
}

/// Reserve `count` consecutive non-zero u32 IDs. We pick a base using a cheap
/// hash of wall-clock time and round up to make sure the whole range stays
/// inside u32 without wrapping past 0.
fn alloc_id_range(count: u32) -> u32 {
    use std::time::SystemTime;
    let n = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u32)
        .unwrap_or(0xC0FE_BEEF);
    let base = (n.wrapping_mul(2654435761) | 1).max(1);
    if base.saturating_add(count) < base {
        // Would wrap past u32::MAX — restart low.
        1
    } else {
        base
    }
}

fn transmit_data(out: &mut dyn Write, id: u32, frame: &PreparedFrame) -> Result<()> {
    // a=t = transmit only, f=32 = RGBA, t=d = direct (base64 payload).
    let payload = base64::engine::general_purpose::STANDARD.encode(&frame.rgba);
    let mut chunks = payload.as_bytes().chunks(4096).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = chunks.peek().is_some();
        if first {
            write!(
                out,
                "\x1b_Ga=t,f=32,t=d,s={},v={},i={},q=2,m={};",
                frame.w,
                frame.h,
                id,
                if more { 1 } else { 0 }
            )?;
            first = false;
        } else {
            write!(out, "\x1b_Gm={},q=2;", if more { 1 } else { 0 })?;
        }
        out.write_all(chunk)?;
        out.write_all(b"\x1b\\")?;
    }
    Ok(())
}

impl Renderer for KittyRenderer {
    fn render_frame(&mut self, out: &mut dyn Write, frame_idx: usize) -> Result<Duration> {
        if !self.uploaded {
            self.upload_all(out)?;
            self.uploaded = true;
        }

        let frame = &self.frames[frame_idx];
        let id = self.frame_id(frame_idx);

        // Placements in Kitty are keyed on (image_id, placement_id) — different
        // image IDs with the same p=1 produce DIFFERENT placements, which
        // accumulate (= ghosting). The fix is to place the new frame first and
        // then remove the previous frame's placement, in that order, so the
        // screen is never blank between frames (= no flicker).
        write!(out, "\x1b8")?; // DECRC: back to top-left of image area
        write!(
            out,
            "\x1b_Ga=p,i={},p={},c={},C=1,q=2;\x1b\\",
            id, self.placement_id, self.cols
        )?;

        if let Some(prev) = self.prev_placed.replace(id) {
            if prev != id {
                // d=i deletes ALL placements of image `prev`, but keeps the
                // image data — so the next loop iteration can re-place it
                // without re-uploading the bytes.
                write!(out, "\x1b_Ga=d,d=i,i={},q=2;\x1b\\", prev)?;
            }
        }

        Ok(frame.delay)
    }

    fn frame_count(&self) -> usize {
        self.frames.len()
    }

    fn finish(&mut self, out: &mut dyn Write) -> Result<()> {
        // Delete every image we uploaded (and its placements) so nothing
        // lingers in the terminal's image store after we exit.
        for i in 0..self.frames.len() {
            let id = self.frame_id(i);
            write!(out, "\x1b_Ga=d,d=I,i={},q=2;\x1b\\", id)?;
        }
        Ok(())
    }
}
