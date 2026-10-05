mod omarchy;

use ratatui::style::{Color, Style};

pub use omarchy::{find as find_omarchy, spawn_watcher};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dark,
    Light,
}

/// Semantic colors shared by every screen.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub bg: Color,
    pub bg_deep: Color,
    pub bg_raised: Color,
    pub border: Color,
    pub selection: Color,
    pub selection_fg: Color,
    pub fg: Color,
    pub fg_dim: Color,
    pub muted: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub red: Color,
    pub yellow: Color,
    pub green: Color,
    pub cyan: Color,
    pub blue: Color,
    pub magenta: Color,
}

impl Theme {
    pub fn base(&self) -> Style {
        Style::default().bg(self.bg).fg(self.fg)
    }

    pub fn status(&self) -> Style {
        if self.kind == Kind::Light && !matches!(self.fg, Color::Reset) {
            Style::default().bg(self.fg).fg(self.bg)
        } else {
            Style::default().bg(self.bg_deep).fg(self.fg)
        }
    }

    pub fn selected(&self) -> Style {
        Style::default().bg(self.selection).fg(self.selection_fg)
    }
}

#[allow(dead_code)]
pub fn mocha() -> Theme {
    pal(
        "catppuccin-mocha",
        "Catppuccin Mocha",
        Kind::Dark,
        [
            "1e1e2e", "11111b", "313244", "45475a", "45475a", "cdd6f4", "bac2de", "6c7086",
            "cba6f7", "f38ba8", "f9e2af", "a6e3a1", "94e2d5", "89b4fa", "f5c2e7",
        ],
    )
}

/// design-nex.us palette. Logo gradient lives in `brand`, not here.
pub fn dnx() -> Theme {
    pal(
        "dnx",
        "Design Nex.us",
        Kind::Dark,
        [
            "181922", "12131a", "21222c", "44475a", "44475a", "f8f8f2", "bd93f9", "6272a4",
            "ff79c6", "ff5555", "f1fa8c", "50fa7b", "8be9fd", "bd93f9", "ff79c6",
        ],
    )
}

pub fn resolve(omarchy: Option<&Theme>) -> Theme {
    match omarchy {
        Some(om) => {
            let mut theme = om.clone();
            apply_dnx_accents(&mut theme);
            theme
        }
        None => dnx(),
    }
}

fn apply_dnx_accents(theme: &mut Theme) {
    let dnx = dnx();
    theme.accent = dnx.accent;
    theme.on_accent = dnx.on_accent;
    theme.magenta = dnx.magenta;
    theme.cyan = dnx.cyan;
    theme.green = dnx.green;
    theme.red = dnx.red;
    theme.yellow = dnx.yellow;
    theme.blue = dnx.blue;
}

/// bg, deep, raised, border, selection, fg, dim, muted, accent, red, yellow, green, cyan, blue, magenta
fn pal(id: &'static str, name: &'static str, kind: Kind, c: [&str; 15]) -> Theme {
    let accent = hex(c[8]);
    let fg = hex(c[5]);
    let bg = hex(c[0]);
    let selection = hex(c[4]);
    Theme {
        id: id.into(),
        name: name.into(),
        kind,
        bg,
        bg_deep: hex(c[1]),
        bg_raised: hex(c[2]),
        border: hex(c[3]),
        selection,
        selection_fg: readable(selection, fg, bg),
        fg,
        fg_dim: hex(c[6]),
        muted: hex(c[7]),
        accent,
        on_accent: on_color(accent),
        red: hex(c[9]),
        yellow: hex(c[10]),
        green: hex(c[11]),
        cyan: hex(c[12]),
        blue: hex(c[13]),
        magenta: hex(c[14]),
    }
}

pub fn hex(s: &str) -> Color {
    parse_hex(s).unwrap_or(Color::White)
}

pub fn parse_hex(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    Some(Color::Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

pub fn luminance(color: Color) -> f32 {
    match color {
        Color::Rgb(r, g, b) => 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32,
        Color::Black => 0.0,
        Color::White => 255.0,
        Color::Yellow | Color::Cyan | Color::Green | Color::Gray => 170.0,
        _ => 50.0,
    }
}

pub fn on_color(bg: Color) -> Color {
    if luminance(bg) > 150.0 {
        Color::Black
    } else {
        Color::Rgb(255, 255, 255)
    }
}

pub fn readable(bg: Color, prefer: Color, alt: Color) -> Color {
    let prefer_gap = (luminance(bg) - luminance(prefer)).abs();
    let alt_gap = (luminance(bg) - luminance(alt)).abs();
    if prefer_gap >= 90.0 || prefer_gap >= alt_gap {
        prefer
    } else {
        alt
    }
}
