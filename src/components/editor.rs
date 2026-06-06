//! Multi-line editor with slash-command autocomplete.
//!
//! Layout (matching pi-mono):
//!
//! ```text
//! ──────────────────────────────────  (top rule, full width)
//!  > what's the status of the build?▌  (prompt + buffer; fake cursor)
//!                                       (additional buffer rows; soft-wrap)
//! ──────────────────────────────────  (bottom rule)
//!   → /model       Select model       \  inline autocomplete
//!     /resume      Resume session     /  rendered when buffer starts with /
//!     /exit        Quit ko
//! ```
//!
//! No left/right borders. The autocomplete list is part of this same
//! component's render() output — it sits directly under the bottom rule.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::component::{Action, Component, InputOutcome};
use crate::renderer::CURSOR_MARKER;
use crate::theme::{bold_fg, dimmed, paint_bg, paint_fg, rule, Theme, RESET, REVERSE};
use crate::utils::visible_width;

/// One row in the autocomplete dropdown.
#[derive(Clone, Debug)]
pub struct Suggestion {
    pub trigger: String, // text to insert (e.g. "/model")
    pub label: String,   // human label (often == trigger)
    pub description: String,
}

/// Provider of suggestions given the current buffer line.
pub trait SuggestionProvider: Send + Sync {
    fn suggest(&self, line: &str) -> Vec<Suggestion>;
}

/// Static slash-command list provider — given a registry, returns
/// fuzzy-matched commands when the line starts with `/`.
pub struct SlashProvider {
    pub commands: Vec<Suggestion>,
}

impl SlashProvider {
    pub fn new(commands: Vec<Suggestion>) -> Self {
        Self { commands }
    }
}

impl SuggestionProvider for SlashProvider {
    fn suggest(&self, line: &str) -> Vec<Suggestion> {
        if !line.starts_with('/') {
            return Vec::new();
        }
        let q = line.trim_start_matches('/');
        let mut out: Vec<Suggestion> = self
            .commands
            .iter()
            .filter(|s| s.trigger.trim_start_matches('/').starts_with(q))
            .cloned()
            .collect();
        if out.is_empty() {
            // fall back to substring match for forgiving typing
            out = self
                .commands
                .iter()
                .filter(|s| s.trigger.contains(q))
                .cloned()
                .collect();
        }
        out
    }
}

pub struct Editor {
    theme: Theme,
    /// Logical lines of the buffer.
    rows: Vec<String>,
    /// Cursor row/col (col in graphemes).
    cursor: (usize, usize),
    /// Submitted history (for up/down history navigation).
    history: Vec<String>,
    history_idx: Option<usize>,
    /// "/" autocomplete state.
    suggestions: Vec<Suggestion>,
    selected: usize,
    provider: Option<Box<dyn SuggestionProvider>>,
    /// Whether the editor accepts input. False while a request is in flight.
    pub disabled: bool,
    /// Hint shown to the right of the prompt when buffer is empty.
    pub placeholder: String,
}

