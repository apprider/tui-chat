//! Markdown rendering with syntax highlighting.
//!
//! Uses `pulldown-cmark` for parsing and `syntect` for highlighting
//! fenced code blocks. Supports the subset of CommonMark that LLM
//! output tends to use:
//!
//! - ATX headings (`#`–`######`)
//! - Paragraphs with `**bold**`, `*italic*`, `\`inline code\``, links
//! - Unordered (`-`, `*`, `+`) and ordered (`1.`) lists
//! - Fenced code blocks with optional language tag
//! - Block quotes (`>`)
//! - Horizontal rules
//!
//! Tables, footnotes, task lists, and nested-list edge cases are
//! deferred — pulldown-cmark exposes them but rendering them well is its
//! own project.

use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;

use crate::theme::{paint_fg, Theme, BOLD, RESET};
use crate::utils::{visible_width, wrap_text};

fn syntax_set() -> &'static SyntaxSet {
    static S: OnceLock<SyntaxSet> = OnceLock::new();
    S.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    static T: OnceLock<ThemeSet> = OnceLock::new();
    T.get_or_init(ThemeSet::load_defaults)
}

/// Render `text` to a vector of pre-styled ANSI lines fitting in
/// `width` cells.
pub fn render(theme: &Theme, text: &str, width: u16) -> Vec<String> {
    let mut r = MdRenderer::new(*theme, width);
    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(text, opts);
    for ev in parser {
        r.handle(ev);
    }
    r.finish()
}

struct MdRenderer {
    theme: Theme,
    width: u16,

    /// Already-finalized lines (each ≤ width cells, ANSI-styled).
    lines: Vec<String>,

    /// Buffer for the current paragraph, heading, or table cell.
    paragraph: String,

    /// Stack of inline style modifiers applied to text events.
    inline_stack: Vec<InlineStyle>,

    /// While inside a code block: the language tag and accumulated source.
    in_code: Option<(String, String)>,

    /// List context stack — each item is the next ordinal, or None for bullets.
    list_stack: Vec<Option<u64>>,

    /// Block-quote depth — affects paragraph prefix.
    quote_depth: u16,

    /// Current heading level if inside a heading.
    heading: Option<HeadingLevel>,

    /// Active table — populated between Start(Table) and End(Table).
    table: Option<TableCtx>,

    /// Paragraph buffer saved when entering a cell, restored on cell end.
    saved_paragraph: Option<String>,
}

struct TableCtx {
    alignments: Vec<Alignment>,
    head: Vec<String>,
    body: Vec<Vec<String>>,
    current_row: Vec<String>,
    in_head: bool,
}

#[derive(Clone, Copy)]
enum InlineStyle {
    Bold,
    Italic,
    Strike,
    Code,
}

impl MdRenderer {
    fn new(theme: Theme, width: u16) -> Self {
        Self {
            theme,
            width,
            lines: Vec::new(),
            paragraph: String::new(),
            inline_stack: Vec::new(),
            in_code: None,
            list_stack: Vec::new(),
            quote_depth: 0,
            heading: None,
            table: None,
            saved_paragraph: None,
        }
    }

    fn left_pad(&self) -> String {
        let mut s = "  ".to_string();
        for _ in 0..self.quote_depth {
            s.push_str(&paint_fg(self.theme.border_muted, "│ "));
        }
        s
    }

    fn body_width(&self) -> u16 {
        let pad = 2 + (self.quote_depth as usize) * 2;
        self.width.saturating_sub(pad as u16)
    }

