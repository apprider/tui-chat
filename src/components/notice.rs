//! System notice component with severity levels.
//!
//! A styled block with a subtle left border and tinted background,
//! used for system messages, warnings, and errors.
//!
//! ```text
//! │ ⚠ Rate limit approaching
//! ```
//!
//! Three severity levels:
//! - **Info** — muted border, no icon
//! - **Warning** — yellow border, ⚠ icon
//! - **Error** — red border, ✗ icon

use crate::component::Component;
use crate::theme::{paint_bg, paint_fg, Theme};
use crate::utils::visible_width;

/// Severity level of a notice.
#[derive(Clone, Copy, Debug)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A one-line or multi-line system notice with a left border.
pub struct Notice {
    theme: Theme,
    severity: Severity,
    text: String,
    /// Width of the left border indicator (1 cell).
    border_w: u16,
}

impl Notice {
    fn new(theme: Theme, severity: Severity, text: impl Into<String>) -> Self {
        Self {
            theme,
            severity,
            text: text.into(),
            border_w: 1,
        }
    }

    /// Convenience constructor for an info notice.
    pub fn info(theme: Theme, text: impl Into<String>) -> Self {
        Self::new(theme, Severity::Info, text)
    }

    /// Convenience constructor for a warning notice.
    pub fn warning(theme: Theme, text: impl Into<String>) -> Self {
        Self::new(theme, Severity::Warning, text)
    }

    /// Convenience constructor for an error notice.
    pub fn error(theme: Theme, text: impl Into<String>) -> Self {
        Self::new(theme, Severity::Error, text)
    }

    /// Set the left border width (default 1).
    pub fn with_border_width(mut self, w: u16) -> Self {
        self.border_w = w.max(1);
        self
    }

    fn border_color(&self) -> crate::theme::Rgb {
        match self.severity {
            Severity::Info => self.theme.border_muted,
            Severity::Warning => self.theme.warning,
            Severity::Error => self.theme.error,
        }
    }

    fn icon(&self) -> &'static str {
        match self.severity {
            Severity::Info => "",
            Severity::Warning => "⚠ ",
            Severity::Error => "✗ ",
        }
    }
}

impl Component for Notice {
    fn render(&self, width: u16) -> Vec<String> {
        let inner_w = width.saturating_sub(2 + self.border_w); // pad + border
        if inner_w == 0 {
            return vec![String::new()];
        }

        let lines: Vec<String> = self
            .text
            .lines()
            .flat_map(|line| crate::utils::wrap_text(line, inner_w))
            .collect();

        let border_repeat = "│".repeat(self.border_w as usize);
        let border = paint_fg(self.border_color(), &border_repeat);

        let icon = self.icon();
        let icon_ansi = if icon.is_empty() {
            String::new()
        } else {
            let color = match self.severity {
                Severity::Warning => self.theme.warning,
                Severity::Error => self.theme.error,
                _ => self.theme.fg,
            };
            paint_fg(color, icon)
        };

        let mut out = Vec::new();
        let _pad = " ".repeat(self.border_w as usize + 1);
        let bg = match self.severity {
            Severity::Info => self.theme.tool_pending_bg,
            Severity::Warning => self.theme.tool_pending_bg,
            Severity::Error => self.theme.error_bg,
        };

        for line in lines {
            let visible = visible_width(&line);
            let trail = (inner_w as usize).saturating_sub(visible);
            let mut s = String::new();
            s.push(' ');
            s.push_str(&border);
            s.push(' ');
            if out.is_empty() && !icon_ansi.is_empty() {
                s.push_str(&icon_ansi);
            }
            s.push_str(&line);
            for _ in 0..trail {
                s.push(' ');
            }
            s.push(' ');
            // Apply background tint
            s = paint_bg(bg, &s);
            out.push(s);
        }

        if out.is_empty() {
            out.push(format!(" {} ", paint_bg(bg, &border)));
        }

        out.push(String::new()); // breathing room
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::utils::strip_ansi;
    use crate::theme::Theme;

    fn plain(lines: &[String]) -> Vec<String> {
        lines.iter().map(|l| strip_ansi(l)).collect()
    }

    #[test]
    fn info_renders_border() {
        let n = Notice::info(Theme::dark(), "hello");
        let lines = n.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("│"), "has left border: {:?}", p);
        assert!(p[0].contains("hello"), "has text: {:?}", p);
    }

    #[test]
    fn warning_has_icon() {
        let n = Notice::warning(Theme::dark(), "careful");
        let lines = n.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("⚠"), "has warning icon: {:?}", p);
        assert!(p[0].contains("careful"), "has text: {:?}", p);
    }

    #[test]
    fn error_has_icon() {
        let n = Notice::error(Theme::dark(), "boom");
        let lines = n.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("✗"), "has error icon: {:?}", p);
    }

    #[test]
    fn info_no_icon() {
        let n = Notice::info(Theme::dark(), "info msg");
        let lines = n.render(40);
        let p = plain(&lines);
        assert!(!p[0].contains("⚠"), "info has no icon: {:?}", p);
        assert!(!p[0].contains("✗"), "info has no icon: {:?}", p);
    }

    #[test]
    fn wraps_long_text() {
        let n = Notice::info(Theme::dark(), "this is a very long message that should wrap to multiple lines when rendered in a narrow viewport");
        let lines = n.render(30);
        assert!(lines.len() > 2, "should wrap: {}", lines.len());
    }

    #[test]
    fn custom_border_width() {
        let n = Notice::info(Theme::dark(), "test").with_border_width(3);
        let lines = n.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("│││"), "triple border: {:?}", p);
    }

    #[test]
    fn empty_text_renders_border_only() {
        let n = Notice::info(Theme::dark(), "");
        let lines = n.render(40);
        assert!(!lines.is_empty());
        let p = plain(&lines);
        assert!(p[0].contains("│"), "border still shown: {:?}", p);
    }

    #[test]
    fn multiline_input() {
        let n = Notice::warning(Theme::dark(), "line one\nline two\nline three");
        let lines = n.render(40);
        let p = plain(&lines);
        assert!(p.iter().any(|l| l.contains("line one")));
        assert!(p.iter().any(|l| l.contains("line two")));
        assert!(p.iter().any(|l| l.contains("line three")));
    }
}
