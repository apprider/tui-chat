//! Width / wrapping / ANSI-stripping helpers.
//!
//! All TUI layout decisions go through these — never use `s.len()` or
//! `s.chars().count()` for visible width. ANSI escapes are zero-width
//! and many Unicode codepoints are double-width.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Visible cell width of `s`, ignoring ANSI escape sequences.
pub fn visible_width(s: &str) -> usize {
    strip_ansi(s).width()
}

/// Strip all ANSI escape sequences from `s`. Conservative: handles CSI
/// (`ESC [ … <final byte>`), OSC (`ESC ] … BEL` / `ESC \\`), and bare
/// SGR resets. Anything else is passed through.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.next() {
                Some('[') => {
                    // CSI: read until final byte in 0x40..=0x7E
                    for cc in chars.by_ref() {
                        if (0x40..=0x7E).contains(&(cc as u32)) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC: read until BEL or ESC \
                    while let Some(cc) = chars.next() {
                        if cc == '\x07' {
                            break;
                        }
                        if cc == '\x1b' {
                            // consume the trailing '\'
                            let _ = chars.next();
                            break;
                        }
                    }
                }
                Some(_) => {} // Unknown escape — drop the introducer + next char
                None => break,
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Truncate `s` so its visible width is ≤ `max`, preserving ANSI codes.
/// If truncation occurs, append a single `…` (counted toward width).
pub fn truncate_to_width(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let visible = visible_width(s);
    if visible <= max {
        return s.to_string();
    }
    // Walk graphemes, copying ANSI escapes through, until budget exhausted.
    let mut out = String::with_capacity(s.len());
    let mut budget = max.saturating_sub(1); // leave room for ellipsis
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Copy the whole escape verbatim
            out.push(c);
            match chars.next() {
                Some(cc @ '[') => {
                    out.push(cc);
                    for cc in chars.by_ref() {
                        out.push(cc);
                        if (0x40..=0x7E).contains(&(cc as u32)) {
                            break;
                        }
                    }
                }
                Some(cc @ ']') => {
                    out.push(cc);
                    while let Some(cc) = chars.next() {
                        out.push(cc);
                        if cc == '\x07' {
                            break;
                        }
                        if cc == '\x1b' {
                            if let Some(nc) = chars.next() {
                                out.push(nc);
                            }
                            break;
                        }
                    }
                }
                Some(cc) => out.push(cc),
                None => break,
            }
            continue;
        }
        let w = UnicodeWidthStr::width(c.to_string().as_str());
        if w > budget {
            break;
        }
        budget -= w;
        out.push(c);
    }
    out.push_str("\x1b[0m…");
    out
}

/// Word-wrap `s` to `width`, preserving ANSI escape sequences. Long
/// unbreakable runs are hard-broken at the cell boundary.
///
/// This is intentionally simple — pi has a richer wrapper that
/// reapplies SGR per line. For our needs the assistant Markdown
/// renderer (when added) emits already-wrapped lines, and user input
/// wraps without styling.
pub fn wrap_text(s: &str, width: u16) -> Vec<String> {
    let width = width.max(1) as usize;
    let mut lines: Vec<String> = Vec::new();
    for raw in s.split('\n') {
        if raw.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_w = 0usize;
        for word in raw.split_word_bounds() {
            let ww = UnicodeWidthStr::width(word);
            if ww == 0 {
                current.push_str(word);
                continue;
            }
            if current_w + ww <= width {
                current.push_str(word);
                current_w += ww;
            } else if ww > width {
                // Word longer than the line — flush current then hard-break.
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                    current_w = 0;
                }
                let mut chunk = String::new();
                let mut chunk_w = 0usize;
                for g in word.graphemes(true) {
                    let gw = UnicodeWidthStr::width(g);
                    if chunk_w + gw > width {
                        lines.push(std::mem::take(&mut chunk));
                        chunk_w = 0;
                    }
                    chunk.push_str(g);
                    chunk_w += gw;
                }
                if !chunk.is_empty() {
                    current = chunk;
                    current_w = chunk_w;
                }
            } else {
                lines.push(std::mem::take(&mut current));
                if word.chars().all(|c| c.is_whitespace()) {
                    current_w = 0;
                } else {
                    current.push_str(word);
                    current_w = ww;
                }
            }
        }
        lines.push(current);
    }
    lines
}

