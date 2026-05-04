use std::io::{self, Write};
use std::os::unix::io::AsRawFd;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    Light,
    #[default]
    Dark,
}

static THEME: OnceLock<Theme> = OnceLock::new();

pub fn current() -> Theme {
    THEME.get().copied().unwrap_or_default()
}

/// Detect the terminal background and store the result. Safe to call once at
/// startup, before entering any rendering loop. Falls back to `Dark` if the
/// terminal does not respond to OSC 11.
pub fn init() {
    let detected = detect().unwrap_or_default();
    let _ = THEME.set(detected);
}

/// Heading marker / text color for the given tier (1..=6).
pub fn heading_style(tier: u8) -> Style {
    let fg = match (current(), tier) {
        (Theme::Light, 1) => Color::Blue,
        (Theme::Light, 2) => Color::Indexed(28),
        (Theme::Light, 3) => Color::Magenta,
        (Theme::Light, _) => Color::DarkGray,
        (Theme::Dark, 1) => Color::Cyan,
        (Theme::Dark, 2) => Color::Blue,
        (Theme::Dark, 3) => Color::Magenta,
        (Theme::Dark, _) => Color::Gray,
    };
    Style::default().fg(fg).add_modifier(Modifier::BOLD)
}

/// Foreground color for unordered list bullets (`• `).
pub fn bullet_color() -> Color {
    match current() {
        Theme::Light => Color::Black,
        Theme::Dark => Color::White,
    }
}

fn detect() -> Option<Theme> {
    enable_raw_mode().ok()?;
    let result = query_background();
    let _ = disable_raw_mode();
    let (r, g, b) = result?;
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    Some(if lum > 0.5 { Theme::Light } else { Theme::Dark })
}

fn query_background() -> Option<(f64, f64, f64)> {
    let mut stdout = io::stdout();
    stdout.write_all(b"\x1b]11;?\x1b\\").ok()?;
    stdout.flush().ok()?;

    let bytes = read_with_timeout(Duration::from_millis(100))?;
    parse_osc11(&bytes)
}

fn read_with_timeout(timeout: Duration) -> Option<Vec<u8>> {
    let fd = io::stdin().as_raw_fd();
    let deadline = Instant::now() + timeout;
    let mut buf: Vec<u8> = Vec::new();

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }

        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let timeout_ms = remaining.as_millis().min(i32::MAX as u128) as i32;
        let ret = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if ret <= 0 {
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

        // OSC response terminates with BEL (\x07) or ST (\x1b\\).
        let terminated = buf.contains(&0x07)
            || buf.windows(2).any(|w| w == [0x1b, b'\\']);
        if terminated {
            break;
        }
    }

    if buf.is_empty() {
        None
    } else {
        Some(buf)
    }
}

fn parse_osc11(response: &[u8]) -> Option<(f64, f64, f64)> {
    let s = std::str::from_utf8(response).ok()?;
    let idx = s.find("rgb:")?;
    let rest = &s[idx + 4..];
    let end = rest
        .find(|c: char| c == '\x07' || c == '\x1b')
        .unwrap_or(rest.len());
    let rgb = &rest[..end];
    let parts: Vec<&str> = rgb.split('/').collect();
    if parts.len() != 3 {
        return None;
    }
    let parse = |p: &str| -> Option<f64> {
        let p = p.trim();
        if p.is_empty() {
            return None;
        }
        let v = u32::from_str_radix(p, 16).ok()?;
        let max = ((1u64 << (p.len() * 4)) - 1) as f64;
        Some(v as f64 / max)
    };
    Some((parse(parts[0])?, parse(parts[1])?, parse(parts[2])?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_8_bit_rgb() {
        let (r, g, b) = parse_osc11(b"\x1b]11;rgb:ff/ff/ff\x07").unwrap();
        assert!((r - 1.0).abs() < 1e-6);
        assert!((g - 1.0).abs() < 1e-6);
        assert!((b - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parses_16_bit_rgb() {
        let (r, g, b) = parse_osc11(b"\x1b]11;rgb:0000/0000/0000\x1b\\").unwrap();
        assert_eq!(r, 0.0);
        assert_eq!(g, 0.0);
        assert_eq!(b, 0.0);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_osc11(b"hello").is_none());
        assert!(parse_osc11(b"\x1b]11;rgb:gg/gg/gg\x07").is_none());
    }
}