impl Editor {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            rows: vec![String::new()],
            cursor: (0, 0),
            history: Vec::new(),
            history_idx: None,
            suggestions: Vec::new(),
            selected: 0,
            provider: None,
            disabled: false,
            placeholder: "type a message · / for commands".into(),
        }
    }

    pub fn set_provider(&mut self, p: Box<dyn SuggestionProvider>) {
        self.provider = Some(p);
        self.refresh_suggestions();
    }

    pub fn buffer(&self) -> String {
        self.rows.join("\n")
    }

    pub fn clear(&mut self) {
        self.rows = vec![String::new()];
        self.cursor = (0, 0);
        self.history_idx = None;
        self.refresh_suggestions();
    }

    fn current_line(&self) -> &str {
        &self.rows[self.cursor.0]
    }

    fn refresh_suggestions(&mut self) {
        let line = self.current_line().to_string();
        self.suggestions = self
            .provider
            .as_ref()
            .map(|p| p.suggest(&line))
            .unwrap_or_default();
        if self.selected >= self.suggestions.len() {
            self.selected = 0;
        }
    }

    fn insert_str(&mut self, s: &str) {
        let (r, c) = self.cursor;
        let row = &mut self.rows[r];
        let bytes_before = grapheme_byte_offset(row, c);
        row.insert_str(bytes_before, s);
        self.cursor.1 += s.graphemes(true).count();
        self.refresh_suggestions();
    }

    fn backspace(&mut self) {
        let (r, c) = self.cursor;
        if c > 0 {
            let row = &mut self.rows[r];
            let prev = grapheme_byte_offset(row, c - 1);
            let cur = grapheme_byte_offset(row, c);
            row.replace_range(prev..cur, "");
            self.cursor.1 = c - 1;
        } else if r > 0 {
            // Join with previous line
            let removed = self.rows.remove(r);
            let new_col = self.rows[r - 1].graphemes(true).count();
            self.rows[r - 1].push_str(&removed);
            self.cursor = (r - 1, new_col);
        }
        self.refresh_suggestions();
    }

    fn delete_forward(&mut self) {
        let (r, c) = self.cursor;
        let row = &mut self.rows[r];
        let total = row.graphemes(true).count();
        if c < total {
            let cur = grapheme_byte_offset(row, c);
            let nxt = grapheme_byte_offset(row, c + 1);
            row.replace_range(cur..nxt, "");
        } else if r + 1 < self.rows.len() {
            let next = self.rows.remove(r + 1);
            self.rows[r].push_str(&next);
        }
        self.refresh_suggestions();
    }

    fn newline(&mut self) {
        let (r, c) = self.cursor;
        let row = &self.rows[r];
        let cut = grapheme_byte_offset(row, c);
        let tail = row[cut..].to_string();
        self.rows[r].truncate(cut);
        self.rows.insert(r + 1, tail);
        self.cursor = (r + 1, 0);
        self.refresh_suggestions();
    }

    fn move_left(&mut self) {
        if self.cursor.1 > 0 {
            self.cursor.1 -= 1;
        } else if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
            self.cursor.1 = self.rows[self.cursor.0].graphemes(true).count();
        }
    }
    fn move_right(&mut self) {
        let len = self.rows[self.cursor.0].graphemes(true).count();
        if self.cursor.1 < len {
            self.cursor.1 += 1;
        } else if self.cursor.0 + 1 < self.rows.len() {
            self.cursor.0 += 1;
            self.cursor.1 = 0;
        }
    }
    fn move_up(&mut self) {
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
            let len = self.rows[self.cursor.0].graphemes(true).count();
            self.cursor.1 = self.cursor.1.min(len);
        }
    }
    fn move_down(&mut self) {
        if self.cursor.0 + 1 < self.rows.len() {
            self.cursor.0 += 1;
            let len = self.rows[self.cursor.0].graphemes(true).count();
            self.cursor.1 = self.cursor.1.min(len);
        }
    }

    fn move_home(&mut self) {
        self.cursor.1 = 0;
    }
    fn move_end(&mut self) {
        self.cursor.1 = self.rows[self.cursor.0].graphemes(true).count();
    }

    fn submit(&mut self) -> Option<String> {
        let text = self.buffer();
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            return None;
        }
        self.history.push(trimmed.clone());
        if self.history.len() > 200 {
            self.history.remove(0);
        }
        self.clear();
        Some(trimmed)
    }

    fn accept_suggestion(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        let trigger = self.suggestions[self.selected].trigger.clone();
        // Replace current line with trigger + space.
        let r = self.cursor.0;
        self.rows[r] = format!("{trigger} ");
        self.cursor.1 = self.rows[r].graphemes(true).count();
        self.refresh_suggestions();
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let new_idx = match self.history_idx {
            None => self.history.len().saturating_sub(1),
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.history_idx = Some(new_idx);
        self.rows = vec![self.history[new_idx].clone()];
        self.cursor = (0, self.rows[0].graphemes(true).count());
        self.refresh_suggestions();
    }
    fn history_next(&mut self) {
        if let Some(i) = self.history_idx {
            if i + 1 < self.history.len() {
                self.history_idx = Some(i + 1);
                self.rows = vec![self.history[i + 1].clone()];
                self.cursor = (0, self.rows[0].graphemes(true).count());
            } else {
                self.history_idx = None;
                self.clear();
            }
        }
    }
}

impl Component for Editor {
    fn wants_cursor(&self) -> bool {
        !self.disabled
    }

