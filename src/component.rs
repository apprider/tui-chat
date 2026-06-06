//! Component trait and Container — ported from pi-mono's TUI.
//!
//! A Component renders into a flat `Vec<String>`, one entry per visual
//! line, each line already containing ANSI escape sequences. Lines must
//! not exceed `width` cells of *visible* output (use
//! `crate::utils::visible_width` to measure).
//!
//! The renderer composes children top-to-bottom into one flat vector,
//! diffs it against the previous frame, and writes only the changed
//! tail rows. There is no z-order or absolute positioning — layering is
//! implicit in child order.

/// A renderable unit. Children may have interior mutability (the editor
/// owns its buffer); the renderer asks each component for its current
/// frame on every redraw pass.
pub trait Component {
    /// Render this component as a vector of lines (each ≤ `width` cells).
    fn render(&self, width: u16) -> Vec<String>;

    /// Forward raw input to the component when it is focused. Default:
    /// ignore. Components that own input state (the editor, dialogs)
    /// override this.
    fn handle_input(&mut self, _ev: &crossterm::event::Event) -> InputOutcome {
        InputOutcome::Ignored
    }

    /// Drop any cached render state. Called on resize and theme change.
    fn invalidate(&mut self) {}

    /// Whether the cursor should be shown when this component is focused.
    /// Editor returns true; passive components return false.
    fn wants_cursor(&self) -> bool {
        false
    }
}

/// Outcome of dispatching an input event to a component.
#[derive(Clone, Debug)]
pub enum InputOutcome {
    /// Event consumed; request a re-render but no other action.
    Consumed,
    /// Event consumed AND the component wants to emit a high-level action
    /// to the App (e.g. submit a chat message, run a slash command, exit).
    Action(Action),
    /// Event was not relevant to this component.
    Ignored,
}

/// High-level actions a component can request from the App. Kept small
/// — the App routes these to slash handlers / chat send / lifecycle.
#[derive(Clone, Debug)]
pub enum Action {
    /// Submit text from the editor. May be a chat message or a slash
    /// command; the App decides based on the leading character.
    Submit(String),
    /// Cancel current operation (Esc).
    Cancel,
    /// Exit the app cleanly (Ctrl+D on empty buffer, or /exit).
    Exit,
    /// Force a redraw (e.g. theme/state changed externally).
    Redraw,
}

/// A simple ordered group of children. The renderer walks containers
/// recursively.
pub struct Container {
    pub children: Vec<Box<dyn Component>>,
}

impl Container {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    pub fn push(&mut self, c: Box<dyn Component>) {
        self.children.push(c);
    }

    pub fn clear(&mut self) {
        self.children.clear();
    }

    pub fn len(&self) -> usize {
        self.children.len()
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for Container {
    fn render(&self, width: u16) -> Vec<String> {
        let mut out = Vec::with_capacity(self.children.len() * 2);
        for child in &self.children {
            out.extend(child.render(width));
        }
        out
    }

    fn invalidate(&mut self) {
        for child in &mut self.children {
            child.invalidate();
        }
    }
}
