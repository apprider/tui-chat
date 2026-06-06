//! High-level chat application builder.
//!
//! Wires the editor, chat scrollback, status line, and renderer into a
//! single state machine. The caller drives the loop (no async runtime
//! assumed), but a `run_blocking()` convenience is provided for simple
//! scripts.
//!
//! ## Quick start — blocking
//!
//! ```rust,no_run
//! use std::io;
//! use tui_chat::app::{ChatApp, Delta};
//! use tui_chat::theme::Theme;
//!
//! let mut app = ChatApp::new(Theme::dark());
//! app.run_blocking(|msg| {
//!     Ok(format!("Echo: {msg}"))
//! }).unwrap();
//! ```
//!
//! ## Custom event loop
//!
//! ```rust,no_run
//! use crossterm::event::{Event, poll, read};
//! use std::io::{self, Write};
//! use std::time::Duration;
//! use tui_chat::app::{ChatApp, AppEvent, AppAction};
//! use tui_chat::renderer::{Renderer, terminal_width};
//! use tui_chat::theme::Theme;
//!
//! let mut app = ChatApp::new(Theme::dark());
//! let mut renderer = Renderer::new();
//! let mut stdout = io::stdout();
//!
//! loop {
//!     let width = terminal_width();
//!     let frame = app.render(width);
//!     renderer.render(&mut stdout, frame, width).unwrap();
//!
//!     if poll(Duration::from_millis(50)).unwrap() {
//!         let ev = read().unwrap();
//!         match app.on_event(AppEvent::Key(ev)) {
//!             AppAction::Exit => break,
//!             AppAction::Submit(text) => {
//!                 // Send to your backend, get deltas back
//!                 // app.on_event(AppEvent::Delta(Delta::Text(chunk)));
//!             }
//!             _ => {}
//!         }
//!     }
//! }
//! ```

use std::io::{self, Write};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;

use crate::commands::CommandRegistry;
use crate::component::{Action, Component, Container, InputOutcome};
use crate::components::editor::Editor;
use crate::components::messages::{AssistantMessage, StatusLine, UserMessage};
use crate::components::text::Spacer;
use crate::renderer::{terminal_width, Renderer};
use crate::theme::{paint_fg, Theme};

/// One chunk of assistant output delivered by the caller's backend.
#[derive(Clone, Debug)]
pub enum Delta {
    /// Text to append to the in-flight assistant message.
    Text(String),
    /// Thinking / reasoning text rendered in muted styling.
    Thinking(String),
    /// Finalize the streaming message and move it to scrollback.
    Done,
    /// Show an error note and clear streaming state.
    Error(String),
}

/// Events the caller feeds into `ChatApp::on_event`.
#[derive(Clone, Debug)]
pub enum AppEvent {
    /// A crossterm key (or paste, resize) event.
    Key(Event),
    /// A delta from the backend.
    Delta(Delta),
    /// Timer tick — drives the spinner.
    Timer,
}

/// Actions the caller may want to react to after `on_event`.
#[derive(Clone, Debug)]
pub enum AppAction {
    /// Nothing special — just re-render.
    Continue,
    /// The user submitted a chat message. The caller should send it to
    /// the backend and start feeding `AppEvent::Delta`s back in.
    Submit(String),
    /// The user submitted a slash command. The caller may choose to
    /// dispatch via `CommandRegistry` or handle directly.
    Command(String),
    /// Clean exit requested.
    Exit,
}

/// High-level chat application. Owns all UI state and exposes a
/// `tick`-style event loop (no async runtime required).
pub struct ChatApp {
    theme: Theme,
    chat: Container,
    editor: Editor,
    status: StatusLine,
    pub renderer: Renderer,
    streaming: Option<AssistantMessage>,
    registry: CommandRegistry,
}