/// Remove every occurrence of `marker` from `s` in place.
pub fn strip_ansi_inplace(s: &mut String, marker: &str) {
    if marker.is_empty() || !s.contains(marker) {
        return;
    }
    *s = s.replace(marker, "");
}

/// Compose a line with `left` text on the left and `right` text on
/// the right, separated by spaces so the total visible width is
/// `inner`. If the combined string is too wide, drop `right` first;
/// if even the left doesn't fit, truncate it with `…`.
pub fn compose_left_right(left: &str, right: &str, inner: usize) -> String {
    let lw = visible_width(left);
    let rw = visible_width(right);
    if lw + rw + 2 <= inner {
        let pad = inner - lw - rw;
        let mut s = String::new();
        s.push_str(left);
        for _ in 0..pad {
            s.push(' ');
        }
        s.push_str(right);
        s
    } else if lw + 2 <= inner {
        truncate_to_width(left, inner)
    } else {
        truncate_to_width(left, inner)
    }
}

/// Compact human-readable number formatting (e.g. `1.2k`, `4M`).
/// Handles values from 0 to billions. Intended for token counts,
/// byte counts, or any large scalar that shouldn't waste horizontal
/// space.
pub fn format_compact(n: u64) -> String {
    if n < 1_000 {
        format!("{n}")
    } else if n < 10_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else if n < 1_000_000 {
        format!("{}k", n / 1_000)
    } else if n < 10_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n < 1_000_000_000 {
        format!("{}M", n / 1_000_000)
    } else if n < 10_000_000_000 {
        format!("{:.1}B", n as f64 / 1_000_000_000.0)
    } else {
        format!("{}B", n / 1_000_000_000)
    }
}

/// Pad a line on the right with spaces up to `width` cells. Used to
/// "paint" backgrounds across the full line (e.g. user message Box).
pub fn pad_right(s: &str, width: u16) -> String {
    let w = visible_width(s);
    if w >= width as usize {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + (width as usize - w));
    out.push_str(s);
    for _ in 0..(width as usize - w) {
        out.push(' ');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_simple_csi() {
        assert_eq!(strip_ansi("\x1b[31mhi\x1b[0m"), "hi");
    }

    #[test]
    fn width_ignores_ansi() {
        assert_eq!(visible_width("\x1b[1;31mhello\x1b[0m"), 5);
    }

    #[test]
    fn wrap_basic() {
        let lines = wrap_text("hello world from ko", 10);
        assert!(lines.iter().all(|l| visible_width(l) <= 10));
    }

    #[test]
    fn compose_fits_both() {
        let s = compose_left_right("hello", "world", 12);
        assert_eq!(strip_ansi(&s), "hello  world");
    }

    #[test]
    fn compose_drops_right_when_narrow() {
        let s = compose_left_right("hello", "world", 7);
        assert_eq!(strip_ansi(&s), "hello");
    }

    #[test]
    fn compose_truncates_left_when_too_narrow() {
        let s = compose_left_right("long text here", "world", 5);
        assert_eq!(strip_ansi(&s), "long…");
    }

    #[test]
    fn compact_small() {
        assert_eq!(format_compact(42), "42");
    }

    #[test]
    fn compact_thousands() {
        assert_eq!(format_compact(9_876), "9.9k");
        assert_eq!(format_compact(12_000), "12k");
    }

    #[test]
    fn compact_millions() {
        assert_eq!(format_compact(4_200_000), "4.2M");
        assert_eq!(format_compact(50_000_000), "50M");
    }
}
