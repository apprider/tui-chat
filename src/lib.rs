//! Inline chat TUI components — generic building blocks for terminal
//! chat interfaces with markdown rendering, syntax highlighting,
//! multi-line editors, and a differential scrollback renderer.

pub mod app;
pub mod commands;
pub mod component;
pub mod components;
pub mod renderer;
pub mod theme;
pub mod utils;

pub use renderer::{Renderer, terminal_height, terminal_width, CURSOR_MARKER};
pub use utils::{compose_left_right, format_compact};