    fn handle(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => {
                if let Some((_, code)) = self.in_code.as_mut() {
                    code.push_str(&t);
                } else {
                    self.paragraph.push_str(&self.style_text(&t));
                }
            }
            Event::Code(c) => {
                self.paragraph
                    .push_str(&paint_fg(self.theme.code, &format!("`{c}`")));
            }
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::FootnoteReference(_) => {}
            Event::SoftBreak => {
                self.paragraph.push(' ');
            }
            Event::HardBreak => {
                self.flush_paragraph();
            }
            Event::Rule => {
                let pad = self.left_pad();
                let mut s = String::from(&pad);
                let bw = self.body_width() as usize;
                s.push_str(&self.theme.border_muted.fg());
                for _ in 0..bw {
                    s.push('─');
                }
                s.push_str(RESET);
                self.lines.push(s);
            }
            Event::TaskListMarker(checked) => {
                let mark = if checked { "[x] " } else { "[ ] " };
                self.paragraph.push_str(&paint_fg(self.theme.muted, mark));
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.heading = Some(level);
            }
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(s) => s.to_string(),
                    _ => String::new(),
                };
                self.in_code = Some((lang, String::new()));
            }
            Tag::List(start) => {
                self.list_stack.push(start);
            }
            Tag::Item => {
                let pad = self.left_pad();
                let bullet = if let Some(Some(n)) = self.list_stack.last_mut() {
                    let s = format!("{}. ", *n);
                    *n += 1;
                    paint_fg(self.theme.list_bullet, &s)
                } else {
                    paint_fg(self.theme.list_bullet, "• ")
                };
                self.paragraph.push_str(&pad);
                self.paragraph.push_str(&bullet);
            }
            Tag::BlockQuote(_) => {
                self.flush_paragraph();
                self.quote_depth += 1;
            }
            Tag::Emphasis => self.inline_stack.push(InlineStyle::Italic),
            Tag::Strong => self.inline_stack.push(InlineStyle::Bold),
            Tag::Strikethrough => self.inline_stack.push(InlineStyle::Strike),
            Tag::Link { .. } => self.inline_stack.push(InlineStyle::Code),
            Tag::Table(alignments) => {
                self.flush_paragraph();
                self.table = Some(TableCtx {
                    alignments,
                    head: Vec::new(),
                    body: Vec::new(),
                    current_row: Vec::new(),
                    in_head: false,
                });
            }
            Tag::TableHead => {
                if let Some(t) = self.table.as_mut() {
                    t.in_head = true;
                    t.current_row.clear();
                }
            }
            Tag::TableRow => {
                if let Some(t) = self.table.as_mut() {
                    t.current_row.clear();
                }
            }
            Tag::TableCell => {
                // Divert text events into a fresh cell buffer; restore on cell end.
                self.saved_paragraph = Some(std::mem::take(&mut self.paragraph));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush_paragraph();
                self.lines.push(String::new());
            }
            TagEnd::Heading(_) => {
                let prefix = match self.heading {
                    Some(HeadingLevel::H1) => "# ",
                    Some(HeadingLevel::H2) => "## ",
                    Some(HeadingLevel::H3) => "### ",
                    Some(_) => "#### ",
                    None => "",
                };
                let body = std::mem::take(&mut self.paragraph);
                let pad = self.left_pad();
                let bw = self.body_width();
                let mut full = String::new();
                full.push_str(BOLD);
                full.push_str(&self.theme.heading.fg());
                full.push_str(prefix);
                full.push_str(&body);
                full.push_str(RESET);
                for l in wrap_text_keep_ansi(&full, bw) {
                    self.lines.push(format!("{pad}{l}"));
                }
                self.lines.push(String::new());
                self.heading = None;
            }
            TagEnd::CodeBlock => {
                if let Some((lang, code)) = self.in_code.take() {
                    self.emit_code_block(&lang, &code);
                }
            }
            TagEnd::List(_) => {
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.lines.push(String::new());
                }
            }
            TagEnd::Item => {
                self.flush_paragraph();
            }
            TagEnd::BlockQuote(_) => {
                self.flush_paragraph();
                if self.quote_depth > 0 {
                    self.quote_depth -= 1;
                }
                self.lines.push(String::new());
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.inline_stack.pop();
            }
            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.paragraph);
                self.paragraph = self.saved_paragraph.take().unwrap_or_default();
                if let Some(t) = self.table.as_mut() {
                    t.current_row.push(cell);
                }
            }
            TagEnd::TableRow | TagEnd::TableHead => {
                if let Some(t) = self.table.as_mut() {
                    let row = std::mem::take(&mut t.current_row);
                    if t.in_head {
                        t.head = row;
                    } else {
                        t.body.push(row);
                    }
                    if matches!(tag, TagEnd::TableHead) {
                        t.in_head = false;
                    }
                }
            }
            TagEnd::Table => {
                if let Some(t) = self.table.take() {
                    self.emit_table(t);
                }
            }
            _ => {}
        }
    }

    fn flush_paragraph(&mut self) {
        if self.paragraph.is_empty() {
            return;
        }
        let body = std::mem::take(&mut self.paragraph);
        let pad = self.left_pad();
        let bw = self.body_width();
        for l in wrap_text_keep_ansi(&body, bw) {
            self.lines.push(format!("{pad}{l}"));
        }
    }

    fn emit_code_block(&mut self, lang: &str, code: &str) {
        let pad = self.left_pad();
        let label = if lang.is_empty() { "code" } else { lang };
        // Top edge
        let top = format!(
            "{pad}{}",
            paint_fg(self.theme.code_block_border, &format!("┌─ {label} "))
        );
        self.lines.push(top);

        // Body
        let syntax = syntax_set()
            .find_syntax_by_token(lang)
            .or_else(|| syntax_set().find_syntax_by_extension(lang))
            .unwrap_or_else(|| syntax_set().find_syntax_plain_text());
        let theme = &theme_set().themes["base16-ocean.dark"];
        let mut h = HighlightLines::new(syntax, theme);
        for line in code.split('\n') {
            // Strip trailing CR if present.
            let line = line.strip_suffix('\r').unwrap_or(line);
            // Skip the typical trailing empty line after the closing fence.
            // We still emit empty middle lines.
            let highlighted = match h.highlight_line(line, &syntax_set()) {
                Ok(ranges) => as_24_bit_terminal_escaped(&ranges, false),
                Err(_) => paint_fg(self.theme.code_block, line),
            };
            let mut s = String::new();
            s.push_str(&pad);
            s.push_str(&paint_fg(self.theme.code_block_border, "│ "));
            s.push_str(&highlighted);
            s.push_str(RESET);
            self.lines.push(s);
        }
        // Drop a single trailing empty body row that pulldown-cmark adds.
        if matches!(self.lines.last(), Some(l) if visible_width(l) <= visible_width(&format!("{pad}│ ")))
        {
            self.lines.pop();
        }
        // Bottom edge
        self.lines.push(format!(
            "{pad}{}",
            paint_fg(self.theme.code_block_border, "└─")
        ));
        self.lines.push(String::new());
    }

    fn emit_table(&mut self, t: TableCtx) {
        if t.head.is_empty() && t.body.is_empty() {
            return;
        }
        let pad = self.left_pad();
        let body_w = self.body_width() as usize;

        let cols = t
            .head
            .len()
            .max(t.body.iter().map(|r| r.len()).max().unwrap_or(0));
        if cols == 0 {
            return;
        }

        // Natural column widths (visible cells of widest cell per column).
        let mut col_w: Vec<usize> = vec![0; cols];
        for c in 0..cols {
            if let Some(cell) = t.head.get(c) {
                col_w[c] = col_w[c].max(visible_width(cell));
            }
            for row in &t.body {
                if let Some(cell) = row.get(c) {
                    col_w[c] = col_w[c].max(visible_width(cell));
                }
            }
        }
        // Each column gets at least 3 cells plus 2 cells of padding on each
        // side. Total table width = sum(col_w) + cols + 1 borders + cols * 2 paddings.
        for w in col_w.iter_mut() {
            *w = (*w).max(3);
        }
        let frame_overhead = cols + 1 + cols * 2; // borders + padding
        let max_data_w = body_w.saturating_sub(frame_overhead);
        let total: usize = col_w.iter().sum();
        if total > max_data_w {
            // Shrink proportionally to fit within max_data_w.
            let scale = max_data_w as f64 / total as f64;
            let mut shrunk: Vec<usize> = col_w
                .iter()
                .map(|w| ((*w as f64) * scale).floor() as usize)
                .collect();
            // Each column still needs at least 3 cells.
            for w in shrunk.iter_mut() {
                *w = (*w).max(3);
            }
            // If we overshot due to the floor() / min(3) clamp, trim from the
            // widest column iteratively.
            while shrunk.iter().sum::<usize>() > max_data_w {
                if let Some((idx, _)) = shrunk.iter().enumerate().max_by_key(|(_, w)| **w) {
                    if shrunk[idx] > 3 {
                        shrunk[idx] -= 1;
                    } else {
                        break;
                    }
                }
            }
            col_w = shrunk;
        }

        let border_color = self.theme.border_muted.fg();
        let reset = RESET;
        let h = "─";
        let v = paint_fg(self.theme.border_muted, "│");
        let edge = |left: &str, sep: &str, right: &str| -> String {
            let mut s = String::new();
            s.push_str(&pad);
            s.push_str(&border_color);
            s.push_str(left);
            for (i, w) in col_w.iter().enumerate() {
                for _ in 0..(*w + 2) {
                    s.push_str(h);
                }
                if i + 1 < col_w.len() {
                    s.push_str(sep);
                }
            }
            s.push_str(right);
            s.push_str(reset);
            s
        };

        // Top edge
        self.lines.push(edge("┌", "┬", "┐"));

        // Header row (if present)
        if !t.head.is_empty() {
            let row = self.format_table_row(&t.head, &col_w, &t.alignments, &v, true);
            self.lines.push(format!("{pad}{row}"));
            self.lines.push(edge("├", "┼", "┤"));
        }

        // Body rows with horizontal separators between them.
        for (i, body_row) in t.body.iter().enumerate() {
            if i > 0 {
                self.lines.push(edge("├", "┼", "┤"));
            }
            let row = self.format_table_row(body_row, &col_w, &t.alignments, &v, false);
            self.lines.push(format!("{pad}{row}"));
        }

        // Bottom edge
        self.lines.push(edge("└", "┴", "┘"));
        self.lines.push(String::new());
    }

    fn format_table_row(
        &self,
        cells: &[String],
        col_w: &[usize],
        alignments: &[Alignment],
        v: &str,
        is_header: bool,
    ) -> String {
        let mut s = String::new();
        s.push_str(v);
        for (i, w) in col_w.iter().enumerate() {
            let raw = cells.get(i).cloned().unwrap_or_default();
            let truncated = if visible_width(&raw) > *w {
                crate::utils::truncate_to_width(&raw, *w)
            } else {
                raw
            };
            let align = alignments.get(i).copied().unwrap_or(Alignment::None);
            let visible = visible_width(&truncated);
            let pad_total = w.saturating_sub(visible);
            let (left, right) = match align {
                Alignment::Right => (pad_total, 0usize),
                Alignment::Center => (pad_total / 2, pad_total - pad_total / 2),
                _ => (0usize, pad_total),
            };
            s.push(' ');
            for _ in 0..left {
                s.push(' ');
            }
            if is_header {
                s.push_str(BOLD);
                s.push_str(&self.theme.heading.fg());
                s.push_str(&truncated);
                s.push_str(RESET);
            } else {
                s.push_str(&truncated);
            }
            for _ in 0..right {
                s.push(' ');
            }
            s.push(' ');
            s.push_str(v);
        }
        s
    }

    fn style_text(&self, s: &str) -> String {
        if self.inline_stack.is_empty() {
            return s.to_string();
        }
        let mut prefix = String::new();
        let mut color: Option<&str> = None;
        for st in &self.inline_stack {
            match st {
                InlineStyle::Bold => prefix.push_str(BOLD),
                InlineStyle::Italic => prefix.push_str("\x1b[3m"),
                InlineStyle::Strike => prefix.push_str("\x1b[9m"),
                InlineStyle::Code => {
                    color = Some("link");
                }
            }
        }
        let mut out = String::with_capacity(s.len() + 16);
        out.push_str(&prefix);
        if let Some(_) = color {
            out.push_str(&self.theme.link.fg());
            out.push_str("\x1b[4m");
        }
        out.push_str(s);
        out.push_str(RESET);
        out
    }

    fn finish(mut self) -> Vec<String> {
        self.flush_paragraph();
        // Trim leading/trailing blank lines.
        while matches!(self.lines.first(), Some(l) if l.is_empty()) {
            self.lines.remove(0);
        }
        while matches!(self.lines.last(), Some(l) if l.is_empty()) {
            self.lines.pop();
        }
        self.lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::strip_ansi;

    #[test]
    fn renders_simple_table() {
        let theme = Theme::dark();
        let md = "| Lang   | Year |\n|--------|-----:|\n| Rust   | 2010 |\n| Python | 1991 |\n";
        let lines = render(&theme, md, 80);
        let plain: Vec<String> = lines.iter().map(|l| strip_ansi(l)).collect();
        // Expect borders + header + 2 body rows + seps + bottom + blank.
        assert!(
            plain.iter().any(|l| l.contains("Lang")),
            "header rendered: {plain:?}"
        );
        assert!(
            plain.iter().any(|l| l.contains("Rust")),
            "body row 1: {plain:?}"
        );
        assert!(
            plain.iter().any(|l| l.contains("Python")),
            "body row 2: {plain:?}"
        );
        assert!(plain.iter().any(|l| l.contains("┌")), "top edge: {plain:?}");
        assert!(
            plain.iter().any(|l| l.contains("└")),
            "bottom edge: {plain:?}"
        );
        // Count horizontal separators (├) — should be 2:
        // one after header, one between body rows.
        let sep_count = plain.iter().filter(|l| l.contains("├")).count();
        assert_eq!(sep_count, 2, "expected 2 horizontal separators: {plain:?}");
    }

    #[test]
    fn renders_table_with_inline_styles() {
        let theme = Theme::dark();
        let md = "| Name | Note |\n|------|------|\n| **bold** | *italic* |\n";
        let lines = render(&theme, md, 80);
        // The structure should still be a valid table; styles apply within cells.
        assert!(lines.iter().any(|l| l.contains("┌")));
        assert!(lines.iter().any(|l| l.contains("│")));
    }
}

/// Wrap a styled string preserving inline ANSI escapes — we simply use
/// the visible-width word wrapper, then if the line started with an SGR
/// escape carry it onto wrapped continuations. Good enough for the
/// common case where styles span a few words.
fn wrap_text_keep_ansi(s: &str, width: u16) -> Vec<String> {
    if visible_width(s) <= width as usize {
        return vec![s.to_string()];
    }
    wrap_text(s, width)
}
