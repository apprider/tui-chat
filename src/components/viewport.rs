//! Scrollable viewport that clips a large child list to a visible
//! window.
//!
//! `Container` grows forever — every keystroke re-renders every child.
//! `Viewport` keeps the full child list but only emits the rows that
//! fit inside its `visible_height`, making large chat histories usable.
//!
//! ```text
//! ┌─ Viewport (visible_height = 6) ─────────┐
//! │  ··· 12 lines above · scroll ↑ ···     │  ← indicator (replaces row)
//! │  I'm doing well                        │
//! │  thanks for asking                     │
//! │  anything else?                        │
//! │                                        │
//! │  > type a message                      │
//! └────────────────────────────────────────┘
//! ```
//!
//! The viewport owns its children (like `Container`) but tracks a
//! scroll offset in *rows*, not child indices, because each child may
//! produce a variable number of lines.

use crate::component::Component;
use crate::theme::{dimmed, Theme};

/// A scrollable container that renders only the visible slice of its
/// children.
pub struct Viewport {
    children: Vec<Box<dyn Component>>,
    /// How many rows this viewport emits on render.
    visible_height: u16,
    /// Number of rows from the top of the flattened child output to
    /// the first visible row.
    scroll_offset: usize,
    /// When true, new `push()` calls schedule an auto-scroll to bottom
    /// on the next `render()`.
    auto_scroll: bool,
    /// Show a subtle indicator line when scrolled away from top.
    show_indicator: bool,
    /// Set by `push()` when `auto_scroll` is true — consumed on the
    /// next `render()` to snap scroll to the new bottom.
    pending_auto_scroll: bool,
    theme: Theme,
}

impl Viewport {
    pub fn new(theme: Theme, visible_height: u16) -> Self {
        Self {
            children: Vec::new(),
            visible_height: visible_height.max(1),
            scroll_offset: 0,
            auto_scroll: true,
            show_indicator: true,
            pending_auto_scroll: false,
            theme,
        }
    }

    /// Change the visible window size (e.g. after terminal resize).
    pub fn set_visible_height(&mut self, h: u16) {
        self.visible_height = h.max(1);
    }

    /// Enable or disable auto-scroll-to-bottom on new content.
    pub fn set_auto_scroll(&mut self, v: bool) {
        self.auto_scroll = v;
    }

    /// Enable or disable the scroll indicator line.
    pub fn set_show_indicator(&mut self, v: bool) {
        self.show_indicator = v;
    }

    /// Add a child. If `auto_scroll` is on, the next `render()` will
    /// snap to the new bottom.
    pub fn push(&mut self, c: Box<dyn Component>) {
        self.children.push(c);
        if self.auto_scroll {
            self.pending_auto_scroll = true;
        }
    }

    /// Remove all children and reset scroll.
    pub fn clear(&mut self) {
        self.children.clear();
        self.scroll_offset = 0;
        self.pending_auto_scroll = false;
    }

    /// Number of child components.
    pub fn len(&self) -> usize {
        self.children.len()
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// Scroll up by `n` rows. Returns true if the offset changed.
    pub fn scroll_up(&mut self, n: usize) -> bool {
        let old = self.scroll_offset;
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
        old != self.scroll_offset
    }

    /// Scroll down by `n` rows. Returns true if the offset changed.
    pub fn scroll_down(&mut self, n: usize) -> bool {
        let old = self.scroll_offset;
        self.scroll_offset = self.scroll_offset.saturating_add(n);
        old != self.scroll_offset
    }

    /// Jump to the top of the content.
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    /// Jump to the bottom of the content.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = usize::MAX;
    }

    /// Whether the viewport is currently scrolled to the top.
    pub fn at_top(&self) -> bool {
        self.scroll_offset == 0
    }

    /// Current scroll offset in rows.
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Compute total flattened line count at the given width.
    pub fn total_lines(&self, width: u16) -> usize {
        self.build_flat(width).len()
    }

    /// Whether the viewport is scrolled to the bottom at the given width.
    /// This requires a full render pass, so prefer calling it only when
    /// needed (e.g. after explicit scroll commands, not every frame).
    pub fn at_bottom(&self, width: u16) -> bool {
        if self.children.is_empty() {
            return true;
        }
        let total = self.total_lines(width);
        let max = total.saturating_sub(self.visible_height as usize);
        self.scroll_offset >= max
    }

