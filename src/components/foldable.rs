//! Collapsible / foldable content block.
//!
//! Like HTML `<details>` — a header line with a toggle arrow that
//! expands/collapses the body content. Useful for hiding long reasoning
//! traces, raw JSON, stack traces, etc.
//!
//! ```text
//!   ▸ Show reasoning
//!   │ The user wants...                         ← hidden when collapsed
//!   │ Step 1: parse the query...
//! ```
//!
//! ```text
//!   ▼ Hide reasoning
//!   │ The user wants...
//!   │ Step 1: parse the query...
//! ```

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::component::{Component, InputOutcome};
use crate::theme::{dimmed, paint_fg, Theme};
use crate::utils::wrap_text;

/// A collapsible content block with a toggle header.
pub struct Foldable {
    theme: Theme,
    label: String,
    content: String,
    collapsed: bool,
    /// Whether this component currently has keyboard focus.
    focused: bool,
}

impl Foldable {
    pub fn new(theme: Theme, label: impl Into<String>) -> Self {
        Self {
            theme,
            label: label.into(),
            content: String::new(),
            collapsed: true,
            focused: false,
        }
    }

    /// Set the body text.
    pub fn content(mut self, text: impl Into<String>) -> Self {
        self.content = text.into();
        self
    }

    /// Set initial collapsed state (default: true = hidden).
    pub fn collapsed(mut self, v: bool) -> Self {
        self.collapsed = v;
        self
    }

    /// Toggle between collapsed and expanded.
    pub fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
    }

    /// Expand the block.
    pub fn expand(&mut self) {
        self.collapsed = false;
    }

    /// Collapse the block.
    pub fn collapse(&mut self) {
        self.collapsed = true;
    }

    /// Whether the block is currently collapsed.
    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }
}

impl Component for Foldable {
    fn render(&self, width: u16) -> Vec<String> {
        let mut out = Vec::new();
        let inner = width.saturating_sub(4); // "  ▼ " prefix

        // Header row: arrow + label.
        let arrow = if self.collapsed { "▸" } else { "▼" };
        let arrow_color = if self.focused {
            self.theme.accent
        } else {
            self.theme.muted
        };
        let mut header = String::new();
        header.push_str("  ");
        header.push_str(&paint_fg(arrow_color, arrow));
        header.push(' ');
        let label_text = if self.collapsed {
            format!("Show {}", self.label)
        } else {
            format!("Hide {}", self.label)
        };
        header.push_str(&dimmed(&self.theme, &label_text));
        out.push(header);

        if !self.collapsed {
            // Body: indented with a muted vertical bar.
            let bar = paint_fg(self.theme.border_muted, "│");
            let body_w = inner.saturating_sub(2);
            for line in self.content.lines() {
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

        out.push(String::new()); // breathing room
        out
    }

    fn handle_input(&mut self, ev: &Event) -> InputOutcome {
        let Event::Key(KeyEvent { code, .. }) = ev else {
            return InputOutcome::Ignored;
        };
        match code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.toggle();
                InputOutcome::Consumed
            }
            _ => InputOutcome::Ignored,
        }
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
    fn collapsed_shows_header_only() {
        let f = Foldable::new(Theme::dark(), "reasoning")
            .content("hidden body")
            .collapsed(true);
        let lines = f.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("▸"), "collapsed arrow: {:?}", p[0]);
        assert!(p[0].contains("Show reasoning"), "show label: {:?}", p[0]);
        assert!(!p.iter().any(|l| l.contains("hidden body")));
    }

    #[test]
    fn expanded_shows_body() {
        let f = Foldable::new(Theme::dark(), "reasoning")
            .content("line one\nline two")
            .collapsed(false);
        let lines = f.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("▼"), "expanded arrow: {:?}", p[0]);
        assert!(p[0].contains("Hide reasoning"), "hide label: {:?}", p[0]);
        assert!(p.iter().any(|l| l.contains("line one")));
        assert!(p.iter().any(|l| l.contains("line two")));
    }

    #[test]
    fn toggle_flips_state() {
        let mut f = Foldable::new(Theme::dark(), "x").collapsed(true);
        assert!(f.is_collapsed());
        f.toggle();
        assert!(!f.is_collapsed());
        f.toggle();
        assert!(f.is_collapsed());
    }

    #[test]
    fn enter_toggles() {
        let mut f = Foldable::new(Theme::dark(), "x").collapsed(true);
        let ev = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let outcome = f.handle_input(&ev);
        assert!(matches!(outcome, InputOutcome::Consumed));
        assert!(!f.is_collapsed());
    }

    #[test]
    fn space_toggles() {
        let mut f = Foldable::new(Theme::dark(), "x").collapsed(true);
        let ev = Event::Key(KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        ));
        let outcome = f.handle_input(&ev);
        assert!(matches!(outcome, InputOutcome::Consumed));
        assert!(!f.is_collapsed());
    }

    #[test]
    fn body_has_left_border() {
        let f = Foldable::new(Theme::dark(), "details")
            .content("inner text")
            .collapsed(false);
        let lines = f.render(40);
        let p = plain(&lines);
        assert!(p.iter().any(|l| l.contains("│")), "border present: {:?}", p);
    }
}
