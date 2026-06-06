//! Basic passive components: Spacer, Text, TruncatedText.

use crate::component::Component;
use crate::theme::{Rgb, RESET};
use crate::utils::{truncate_to_width, visible_width, wrap_text};

/// Empty rows for vertical spacing.
pub struct Spacer(pub u16);

impl Component for Spacer {
    fn render(&self, _width: u16) -> Vec<String> {
        vec![String::new(); self.0 as usize]
    }
}

/// Multi-line text with word wrapping, optional padding, optional bg.
/// Padding/bg are kept on the struct so the same component can carry
/// styled blocks (e.g. tinted system notes) without callers needing to
/// hand-pad ANSI strings.
pub struct Text {
    text: String,
    pad_x: u16,
    pad_y: u16,
    bg: Option<Rgb>,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            pad_x: 0,
            pad_y: 0,
            bg: None,
        }
    }
}

impl Component for Text {
    fn render(&self, width: u16) -> Vec<String> {
        let inner_w = width.saturating_sub(self.pad_x * 2);
        let mut wrapped = wrap_text(&self.text, inner_w);
        if wrapped.is_empty() {
            wrapped.push(String::new());
        }
        let pad = " ".repeat(self.pad_x as usize);
        let mut lines: Vec<String> = wrapped
            .into_iter()
            .map(|l| {
                let visible = visible_width(&l);
                let trail = (inner_w as usize).saturating_sub(visible);
                let mut s = String::new();
                s.push_str(&pad);
                s.push_str(&l);
                for _ in 0..trail {
                    s.push(' ');
                }
                s.push_str(&pad);
                s
            })
            .collect();
        for _ in 0..self.pad_y {
            lines.insert(0, " ".repeat(width as usize));
            lines.push(" ".repeat(width as usize));
        }
        if let Some(bg) = self.bg {
            lines = lines
                .into_iter()
                .map(|l| {
                    let mut out = String::with_capacity(l.len() + 24);
                    out.push_str(&bg.bg());
                    out.push_str(&l);
                    out.push_str(RESET);
                    out
                })
                .collect();
        }
        lines
    }
}

/// Single-line text truncated with `…` to fit viewport width.
pub struct TruncatedText {
    text: String,
}

impl TruncatedText {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl Component for TruncatedText {
    fn render(&self, width: u16) -> Vec<String> {
        let s = self.text.replace('\n', " ");
        vec![truncate_to_width(&s, width as usize)]
    }
}