    fn build_flat(&self, width: u16) -> Vec<String> {
        let mut flat = Vec::new();
        for child in &self.children {
            flat.extend(child.render(width));
        }
        flat
    }
}

impl Component for Viewport {
    fn render(&self, width: u16) -> Vec<String> {
        let flat = self.build_flat(width);
        let total = flat.len();
        let vh = self.visible_height as usize;

        // Auto-scroll if pending.
        let mut scroll = self.scroll_offset;
        if self.pending_auto_scroll {
            scroll = total.saturating_sub(vh);
        }

        // Clamp to valid range.
        let max_scroll = total.saturating_sub(vh);
        if scroll > max_scroll {
            scroll = max_scroll;
        }

        let mut out = Vec::with_capacity(vh);
        let _end = (scroll + vh).min(total);

        // Indicator: when scrolled down, show a subtle line that
        // replaces the first content row.
        let show_indicator = self.show_indicator && scroll > 0;
        let content_rows = if show_indicator { vh.saturating_sub(1) } else { vh };

        if show_indicator {
            let indicator = dimmed(
                &self.theme,
                &format!("  ↑ {scroll} lines above · scroll ↑ to top"),
            );
            out.push(indicator);
        }

        // Slice the visible content.
        let content_end = (scroll + content_rows).min(total);
        out.extend(flat[scroll..content_end].iter().cloned());

        // Pad with empty rows if the viewport isn't full.
        while out.len() < vh {
            out.push(String::new());
        }

        out.truncate(vh);
        out
    }

    fn invalidate(&mut self) {
        for child in &mut self.children {
            child.invalidate();
        }
    }
}

