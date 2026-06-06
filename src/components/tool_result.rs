//! Tool call / result block.
//!
//! A styled box showing a tool name and its result, with optional
//! expand/collapse. Used when an LLM calls a tool and you want to
//! surface the result inline.
//!
//! ```text
//!   ● read_file                                ← header
//!   │ fn main() {                              ← result body
//!   │     println!("hello");
//!   │ }
//! ```
//!
//! Or when collapsed:
//! ```text
//!   ● read_file · 3 lines                     ← compact
//! ```

use crate::component::Component;
use crate::theme::{bold_fg, dimmed, paint_fg, Theme};
use crate::utils::wrap_text;

/// Outcome of a tool execution.
#[derive(Clone, Copy, Debug)]
pub enum ToolStatus {
    Running,
    Success,
    Error,
}

/// A block showing a tool name and its result.
pub struct ToolResult {
    theme: Theme,
    name: String,
    result: String,
    status: ToolStatus,
    collapsed: bool,
    /// Max lines to show when collapsed (default 3).
    preview_lines: usize,
}

impl ToolResult {
    pub fn new(theme: Theme, name: impl Into<String>) -> Self {
        Self {
            theme,
            name: name.into(),
            result: String::new(),
            status: ToolStatus::Running,
            collapsed: false,
            preview_lines: 3,
        }
    }

    /// Set the result body.
    pub fn result(mut self, text: impl Into<String>) -> Self {
        self.result = text.into();
        self.status = ToolStatus::Success;
        self
    }

    /// Mark as failed with an error message.
    pub fn error(mut self, text: impl Into<String>) -> Self {
        self.result = text.into();
        self.status = ToolStatus::Error;
        self
    }

    /// Set collapsed state.
    pub fn collapsed(mut self, v: bool) -> Self {
        self.collapsed = v;
        self
    }

    /// Set max preview lines when collapsed.
    pub fn with_preview_lines(mut self, n: usize) -> Self {
        self.preview_lines = n.max(1);
        self
    }

    fn glyph(&self) -> &'static str {
        match self.status {
            ToolStatus::Running => "●",
            ToolStatus::Success => "●",
            ToolStatus::Error => "✗",
        }
    }

    fn glyph_color(&self) -> crate::theme::Rgb {
        match self.status {
            ToolStatus::Running => self.theme.accent,
            ToolStatus::Success => self.theme.success,
            ToolStatus::Error => self.theme.error,
        }
    }
}

impl Component for ToolResult {
    fn render(&self, width: u16) -> Vec<String> {
        let mut out = Vec::new();
        let inner = width.saturating_sub(4); // pad + gutter

        // Header: glyph + name + preview count when collapsed.
        let glyph = paint_fg(self.glyph_color(), self.glyph());
        let name = bold_fg(self.theme.fg, &self.name);
        let mut header = String::new();
        header.push_str("  ");
        header.push_str(&glyph);
        header.push(' ');
        header.push_str(&name);

        if self.collapsed {
            let lines: usize = self.result.lines().count();
            let preview = format!(" · {lines} lines");
            header.push_str(&dimmed(&self.theme, &preview));
        }

        out.push(header);

        if !self.collapsed && !self.result.is_empty() {
            let bar = paint_fg(self.theme.border_muted, "│");
            let body_w = inner.saturating_sub(2);
            for line in self.result.lines() {
                for wrapped in wrap_text(line, body_w) {
                    let mut s = String::new();
                    s.push_str("   ");
                    s.push_str(&bar);
                    s.push(' ');
                    s.push_str(&wrapped);
                    out.push(s);
                }
            }
        }

        out.push(String::new());
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
    fn running_shows_glyph() {
        let t = ToolResult::new(Theme::dark(), "ls");
        let lines = t.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("●"), "running glyph: {:?}", p[0]);
        assert!(p[0].contains("ls"), "name: {:?}", p[0]);
    }

    #[test]
    fn success_shows_body() {
        let t = ToolResult::new(Theme::dark(), "read_file")
            .result("fn main() {}")
            .collapsed(false);
        let lines = t.render(40);
        let p = plain(&lines);
        assert!(p.iter().any(|l| l.contains("fn main()")), "body: {:?}", p);
        assert!(p.iter().any(|l| l.contains("│")), "border: {:?}", p);
    }

    #[test]
    fn collapsed_shows_line_count() {
        let t = ToolResult::new(Theme::dark(), "cat")
            .result("one\ntwo\nthree\nfour")
            .collapsed(true);
        let lines = t.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("4 lines"), "line count: {:?}", p[0]);
        assert!(!p.iter().any(|l| l.contains("one")));
    }

    #[test]
    fn error_shows_red_glyph() {
        let t = ToolResult::new(Theme::dark(), "rm").error("permission denied");
        let lines = t.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("✗"), "error glyph: {:?}", p[0]);
        assert!(p[0].contains("rm"), "name: {:?}", p[0]);
        assert!(p.iter().any(|l| l.contains("permission denied")));
    }

    #[test]
    fn empty_result_renders_header_only() {
        let t = ToolResult::new(Theme::dark(), "touch").collapsed(false);
        let lines = t.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("touch"));
        assert_eq!(p.len(), 2); // header + blank
    }
}
