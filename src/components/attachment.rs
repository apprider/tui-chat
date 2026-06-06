//! Image / file attachment placeholder.
//!
//! Terminals can't render images (well), but a styled box showing
//! filename, size, and mime-type is standard. Used when a user
//! attaches a file or the LLM references one.
//!
//! ```text
//!   📎 screenshot.png                          ← header
//!   │ image/png · 1920×1080 · 2.3 MB           ← meta
//! ```

use crate::component::Component;
use crate::theme::{dimmed, paint_fg, Theme};

/// A placeholder for a non-text attachment.
pub struct Attachment {
    theme: Theme,
    filename: String,
    /// MIME type or human-readable format (e.g. "image/png").
    mime: String,
    /// Extra metadata shown dimmed — dimensions, size, duration, etc.
    meta: String,
}

impl Attachment {
    pub fn new(
        theme: Theme,
        filename: impl Into<String>,
        mime: impl Into<String>,
    ) -> Self {
        Self {
            theme,
            filename: filename.into(),
            mime: mime.into(),
            meta: String::new(),
        }
    }

    /// Add extra metadata (dimensions, file size, etc.).
    pub fn with_meta(mut self, meta: impl Into<String>) -> Self {
        self.meta = meta.into();
        self
    }

    /// Convenience: set all fields at once.
    pub fn with_details(
        mut self,
        mime: impl Into<String>,
        meta: impl Into<String>,
    ) -> Self {
        self.mime = mime.into();
        self.meta = meta.into();
        self
    }
}

impl Component for Attachment {
    fn render(&self, width: u16) -> Vec<String> {
        let mut out = Vec::new();
        let _inner = width.saturating_sub(4);

        // Header: paperclip + filename.
        let clip = paint_fg(self.theme.accent, "📎");
        let name = paint_fg(self.theme.fg, &self.filename);
        let mut header = String::new();
        header.push_str("  ");
        header.push_str(&clip);
        header.push(' ');
        header.push_str(&name);
        out.push(header);

        // Meta line: mime + details.
        let bar = paint_fg(self.theme.border_muted, "│");
        let mut meta = String::new();
        meta.push_str("   ");
        meta.push_str(&bar);
        meta.push(' ');
        meta.push_str(&dimmed(&self.theme, &self.mime));
        if !self.meta.is_empty() {
            meta.push_str(&dimmed(&self.theme, " · "));
            meta.push_str(&dimmed(&self.theme, &self.meta));
        }
        out.push(meta);

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
    fn renders_filename() {
        let a = Attachment::new(Theme::dark(), "photo.png", "image/png");
        let lines = a.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("photo.png"), "filename: {:?}", p[0]);
        assert!(p[1].contains("image/png"), "mime: {:?}", p[1]);
    }

    #[test]
    fn renders_meta() {
        let a = Attachment::new(Theme::dark(), "vid.mp4", "video/mp4")
            .with_meta("2.3 MB · 0:42");
        let lines = a.render(40);
        let p = plain(&lines);
        assert!(p[1].contains("2.3 MB"), "meta: {:?}", p[1]);
        assert!(p[1].contains("0:42"), "meta: {:?}", p[1]);
    }

    #[test]
    fn with_details_overwrites() {
        let a = Attachment::new(Theme::dark(), "doc.pdf", "text/plain")
            .with_details("application/pdf", "12 pages · 340 KB");
        let lines = a.render(40);
        let p = plain(&lines);
        assert!(p[1].contains("application/pdf"), "mime: {:?}", p[1]);
        assert!(p[1].contains("340 KB"), "meta: {:?}", p[1]);
    }

    #[test]
    fn has_left_border() {
        let a = Attachment::new(Theme::dark(), "x", "y");
        let lines = a.render(40);
        let p = plain(&lines);
        assert!(p[1].contains("│"), "border: {:?}", p[1]);
    }
}
