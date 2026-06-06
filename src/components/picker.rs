//! Full-screen picker that replaces the editor in-place — pi-mono's
//! `showSelector` pattern.
//!
//! Activated by slash commands or any trigger that needs the user to
//! choose from a list. When active, the picker captures all keystrokes
//! and renders in the editor's slot. The caller's event loop swaps the
//! editor for the picker while `Some(picker)` is present.
//!
//! Layout:
//!
//! ```text
//! ──────────────────────────────────────────────
//!   Select model
//!   > sonnet▌                                   ← filter input
//!
//!     assistant   ollama/gemma4:31b-cloud
//!  →  coder       openai/gpt-4o ✓               ← selected (✓ = current)
//!     reviewer    anthropic/claude-opus
//!     (3/8)
//!
//!     ↑↓ navigate · enter select · esc cancel
//! ──────────────────────────────────────────────
//! ```
//!
//! The picker is fully generic — it carries no knowledge of what the
//! items mean. The caller decides what "selected" means via the
//! `PickerOutcome::Selected` event.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::component::Component;
use crate::renderer::CURSOR_MARKER;
use crate::theme::{bold_fg, dimmed, paint_bg, paint_fg, rule, Theme, RESET, REVERSE};
use crate::utils::{truncate_to_width, visible_width};

/// One selectable row in the picker.
#[derive(Clone, Debug)]
pub struct PickerItem {
    /// Unique selector (e.g. agent ID, model id). Opaque to the picker.
    pub key: String,
    /// Primary display column.
    pub label: String,
    /// Secondary display column (dimmed).
    pub description: String,
    /// Marked with ✓ when this item is the currently-active choice.
    pub current: bool,
}

impl PickerItem {
    pub fn new(key: impl Into<String>, label: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            description: description.into(),
            current: false,
        }
    }

    /// Chainable builder: mark this item as the current / active one.
    pub fn mark_current(mut self) -> Self {
        self.current = true;
        self
    }
}

/// Outcome of dispatching an input event to the picker.
#[derive(Clone, Debug)]
pub enum PickerOutcome {
    /// User cancelled — close the picker without action.
    Cancel,
    /// User selected an item — close the picker and let the caller act.
    Selected(PickerItem),
    /// User edited the filter or moved the selection — repaint only.
    Continue,
}

/// Full-screen picker with filter, scrollable viewport, and keyboard
/// navigation.
pub struct Picker {
    theme: Theme,
    title: String,
    items: Vec<PickerItem>,
    filtered: Vec<usize>,
    filter: String,
    selected: usize,
    /// Max items visible at once (default 10).
    max_visible: usize,
}

impl Picker {
    pub fn new(theme: Theme, title: impl Into<String>, items: Vec<PickerItem>) -> Self {
        let mut s = Self {
            theme,
            title: title.into(),
            items,
            filtered: Vec::new(),
            filter: String::new(),
            selected: 0,
            max_visible: 10,
        };
        s.recompute_filter();
        // Default selection: first item marked `current`, else 0.
        if let Some(idx) = s.filtered.iter().position(|&i| s.items[i].current) {
            s.selected = idx;
        }
        s
    }

    /// Change the visible window size (default 10).
    pub fn with_max_visible(mut self, n: usize) -> Self {
        self.max_visible = n.max(3);
        self
    }

    /// Current filter text (for display or debugging).
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Number of items matching the current filter.
    pub fn filtered_count(&self) -> usize {
        self.filtered.len()
    }

    /// The currently-selected item index *within the filtered list*.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Reset the picker to its initial state (clear filter, recompute).
    pub fn reset(&mut self) {
        self.filter.clear();
        self.recompute_filter();
        self.selected = self
            .filtered
            .iter()
            .position(|&i| self.items[i].current)
            .unwrap_or(0);
    }

