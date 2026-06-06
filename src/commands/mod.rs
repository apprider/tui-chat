//! Slash-command registry with auto-generated suggestions, built-in
//! `/help`, `/clear`, `/exit`, and generic handler dispatch.
//!
//! ## Example
//!
//! ```rust,no_run
//! use tui_chat::commands::{CommandRegistry, CommandOutcome};
//! use tui_chat::components::text::Text;
//!
//! let mut registry = CommandRegistry::with_builtins();
//! registry.add("/model", "Switch LLM model", |_args| {
//!     CommandOutcome::Message("Open picker here...".into())
//! });
//!
//! // Plug into the editor
//! // editor.set_provider(Box::new(registry.suggestion_provider()));
//!
//! // In the event loop, after Action::Submit(text)
//! // if text.starts_with('/') {
//! //     let outcome = registry.dispatch(&text);
//! //     match outcome {
//! //         CommandOutcome::Message(s) => chat.push(Box::new(Text::new(s))),
//! //         CommandOutcome::Clear => chat.clear(),
//! //         CommandOutcome::Exit => break,
//! //         _ => {}
//! //     }
//! // }
//! ```

use crate::components::editor::{SlashProvider, Suggestion, SuggestionProvider};

/// Outcome of dispatching a slash command. The caller's event loop
/// matches on this and performs the side effect (pushing a message to
/// chat, clearing, exiting, etc.).
#[derive(Clone, Debug)]
pub enum CommandOutcome {
    /// Show a message in the chat area.
    Message(String),
    /// Show an error-styled message.
    Error(String),
    /// Clear the chat scrollback.
    Clear,
    /// Exit the application cleanly.
    Exit,
    /// No user-visible effect. Use this when the handler already did
    /// its work (e.g. spawned an async task via a channel).
    Quiet,
}

impl CommandOutcome {
    /// True if this outcome should consume the event (i.e. not fall
    /// through to normal chat submission).
    pub fn is_consumed(&self) -> bool {
        true
    }

    /// True if the app should exit.
    pub fn is_exit(&self) -> bool {
        matches!(self, CommandOutcome::Exit)
    }
}

/// One registered command.
struct RegisteredCommand {
    name: String,
    description: String,
    handler: Box<dyn Fn(&str) -> CommandOutcome + Send + Sync>,
}

/// Registry of slash commands. Implements `SuggestionProvider` so it
/// can be wired directly into the `Editor`.
///
/// The registry is `Send + Sync` so it can be shared across threads
/// (e.g. held in an `Arc` and accessed from async handlers).
pub struct CommandRegistry {
    commands: Vec<RegisteredCommand>,
    /// Whether `/help` should include built-ins in the listing.
    show_builtins_in_help: bool,
}

impl CommandRegistry {
    /// Create an empty registry. No built-ins are registered — call
    /// [`Self::with_builtins`] if you want `/help`, `/clear`, `/exit`.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            show_builtins_in_help: true,
        }
    }

    /// Create a registry pre-populated with `/help`, `/clear`, and `/exit`.
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        // Register `/help` so it appears in suggestions and the help listing.
        // The actual handler is short-circuited in `dispatch()` because it
        // needs `self` access, but this ensures it shows up everywhere else.
        r.add("/help", "Show available commands", |_args| CommandOutcome::Quiet);
        r.add("/clear", "Clear chat", |_args| CommandOutcome::Clear);
        r.add("/exit", "Exit application", |_args| CommandOutcome::Exit);
        r
    }

    /// Add a command.
    ///
    /// `name` should start with `/` (e.g. `/model`). `handler` receives
    /// the argument string (everything after the command name).
    pub fn add(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        handler: impl Fn(&str) -> CommandOutcome + Send + Sync + 'static,
    ) {
        let name = name.into();
        // Reject duplicates silently — last one wins.
        self.commands.retain(|c| c.name != name);
        self.commands.push(RegisteredCommand {
            name,
            description: description.into(),
            handler: Box::new(handler),
        });
    }

    /// Parse a `/command args` line and run the matching handler.
    ///
    /// Returns `Error("unknown command: /foo")` if no match.
    pub fn dispatch(&self, line: &str) -> CommandOutcome {
        let trimmed = line.trim();
        let mut iter = trimmed.splitn(2, char::is_whitespace);
        let cmd = iter.next().unwrap_or("");
        let args = iter.next().unwrap_or("").trim();

        // Built-ins that need `self` access.
        match cmd {
            "/help" => return CommandOutcome::Message(self.format_help()),
            _ => {}
        }

        for c in &self.commands {
            if c.name == cmd {
                return (c.handler)(args);
            }
        }

        CommandOutcome::Error(format!("unknown command: {cmd}"))
    }

    /// Generate a `Suggestion` list for every registered command.
    /// Suitable for constructing a `SlashProvider` or feeding directly
    /// to the editor.
    pub fn suggestions(&self) -> Vec<Suggestion> {
        self.commands
            .iter()
            .map(|c| Suggestion {
                trigger: c.name.clone(),
                label: c.name.clone(),
                description: c.description.clone(),
            })
            .collect()
    }

    /// Produce a `SlashProvider` from the current command list. This is
    /// a convenience so you don't need to call `suggestions()` manually.
    ///
    /// ```rust,no_run
    /// # use tui_chat::commands::CommandRegistry;
    /// # let registry = CommandRegistry::with_builtins();
    /// # let mut editor = tui_chat::components::editor::Editor::new(tui_chat::theme::Theme::dark());
    /// editor.set_provider(Box::new(registry.suggestion_provider()));
    /// ```
    pub fn suggestion_provider(&self) -> SlashProvider {
        SlashProvider::new(self.suggestions())
    }

    /// Format the `/help` text. Lists every command in two columns.
    fn format_help(&self) -> String {
        let max_name = self
            .commands
            .iter()
            .map(|c| c.name.len())
            .max()
            .unwrap_or(0)
            .max(12);

        let mut s = String::from("commands:\n");
        for c in &self.commands {
            if !self.show_builtins_in_help && is_builtin(&c.name) {
                continue;
            }
            s.push_str(&format!(
                "  {:<width$}  {}\n",
                c.name,
                c.description,
                width = max_name
            ));
        }
        s.trim_end().to_string()
    }

    /// Number of registered commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

