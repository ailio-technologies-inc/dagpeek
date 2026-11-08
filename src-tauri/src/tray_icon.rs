//! Draws the tray icon. macOS gets a template image with counts in the title beside it. Windows
//! tray icons cannot show text, so the running count and failure state go into the image.

use dagpeek_core::{MenuBarStyle, Progress};

const FONT: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Glyph {
    /// Connected, nothing to report.
    Idle,
    /// Runs in progress, shown as one ring. `fraction` is where it has got, when known.
    Active { fraction: Option<f64> },
    /// Not known: unreachable or refused. Never drawn as idle.
    Unknown,
}

pub struct Drawn {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub size: u32,
}

struct Canvas {
    w: u32,
    size: u32,
    px: Vec<u8>,
    ink: [u8; 3],
}

impl Canvas {
    fn new(size: u32, ink: [u8; 3]) -> Self {
        Self::wide(size, size, ink)
    }

    fn wide(w: u32, size: u32, ink: [u8; 3]) -> Self {
        Self { w, size, px: vec![0; (w * size * 4) as usize], ink }
    }

    fn put(&mut self, x: u32, y: u32, color: [u8; 3], alpha: f64) {
        if x >= self.w || y >= self.size || alpha <= 0.0 {
            return;
        }
        let i = ((y * self.w + x) * 4) as usize;
        let a = (alpha.min(1.0) * 255.0) as u8;
        if a >= self.px[i + 3] {
            self.px[i..i + 3].copy_from_slice(&color);
            self.px[i + 3] = a;
        }
    }

    /// An anti-aliased ring, drawn only where `keep(angle)` holds. Angle 0 is 12 o'clock, clockwise, 0..1.
    fn ring(&mut self, radius: f64, width: f64, alpha: f64, keep: impl Fn(f64) -> bool) {
        let c = self.size as f64 / 2.0;
        for y in 0..self.size {
            for x in 0..self.w {
                let (dx, dy) = (x as f64 + 0.5 - c, y as f64 + 0.5 - c);
                let edge = width / 2.0 - ((dx * dx + dy * dy).sqrt() - radius).abs();
                let angle = (dx.atan2(-dy) / std::f64::consts::TAU).rem_euclid(1.0);
                if edge > -0.5 && keep(angle) {
                    self.put(x, y, self.ink, (edge + 0.5).clamp(0.0, 1.0) * alpha);
                }
            }
        }
    }

    fn dot(&mut self, cx: f64, cy: f64, radius: f64, color: [u8; 3]) {
        for y in 0..self.size {
            for x in 0..self.w {
                let d = ((x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - cy).powi(2)).sqrt();
                self.put(x, y, color, (radius - d + 0.5).clamp(0.0, 1.0));
            }
        }
    }

    /// Cuts a soft-edged hole, so a badge stays separate from the ring in a template image.
    fn cut(&mut self, cx: f64, cy: f64, radius: f64) {
        for y in 0..self.size {
            for x in 0..self.w {
                let d = ((x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - cy).powi(2)).sqrt();
                let keep = (d - radius + 0.5).clamp(0.0, 1.0);
                let i = ((y * self.w + x) * 4 + 3) as usize;
                self.px[i] = (self.px[i] as f64 * keep) as u8;
            }
        }
    }

    fn digit(&mut self, digit: usize, left: u32, top: u32, scale: u32) {
        for (row, bits) in FONT[digit].iter().enumerate() {
            for col in 0..3u32 {
                if bits & (0b100 >> col) != 0 {
                    for sx in 0..scale {
                        for sy in 0..scale {
                            self.put(left + col * scale + sx, top + row as u32 * scale + sy, self.ink, 1.0);
                        }
                    }
                }
            }
        }
    }
}

fn draw_ring(c: &mut Canvas, glyph: Glyph) {
    let size = c.size as f64;
    let (radius, width) = (size * 0.38, size * 0.13);
    match glyph {
        Glyph::Idle => c.ring(radius, width, 1.0, |_| true),
        Glyph::Active { fraction: Some(f) } => {
            c.ring(radius, width, 0.3, |_| true);
            c.ring(radius, width, 1.0, move |a| a <= f.clamp(0.0, 1.0));
        }
        Glyph::Active { fraction: None } => {
            c.ring(radius, width, 0.3, |_| true);
            c.ring(radius, width, 1.0, |a| a <= 0.3);
        }
        // A broken ring: unknown must not read as idle.
        Glyph::Unknown => c.ring(radius, width, 1.0, |a| (a * 8.0).fract() < 0.5),
    }
}

