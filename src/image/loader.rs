use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

/// A single frame in RGBA8 form, with the delay to wait before drawing the next.
#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub delay: Duration,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceKind {
    Gif,
    Webp,
    Static,
}

#[derive(Clone)]
pub struct ImageAsset {
    pub kind: SourceKind,
    pub frames: Vec<Frame>,
    /// Original bytes — useful for protocols that accept the file directly (iTerm2 gif).
    pub raw: Vec<u8>,
}

pub fn load(path: &Path) -> Result<ImageAsset> {
    let bytes = fs::read(path).with_context(|| format!("reading image {}", path.display()))?;
    let kind = sniff(&bytes, path);
    let frames = match kind {
        SourceKind::Gif => decode_gif(&bytes)?,
        SourceKind::Webp => decode_webp(&bytes)?,
        SourceKind::Static => decode_static(&bytes)?,
    };
    if frames.is_empty() {
        return Err(anyhow!("no frames decoded from {}", path.display()));
    }
    Ok(ImageAsset { kind, frames, raw: bytes })
}

fn sniff(bytes: &[u8], path: &Path) -> SourceKind {
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return SourceKind::Gif;
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return SourceKind::Webp;
    }
    match path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) {
        Some(ext) if ext == "gif" => SourceKind::Gif,
        Some(ext) if ext == "webp" => SourceKind::Webp,
        _ => SourceKind::Static,
    }
}

fn decode_gif(bytes: &[u8]) -> Result<Vec<Frame>> {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = opts.read_info(bytes).context("opening gif decoder")?;
    let width = decoder.width() as u32;
    let height = decoder.height() as u32;

    // Composite frames over a persistent canvas so we honor dispose modes correctly.
    let mut canvas = vec![0u8; (width * height * 4) as usize];
    let mut frames = Vec::new();
    while let Some(frame) = decoder.read_next_frame().context("decoding gif frame")? {
        // Snapshot canvas if we'll need to restore (DisposalMethod::Previous).
        let prev = matches!(frame.dispose, gif::DisposalMethod::Previous).then(|| canvas.clone());

        composite(&mut canvas, width, frame);
        let delay = Duration::from_millis((frame.delay as u64).max(2) * 10);
        frames.push(Frame { width, height, rgba: canvas.clone(), delay });

        match frame.dispose {
            gif::DisposalMethod::Background => clear_rect(&mut canvas, width, frame),
            gif::DisposalMethod::Previous => {
                if let Some(p) = prev {
                    canvas = p;
                }
            }
            _ => {}
        }
    }
    Ok(frames)
}

fn composite(canvas: &mut [u8], canvas_w: u32, frame: &gif::Frame<'_>) {
    let fx = frame.left as u32;
    let fy = frame.top as u32;
    let fw = frame.width as u32;
    let fh = frame.height as u32;
    for row in 0..fh {
        for col in 0..fw {
            let src_idx = ((row * fw + col) * 4) as usize;
            let dst_idx = (((fy + row) * canvas_w + (fx + col)) * 4) as usize;
            let a = frame.buffer[src_idx + 3];
            if a == 0 {
                continue;
            }
            // Source-over: with binary alpha (gif), just overwrite for opaque pixels.
            canvas[dst_idx..dst_idx + 4].copy_from_slice(&frame.buffer[src_idx..src_idx + 4]);
        }
    }
}

fn clear_rect(canvas: &mut [u8], canvas_w: u32, frame: &gif::Frame<'_>) {
    let fx = frame.left as u32;
    let fy = frame.top as u32;
    let fw = frame.width as u32;
    let fh = frame.height as u32;
    for row in 0..fh {
        let start = (((fy + row) * canvas_w + fx) * 4) as usize;
        let end = start + (fw * 4) as usize;
        for b in &mut canvas[start..end] {
            *b = 0;
        }
    }
}

fn decode_webp(bytes: &[u8]) -> Result<Vec<Frame>> {
    use webp_animation::Decoder;
    let decoder = Decoder::new(bytes).map_err(|e| anyhow!("webp decode init: {e:?}"))?;
    let mut frames = Vec::new();
    let mut last_ts: i32 = 0;
    for frame in decoder.into_iter() {
        let (w, h) = frame.dimensions();
        let ts = frame.timestamp();
        let delta = (ts - last_ts).max(20);
        last_ts = ts;
        frames.push(Frame {
            width: w,
            height: h,
            rgba: frame.data().to_vec(),
            delay: Duration::from_millis(delta as u64),
        });
    }
    if frames.is_empty() {
        // Static webp: fall back to image crate for a single frame.
        return decode_static(bytes);
    }
    Ok(frames)
}

fn decode_static(bytes: &[u8]) -> Result<Vec<Frame>> {
    let img = image::load_from_memory(bytes).context("decoding static image")?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(vec![Frame {
        width: w,
        height: h,
        rgba: rgba.into_raw(),
        delay: Duration::from_millis(100),
    }])
}
