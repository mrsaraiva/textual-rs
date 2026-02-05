//! Key/mouse event diagnostics harness.
//!
//! Displays all key and mouse events in real-time, showing both the raw
//! crossterm data and the canonical normalized representation from
//! `KeyEventData`.
//!
//! Similar to Python Textual's `textual keys` command.
//!
//! **Known limitation:** the runtime intercepts `q` and `Esc` before
//! dispatching to widgets, so those keys will exit the app immediately.
//! All other keys are captured and displayed.

use textual::prelude::*;

// ---------------------------------------------------------------------------
// KeyLog widget
// ---------------------------------------------------------------------------

struct KeyLog {
    id: WidgetId,
    entries: Vec<String>,
}

impl KeyLog {
    fn new() -> Self {
        Self {
            id: WidgetId::new(),
            entries: Vec::new(),
        }
    }

    fn push(&mut self, entry: String) {
        self.entries.push(entry);
        // Keep up to 400 lines (~100 key events at 4 lines each).
        if self.entries.len() > 400 {
            self.entries.remove(0);
        }
    }
}

impl Widget for KeyLog {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn style_type(&self) -> &'static str {
        "KeyLog"
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::Key(key) => {
                let raw_code = format!("{:?}", key.code);
                let raw_mods = format!("{:?}", key.modifiers);
                let raw_kind = format!("{:?}", key.kind);

                let line1 = format!(
                    "Key: {:?}  char={:?}  printable={}",
                    key.name(),
                    key.character,
                    key.is_printable,
                );
                let line2 = format!(
                    "  aliases={:?}  display={:?}  id={:?}",
                    key.aliases(),
                    key.display(),
                    key.identifier(),
                );
                let line3 = format!("  raw=({}, {}, {})", raw_code, raw_mods, raw_kind);
                self.push(line1);
                self.push(line2);
                self.push(line3);
                self.push(String::new());

                ctx.set_handled();
                ctx.request_repaint();
            }
            Event::MouseDown(mouse) => {
                let entry = format!(
                    "MouseDown: screen=({},{}) local=({},{}) target={}",
                    mouse.screen_x,
                    mouse.screen_y,
                    mouse.x,
                    mouse.y,
                    mouse.target.as_u64(),
                );
                self.push(entry);
                self.push(String::new());
                ctx.request_repaint();
            }
            Event::MouseUp(mouse) => {
                let target = mouse
                    .target
                    .map(|t| t.as_u64().to_string())
                    .unwrap_or_else(|| "none".to_string());
                let entry = format!(
                    "MouseUp: screen=({},{}) local=({},{}) target={}",
                    mouse.screen_x, mouse.screen_y, mouse.x, mouse.y, target,
                );
                self.push(entry);
                self.push(String::new());
                ctx.request_repaint();
            }
            Event::MouseScroll(mouse) => {
                let target = mouse
                    .target
                    .map(|t| t.as_u64().to_string())
                    .unwrap_or_else(|| "none".to_string());
                let entry = format!(
                    "MouseScroll: screen=({},{}) local=({},{}) target={} delta=({}, {}) mods={:?}",
                    mouse.screen_x,
                    mouse.screen_y,
                    mouse.x,
                    mouse.y,
                    target,
                    mouse.delta_x,
                    mouse.delta_y,
                    mouse.modifiers,
                );
                self.push(entry);
                self.push(String::new());
                ctx.request_repaint();
            }
            Event::AppFocus(focused) => {
                let entry = format!("AppFocus: {}", focused);
                self.push(entry);
                self.push(String::new());
                ctx.request_repaint();
            }
            Event::Resize(w, h) => {
                let entry = format!("Resize: {}x{}", w, h);
                self.push(entry);
                self.push(String::new());
                ctx.request_repaint();
            }
            _ => {}
        }
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        let text = if self.entries.is_empty() {
            "(waiting for input...)".to_string()
        } else {
            // Render newest entries first so the latest event is always
            // visible at the top without requiring auto-scroll.
            let reversed: Vec<&str> = self.entries.iter().rev().map(|s| s.as_str()).collect();
            reversed.join("\n")
        };
        Label::new(text).render(console, options)
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }

    let mut app = App::new()?;
    let root = AppRoot::new()
        .with_child(Label::new(
            "Key Diagnostics -- press any key (q/Esc exits via runtime)",
        ))
        .with_child(Spacer::new(1))
        .with_child(KeyLog::new())
        .with_child(Spacer::new(1))
        .with_child(Label::new(
            "All keys/mouse/focus/resize events captured above",
        ));
    let mut scroll_root = ScrollView::new(root).scroll_step(2);
    app.run_widget_tree(&mut scroll_root).await
}