/// A template image for the macOS menu bar, at 2x of its 22 pt height.
pub fn draw_macos(glyph: Glyph, failing: bool) -> Drawn {
    let mut c = Canvas::new(44, [0, 0, 0]);
    draw_ring(&mut c, glyph);
    if glyph == Glyph::Idle {
        c.dot(22.0, 22.0, 5.0, [0, 0, 0]);
    }
    if failing {
        c.cut(35.0, 9.0, 10.5);
        c.dot(35.0, 9.0, 7.0, [0, 0, 0]);
    }
    Drawn { rgba: c.px, width: 44, size: 44 }
}

/// A colour icon for the Windows notification area: `running` in the middle, a red dot for failures.
pub fn draw_windows(glyph: Glyph, running: usize, failures: usize, dark_taskbar: bool) -> Drawn {
    let ink = if dark_taskbar { [245, 247, 250] } else { [24, 27, 33] };
    let mut c = Canvas::new(32, ink);
    draw_ring(&mut c, glyph);
    if running > 0 {
        let d = running.min(9);
        c.digit(d, 16 - 3, 16 - 5, 2);
    }
    if failures > 0 {
        c.dot(25.0, 7.0, 6.5, [222, 56, 56]);
    }
    Drawn { rgba: c.px, width: 32, size: 32 }
}

fn short(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        _ => format!("{}h", secs / 3600),
    }
}

/// The macOS title for a chosen style. `single` is the progress of the only running run.
pub fn title(style: MenuBarStyle, running: usize, queued: usize, failures: usize, single: Option<&Progress>) -> Option<String> {
    match style {
        MenuBarStyle::Icon => None,
        MenuBarStyle::Failures => (failures > 0).then(|| failures.to_string()),
        MenuBarStyle::Progress => match single {
            Some(p) => Some(match p.eta_secs {
                Some(eta) => format!("{}/{} \u{b7} {}", p.done, p.total, short(eta)),
                None => format!("{}/{}", p.done, p.total),
            }),
            None if running > 1 => Some(format!("{running} running")),
            None => None,
        },
        MenuBarStyle::Words => {
            let mut parts = vec![];
            if running > 0 {
                parts.push(format!("{running} running"));
            } else if queued > 0 {
                parts.push(format!("{queued} queued"));
            }
            if failures > 0 {
                parts.push(format!("{failures} failed"));
            }
            (!parts.is_empty()).then(|| parts.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_sum(d: &Drawn) -> u64 {
        d.rgba.chunks(4).map(|p| p[3] as u64).sum()
    }

    #[test]
    fn progress_ring_fills_with_the_fraction() {
        let low = alpha_sum(&draw_macos(Glyph::Active { fraction: Some(0.1) }, false));
        let high = alpha_sum(&draw_macos(Glyph::Active { fraction: Some(0.9) }, false));
        assert!(high > low);
    }

    #[test]
    fn unknown_differs_from_idle() {
        assert_ne!(draw_macos(Glyph::Unknown, false).rgba, draw_macos(Glyph::Idle, false).rgba);
        assert_ne!(draw_macos(Glyph::Idle, true).rgba, draw_macos(Glyph::Idle, false).rgba);
        assert_ne!(draw_windows(Glyph::Unknown, 0, 0, true).rgba, draw_windows(Glyph::Idle, 0, 0, true).rgba);
    }

    #[test]
    fn each_style_has_its_own_title() {
        let p = Progress { done: 3, total: 6, current_step: None, fraction: Some(0.5), eta_secs: Some(330) };
        assert_eq!(title(MenuBarStyle::Icon, 2, 3, 1, None), None);
        assert_eq!(title(MenuBarStyle::Failures, 1, 0, 11, Some(&p)).as_deref(), Some("11"));
        assert_eq!(title(MenuBarStyle::Failures, 1, 0, 0, Some(&p)), None);
        assert_eq!(title(MenuBarStyle::Progress, 1, 0, 11, Some(&p)).as_deref(), Some("3/6 \u{b7} 5m"));
        assert_eq!(title(MenuBarStyle::Progress, 2, 0, 0, None).as_deref(), Some("2 running"));
        assert_eq!(title(MenuBarStyle::Words, 2, 3, 1, None).as_deref(), Some("2 running, 1 failed"));
        assert_eq!(title(MenuBarStyle::Words, 0, 3, 0, None).as_deref(), Some("3 queued"));
    }

    #[test]
    fn buffers_match_their_size() {
        let d = draw_windows(Glyph::Idle, 3, 1, false);
        assert_eq!(d.rgba.len() as u32, d.width * d.size * 4);
    }
}