    fn recompute_filter(&mut self) {
        let q = self.filter.to_lowercase();
        if q.is_empty() {
            self.filtered = (0..self.items.len()).collect();
        } else {
            self.filtered = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, it)| {
                    let hay = format!("{} {}", it.label, it.description).to_lowercase();
                    if hay.contains(&q) {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect();
        }
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }

    /// Process a crossterm event and return what happened.
    pub fn handle(&mut self, ev: &Event) -> PickerOutcome {
        let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = ev
        else {
            return PickerOutcome::Continue;
        };
        let ctrl = modifiers.contains(KeyModifiers::CONTROL);
        match code {
            KeyCode::Esc => return PickerOutcome::Cancel,
            KeyCode::Char('c') if ctrl => return PickerOutcome::Cancel,
            KeyCode::Enter => {
                if let Some(&idx) = self.filtered.get(self.selected) {
                    return PickerOutcome::Selected(self.items[idx].clone());
                }
                return PickerOutcome::Cancel;
            }
            KeyCode::Up => {
                if !self.filtered.is_empty() {
                    self.selected = if self.selected == 0 {
                        self.filtered.len() - 1
                    } else {
                        self.selected - 1
                    };
                }
            }
            KeyCode::Down => {
                if !self.filtered.is_empty() {
                    self.selected = (self.selected + 1) % self.filtered.len();
                }
            }
            KeyCode::Backspace => {
                let mut chars = self.filter.chars();
                chars.next_back();
                self.filter = chars.as_str().to_string();
                self.recompute_filter();
            }
            KeyCode::Char(c) => {
                self.filter.push(*c);
                self.recompute_filter();
            }
            _ => {}
        }
        PickerOutcome::Continue
    }
}

impl Component for Picker {
    fn wants_cursor(&self) -> bool {
        true
    }

