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
**Status:** ✅ Complete — see `src/components/notice.rs`

Pre-built `Notice` with info/warning/error severity, a left border, and
a subtle background tint.

**API:**
```rust
use tui_chat::components::notice::Notice;

chat.push(Box::new(Notice::info(theme, "Session saved")));
chat.push(Box::new(Notice::warning(theme, "Rate limit approaching")));
chat.push(Box::new(Notice::error(theme, "API key invalid")));
```

**Features:**
- Three severities: `Info` (no icon), `Warning` (⚠), `Error` (✗)
- Left border colored by severity
- Background tint: `tool_pending_bg` for info/warning, `error_bg` for error
- Multi-line support with word wrapping
- Customizable border width via `with_border_width(n)`

**File:** `src/components/notice.rs`

---

### 6. Collapsible / Foldable Blocks
**Status:** ✅ Complete — see `src/components/foldable.rs`

Hide long content behind a `▸ Show reasoning` toggle. Like HTML
`<details>`.

**API:**
```rust
use tui_chat::components::foldable::Foldable;

chat.push(Box::new(Foldable::new(theme, "Reasoning")
    .collapsed(true)
    .content("The user wants...")));
```

**Features:**
- Toggle via `Enter` or `Space` when focused
- Header shows `▸ Show label` / `▼ Hide label`
- Body indented with muted vertical bar
- `toggle()`, `expand()`, `collapse()` programmatic control

**File:** `src/components/foldable.rs`

---

### 7. Tool Call / Result Block
**Status:** ✅ Complete — see `src/components/tool_result.rs`

Styled block showing a tool name and its result, with optional
collapse and status indicators.

**API:**
```rust
use tui_chat::components::tool_result::ToolResult;

chat.push(Box::new(ToolResult::new(theme, "read_file")
    .result("fn main() { ... }")
    .collapsed(false)));

chat.push(Box::new(ToolResult::new(theme, "rm")
    .error("permission denied")));
```

**Features:**
- Status: `Running` (● accent), `Success` (● green), `Error` (✗ red)
- Collapsed mode shows line count: `● tool · 5 lines`
- Body indented with muted vertical bar
- `with_preview_lines(n)` for collapsed preview limit

**File:** `src/components/tool_result.rs`

---

### 8. Image / File Attachment Placeholder
**Status:** ✅ Complete — see `src/components/attachment.rs`

Styled box showing filename, MIME type, and metadata for non-text
content.

**API:**
```rust
use tui_chat::components::attachment::Attachment;

chat.push(Box::new(Attachment::new(theme, "screenshot.png", "image/png")
    .with_meta("1920×1080 · 2.3 MB")));
```

**Features:**
- 📎 paperclip icon in accent color
- Filename in foreground color
- MIME type + metadata in dimmed text
- Left border on meta line
- `with_details(mime, meta)` for bulk setting

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

| Feature | Status | Notes |
|---|---|---|
| Core component trait + Container | ✅ | |
| Differential renderer | ✅ | DEC 2026 sync output |
| Markdown + syntax highlighting | ✅ | `pulldown-cmark` + `syntect` |
| Multi-line editor | ✅ | Slash autocomplete, history |
| User / Assistant / StatusLine messages | ✅ | |
| Text primitives (Spacer, Text, TruncatedText) | ✅ | |
| Theme system (dark) | ✅ | ANSI truecolor |
| ANSI width/wrap utilities | ✅ | `visible_width`, `wrap_text`, etc. |
| Generic `StatusBar` | ✅ | `StatusBar` builder |
| `compose_left_right` layout primitive | ✅ | Truncation-aware |
| `format_compact` number formatter | ✅ | `1.2k`, `4M`, etc. |
| **Tier 1** | | |
| Generic Picker | ✅ | Filter, scroll, keyboard nav |
| Scrollable Viewport | ✅ | Auto-scroll, indicator, resize |
| Command Registry | ✅ | `/help`, `/clear`, `/exit` + custom |
| ChatApp Builder | ✅ | `run_blocking()` + custom event loop |
| **Tier 2** | | |
| System Notice | ✅ | Info/Warning/Error with border + tint |
| Foldable Blocks | ✅ | `▸ Show` / `▼ Hide` toggle |
| Tool Result Block | ✅ | Running/Success/Error status |
| Attachment Placeholder | ✅ | 📎 filename + mime + meta |

---

## Contributing

Pick a Tier 1 item and open an issue before starting work. The crate follows Ko conventions:
- No comments that narrate WHAT, only WHY.
- Doc comments for public API.
- Tests for any new layout math.
- No `reqwest::blocking` from inside tokio runtime.
