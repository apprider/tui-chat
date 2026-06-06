//! Color palette ported from pi-mono's `dark.json` theme.
//!
//! All colors are emitted as ANSI truecolor escape sequences so the look
//! matches pi exactly across modern terminals (iTerm2, kitty, Ghostty,
//! Alacritty, WezTerm, modern Windows Terminal). Terminals without 24-bit
//! color will fall back to nearest-256 automatically.

use std::fmt::Write;

#[derive(Clone, Copy, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn fg(self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.0, self.1, self.2)
    }
    pub fn bg(self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.0, self.1, self.2)
    }
}

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const ITALIC: &str = "\x1b[3m";
pub const UNDERLINE: &str = "\x1b[4m";
pub const REVERSE: &str = "\x1b[7m";

/// Pi-mono dark theme.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub accent: Rgb,
    pub border: Rgb,
    pub border_accent: Rgb,
    pub border_muted: Rgb,
    pub success: Rgb,
    pub error: Rgb,
    pub warning: Rgb,
    pub muted: Rgb,
    pub dim: Rgb,
    pub fg: Rgb,
    pub user_msg_bg: Rgb,
    pub tool_pending_bg: Rgb,
    pub success_bg: Rgb,
    pub error_bg: Rgb,
    pub heading: Rgb,
    pub link: Rgb,
    pub code: Rgb,
    pub code_block: Rgb,
    pub code_block_border: Rgb,
    pub list_bullet: Rgb,
    pub select_bg: Rgb,
}

impl Theme {
    pub const fn dark() -> Self {
        Self {
            accent: Rgb(0x8a, 0xbe, 0xb7),
            border: Rgb(0x5f, 0x87, 0xff),
            border_accent: Rgb(0x00, 0xd7, 0xff),
            border_muted: Rgb(0x50, 0x50, 0x50),
            success: Rgb(0xb5, 0xbd, 0x68),
            error: Rgb(0xcc, 0x66, 0x66),
            warning: Rgb(0xff, 0xff, 0x00),
            muted: Rgb(0x80, 0x80, 0x80),
            dim: Rgb(0x66, 0x66, 0x66),
            fg: Rgb(0xc5, 0xc8, 0xc6),
            user_msg_bg: Rgb(0x34, 0x35, 0x41),
            tool_pending_bg: Rgb(0x28, 0x28, 0x32),
            success_bg: Rgb(0x28, 0x32, 0x28),
            error_bg: Rgb(0x3c, 0x28, 0x28),
            heading: Rgb(0xf0, 0xc6, 0x74),
            link: Rgb(0x81, 0xa2, 0xbe),
            code: Rgb(0x8a, 0xbe, 0xb7),
            code_block: Rgb(0xb5, 0xbd, 0x68),
            code_block_border: Rgb(0x50, 0x50, 0x50),
            list_bullet: Rgb(0x8a, 0xbe, 0xb7),
            select_bg: Rgb(0x3a, 0x3a, 0x4a),
        }
    }
}

/// Convenience: paint `text` foreground with `c`, wrap with reset.
pub fn paint_fg(c: Rgb, text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    out.push_str(&c.fg());
    out.push_str(text);
    out.push_str(RESET);
    out
}

/// Convenience: paint `text` background with `c`, wrap with reset.
pub fn paint_bg(c: Rgb, text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    out.push_str(&c.bg());
    out.push_str(text);
    out.push_str(RESET);
    out
}

/// Convenience: bold + foreground color.
pub fn bold_fg(c: Rgb, text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 20);
    out.push_str(BOLD);
    out.push_str(&c.fg());
    out.push_str(text);
    out.push_str(RESET);
    out
}

/// Convenience: dimmed text (uses theme dim color, not SGR DIM, to match pi).
pub fn dimmed(theme: &Theme, text: &str) -> String {
    paint_fg(theme.dim, text)
}

/// Build a horizontal rule of `width` cells using `─`.
pub fn rule(theme: &Theme, width: u16) -> String {
    let mut out = String::with_capacity((width as usize) * 4 + 16);
    out.push_str(&theme.border_muted.fg());
    for _ in 0..width {
        out.push('─');
    }
    out.push_str(RESET);
    out
}

/// Mute helper: style with `theme.muted`.
pub fn muted(theme: &Theme, text: &str) -> String {
    paint_fg(theme.muted, text)
}

#[allow(dead_code)]
pub(crate) fn write_fg(buf: &mut String, c: Rgb) {
    let _ = write!(buf, "\x1b[38;2;{};{};{}m", c.0, c.1, c.2);
}

#[allow(dead_code)]
pub(crate) fn write_bg(buf: &mut String, c: Rgb) {
    let _ = write!(buf, "\x1b[48;2;{};{};{}m", c.0, c.1, c.2);
}
