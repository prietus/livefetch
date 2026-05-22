use std::io::{self, Write};
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct CellSize {
    pub cols: u16,
    #[allow(dead_code)]
    pub rows: u16,
    /// Pixel size of a single cell, if the terminal answered CSI 16t.
    pub cell_px: Option<(u16, u16)>,
}

pub fn cell_pixel_size() -> CellSize {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let cell_px = query_cell_pixels();
    CellSize { cols, rows, cell_px }
}

/// Asks the terminal for the pixel size of a cell using `CSI 16 t`.
/// Returns `None` if the terminal doesn't respond in time.
fn query_cell_pixels() -> Option<(u16, u16)> {
    if !crossterm::tty::IsTty::is_tty(&io::stdout()) {
        return None;
    }
    let _raw = RawGuard::enable().ok()?;
    let mut out = io::stdout();
    out.write_all(b"\x1b[16t").ok()?;
    out.flush().ok()?;
    let resp = read_response(Duration::from_millis(150))?;
    parse_16t(&resp)
}

fn parse_16t(buf: &[u8]) -> Option<(u16, u16)> {
    // Expected: ESC [ 6 ; <H> ; <W> t  (possibly preceded by other bytes)
    let s = std::str::from_utf8(buf).ok()?;
    let start = s.find("\x1b[")?;
    let tail = &s[start + 2..];
    let end = tail.find('t')?;
    let body = &tail[..end];
    let parts: Vec<&str> = body.split(';').collect();
    if parts.len() != 3 || parts[0] != "6" {
        return None;
    }
    let h: u16 = parts[1].parse().ok()?;
    let w: u16 = parts[2].parse().ok()?;
    if w == 0 || h == 0 {
        return None;
    }
    Some((w, h))
}

#[cfg(unix)]
fn read_response(timeout: Duration) -> Option<Vec<u8>> {
    use std::os::unix::io::AsRawFd;
    use std::time::Instant;

    let fd = io::stdin().as_raw_fd();
    let deadline = Instant::now() + timeout;
    let mut buf = Vec::with_capacity(64);

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        let timeout_ms = remaining.as_millis().min(i32::MAX as u128) as i32;
        let ret = unsafe { libc::poll(&mut pfd as *mut _, 1, timeout_ms) };
        if ret < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if ret == 0 {
            break;
        }
        if pfd.revents & libc::POLLIN == 0 {
            break;
        }
        let mut chunk = [0u8; 64];
        let n = unsafe {
            libc::read(
                fd,
                chunk.as_mut_ptr() as *mut libc::c_void,
                chunk.len(),
            )
        };
        if n <= 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n as usize]);
        // CSI t-form responses end in 't'. Bail as soon as we see one.
        if buf.contains(&b't') {
            return Some(buf);
        }
    }
    if buf.is_empty() {
        None
    } else {
        Some(buf)
    }
}

#[cfg(not(unix))]
fn read_response(_: Duration) -> Option<Vec<u8>> {
    // Windows path: deferred. Terminal sizing falls back to (8, 16) which is
    // close enough for Windows Terminal's default font.
    None
}

struct RawGuard;

impl RawGuard {
    fn enable() -> io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}
