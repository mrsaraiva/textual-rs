/// Port of Python Textual `docs/examples/guide/input/key03.py`.
///
/// Demonstrates key event handling on custom widgets: four `KeyLogger` panes
/// arranged in a 2×2 grid. Clicking a pane focuses it; then key presses are
/// logged to the focused pane.
///
/// Python original uses a `KeyLogger(RichLog)` subclass with `on_key` — here
/// `KeyLogger` wraps `RichLog` and intercepts `Event::Key` in `on_event`.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    layout: grid;
    grid-size: 2 2;
    grid-columns: 1fr;
}

KeyLogger {
    border: blank;
}

KeyLogger:hover {
    border: wide $secondary;
}

KeyLogger:focus {
    border: wide $accent;
}
"#;

// ---------------------------------------------------------------------------
// KeyLogger widget — a RichLog that logs every key event it receives.
// ---------------------------------------------------------------------------

struct KeyLogger {
    log: RichLog,
}

impl KeyLogger {
    fn new() -> Self {
        Self {
            log: RichLog::new(),
        }
    }
}

impl Widget for KeyLogger {
    fn style_type(&self) -> &'static str {
        "KeyLogger"
    }

    fn focusable(&self) -> bool {
        true
    }

    fn can_focus(&self) -> bool {
        true
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.log.render(console, options)
    }

    fn compose(&mut self) -> textual::compose::ComposeResult {
        self.log.compose()
    }

    fn on_node_state_changed(&mut self, old: NodeState, new: NodeState) {
        self.log.on_node_state_changed(old, new);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        if let Event::Key(key) = event {
            let display = format!(
                "Key(key={:?}, character={:?}, is_printable={})",
                key.name(),
                key.character,
                key.is_printable,
            );
            self.log.write(display);
            ctx.request_repaint();
            ctx.set_handled();
            return;
        }
        self.log.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        self.log.on_event_capture(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
        self.log.on_message(message, ctx);
    }

    fn on_mouse_scroll(&mut self, dx: i32, dy: i32, ctx: &mut textual::event::WidgetCtx) {
        self.log.on_mouse_scroll(dx, dy, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.log.on_mouse_move(x, y)
    }

    fn set_inline_style(&mut self, style: Style) {
        self.log.set_inline_style(style);
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.log.take_node_seed()
    }

    fn scroll_offset(&self) -> (usize, usize) {
        self.log.scroll_offset()
    }

    fn scroll_offset_f32(&self) -> (f32, f32) {
        self.log.scroll_offset_f32()
    }

    fn scroll_virtual_content_size(&self) -> Option<(usize, usize)> {
        self.log.scroll_virtual_content_size()
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct InputApp;

impl TextualApp for InputApp {
    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(KeyLogger::new())
            .with_child(KeyLogger::new())
            .with_child(KeyLogger::new())
            .with_child(KeyLogger::new())
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(InputApp::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS PROBE: focusing a `KeyLogger` pane (via Tab) then pressing a key
    /// must log the key into that pane and change the rendered frame. Guards the
    /// focus -> per-widget on_event(Key) -> RichLog write path.
    #[test]
    fn liveness_focus_then_keypress_logs_to_pane_and_changes_frame() {
        textual::run_test(InputApp::default(), |pilot| {
            // Tab focuses the first KeyLogger (focus border changes the frame).
            pilot.press(&["tab"])?;
            let focused = pilot.app().frame_fingerprint();

            // Pressing a key while a pane is focused logs into that pane.
            pilot.press(&["x"])?;
            let after_key = pilot.app().frame_fingerprint();
            assert_ne!(
                focused, after_key,
                "pressing a key while a KeyLogger is focused must log it and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