// Viewport is a passive clipping layer — scrolling is driven by the
// caller's event loop (e.g. Shift+PgUp / Shift+PgDown / mouse wheel).
// It does not implement handle_input because it doesn't own focus.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::components::text::Text;
    use crate::theme::Theme;
    use crate::utils::strip_ansi;

    fn make_viewport() -> Viewport {
        let theme = Theme::dark();
        let mut vp = Viewport::new(theme, 5);
        vp.set_show_indicator(false);
        vp.set_auto_scroll(false);
        vp
    }

    fn plain(lines: &[String]) -> Vec<String> {
        lines.iter().map(|l| strip_ansi(l)).collect()
    }

    #[test]
    fn renders_all_when_content_fits() {
        let mut vp = make_viewport();
        vp.push(Box::new(Text::new("line one")));
        vp.push(Box::new(Text::new("line two")));

        let lines = vp.render(40);
        let p = plain(&lines);
        assert_eq!(lines.len(), 5); // visible_height padded
        assert!(p.iter().any(|l| l.contains("line one")));
        assert!(p.iter().any(|l| l.contains("line two")));
    }

    #[test]
    fn clips_to_visible_window() {
        let mut vp = make_viewport();
        for i in 0..20 {
            vp.push(Box::new(Text::new(format!("message {}", i))));
        }

        let lines = vp.render(40);
        let p = plain(&lines);
        // No scroll yet — first 5 visible
        assert!(p[0].contains("message 0"), "first: {:?}", p[0]);
        assert!(p[4].contains("message 4"), "last: {:?}", p[4]);
        assert!(!p.iter().any(|l| l.contains("message 5")));
    }

    #[test]
    fn scroll_down_moves_window() {
        let mut vp = make_viewport();
        for i in 0..20 {
            vp.push(Box::new(Text::new(format!("message {}", i))));
        }

        vp.scroll_down(3);
        let lines = vp.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("message 3"), "first: {:?}", p[0]);
        assert!(p[4].contains("message 7"), "last: {:?}", p[4]);
    }

    #[test]
    fn scroll_up_moves_back() {
        let mut vp = make_viewport();
        for i in 0..20 {
            vp.push(Box::new(Text::new(format!("message {}", i))));
        }

        vp.scroll_down(10);
        vp.scroll_up(2);
        let lines = vp.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("message 8"), "first: {:?}", p[0]);
    }

    #[test]
    fn scroll_to_bottom_shows_last_items() {
        let mut vp = make_viewport();
        for i in 0..20 {
            vp.push(Box::new(Text::new(format!("message {}", i))));
        }

        vp.scroll_to_bottom();
        let lines = vp.render(40);
        let p = plain(&lines);
        assert!(p[0].contains("message 15"), "first: {:?}", p[0]);
        assert!(p[4].contains("message 19"), "last: {:?}", p[4]);
    }

    #[test]
    fn auto_scroll_snaps_to_bottom() {
        let mut vp = make_viewport();
        vp.set_auto_scroll(true);

        for i in 0..10 {
            vp.push(Box::new(Text::new(format!("message {}", i))));
        }
        // pending_auto_scroll should be set
        let lines = vp.render(40);
        let p = plain(&lines);
        // 10 items, visible_height 5 → should show last 5
        assert!(p[0].contains("message 5"), "first: {:?}", p[0]);
        assert!(p[4].contains("message 9"), "last: {:?}", p[4]);
    }

    #[test]
    fn at_top_and_at_bottom() {
        let mut vp = make_viewport();
        for i in 0..20 {
            vp.push(Box::new(Text::new(format!("message {}", i))));
        }

        assert!(vp.at_top());
        assert!(!vp.at_bottom(40));

        vp.scroll_to_bottom();
        assert!(!vp.at_top());
        assert!(vp.at_bottom(40));
    }

    #[test]
    fn clear_resets_everything() {
        let mut vp = make_viewport();
        vp.push(Box::new(Text::new("hello")));
        vp.scroll_down(5);

        vp.clear();
        assert!(vp.is_empty());
        assert_eq!(vp.scroll_offset(), 0);
        let lines = vp.render(40);
        assert!(lines.iter().all(|l| l.is_empty()));
    }

    #[test]
    fn scroll_does_not_go_negative() {
        let mut vp = make_viewport();
        vp.scroll_up(100);
        assert_eq!(vp.scroll_offset(), 0);
    }

    #[test]
    fn scroll_clamps_at_max() {
        let mut vp = make_viewport();
        for i in 0..20 {
            vp.push(Box::new(Text::new(format!("msg {}", i))));
        }
        vp.scroll_down(1000);
        let lines = vp.render(40);
        let p = plain(&lines);
        // Should show the last 5
        assert!(p[0].contains("msg 15"), "first: {:?}", p[0]);
        assert!(p[4].contains("msg 19"), "last: {:?}", p[4]);
    }

    #[test]
    fn indicator_shown_when_scrolled() {
        let mut vp = make_viewport();
        vp.set_show_indicator(true);
        for i in 0..20 {
            vp.push(Box::new(Text::new(format!("msg {}", i))));
        }

        vp.scroll_down(5);
        let lines = vp.render(40);
        let p = plain(&lines);
        // Indicator replaces the first visible content row (msg 5).
        assert!(p[0].contains("↑"), "indicator: {:?}", p[0]);
        assert!(!p[0].contains("msg 5"), "should not show msg 5 in indicator");
        // Content continues from the scroll offset.
        assert!(p[1].contains("msg 5"), "content row 1: {:?}", p[1]);
        assert!(p[2].contains("msg 6"), "content row 2: {:?}", p[2]);
    }

    #[test]
    fn total_lines_counts_all_rows() {
        let mut vp = make_viewport();
        for i in 0..10 {
            vp.push(Box::new(Text::new(format!("msg {}", i))));
        }
        assert_eq!(vp.total_lines(40), 10);
    }

    #[test]
    fn resize_changes_visible_height() {
        let mut vp = make_viewport();
        for i in 0..20 {
            vp.push(Box::new(Text::new(format!("msg {}", i))));
        }
        vp.scroll_to_bottom();

        vp.set_visible_height(3);
        let lines = vp.render(40);
        assert_eq!(lines.len(), 3);
        let p = plain(&lines);
        assert!(p[0].contains("msg 17"), "first: {:?}", p[0]);
        assert!(p[2].contains("msg 19"), "last: {:?}", p[2]);
    }
}