impl SuggestionProvider for CommandRegistry {
    fn suggest(&self, line: &str) -> Vec<Suggestion> {
        if !line.starts_with('/') {
            return Vec::new();
        }
        let q = line.trim_start_matches('/');
        let mut out: Vec<Suggestion> = self
            .commands
            .iter()
            .filter(|c| c.name.trim_start_matches('/').starts_with(q))
            .map(|c| Suggestion {
                trigger: c.name.clone(),
                label: c.name.clone(),
                description: c.description.clone(),
            })
            .collect();
        if out.is_empty() {
            // substring fallback
            out = self
                .commands
                .iter()
                .filter(|c| c.name.contains(q))
                .map(|c| Suggestion {
                    trigger: c.name.clone(),
                    label: c.name.clone(),
                    description: c.description.clone(),
                })
                .collect();
        }
        out
    }
}

fn is_builtin(name: &str) -> bool {
    matches!(name, "/help" | "/clear" | "/exit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_registered_by_default() {
        let r = CommandRegistry::with_builtins();
        assert!(r.len() >= 3);
    }

    #[test]
    fn empty_registry_has_no_commands() {
        let r = CommandRegistry::new();
        assert!(r.is_empty());
    }

    #[test]
    fn dispatch_help() {
        let r = CommandRegistry::with_builtins();
        match r.dispatch("/help") {
            CommandOutcome::Message(s) => {
                assert!(s.contains("/help"));
                assert!(s.contains("/clear"));
                assert!(s.contains("/exit"));
            }
            other => panic!("expected Message, got {:?}", other),
        }
    }

    #[test]
    fn dispatch_clear() {
        let r = CommandRegistry::with_builtins();
        assert!(matches!(r.dispatch("/clear"), CommandOutcome::Clear));
    }

    #[test]
    fn dispatch_exit() {
        let r = CommandRegistry::with_builtins();
        assert!(matches!(r.dispatch("/exit"), CommandOutcome::Exit));
        assert!(r.dispatch("/exit").is_exit());
    }

    #[test]
    fn dispatch_unknown() {
        let r = CommandRegistry::with_builtins();
        match r.dispatch("/bogus") {
            CommandOutcome::Error(s) => assert!(s.contains("unknown")),
            other => panic!("expected Error, got {:?}", other),
        }
    }

    #[test]
    fn dispatch_with_args() {
        let mut r = CommandRegistry::new();
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let cap2 = captured.clone();
        r.add("/echo", "Echo args", move |args| {
            *cap2.lock().unwrap() = args.to_string();
            CommandOutcome::Quiet
        });

        r.dispatch("/echo hello world");
        assert_eq!(*captured.lock().unwrap(), "hello world");
    }

    #[test]
    fn suggestions_match_prefix() {
        let mut r = CommandRegistry::new();
        r.add("/model", "Switch model", |_| CommandOutcome::Quiet);
        r.add("/mode", "Set mode", |_| CommandOutcome::Quiet);

        let sug = r.suggest("/mod");
        assert_eq!(sug.len(), 2);
    }

    #[test]
    fn suggestions_fallback_to_substring() {
        let mut r = CommandRegistry::new();
        r.add("/model", "Switch model", |_| CommandOutcome::Quiet);

        // typing 'del' should still match '/model' via substring
        let sug = r.suggest("/del");
        assert_eq!(sug.len(), 1);
    }

    #[test]
    fn no_suggestions_without_leading_slash() {
        let r = CommandRegistry::with_builtins();
        assert!(r.suggest("help").is_empty());
    }

    #[test]
    fn duplicate_command_last_wins() {
        let mut r = CommandRegistry::new();
        r.add("/foo", "first", |_| CommandOutcome::Message("first".into()));
        r.add("/foo", "second", |_| CommandOutcome::Message("second".into()));

        match r.dispatch("/foo") {
            CommandOutcome::Message(s) => assert_eq!(s, "second"),
            other => panic!("expected second handler, got {:?}", other),
        }
    }

    #[test]
    fn suggestion_provider_convenience() {
        let r = CommandRegistry::with_builtins();
        let provider = r.suggestion_provider();
        let sug = provider.suggest("/he");
        assert!(!sug.is_empty());
    }

    #[test]
    fn send_sync_trait_bounds() {
        fn assert_send_sync<T: Send + Sync>(_: T) {}
        assert_send_sync(CommandRegistry::with_builtins());
    }
}