    fn render(&self, width: u16) -> Vec<String> {
        let inner_w = width.saturating_sub(2); // 1-cell gutter each side
        let mut out = Vec::new();
        // Top rule
        out.push(rule(&self.theme, width));

        // Render rows. We soft-wrap at `inner_w - 2` (extra for "> " prefix).
        let prompt_w = 2u16;
        let body_w = inner_w.saturating_sub(prompt_w);
        let prompt = bold_fg(self.theme.accent, "> ");
        let cont_pad = " ".repeat(prompt_w as usize);

        let buffer_empty = self.rows.len() == 1 && self.rows[0].is_empty();
        if buffer_empty {
            // Show placeholder + cursor
            let mut s = String::new();
            s.push(' ');
            s.push_str(&prompt);
            s.push_str(CURSOR_MARKER);
            s.push_str(REVERSE);
            s.push(' ');
            s.push_str(RESET);
            s.push_str(&dimmed(&self.theme, &self.placeholder));
            out.push(s);
        } else {
            for (ri, row) in self.rows.iter().enumerate() {
                let visual_lines = char_wrap(row, body_w);
                let visual_lines = if visual_lines.is_empty() {
                    vec![String::new()]
                } else {
                    visual_lines
                };
                let cursor_here = self.cursor.0 == ri;
                // Determine which visual sub-row & col the cursor is on.
                let (cur_visual, cur_col) = if cursor_here {
                    visual_position(row, self.cursor.1, body_w)
                } else {
                    (usize::MAX, 0)
                };

                for (vi, vl) in visual_lines.iter().enumerate() {
                    let mut s = String::new();
                    s.push(' ');
                    if ri == 0 && vi == 0 {
                        s.push_str(&prompt);
                    } else {
                        s.push_str(&cont_pad);
                    }
                    if cursor_here && vi == cur_visual {
                        // Insert cursor: render text up to cur_col, marker, reverse-video the next grapheme.
                        let (before, at, after) = split_at_grapheme(vl, cur_col);
                        s.push_str(&before);
                        s.push_str(CURSOR_MARKER);
                        if at.is_empty() {
                            s.push_str(REVERSE);
                            s.push(' ');
                            s.push_str(RESET);
                        } else {
                            s.push_str(REVERSE);
                            s.push_str(&at);
                            s.push_str(RESET);
                        }
                        s.push_str(&after);
                    } else {
                        s.push_str(vl);
                    }
                    out.push(s);
                }
            }
        }

        // Bottom rule
        out.push(rule(&self.theme, width));

        // Autocomplete list (if any).
        if !self.suggestions.is_empty() {
            let max_label = self
                .suggestions
                .iter()
                .map(|s| visible_width(&s.label))
                .max()
                .unwrap_or(0)
                .max(8);
            for (i, sug) in self.suggestions.iter().take(8).enumerate() {
                let selected = i == self.selected;
                let prefix = if selected {
                    paint_fg(self.theme.accent, "  → ")
                } else {
                    "    ".to_string()
                };
                let label_pad = max_label.saturating_sub(visible_width(&sug.label));
                let label = if selected {
                    bold_fg(self.theme.fg, &sug.label)
                } else {
                    paint_fg(self.theme.fg, &sug.label)
                };
                let mut row = String::new();
                row.push_str(&prefix);
                row.push_str(&label);
                for _ in 0..label_pad {
                    row.push(' ');
                }
                row.push_str("  ");
                row.push_str(&dimmed(&self.theme, &sug.description));
                if selected {
                    // Subtle background highlight, full row width
                    let full = paint_bg(self.theme.select_bg, &row);
                    out.push(full);
                } else {
                    out.push(row);
                }
            }
            // hint row
            let hint = format!(
                "    {}  {}  {}",
                dimmed(&self.theme, "↑↓ navigate"),
                dimmed(&self.theme, "tab/enter select"),
                dimmed(&self.theme, "esc dismiss"),
            );
            out.push(hint);
        }

        out
    }

