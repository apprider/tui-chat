# tui-chat Roadmap

This document tracks planned enhancements for the `tui-chat` crate.

## Philosophy
- **Generic first** — every component must be usable without knowing the caller's domain.
- **Composable** — primitives compose into higher-level widgets; nothing is monolithic.
- **Zero external runtime** — no tokio, no async runtime assumptions. The caller drives the loop.

---

## Tier 1 — Must-haves for a professional chat crate

### 1. Generic Picker / Selector Component
**Status:** ✅ Complete — see `src/components/picker.rs`

A fully generic picker with filter, scrollable viewport, and keyboard
navigation. No domain knowledge — the caller decides what "selected"
means via `PickerOutcome::Selected`.

**API:**
```rust
use tui_chat::components::picker::{Picker, PickerItem, PickerOutcome};

let items = vec![
    PickerItem::new("gpt-4", "GPT-4", "OpenAI · 128k context"),
    PickerItem::new("claude", "Claude", "Anthropic · 200k context").mark_current(),
];

let mut picker = Picker::new(theme, "Select model", items);
match picker.handle(&event) {
    PickerOutcome::Selected(item) => { /* act on item.key */ }
    PickerOutcome::Cancel => { /* dismiss */ }
    PickerOutcome::Continue => { /* re-render */ }
}
```

**Features:**
- Filterable by typing after the `>` prompt
- Scrollable viewport via `with_max_visible(n)`
- `current` checkmark (✓) support
- Keyboard: ↑↓ navigate, Enter select, Esc cancel, type to filter

**File:** `src/components/picker.rs`

---

### 2. Scrollable Viewport / Virtual Container
**Status:** ✅ Complete — see `src/components/viewport.rs`

`Container` grows forever — every keystroke re-renders every child.
`Viewport` clips to `visible_height` rows and supports scrolling with
auto-scroll, resize, and an optional indicator.

**API:**
```rust
use tui_chat::components::viewport::Viewport;
use tui_chat::component::Component;

let mut vp = Viewport::new(theme, 24); // 24 visible rows
vp.push(Box::new(UserMessage::new(theme, "hello")));
vp.scroll_up(5);       // Shift+PgUp
vp.scroll_to_bottom(); // after new message arrives
```

**Features:**
- Maintains full child list but only renders visible slice
- Scroll offset in rows (not messages — one message may span many rows)
- `auto_scroll` flag: new `push()` snaps to bottom on next render
- `scroll_up/down/to_top/to_bottom` with clamping
- Optional indicator line when scrolled away from top
- `set_visible_height()` for terminal resize
- `total_lines(width)` and `at_bottom(width)` for explicit queries

**File:** `src/components/viewport.rs`

---

### 3. Slash Command Registry + Built-in Help
**Status:** ✅ Complete — see `src/commands/mod.rs`

The `Editor` already had `SuggestionProvider`, but there was no command
*registry* or dispatch. `CommandRegistry` wires suggestions, `/help`, and
handler dispatch in one place, and implements `SuggestionProvider` so it
plugs directly into the `Editor`.

**API:**
```rust
use tui_chat::commands::{CommandRegistry, CommandOutcome};

let mut registry = CommandRegistry::with_builtins(); // /help, /clear, /exit
registry.add("/model", "Switch LLM model", |args| {
    CommandOutcome::Message(format!("picked: {args}"))
});

// Wire into editor — auto-complete now knows your commands
editor.set_provider(Box::new(registry.suggestion_provider()));

// In the event loop
let outcome = registry.dispatch(&text);
match outcome {
    CommandOutcome::Message(s) => chat.push(Box::new(Text::new(s))),
    CommandOutcome::Clear => chat.clear(),
    CommandOutcome::Exit => break,
    _ => {}
}
```

**Features:**
- Built-in `/help` (auto-generated from current command list), `/clear`, `/exit`
- Custom command registration with `name`, `description`, `handler`
- Direct `SuggestionProvider` implementation — no separate `SlashProvider` needed
- `suggestion_provider()` convenience for the editor
- Duplicate commands: last one wins
- `Send + Sync` for sharing across threads
- `CommandOutcome`: `Message` | `Error` | `Clear` | `Exit` | `Quiet`

**File:** `src/commands/mod.rs`

---

### 4. Higher-level `ChatApp` / `ChatSession` Builder
**Status:** ✅ Complete — see `src/app.rs`

A state-machine chat application that wires all components together.
The caller drives the loop via `on_event()` (no async runtime required),
and a `run_blocking()` convenience is provided for simple scripts.

**API:**
```rust
use tui_chat::app::{ChatApp, Delta};
use tui_chat::theme::Theme;

// Blocking — simplest possible usage
let mut app = ChatApp::new(Theme::dark());
app.run_blocking(|msg| {
    Ok(format!("Echo: {msg}"))
}).unwrap();
```

**Custom event loop:**
```rust
use tui_chat::app::{ChatApp, AppEvent, AppAction, Delta};

let mut app = ChatApp::new(Theme::dark());

loop {
    let frame = app.render(width);
    renderer.render(&mut stdout, frame, width)?;

    // Poll events from your backend + keyboard
    match app.on_event(AppEvent::Key(ev)) {
        AppAction::Exit => break,
        AppAction::Submit(text) => {
            // Send to backend, feed deltas back
            app.on_event(AppEvent::Delta(Delta::Text(chunk)));
            app.on_event(AppEvent::Delta(Delta::Done));
        }
        _ => {}
    }
}
```

