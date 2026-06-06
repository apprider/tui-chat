//! Generic two-line status bar that handles left/right composition
//! and graceful truncation.
//!
//! This is the building block for app-specific footers. It does not
//! know about tokens, models, or sessions — it just renders dimmed
//! top-line context and bright left/right stats with automatic
//! truncation when the terminal is narrow.
//!
//! ## Example
//!
//! ```rust,no_run
//! use tui_chat::component::Component;
//! use tui_chat::components::status_bar::{StatusBar, StatusColor};
//! use tui_chat::theme::Theme;
//!
//! let theme = Theme::dark();
//! let mut bar = StatusBar::new(theme);
//! bar.add_top("~/my-project").add_top("ready");
//! bar.add_left("items: 42", StatusColor::Accent);
//! bar.add_right("v1.0.0", StatusColor::Muted);
//! let lines = bar.render(80);
//! ```

use crate::component::Component;
use crate::theme::{dimmed, paint_fg, Theme};
use crate::utils::compose_left_right;

/// Which theme color to apply to a status part.
#[derive(Clone, Copy, Debug)]
pub enum StatusColor {
    Accent,
    Border,
    BorderAccent,
    BorderMuted,
    Success,
    Error,
    Warning,
    Muted,
    Dim,
    Fg,
    Heading,
    Link,
    Code,
}

impl StatusColor {
    fn to_rgb(self, theme: Theme) -> crate::theme::Rgb {
        match self {
            StatusColor::Accent => theme.accent,
            StatusColor::Border => theme.border,
            StatusColor::BorderAccent => theme.border_accent,
            StatusColor::BorderMuted => theme.border_muted,
            StatusColor::Success => theme.success,
            StatusColor::Error => theme.error,
            StatusColor::Warning => theme.warning,
            StatusColor::Muted => theme.muted,
            StatusColor::Dim => theme.dim,
            StatusColor::Fg => theme.fg,
            StatusColor::Heading => theme.heading,
            StatusColor::Link => theme.link,
            StatusColor::Code => theme.code,
        }
    }
}

/// One colored fragment in a status bar.
#[derive(Clone, Debug)]
pub struct StatusPart {
    pub text: String,
    pub color: StatusColor,
}

impl StatusPart {
    pub fn new(text: impl Into<String>, color: StatusColor) -> Self {
        Self {
            text: text.into(),
            color,
        }
    }

    fn render(&self, theme: Theme) -> String {
        paint_fg(self.color.to_rgb(theme), &self.text)
    }
}

/// Generic two-line status footer.
///
/// - **Top line**: optional context labels (dimmed, joined by ` · `)
/// - **Bottom line**: `left_parts` joined on the left, `right_parts`
///   joined on the right. When the terminal narrows, the right side
///   is dropped first, then the left is truncated with `…`.
#[derive(Clone, Debug)]
pub struct StatusBar {
    theme: Theme,
    top_parts: Vec<StatusPart>,
    left_parts: Vec<StatusPart>,
    right_parts: Vec<StatusPart>,
    separator: String,
    right_separator: String,
}