    fn handle_input(&mut self, ev: &Event) -> InputOutcome {
        if self.disabled {
            return InputOutcome::Ignored;
        }
        let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = ev
        else {
            return InputOutcome::Ignored;
        };
        let ctrl = modifiers.contains(KeyModifiers::CONTROL);
        let alt = modifiers.contains(KeyModifiers::ALT);
        let shift = modifiers.contains(KeyModifiers::SHIFT);

        match code {
            KeyCode::Char('c') if ctrl => {
                if self.buffer().is_empty() {
                    return InputOutcome::Action(Action::Exit);
                }
                self.clear();
                return InputOutcome::Consumed;
            }
            KeyCode::Char('d') if ctrl && self.buffer().is_empty() => {
                return InputOutcome::Action(Action::Exit);
            }
            KeyCode::Enter if !shift && !alt => {
                // If autocomplete list is open AND buffer is just /<partial>, accept it
                if !self.suggestions.is_empty() {
                    let line = self.current_line();
                    if line.starts_with('/')
                        && self.suggestions.iter().any(|s| s.trigger == line.trim())
                    {
                        // Exact match: submit as command
                        if let Some(text) = self.submit() {
                            return InputOutcome::Action(Action::Submit(text));
                        }
                    } else {
                        self.accept_suggestion();
                        return InputOutcome::Consumed;
                    }
                }
                if let Some(text) = self.submit() {
                    return InputOutcome::Action(Action::Submit(text));
                }
                return InputOutcome::Consumed;
            }
            KeyCode::Enter if shift || alt => {
                self.newline();
                return InputOutcome::Consumed;
            }
            KeyCode::Tab => {
                if !self.suggestions.is_empty() {
                    self.accept_suggestion();
                    return InputOutcome::Consumed;
                }
            }
            KeyCode::Up => {
                if !self.suggestions.is_empty() {
                    if self.selected > 0 {
                        self.selected -= 1;
                    } else {
                        self.selected = self.suggestions.len() - 1;
                    }
                    return InputOutcome::Consumed;
                }
                if self.buffer().is_empty() || self.cursor.0 == 0 {
                    self.history_prev();
                } else {
                    self.move_up();
                }
                return InputOutcome::Consumed;
            }
            KeyCode::Down => {
                if !self.suggestions.is_empty() {
                    self.selected = (self.selected + 1) % self.suggestions.len();
                    return InputOutcome::Consumed;
                }
                if self.cursor.0 + 1 >= self.rows.len() {
                    self.history_next();
                } else {
                    self.move_down();
                }
                return InputOutcome::Consumed;
            }
            KeyCode::Left => {
                self.move_left();
                return InputOutcome::Consumed;
            }
            KeyCode::Right => {
                self.move_right();
                return InputOutcome::Consumed;
            }
            KeyCode::Home => {
                self.move_home();
                return InputOutcome::Consumed;
            }
            KeyCode::End => {
                self.move_end();
                return InputOutcome::Consumed;
            }
            KeyCode::Backspace => {
                self.backspace();
                return InputOutcome::Consumed;
            }
            KeyCode::Delete => {
                self.delete_forward();
                return InputOutcome::Consumed;
            }
            KeyCode::Esc => {
                if !self.suggestions.is_empty() {
                    self.suggestions.clear();
                    return InputOutcome::Consumed;
                }
                if !self.buffer().is_empty() {
                    self.clear();
                    return InputOutcome::Consumed;
                }
                return InputOutcome::Action(Action::Cancel);
            }
            KeyCode::Char('a') if ctrl => {
                self.move_home();
                return InputOutcome::Consumed;
            }
            KeyCode::Char('e') if ctrl => {
                self.move_end();
                return InputOutcome::Consumed;
            }
            KeyCode::Char('u') if ctrl => {
                let r = self.cursor.0;
                let cut = grapheme_byte_offset(&self.rows[r], self.cursor.1);
                self.rows[r].replace_range(..cut, "");
                self.cursor.1 = 0;
                self.refresh_suggestions();
                return InputOutcome::Consumed;
            }
            KeyCode::Char('k') if ctrl => {
                let r = self.cursor.0;
                let cut = grapheme_byte_offset(&self.rows[r], self.cursor.1);
                self.rows[r].truncate(cut);
                self.refresh_suggestions();
                return InputOutcome::Consumed;
            }
            KeyCode::Char(c) => {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                self.insert_str(s);
                return InputOutcome::Consumed;
            }
            _ => {}
        }
        InputOutcome::Ignored
    }
}

fn grapheme_byte_offset(s: &str, n: usize) -> usize {
    let mut off = 0;
    for (i, g) in s.grapheme_indices(true).enumerate() {
        if i == n {
            return g.0;
        }
        off = g.0 + g.1.len();
    }
    off
}

/// Simple character-based wrap to `width` columns. Doesn't break on
/// word boundaries — we want predictable cursor math.
fn char_wrap(s: &str, width: u16) -> Vec<String> {
    if width == 0 {
        return vec![s.to_string()];
    }
    let w = width as usize;
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0;
    for g in s.graphemes(true) {
        let gw = UnicodeWidthStr::width(g).max(1);
        if cur_w + gw > w {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0;
        }
        cur.push_str(g);
        cur_w += gw;
    }
    lines.push(cur);
    lines
}

/// Map a logical column on a single row to (visual_subrow, visual_col)
/// after char-wrapping at `width`.
fn visual_position(s: &str, col: usize, width: u16) -> (usize, usize) {
    if width == 0 {
        return (0, 0);
    }
    let w = width as usize;
    let mut sub = 0usize;
    let mut subw = 0usize;
    for (i, g) in s.graphemes(true).enumerate() {
        if i == col {
            return (sub, subw);
        }
        let gw = UnicodeWidthStr::width(g).max(1);
        if subw + gw > w {
            sub += 1;
            subw = 0;
        }
        subw += gw;
    }
    (sub, subw)
}

/// Split a wrapped visual row at a visual column. Returns (before, at, after)
/// where `at` is exactly one grapheme (or empty if at end).
fn split_at_grapheme(s: &str, col: usize) -> (String, String, String) {
    let mut before = String::new();
    let mut at = String::new();
    let mut after = String::new();
    let mut placed = 0usize;
    let mut found = false;
    for g in s.graphemes(true) {
        let gw = UnicodeWidthStr::width(g).max(1);
        if !found && placed >= col {
            at = g.to_string();
            found = true;
            continue;
        }
        if found {
            after.push_str(g);
        } else {
            before.push_str(g);
            placed += gw;
        }
    }
    (before, at, after)
}
