use anyhow::{anyhow, Result};

use super::loader::{Frame, ImageAsset};

#[derive(Clone, Copy, Debug)]
pub enum ChromaKey {
    None,
    Auto,
    Color(u8, u8, u8),
}

impl ChromaKey {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim().to_ascii_lowercase();
        Ok(match s.as_str() {
            "none" | "" => ChromaKey::None,
            "auto" => ChromaKey::Auto,
            "white" => ChromaKey::Color(255, 255, 255),
            "black" => ChromaKey::Color(0, 0, 0),
            other => {
                let hex = other.trim_start_matches('#');
                if hex.len() != 6 {
                    return Err(anyhow!("--chroma expects none|auto|white|black|#RRGGBB"));
                }
                let r = u8::from_str_radix(&hex[0..2], 16)?;
                let g = u8::from_str_radix(&hex[2..4], 16)?;
                let b = u8::from_str_radix(&hex[4..6], 16)?;
                ChromaKey::Color(r, g, b)
            }
        })
    }
}

/// Mutates the asset in place: pixels matching the chroma color (within tolerance)
/// get alpha=0. Returns true if any frame was modified.
pub fn apply(asset: &mut ImageAsset, key: ChromaKey, tol: u8) -> bool {
    let color = match key {
        ChromaKey::None => return false,
        ChromaKey::Color(r, g, b) => (r, g, b),
        ChromaKey::Auto => match detect_background(&asset.frames[0]) {
            Some(c) => c,
            None => return false,
        },
    };
    let mut touched = false;
    for frame in &mut asset.frames {
        touched |= apply_frame(frame, color, tol);
    }
    touched
}

fn apply_frame(frame: &mut Frame, (kr, kg, kb): (u8, u8, u8), tol: u8) -> bool {
    let mut hit = false;
    let t = tol as i32;
    for px in frame.rgba.chunks_exact_mut(4) {
        let dr = (px[0] as i32 - kr as i32).abs();
        let dg = (px[1] as i32 - kg as i32).abs();
        let db = (px[2] as i32 - kb as i32).abs();
        if dr <= t && dg <= t && db <= t {
            px[3] = 0;
            hit = true;
        }
    }
    hit
}

/// Sample the four corners of the first frame; if a single color dominates,
/// it's the background.
fn detect_background(frame: &Frame) -> Option<(u8, u8, u8)> {
    let w = frame.width as usize;
    let h = frame.height as usize;
    if w < 4 || h < 4 {
        return None;
    }
    let sample = 4usize.min(w.min(h));
    let mut votes: Vec<((u8, u8, u8), u32)> = Vec::new();

    let mut record = |r: u8, g: u8, b: u8, a: u8| {
        if a < 128 {
            return;
        }
        let bucket = (r & 0xF8, g & 0xF8, b & 0xF8);
        if let Some(entry) = votes.iter_mut().find(|(c, _)| *c == bucket) {
            entry.1 += 1;
        } else {
            votes.push((bucket, 1));
        }
    };

    for (cx, cy) in [(0usize, 0usize), (w - sample, 0), (0, h - sample), (w - sample, h - sample)] {
        for dy in 0..sample {
            for dx in 0..sample {
                let idx = ((cy + dy) * w + (cx + dx)) * 4;
                let s = &frame.rgba[idx..idx + 4];
                record(s[0], s[1], s[2], s[3]);
            }
        }
    }
    let total_samples = (sample * sample * 4) as u32;
    let (color, count) = votes.into_iter().max_by_key(|(_, n)| *n)?;
    // Require a dominant color (>60% of opaque corner samples).
    if count * 100 < total_samples * 60 {
        return None;
    }
    Some(color)
}
