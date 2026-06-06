//! Differential inline renderer.
//!
//! Renders to *normal terminal scrollback* (no alternate screen). Each
//! frame is a flat `Vec<String>` of pre-styled ANSI lines; we diff
//! against the previous frame and emit cursor-move + rewrite sequences
//! for only the changed tail rows. The whole frame is wrapped in DEC
//! 2026 synchronized output so terminals that support it flip
//! atomically and we never flicker.
//!
//! Algorithm (pi-mono `tui.ts:888-1187`):
//!
//! ```text
//! width changed     → full redraw (line wrapping changes)
//! frame shrunk      → full redraw (clears stale rows below)
//! lines appended    → MoveDown N, write tail
//! lines changed     → MoveUp to firstChanged, rewrite [firstChanged..n)
//! ```
//!
//! Cursor handling: the hardware cursor is hidden throughout. Components
//! that need a visible cursor (the editor) emit a fake reverse-video
//! block at the right column. This avoids the "drift one row per frame"
//! class of bugs entirely — the hardware cursor stays at the natural end
//! of the last rendered line at all times.

use crossterm::{
    cursor, queue,
    style::Print,
    terminal::{self, Clear, ClearType},
};
use std::io::{self, Write};

use super::utils::strip_ansi_inplace;

/// Pi compatibility: components may emit this sentinel where the hardware
/// cursor should land *if the terminal supports IME placement*. The
/// renderer strips it from the output unconditionally so it never
/// influences the diff. We do not currently re-position the hardware
/// cursor — IME support is a follow-up.
pub const CURSOR_MARKER: &str = "\x1b_pi:c\x07";

pub struct Renderer {
    /// Last frame written (with `CURSOR_MARKER` stripped).
    previous: Vec<String>,
    /// Last terminal width seen.
    last_width: u16,
    /// DEC 2026 synchronized output. Defaults true; terminals that don't
    /// support it ignore the escape.
    pub sync_output: bool,
    /// Whether a frame has been written since the last full redraw.
    primed: bool,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            previous: Vec::new(),
            last_width: 0,
            sync_output: true,
            primed: false,
        }
    }

    /// Forget the previous frame. Use after resize, theme change, or
    /// after some other code wrote to stdout behind our back.
    pub fn invalidate(&mut self) {
        self.previous.clear();
        self.primed = false;
    }

    /// Render `lines` to `out`, emitting the minimal diff against the
    /// previous frame. After this returns, the hardware cursor is at the
    /// natural end of the last rendered line.
    pub fn render<W: Write>(
        &mut self,
        out: &mut W,
        lines: Vec<String>,
        width: u16,
    ) -> io::Result<()> {
        // Strip the cursor marker so styling adjacent to it doesn't
        // perturb diffing.
        let mut lines = lines;
        for l in lines.iter_mut() {
            strip_ansi_inplace(l, CURSOR_MARKER);
        }

        let force_full = width != self.last_width || !self.primed;

        if self.sync_output {
            queue!(out, Print("\x1b[?2026h"))?;
        }
        queue!(out, cursor::Hide)?;

        if force_full {
            self.write_full(out, &lines)?;
        } else if lines.len() < self.previous.len() {
            // Frame shrunk — clear from current position down and rewrite.
            self.write_shrunk(out, &lines)?;
        } else {
            // Same height or grew. Find first differing line.
            let common = self.previous.len().min(lines.len());
            let first_changed = (0..common)
                .find(|&i| self.previous[i] != lines[i])
                .unwrap_or(common);

            if first_changed >= self.previous.len() && lines.len() == self.previous.len() {
                // No change at all.
            } else if first_changed >= self.previous.len() {
                // Pure append: lines grew with no change to existing.
                // Cursor is at end of previous last line. Emit \r\n + tail.
                for line in &lines[self.previous.len()..] {
                    queue!(out, Print("\r\n"), Print(line), Print("\x1b[0m"))?;
                }
            } else {
                // Mid-frame change (and possibly grew). Move cursor up
                // from end-of-previous-frame to first_changed row.
                //
                //   end-of-frame is at row index (prev.len() - 1).
                //   target is at row index first_changed.
                //   distance = (prev.len() - 1) - first_changed.
                let up = (self.previous.len() - 1).saturating_sub(first_changed) as u16;
                queue!(out, Print("\r"))?;
                if up > 0 {
                    queue!(out, cursor::MoveUp(up))?;
                }

                // Rewrite from first_changed through end of new frame.
                let last_idx = lines.len() - 1;
                for (i, line) in lines[first_changed..].iter().enumerate() {
                    queue!(
                        out,
                        Clear(ClearType::CurrentLine),
                        Print(line),
                        Print("\x1b[0m")
                    )?;
                    let absolute = first_changed + i;
                    if absolute < last_idx {
                        queue!(out, Print("\r\n"))?;
                    }
                }
            }
        }

        if self.sync_output {
            queue!(out, Print("\x1b[?2026l"))?;
        }
        out.flush()?;

        self.previous = lines;
        self.last_width = width;
        self.primed = true;
        Ok(())
    }

    fn write_full<W: Write>(&self, out: &mut W, lines: &[String]) -> io::Result<()> {
        if self.primed && !self.previous.is_empty() {
            // Move back to the top of the previous frame, then clear from
            // there down. Cursor is currently at end of previous last
            // line.
            let up = (self.previous.len() - 1) as u16;
            queue!(out, Print("\r"))?;
            if up > 0 {
                queue!(out, cursor::MoveUp(up))?;
            }
            queue!(out, Clear(ClearType::FromCursorDown))?;
        }
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                queue!(out, Print("\r\n"))?;
            }
            queue!(out, Print(line), Print("\x1b[0m"))?;
        }
        Ok(())
    }

    fn write_shrunk<W: Write>(&self, out: &mut W, lines: &[String]) -> io::Result<()> {
        // Move to top of previous frame, clear down, rewrite new frame.
        let up = (self.previous.len() - 1) as u16;
        queue!(out, Print("\r"))?;
        if up > 0 {
            queue!(out, cursor::MoveUp(up))?;
        }
        queue!(out, Clear(ClearType::FromCursorDown))?;
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                queue!(out, Print("\r\n"))?;
            }
            queue!(out, Print(line), Print("\x1b[0m"))?;
        }
        Ok(())
    }

    /// Emit a final newline so subsequent shell prompts start on a fresh
    /// line, and clear our state. The rendered frame remains in
    /// scrollback.
    pub fn finalize<W: Write>(&mut self, out: &mut W) -> io::Result<()> {
        queue!(out, cursor::Show, Print("\r\n"))?;
        out.flush()?;
        self.previous.clear();
        self.primed = false;
        Ok(())
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn terminal_width() -> u16 {
    terminal::size().map(|(w, _)| w).unwrap_or(80)
}

#[allow(dead_code)]
pub fn terminal_height() -> u16 {
    terminal::size().map(|(_, h)| h).unwrap_or(24)
}
