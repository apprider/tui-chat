# tui-chat

Generic inline chat TUI components for terminal applications. Extracted
from the **Ko** agent OS, this crate gives you the same scrollback-based
chat experience ( Markdown, syntax highlighting, multi-line editor with
slash-command autocomplete, and a differential renderer) without tying
you to any particular application logic.

## Key features

- **Inline scrollback rendering** — writes to normal terminal scrollback,
  not the alternate screen. The whole session stays visible after exit.
- **Markdown + syntax highlighting** — `pulldown-cmark` + `syntect` for
  fenced code blocks, tables, lists, block quotes, and inline styles.
- **Multi-line editor** — soft-wraps at viewport width, history navigation,
  fake cursor with reverse-video block, optional suggestion provider for
  `/` commands.
- **Differential renderer** — only rewrites the changed tail of each frame,
  wrapped in DEC 2026 synchronized output to avoid flicker.
- **Generic status bar** — `StatusBar` builder with colored left/right parts
  and smart truncation when the terminal narrows.
- **Layout utilities** — `compose_left_right` and `format_compact` so you
  can build your own app-specific footer in ~20 lines.
- **Theme system** — ANSI truecolor palette (copied from pi-mono's dark
  theme) that you can swap or extend.

## Quick start

Add to your `Cargo.toml`:

```toml
[dependencies]
tui-chat = { path = "../tui-chat" }
crossterm = "0.28"
```

A minimal echo loop:

```rust
use std::io::{self, Write};
use crossterm::{event::{Event, KeyCode, KeyEvent}, terminal, execute};
use tui_chat::{
    component::{Component, Container, Action, InputOutcome},
    components::editor::{Editor, SlashProvider},
    components::messages::{UserMessage, AssistantMessage, StatusLine},
    components::text::Spacer,
    renderer::{Renderer, terminal_width},
    theme::Theme,
};

fn main() -> io::Result<()> {
    let theme = Theme::dark();
    let mut renderer = Renderer::new();
    let mut stdout = io::stdout();

    terminal::enable_raw_mode()?;

    let mut chat = Container::new();
    let mut status = StatusLine::new(theme);
    let mut editor = Editor::new(theme);
    editor.set_provider(Box::new(SlashProvider::new(vec![])));

    loop {
        let width = terminal_width();
        let mut frame = Vec::new();
        frame.extend(chat.render(width));
        frame.extend(status.render(width));
        frame.extend(editor.render(width));
        renderer.render(&mut stdout, frame, width)?;

        if let Event::Key(KeyEvent { code, .. }) = crossterm::event::read()? {
            match editor.handle_input(&Event::Key(KeyEvent::new(code, crossterm::event::KeyModifiers::NONE))) {
                InputOutcome::Action(Action::Submit(text)) => {
                    chat.push(Box::new(UserMessage::new(theme, text.clone())));
                    chat.push(Box::new(AssistantMessage::new(theme, text)));
                    chat.push(Box::new(Spacer(1)));
                }
                InputOutcome::Action(Action::Exit) => break,
                _ => {}
            }
        }
    }

    terminal::disable_raw_mode()?;
    renderer.finalize(&mut stdout)?;
    Ok(())
}
```

For a real integration you would replace the synchronous `crossterm::event::read()`
with an async event loop (see how `ko-cli` wires `tokio::select!` between
key events and an mpsc stream of assistant deltas).

## Crate layout

| Module | What it provides |
|---|---|
| `component` | `Component` trait, `Container`, `Action`, `InputOutcome` |
| `components::editor` | Multi-line input `Editor`, `SuggestionProvider` trait, `SlashProvider` |
| `components::messages` | `UserMessage`, `AssistantMessage`, `StatusLine` |
| `components::status_bar` | `StatusBar` — generic two-line footer builder |
| `components::text` | `Text`, `TruncatedText`, `Spacer` primitives |
| `components::markdown` | `render()` — Markdown → ANSI lines |
| `renderer` | `Renderer` with differential output, `terminal_width()` |
| `theme` | `Theme`, `Rgb`, helper macros for ANSI styling |
| `utils` | `visible_width`, `wrap_text`, `truncate_to_width`, `strip_ansi`, `compose_left_right`, `format_compact` |

## Building your own footer

If the generic `StatusBar` covers your needs, use it directly:

```rust
use tui_chat::component::Component;
use tui_chat::components::status_bar::{StatusBar, StatusColor};
use tui_chat::theme::Theme;

let theme = Theme::dark();
let mut footer = StatusBar::new(theme);
footer.add_top("~/work/my-project");
footer.add_left("ready", StatusColor::Success);
footer.add_right("v1.0.0", StatusColor::Muted);
let lines = footer.render(80);
```

If you need a custom footer (e.g. token usage, pricing, or a
project-specific layout), implement the `Component` trait and compose
with `compose_left_right`:

```rust
use tui_chat::component::Component;
use tui_chat::theme::{dimmed, paint_fg, Theme};
use tui_chat::utils::compose_left_right;

struct MyFooter { theme: Theme }

impl Component for MyFooter {
    fn render(&self, width: u16) -> Vec<String> {
        let inner = (width as usize).saturating_sub(2);
        let left = paint_fg(self.theme.success, "online");
        let right = dimmed(&self.theme, "v2.3.1");
        let line = compose_left_right(&left, &right, inner);
        vec![format!(" {line} ")]
    }
}
```

Ko's own `Footer` (`crates/ko-cli/src/app/components/footer.rs`) uses
exactly this pattern — it reuses `compose_left_right` and
`format_compact` while keeping its own `UsageView` and token-formatting
logic private.