impl ChatApp {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            chat: Container::new(),
            editor: Editor::new(theme),
            status: StatusLine::new(theme),
            renderer: Renderer::new(),
            streaming: None,
            registry: CommandRegistry::with_builtins(),
        }
    }

    /// Replace the default built-in command registry.
    pub fn with_registry(mut self, registry: CommandRegistry) -> Self {
        self.registry = registry;
        self.refresh_provider();
        self
    }

    /// Access the command registry (e.g. to add more commands after
    /// construction).
    pub fn registry(&mut self) -> &mut CommandRegistry {
        self.refresh_provider();
        &mut self.registry
    }

    /// Push a passive message into the chat scrollback.
    pub fn push_message(&mut self, c: Box<dyn Component>) {
        self.chat.push(c);
    }

    /// Clear the chat scrollback.
    pub fn clear_chat(&mut self) {
        self.chat.clear();
    }

    /// True while a backend response is being streamed.
    pub fn is_streaming(&self) -> bool {
        self.streaming.is_some()
    }

    /// True while the editor is disabled (e.g. while streaming).
    pub fn is_editor_disabled(&self) -> bool {
        self.editor.disabled
    }

    /// Disable/enable the editor explicitly.
    pub fn set_editor_disabled(&mut self, v: bool) {
        self.editor.disabled = v;
    }

    /// Set the editor placeholder text.
    pub fn set_placeholder(&mut self, s: impl Into<String>) {
        self.editor.placeholder = s.into();
    }

    /// Process a single event and return what (if anything) the caller
    /// should do next.
    pub fn on_event(&mut self, event: AppEvent) -> AppAction {
        match event {
            AppEvent::Key(ev) => self.handle_key(ev),
            AppEvent::Delta(d) => {
                self.handle_delta(d);
                AppAction::Continue
            }
            AppEvent::Timer => {
                self.status.tick();
                AppAction::Continue
            }
        }
    }

    /// Compose the full frame for the current state at the given width.
    pub fn render(&self, width: u16) -> Vec<String> {
        let mut frame = Vec::new();
        frame.extend(self.chat.render(width));
        if let Some(ref m) = self.streaming {
            frame.extend(m.render(width));
        }
        frame.extend(self.status.render(width));
        frame.extend(self.editor.render(width));
        frame.push(String::new()); // breathing room
        frame
    }

    /// Convenience: run a fully-managed blocking loop. The caller
    /// provides a callback that receives the submitted text and
    /// returns the full assistant response as a `String`. The response
    /// is "fake-streamed" character-by-character so the UI stays alive.
    ///
    /// For real streaming (e.g. SSE), use the `on_event` API directly
    /// in your own event loop.
    pub fn run_blocking<F>(&mut self, mut backend: F) -> io::Result<()>
    where
        F: FnMut(&str) -> Result<String, String>,
    {
        let mut stdout = io::stdout();
        terminal::enable_raw_mode()?;

        writeln!(stdout)?;

        let mut needs_render = true;

        loop {
            let width = terminal_width();

            if needs_render {
                let frame = self.render(width);
                self.renderer.render(&mut stdout, frame, width)?;
                needs_render = false;
            }

            // Block for up to 50 ms waiting for a key event.
            if event::poll(Duration::from_millis(50))? {
                let ev = event::read()?;
                match self.on_event(AppEvent::Key(ev)) {
                    AppAction::Exit => break,
                    AppAction::Submit(text) => {
                        match backend(&text) {
                            Ok(reply) => {
                                // Fake-stream the reply in chunks so the
                                // user sees it appear gradually.
                                let chunk_size = 10;
                                let chars: Vec<char> = reply.chars().collect();
                                for chunk in chars.chunks(chunk_size) {
                                    let s: String = chunk.iter().collect();
                                    self.on_event(AppEvent::Delta(Delta::Text(s)));
                                    let frame = self.render(width);
                                    self.renderer.render(&mut stdout, frame, width)?;
                                    std::thread::sleep(Duration::from_millis(20));
                                }
                                self.on_event(AppEvent::Delta(Delta::Done));
                            }
                            Err(e) => {
                                self.on_event(AppEvent::Delta(Delta::Error(e)));
                            }
                        }
                        needs_render = true;
                    }
                    AppAction::Command(text) => {
                        let outcome = self.registry.dispatch(&text);
                        self.apply_outcome(outcome);
                        needs_render = true;
                    }
                    AppAction::Continue => needs_render = true,
                }
            }

            // Spinner tick when status is active.
            if self.status.is_active() {
                self.on_event(AppEvent::Timer);
                needs_render = true;
            }
        }

        let _ = terminal::disable_raw_mode();
        self.renderer.finalize(&mut stdout)?;
        Ok(())
    }

    // ── internals ──────────────────────────────────────────────────

    fn refresh_provider(&mut self) {
        let provider = self.registry.suggestion_provider();
        self.editor.set_provider(Box::new(provider));
    }

    fn handle_key(&mut self, ev: Event) -> AppAction {
        // Ctrl+C on empty buffer with nothing streaming → exit.
        if let Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers,
            ..
        }) = ev
        {
            if modifiers.contains(KeyModifiers::CONTROL)
                && self.editor.buffer().is_empty()
                && self.streaming.is_none()
            {
                return AppAction::Exit;
            }
        }

        // Resize is handled at the event-loop level (renderer.invalidate).
        if let Event::Resize(_, _) = ev {
            self.renderer.invalidate();
            return AppAction::Continue;
        }

        match self.editor.handle_input(&ev) {
            InputOutcome::Consumed => AppAction::Continue,
            InputOutcome::Action(Action::Submit(text)) => {
                if text.starts_with('/') {
                    AppAction::Command(text)
                } else {
                    self.chat.push(Box::new(UserMessage::new(self.theme, text.clone())));
                    self.status.set("thinking…");
                    self.editor.disabled = true;
                    AppAction::Submit(text)
                }
            }
            InputOutcome::Action(Action::Cancel) => AppAction::Continue,
            InputOutcome::Action(Action::Exit) => AppAction::Exit,
            InputOutcome::Action(Action::Redraw) => {
                self.renderer.invalidate();
                AppAction::Continue
            }
            InputOutcome::Ignored => AppAction::Continue,
        }
    }

    fn handle_delta(&mut self, delta: Delta) {
        match delta {
            Delta::Text(chunk) => {
                if self.streaming.is_none() {
                    self.streaming = Some(AssistantMessage::new(self.theme, String::new()));
                    self.status.clear();
                }
                if let Some(ref mut m) = self.streaming {
                    m.append(&chunk);
                }
            }
            Delta::Thinking(text) => {
                if self.streaming.is_none() {
                    self.streaming = Some(AssistantMessage::new(self.theme, String::new()));
                }
                if let Some(ref mut m) = self.streaming {
                    m.append_thinking(&text);
                }
            }
            Delta::Done => {
                if let Some(m) = self.streaming.take() {
                    self.chat.push(Box::new(m));
                    self.chat.push(Box::new(Spacer(1)));
                }
                self.editor.disabled = false;
            }
            Delta::Error(e) => {
                self.streaming = None;
                self.chat.push(Box::new(Spacer(1)));
                self.chat.push(Box::new(crate::components::text::Text::new(
                    paint_fg(self.theme.error, &format!("✗ {e}")),
                )));
                self.chat.push(Box::new(Spacer(1)));
                self.editor.disabled = false;
                self.status.clear();
            }
        }
    }

    fn apply_outcome(&mut self, outcome: crate::commands::CommandOutcome) {
        use crate::commands::CommandOutcome;
        use crate::components::text::Text;
        use crate::theme::paint_fg;

        match outcome {
            CommandOutcome::Message(s) => {
                self.chat.push(Box::new(Text::new(paint_fg(self.theme.muted, &s))));
                self.chat.push(Box::new(Spacer(1)));
            }
            CommandOutcome::Error(s) => {
                self.chat.push(Box::new(Text::new(paint_fg(
                    self.theme.error,
                    &format!("✗ {s}"),
                ))));
                self.chat.push(Box::new(Spacer(1)));
            }
            CommandOutcome::Clear => self.chat.clear(),
            CommandOutcome::Exit => {
                // In the blocking runner we can't force-exit from here;
                // the caller's loop checks for AppAction::Exit.
                // For now we just push a note.
                self.chat.push(Box::new(Text::new(paint_fg(
                    self.theme.muted,
                    "exiting…",
                ))));
            }
            CommandOutcome::Quiet => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn new_app_has_empty_chat() {
        let app = ChatApp::new(Theme::dark());
        assert!(app.chat.is_empty());
        assert!(!app.is_streaming());
    }

    #[test]
    fn submit_message_creates_user_msg() {
        let mut app = ChatApp::new(Theme::dark());
        // Type "hello" via key events
        for ch in "hello".chars() {
            app.on_event(AppEvent::Key(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            ))));
        }
        let action = app.on_event(AppEvent::Key(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));
        assert!(matches!(action, AppAction::Submit(s) if s == "hello"));
        assert!(app.is_editor_disabled());
    }

    #[test]
    fn submit_slash_is_command() {
        let mut app = ChatApp::new(Theme::dark());
        app.on_event(AppEvent::Key(Event::Key(KeyEvent::new(
            KeyCode::Char('/'),
            KeyModifiers::NONE,
        ))));
        for ch in "help".chars() {
            app.on_event(AppEvent::Key(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            ))));
        }
        let action = app.on_event(AppEvent::Key(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ))));
        assert!(matches!(action, AppAction::Command(s) if s == "/help"));
    }

    #[test]
    fn delta_text_builds_streaming() {
        let mut app = ChatApp::new(Theme::dark());
        app.on_event(AppEvent::Delta(Delta::Text("hi".into())));
        assert!(app.is_streaming());
    }

    #[test]
    fn delta_done_moves_to_chat() {
        let mut app = ChatApp::new(Theme::dark());
        app.on_event(AppEvent::Delta(Delta::Text("hello".into())));
        app.on_event(AppEvent::Delta(Delta::Done));
        assert!(!app.is_streaming());
        assert!(!app.is_editor_disabled());
        assert!(!app.chat.is_empty());
    }

    #[test]
    fn delta_error_clears_streaming() {
        let mut app = ChatApp::new(Theme::dark());
        app.on_event(AppEvent::Delta(Delta::Text("partial".into())));
        app.on_event(AppEvent::Delta(Delta::Error("boom".into())));
        assert!(!app.is_streaming());
        assert!(!app.is_editor_disabled());
    }

    #[test]
    fn clear_chat_works() {
        let mut app = ChatApp::new(Theme::dark());
        app.on_event(AppEvent::Delta(Delta::Text("x".into())));
        app.on_event(AppEvent::Delta(Delta::Done));
        assert!(!app.chat.is_empty());
        app.clear_chat();
        assert!(app.chat.is_empty());
    }

    #[test]
    fn timer_ticks_status() {
        let mut app = ChatApp::new(Theme::dark());
        app.status.set("loading");
        assert!(app.status.is_active());
        app.on_event(AppEvent::Timer);
        // Status should still be active, frame advanced
        assert!(app.status.is_active());
    }

    #[test]
    fn renders_without_panic() {
        let app = ChatApp::new(Theme::dark());
        let lines = app.render(80);
        assert!(!lines.is_empty());
    }

    #[test]
    fn exit_action_on_ctrl_d_empty() {
        let mut app = ChatApp::new(Theme::dark());
        // Ctrl+D is Enter with ctrl modifier on empty buffer
        let ev = Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
        let action = app.on_event(AppEvent::Key(ev));
        // Our key handler doesn't special-case Ctrl+D; only Ctrl+C on empty.
        // So this falls through to the editor's handle_input.
        // The editor's Ctrl+D handler returns Action::Exit when buffer is empty.
        assert!(matches!(action, AppAction::Exit));
    }
}