impl StatusBar {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            top_parts: Vec::new(),
            left_parts: Vec::new(),
            right_parts: Vec::new(),
            separator: " · ".to_string(),
            right_separator: " · ".to_string(),
        }
    }

    /// Change the separator between parts (default `" · "`).
    pub fn set_separator(mut self, sep: impl Into<String>) -> Self {
        self.separator = sep.into();
        self
    }

    /// Change the separator used between *right-side* parts.
    /// Defaults to the same as `separator`.
    pub fn set_right_separator(mut self, sep: impl Into<String>) -> Self {
        self.right_separator = sep.into();
        self
    }

    /// Add a dimmed label to the top context line.
    pub fn add_top(&mut self, text: impl Into<String>) -> &mut Self {
        self.top_parts.push(StatusPart::new(text, StatusColor::Dim));
        self
    }

    /// Add a colored part to the left side of the bottom line.
    pub fn add_left(&mut self, text: impl Into<String>, color: StatusColor) -> &mut Self {
        self.left_parts.push(StatusPart::new(text, color));
        self
    }

    /// Add a colored part to the right side of the bottom line.
    pub fn add_right(&mut self, text: impl Into<String>, color: StatusColor) -> &mut Self {
        self.right_parts.push(StatusPart::new(text, color));
        self
    }

    /// Set all top parts at once (replaces existing).
    pub fn with_top(&mut self, parts: Vec<StatusPart>) -> &mut Self {
        self.top_parts = parts;
        self
    }

    /// Set all left parts at once (replaces existing).
    pub fn with_left(&mut self, parts: Vec<StatusPart>) -> &mut Self {
        self.left_parts = parts;
        self
    }

    /// Set all right parts at once (replaces existing).
    pub fn with_right(&mut self, parts: Vec<StatusPart>) -> &mut Self {
        self.right_parts = parts;
        self
    }

    fn render_parts(&self, parts: &[StatusPart], sep: &str) -> String {
        if parts.is_empty() {
            return String::new();
        }
        let mut s = String::new();
        for (i, p) in parts.iter().enumerate() {
            if i > 0 {
                s.push_str(&dimmed(&self.theme, sep));
            }
            s.push_str(&p.render(self.theme));
        }
        s
    }

    fn render_top(&self) -> String {
        if self.top_parts.is_empty() {
            return String::new();
        }
        let sep = dimmed(&self.theme, &self.separator);
        let mut s = String::new();
        for (i, p) in self.top_parts.iter().enumerate() {
            if i > 0 {
                s.push_str(&sep);
            }
            s.push_str(&p.render(self.theme));
        }
        s
    }
}

impl Component for StatusBar {
    fn render(&self, width: u16) -> Vec<String> {
        let inner = (width as usize).saturating_sub(2); // 1-cell pad each side
        let mut lines = Vec::new();

        // Top line
        let top = self.render_top();
        if !top.is_empty() {
            lines.push(format!(
                " {} ",
                crate::utils::truncate_to_width(&top, inner)
            ));
        }

        // Bottom line: left + right
        let left = self.render_parts(&self.left_parts, &self.separator);
        let right = self.render_parts(&self.right_parts, &self.right_separator);
        let line = compose_left_right(&left, &right, inner);
        lines.push(format!(" {line} "));

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::strip_ansi;

    #[test]
    fn renders_basic_bar() {
        let theme = Theme::dark();
        let mut bar = StatusBar::new(theme);
        bar.add_top("~/work");
        bar.add_left("ready", StatusColor::Success);
        bar.add_right("v1.0", StatusColor::Muted);

        let lines = bar.render(80);
        assert_eq!(lines.len(), 2);

        let plain: Vec<String> = lines.iter().map(|l| strip_ansi(l)).collect();
        assert!(plain[0].contains("~/work"), "top line: {:?}", plain);
        assert!(plain[1].contains("ready"), "left side: {:?}", plain);
        assert!(plain[1].contains("v1.0"), "right side: {:?}", plain);
    }

    #[test]
    fn truncates_narrow_terminal() {
        let theme = Theme::dark();
        let mut bar = StatusBar::new(theme);
        bar.add_left("loooooooooooooong", StatusColor::Fg);
        bar.add_right("short", StatusColor::Muted);

        let lines = bar.render(12);
        let plain = strip_ansi(&lines[0]);
        // Only left should survive (and be truncated)
        assert!(plain.contains("looooo"), "truncated left: {}", plain);
        assert!(
            !plain.contains("short"),
            "right should be dropped: {}",
            plain
        );
    }

    #[test]
    fn empty_top_line_suppressed() {
        let theme = Theme::dark();
        let bar = StatusBar::new(theme);
        let lines = bar.render(40);
        // Only bottom line rendered
        assert_eq!(lines.len(), 1);
    }
}
