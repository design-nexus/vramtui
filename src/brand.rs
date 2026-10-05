use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Website wordmark gradient: #ff43dc → #d64df1 → #a450f6 → #6c65fa → #5865f2
const STOPS: [(u8, u8, u8); 5] = [
    (0xff, 0x43, 0xdc),
    (0xd6, 0x4d, 0xf1),
    (0xa4, 0x50, 0xf6),
    (0x6c, 0x65, 0xfa),
    (0x58, 0x65, 0xf2),
];

const PIXELS_PER_TICK: usize = 3;

/// 7-row binary glyphs, same grid as design-nex.us.
const GLYPHS: &[(&str, &[&str])] = &[
    ("V", &["10001", "10001", "10001", "01010", "01010", "00100", "00100"]),
    ("R", &["1110", "1001", "1001", "1110", "1010", "1001", "1001"]),
    ("A", &["0110", "1001", "1001", "1111", "1001", "1001", "1001"]),
    ("M", &["10001", "11011", "10101", "10001", "10001", "10001", "10001"]),
    ("T", &["11111", "00100", "00100", "00100", "00100", "00100", "00100"]),
    ("U", &["1001", "1001", "1001", "1001", "1001", "1001", "1111"]),
    ("I", &["111", "010", "010", "010", "010", "010", "111"]),
];

const LABEL: &str = "VRAMTUI";

#[derive(Clone, Copy, Debug)]
pub struct Pixel {
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Debug)]
pub struct Logo {
    pub pixels: Vec<Pixel>,
    pub columns: u16,
    pub revealed: usize,
    pub tick: u32,
    pub done: bool,
}

impl Logo {
    pub fn new() -> Self {
        let (pixels, columns) = build_pixels();
        Self {
            pixels,
            columns,
            revealed: 0,
            tick: 0,
            done: false,
        }
    }

    pub fn replay(&mut self) {
        self.revealed = 0;
        self.tick = 0;
        self.done = false;
    }

    pub fn step(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        if self.revealed < self.pixels.len() {
            self.revealed = (self.revealed + PIXELS_PER_TICK).min(self.pixels.len());
            if self.revealed >= self.pixels.len() {
                self.done = true;
            }
        }
    }

    pub fn color_for_x(&self, x: u16) -> Color {
        let span = self.columns.saturating_sub(1).max(1);
        let t = x as f32 / span as f32;
        let base = gradient_at(t);
        if !self.done {
            return base;
        }
        let wave = (self.tick as f32 * 0.18 + x as f32 * 0.32).sin();
        let amount = 0.08 + 0.22 * (wave * 0.5 + 0.5);
        brighten(base, amount)
    }
}

pub fn gradient_at(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    if STOPS.len() == 1 || t <= 0.0 {
        let (r, g, b) = STOPS[0];
        return Color::Rgb(r, g, b);
    }
    if t >= 1.0 {
        let (r, g, b) = STOPS[STOPS.len() - 1];
        return Color::Rgb(r, g, b);
    }
    let scaled = t * (STOPS.len() - 1) as f32;
    let index = scaled.floor() as usize;
    let next = (index + 1).min(STOPS.len() - 1);
    let blend = scaled - index as f32;
    let (r1, g1, b1) = STOPS[index];
    let (r2, g2, b2) = STOPS[next];
    Color::Rgb(mix(r1, r2, blend), mix(g1, g2, blend), mix(b1, b2, blend))
}

pub fn brighten(color: Color, amount: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            lift(r, amount),
            lift(g, amount),
            lift(b, amount),
        ),
        other => other,
    }
}

fn lift(channel: u8, amount: f32) -> u8 {
    let amount = amount.clamp(0.0, 1.0);
    (channel as f32 + (255.0 - channel as f32) * amount).round() as u8
}

fn mix(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)).round() as u8
}

fn glyph(ch: char) -> Option<&'static [&'static str]> {
    GLYPHS
        .iter()
        .find(|(name, _)| name.starts_with(ch))
        .map(|(_, rows)| *rows)
}

fn build_pixels() -> (Vec<Pixel>, u16) {
    let mut pixels = Vec::new();
    let mut cursor = 0u16;
    for ch in LABEL.chars() {
        let Some(rows) = glyph(ch) else {
            continue;
        };
        let width = rows.first().map(|row| row.len()).unwrap_or(0) as u16;
        for (y, row) in rows.iter().enumerate() {
            for (x, on) in row.chars().enumerate() {
                if on == '1' {
                    pixels.push(Pixel {
                        x: cursor + x as u16,
                        y: y as u16,
                    });
                }
            }
        }
        cursor += width + 1;
    }
    let columns = cursor.saturating_sub(1);
    (pixels, columns)
}

/// Seven terminal rows of full blocks. `scale_x` repeats each pixel horizontally.
pub fn paint_pixel_rows(logo: &Logo, scale_x: u16) -> Vec<Line<'static>> {
    let scale = scale_x.max(1) as usize;
    let cols = logo.columns as usize * scale;
    let mut cells = vec![vec![' '; cols]; 7];
    let mut colors = vec![vec![None; cols]; 7];
    for px in logo.pixels.iter().take(logo.revealed) {
        let y = px.y as usize;
        if y >= 7 {
            continue;
        }
        let color = logo.color_for_x(px.x);
        for dx in 0..scale {
            let x = px.x as usize * scale + dx;
            if x >= cols {
                continue;
            }
            cells[y][x] = '█';
            colors[y][x] = Some(color);
        }
    }
    cells
        .into_iter()
        .zip(colors)
        .map(|(chars, cols)| {
            Line::from(
                chars
                    .into_iter()
                    .zip(cols)
                    .map(|(ch, color)| {
                        if ch == ' ' {
                            Span::raw(" ")
                        } else {
                            Span::styled(
                                ch.to_string(),
                                Style::default().fg(color.unwrap_or(Color::White)),
                            )
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

pub fn paint_word() -> Line<'static> {
    let chars: Vec<char> = LABEL.chars().collect();
    let last = chars.len().saturating_sub(1).max(1);
    Line::from(
        chars
            .into_iter()
            .enumerate()
            .map(|(i, ch)| {
                let t = i as f32 / last as f32;
                Span::styled(
                    ch.to_string(),
                    Style::default()
                        .fg(gradient_at(t))
                        .add_modifier(Modifier::BOLD),
                )
            })
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wordmark_fits_a_narrow_panel() {
        let logo = Logo::new();
        assert_eq!(logo.columns, 36);
        assert!(logo.pixels.len() > 80);
        assert_eq!(logo.pixels.iter().map(|p| p.y).max(), Some(6));
    }

    #[test]
    fn gradient_runs_pink_to_indigo() {
        assert_eq!(gradient_at(0.0), Color::Rgb(0xff, 0x43, 0xdc));
        assert_eq!(gradient_at(1.0), Color::Rgb(0x58, 0x65, 0xf2));
    }

    #[test]
    fn replay_hides_pixels_again() {
        let mut logo = Logo::new();
        for _ in 0..80 {
            logo.step();
        }
        assert!(logo.done);
        assert_eq!(logo.revealed, logo.pixels.len());
        logo.replay();
        assert!(!logo.done);
        assert_eq!(logo.revealed, 0);
    }
}