    fn render(&self, width: u16) -> Vec<String> {
        let mut out = Vec::new();
        out.push(rule(&self.theme, width));

        // Title row.
        out.push(format!("  {}", bold_fg(self.theme.heading, &self.title)));

        // Filter input row.
        let mut filter_row = String::new();
        filter_row.push(' ');
        filter_row.push_str(&bold_fg(self.theme.accent, "> "));
        filter_row.push_str(&self.filter);
        filter_row.push_str(CURSOR_MARKER);
        filter_row.push_str(REVERSE);
        filter_row.push(' ');
        filter_row.push_str(RESET);
        if self.filter.is_empty() {
            filter_row.push_str(&dimmed(&self.theme, "type to filter"));
        }
        out.push(filter_row);
        out.push(String::new());

        // Determine label column width.
        let max_label_w = self
            .items
            .iter()
            .map(|i| visible_width(&i.label))
            .max()
            .unwrap_or(0)
            .clamp(8, 32);

        // Visible window of filtered items.
        let total = self.filtered.len();
        let start = if total > self.max_visible {
            // Keep selected centered when possible.
            let half = self.max_visible / 2;
            self.selected.saturating_sub(half).min(total - self.max_visible)
        } else {
            0
        };
        let end = (start + self.max_visible).min(total);

        if total == 0 {
            out.push(format!("  {}", dimmed(&self.theme, "no matches")));
        } else {
            for (visible_i, &item_idx) in self.filtered[start..end].iter().enumerate() {
                let real_i = start + visible_i;
                let item = &self.items[item_idx];
                let selected = real_i == self.selected;
                let prefix = if selected {
                    paint_fg(self.theme.accent, " →  ")
                } else {
                    "    ".to_string()
                };
                let label_pad = max_label_w.saturating_sub(visible_width(&item.label));
                let label = if selected {
                    bold_fg(self.theme.fg, &item.label)
                } else {
                    paint_fg(self.theme.fg, &item.label)
                };
                let mark = if item.current {
                    format!(" {}", paint_fg(self.theme.success, "✓"))
                } else {
                    String::new()
                };
                let desc_w = (width as usize)
                    .saturating_sub(prefix_width(&prefix) + max_label_w + 4 + mark.len());
                let desc = truncate_to_width(&item.description, desc_w.max(0));
                let mut row = String::new();
                row.push_str(&prefix);
                row.push_str(&label);
                for _ in 0..label_pad {
                    row.push(' ');
                }
                row.push_str("  ");
                row.push_str(&dimmed(&self.theme, &desc));
                row.push_str(&mark);
                if selected {
                    out.push(paint_bg(self.theme.select_bg, &row));
                } else {
                    out.push(row);
                }
            }
            if total > self.max_visible {
                out.push(format!(
                    "    {}",
                    dimmed(&self.theme,
                        &format!("({}/{})", self.selected + 1, total)
                    )
                ));
            }
        }
        out.push(String::new());
        out.push(format!(
            "    {}",
            dimmed(&self.theme, "↑↓ navigate · enter select · esc cancel"
            )
        ));
        out.push(rule(&self.theme, width));
        out
    }
}

fn prefix_width(s: &str) -> usize {
    visible_width(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;

    fn make_picker() -> Picker {
        let theme = Theme::dark();
        let items = vec![
            PickerItem::new("a", "alpha", "the first"),
            PickerItem::new("b", "beta", "the second").mark_current(),
            PickerItem::new("c", "gamma", "the third"),
        ];
        Picker::new(theme, "Choose", items)
    }

    #[test]
    fn default_selects_current() {
        let p = make_picker();
        assert_eq!(p.selected, 1); // beta is current
    }

    #[test]
    fn filter_reduces_list() {
        let mut p = make_picker();
        assert_eq!(p.filtered.len(), 3);

        // Type 'b' — should match beta
        p.handle(&make_key(KeyCode::Char('b')));
        assert_eq!(p.filtered.len(), 1);
        assert_eq!(p.items[p.filtered[0]].label, "beta");
    }

    #[test]
    fn select_on_enter() {
        let mut p = make_picker();
        let outcome = p.handle(&make_key(KeyCode::Enter));
        match outcome {
            PickerOutcome::Selected(item) => assert_eq!(item.label, "beta"),
            other => panic!("expected Selected, got {:?}", other),
        }
    }

    #[test]
    fn cancel_on_esc() {
        let mut p = make_picker();
        let outcome = p.handle(&make_key(KeyCode::Esc));
        assert!(matches!(outcome, PickerOutcome::Cancel));
    }

    #[test]
    fn navigate_up_down() {
        let mut p = make_picker();
        assert_eq!(p.selected, 1); // beta

        p.handle(&make_key(KeyCode::Up));
        assert_eq!(p.selected, 0); // alpha

        p.handle(&make_key(KeyCode::Down));
        assert_eq!(p.selected, 1); // beta

        p.handle(&make_key(KeyCode::Down));
        assert_eq!(p.selected, 2); // gamma

        p.handle(&make_key(KeyCode::Down));
        assert_eq!(p.selected, 0); // wraps to alpha
    }

    #[test]
    fn renders_without_panic() {
        let p = make_picker();
        let lines = p.render(80);
        assert!(!lines.is_empty());
        assert!(lines.iter().any(|l| l.contains("Choose")));
    }

    #[test]
    fn renders_no_matches() {
        let mut p = make_picker();
        // Type 'z' — no match
        p.handle(&make_key(KeyCode::Char('z')));
        let lines = p.render(80);
        assert!(lines.iter().any(|l| l.contains("no matches")));
    }

    #[test]
    fn with_max_visible_limits_rows() {
        let theme = Theme::dark();
        let items: Vec<PickerItem> = (0..50)
            .map(|i| PickerItem::new(format!("k{i}"), format!("item-{i}"), format!("desc {i}")))
            .collect();
        let p = Picker::new(theme, "Many", items).with_max_visible(5);
        let lines = p.render(80);
        // Should not render all 50 items
        let item_rows = lines.iter().filter(|l| l.contains("item-")).count();
        assert_eq!(item_rows, 5);
    }

    fn make_key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }
}
