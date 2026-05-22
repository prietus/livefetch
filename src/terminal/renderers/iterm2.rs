use std::io::Write;
use std::time::Duration;

use anyhow::{anyhow, Result};
use base64::Engine;

use crate::image::{Frame, ImageAsset, SourceKind};

use super::Renderer;

/// iTerm2 inline image renderer.
///
/// iTerm2 natively animates inline GIFs, so when we can hand it a GIF we send the
/// whole file once and stop ticking. When the source needs alpha (chroma-keyed,
/// or originally a WebP) we re-encode the decoded frames as an animated GIF with
/// a transparent palette entry. As a last resort we emit a single PNG frame.
pub struct Iterm2Renderer {
    bytes: Vec<u8>,
    cols: u16,
    rows: u16,
    emitted: bool,
}

impl Iterm2Renderer {
    pub fn new(asset: ImageAsset, area_cells: (u16, u16)) -> Self {
        let (cols, rows) = area_cells;
        let has_alpha = frames_have_alpha(&asset.frames);

        let bytes = if asset.kind == SourceKind::Gif && !has_alpha {
            // Original GIF is fully opaque — let iTerm2 animate the raw bytes.
            asset.raw
        } else if asset.frames.len() > 1 {
            // Animated source with (potentially) transparency: re-encode as GIF.
            reencode_gif(&asset.frames).unwrap_or_else(|_| encode_png(&asset.frames[0]))
        } else {
            // Single still frame: PNG keeps alpha cleanly.
            encode_png(&asset.frames[0])
        };

        Iterm2Renderer { bytes, cols, rows, emitted: false }
    }
}

fn frames_have_alpha(frames: &[Frame]) -> bool {
    frames
        .iter()
        .any(|f| f.rgba.chunks_exact(4).any(|p| p[3] < 255))
}

fn reencode_gif(frames: &[Frame]) -> Result<Vec<u8>> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame as ImgFrame, ImageBuffer};

    let mut buf = Vec::new();
    {
        let mut enc = GifEncoder::new_with_speed(&mut buf, 10);
        enc.set_repeat(Repeat::Infinite).map_err(|e| anyhow!("set_repeat: {e}"))?;
        for f in frames {
            let img = ImageBuffer::from_raw(f.width, f.height, f.rgba.clone())
                .ok_or_else(|| anyhow!("bad rgba buffer"))?;
            let ms = f.delay.as_millis().max(20) as u32;
            let delay = Delay::from_numer_denom_ms(ms, 1);
            enc.encode_frame(ImgFrame::from_parts(img, 0, 0, delay))
                .map_err(|e| anyhow!("encode_frame: {e}"))?;
        }
    }
    Ok(buf)
}

fn encode_png(frame: &Frame) -> Vec<u8> {
    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())
        .expect("rgba buffer matches dimensions");
    let mut buf = Vec::new();
    let _ = image::DynamicImage::ImageRgba8(img).write_to(
        &mut std::io::Cursor::new(&mut buf),
        image::ImageFormat::Png,
    );
    buf
}

impl Renderer for Iterm2Renderer {
    fn render_frame(&mut self, out: &mut dyn Write, _frame_idx: usize) -> Result<Duration> {
        if self.emitted {
            return Ok(Duration::from_millis(500));
        }
        // Restore to the saved anchor; iTerm2 handles animation internally
        // after this single emission.
        write!(out, "\x1b8")?;

        let payload = base64::engine::general_purpose::STANDARD.encode(&self.bytes);
        write!(
            out,
            "\x1b]1337;File=inline=1;preserveAspectRatio=1;width={};height={}:{}\x07",
            self.cols, self.rows, payload
        )?;
        self.emitted = true;
        Ok(Duration::from_millis(500))
    }

    fn frame_count(&self) -> usize {
        1
    }
}
