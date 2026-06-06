//! Chat message components: user message (boxed bg), streaming
//! assistant message (full Markdown via `components::markdown`), and
//! the live status row.

use crate::component::Component;
use crate::theme::{paint_fg, Theme, RESET};
use crate::utils::{visible_width, wrap_text};

/// User-typed message — wrapped in a box with a tinted background.
pub struct UserMessage {
    theme: Theme,
    text: String,
}

impl UserMessage {
    pub fn new(theme: Theme, text: impl Into<String>) -> Self {
        Self {
            theme,
            text: text.into(),
        }
    }
}

impl Component for UserMessage {
    fn render(&self, width: u16) -> Vec<String> {
        let inner_w = width.saturating_sub(4); // 2 cells pad each side
        let mut out = Vec::new();
        let bg = self.theme.user_msg_bg.bg();
        // top pad
        out.push(format!("{}{}{}", bg, " ".repeat(width as usize), RESET));
        for line in wrap_text(&self.text, inner_w) {
            let visible = visible_width(&line);
            let trail = (inner_w as usize).saturating_sub(visible);
            let mut s = String::new();
            s.push_str(&bg);
            s.push_str("  ");
            s.push_str(&line);
            for _ in 0..trail {
                s.push(' ');
            }
            s.push_str("  ");
            s.push_str(RESET);
            out.push(s);
        }
        // bottom pad
        out.push(format!("{}{}{}", bg, " ".repeat(width as usize), RESET));
        out.push(String::new());
        out
    }
}

/// Assistant response — full Markdown rendering with syntax highlighting
/// (delegated to `components::markdown`). Tracks thinking/reasoning text
/// separately so it can be rendered inline above the main response in
/// muted styling, à la pi-mono and Claude Code.
pub struct AssistantMessage {
    theme: Theme,
    thinking: String,
    text: String,
}

impl AssistantMessage {
    pub fn new(theme: Theme, text: impl Into<String>) -> Self {
        Self {
            theme,
            thinking: String::new(),
            text: text.into(),
        }
    }

    pub fn append(&mut self, chunk: &str) {
        self.text.push_str(chunk);
    }

    pub fn append_thinking(&mut self, chunk: &str) {
        self.thinking.push_str(chunk);
    }
}

impl Component for AssistantMessage {
    fn render(&self, width: u16) -> Vec<String> {
        let mut out = Vec::new();

        if !self.thinking.is_empty() {
            // Header label
            out.push(format!("  {}", paint_fg(self.theme.dim, "↳ thinking"),));
            // Wrap thinking text into the body width with muted dim styling.
            let inner_w = width.saturating_sub(4);
            for raw_line in self.thinking.split('\n') {
                let trimmed = raw_line.trim_end();
                if trimmed.is_empty() {
                    out.push(String::new());
                    continue;
                }
                for w in crate::utils::wrap_text(trimmed, inner_w) {
                    out.push(format!("  {}", paint_fg(self.theme.dim, &w)));
                }
            }
            out.push(String::new());
        }

        if !self.text.is_empty() {
            out.extend(crate::components::markdown::render(
                &self.theme,
                &self.text,
                width,
            ));
        }
        out.push(String::new());
        out
    }
}

/// A status row shown above the editor while a request is in flight.
pub struct StatusLine {
    theme: Theme,
    msg: String,
    spinner_frame: usize,
}

impl StatusLine {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            msg: String::new(),
            spinner_frame: 0,
        }
    }

    pub fn set(&mut self, msg: impl Into<String>) {
        self.msg = msg.into();
    }
    pub fn clear(&mut self) {
        self.msg.clear();
    }
    pub fn tick(&mut self) {
        self.spinner_frame = self.spinner_frame.wrapping_add(1);
    }
    pub fn is_active(&self) -> bool {
        !self.msg.is_empty()
    }
}

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

impl Component for StatusLine {
    fn render(&self, _width: u16) -> Vec<String> {
        if self.msg.is_empty() {
            return vec![];
        }
        let frame = SPINNER[self.spinner_frame % SPINNER.len()];
        vec![format!(
            "  {} {}",
            paint_fg(self.theme.accent, frame),
            paint_fg(self.theme.muted, &self.msg),
        )]
    }
}