**Features:**
- `ChatApp::new(theme)` — one-liner setup with all defaults
- `with_registry(registry)` — swap in a custom `CommandRegistry`
- `on_event(event)` — state-machine tick, returns `AppAction`
- `render(width)` — compose the full frame
- `run_blocking(backend)` — fully managed loop for simple use cases
- Slash commands dispatched automatically via built-in registry
- Streaming deltas: `Delta::Text` | `Delta::Thinking` | `Delta::Done` | `Delta::Error`
- Spinner animation driven by `AppEvent::Timer`
- Ctrl+C on empty buffer → exit (mirrors shell convention)

**File:** `src/app.rs`

---

## Tier 2 — Rich message types

### 5. System Notice Component
**Status:** 📋 Planned

Pre-built `Notice` with info/warning/error severity and a subtle left border.

**Target API:**
```rust
use tui_chat::components::notice::Notice;

chat.push(Box::new(Notice::info("Session saved")));
chat.push(Box::new(Notice::warning("Rate limit approaching")));
chat.push(Box::new(Notice::error("API key invalid")));
```

**File:** `src/components/notice.rs`

---

### 6. Collapsible / Foldable Blocks
**Status:** 📋 Planned

Hide long content behind a `▸ Show reasoning` toggle. Like HTML `<details>`.

**Target API:**
```rust
use tui_chat::components::foldable::Foldable;

chat.push(Box::new(Foldable::new(theme, "Reasoning")
    .collapsed(true)
    .content("The user wants...")));
```

**File:** `src/components/foldable.rs`

---

### 7. Tool Call / Result Block
**Status:** 📋 Planned

Reusable styled block for tool executions with expand/collapse.

**Target API:**
```rust
use tui_chat::components::tool_result::ToolResult;

chat.push(Box::new(ToolResult::new(theme, "read_file")
    .success("fn main() { ... }")
    .collapsed(true)));
```

**File:** `src/components/tool_result.rs`

---

### 8. Image / File Attachment Placeholder
**Status:** 📋 Planned

Styled box showing filename, size, mime-type for non-text content.

**Target API:**
```rust
use tui_chat::components::attachment::Attachment;

chat.push(Box::new(Attachment::new(theme, "screenshot.png", "1920x1080")));
```

**File:** `src/components/attachment.rs`

---

## Tier 3 — UX polish

### 9. Code Block Line Numbers + Copy Action
**Status:** 📋 Planned**

Add optional line-number gutter and a keyboard shortcut to copy code blocks to clipboard.

**Features:**
- `render_codeblock(theme, text, width, line_numbers: bool)` variant
- `y` keybinding when focus is on a code block
- Optional dependency on `arboard` or `clipboard` crate (feature flag)

**File:** `src/components/markdown.rs` (extension)

---

### 10. Multiple Built-in Themes
**Status:** 📋 Planned

Currently only `Theme::dark()`. Add light, high-contrast, and a builder.

**Target API:**
```rust
let dark = Theme::dark();
let light = Theme::light();
let custom = Theme::builder()
    .accent(Rgb(0xff, 0x00, 0x00))
    .build();
```

**File:** `src/theme.rs` (extension)

---

### 11. Help Overlay / Keybinding Cheat Sheet
**Status:** 📋 Planned

Full-screen or inline overlay showing active keybindings.

**Features:**
- Auto-generated from `CommandRegistry` + editor defaults
- `?` keybinding toggles overlay
- Rendered as a picker-like full-screen component

**File:** `src/components/help_overlay.rs`

---

### 12. Toast / Notification System
**Status:** 📋 Planned

Ephemeral top-right messages that auto-dismiss.

**Target API:**
```rust
use tui_chat::components::toasts::Toasts;

toasts.show("Copied!", Duration::from_secs(3));
```

**File:** `src/components/toasts.rs`

---

## Tier 4 — Persistence & lifecycle

### 13. Editor History to Disk
**Status:** 📋 Planned

Save input history to a file, restore on next launch.

**Features:**
- Configurable path (`~/.config/<app>/history`)
- Max entry limit
- Deduplication of consecutive identical entries

**File:** `src/components/editor.rs` (extension)

---

### 14. Session Save / Restore
**Status:** 📋 Planned

Serialize chat scrollback to JSON/markdown.

**Features:**
- Export to markdown (with ANSI stripped)
- Export to JSON (with metadata)
- Import/restore on launch

**File:** `src/session.rs` (new)

---

### 15. Bracketed Paste Improvements
**Status:** 📋 Planned

Better handling of large pastes — preview before sending.

**Features:**
- Detect multi-line paste > N lines
- Show "Paste N lines — send? (y/n)" confirmation
- Respect bracketed-paste mode

**File:** `src/components/editor.rs` (extension)

---

## Completed ✅

| Feature | PR / Commit | Notes |
|---|---|---|
| Core component trait + Container | Initial commit | |
| Differential renderer | Initial commit | DEC 2026 sync output |
| Markdown + syntax highlighting | Initial commit | `pulldown-cmark` + `syntect` |
| Multi-line editor | Initial commit | Slash autocomplete, history |
| User / Assistant / StatusLine messages | Initial commit | |
| Text primitives (Spacer, Text, TruncatedText) | Initial commit | |
| Theme system (dark) | Initial commit | ANSI truecolor |
| ANSI width/wrap utilities | Initial commit | `visible_width`, `wrap_text`, etc. |
| Generic `StatusBar` | Initial commit | `StatusBar` builder |
| `compose_left_right` layout primitive | Initial commit | Truncation-aware |
| `format_compact` number formatter | Initial commit | `1.2k`, `4M`, etc. |

---

## Contributing

Pick a Tier 1 item and open an issue before starting work. The crate follows Ko conventions:
- No comments that narrate WHAT, only WHY.
- Doc comments for public API.
- Tests for any new layout math.
- No `reqwest::blocking` from inside tokio runtime.
