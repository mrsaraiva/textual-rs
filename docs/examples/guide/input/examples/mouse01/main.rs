/// Port of Python Textual `docs/examples/guide/input/mouse01.py`.
///
/// Displays a `RichLog` of `MouseMove` events and a `Ball` (Static) widget
/// that tracks the mouse pointer.
///
/// Python uses `App.on_mouse_move` which fires globally. In Rust there is no
/// global mouse-move hook on `TextualApp`, so this port uses a custom message:
/// - `MouseScreen` is a passthrough container that intercepts `Event::MouseMove`
///   in `on_event_capture` and posts a `MouseMoved` message.
/// - `MouseApp::on_message_with_app` receives the message, writes to the
///   `RichLog`, and updates the `Ball`'s CSS `offset` so it follows the cursor.
use textual::prelude::*;
use textual::style::{Offset, OffsetValue};

// ---------------------------------------------------------------------------
// CSS (mirrors mouse01.tcss)
// ---------------------------------------------------------------------------

const CSS: &str = r##"
Screen {
    layers: log ball;
}

RichLog {
    layer: log;
}

Ball {
    layer: ball;
    width: auto;
    height: 1;
    background: $secondary;
    border: tall $secondary;
    color: $background;
    box-sizing: content-box;
    text-style: bold;
    padding: 0 4;
}
"##;

// ---------------------------------------------------------------------------
// Custom message: carries the screen coordinates of each MouseMove event.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MouseMoved {
    screen_x: u16,
    screen_y: u16,
}

textual::impl_message!(MouseMoved);

// ---------------------------------------------------------------------------
// Ball widget — a Static that labels itself.
// Ball does not need custom logic; it is positioned via CSS `offset`.
// ---------------------------------------------------------------------------

struct Ball {
    inner: Static,
}

impl Ball {
    fn new() -> Self {
        Self {
            inner: Static::new("Textual"),
        }
    }
}

impl Widget for Ball {
    fn style_type(&self) -> &'static str {
        "Ball"
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.inner.take_composed_children()
    }

    fn on_node_state_changed(&mut self, old: NodeState, new: NodeState) {
        self.inner.on_node_state_changed(old, new);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.inner.on_message(message, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.inner.on_mouse_move(x, y)
    }

    fn on_mouse_scroll(&mut self, dx: i32, dy: i32, ctx: &mut EventCtx) {
        self.inner.on_mouse_scroll(dx, dy, ctx);
    }

    fn set_inline_style(&mut self, style: Style) {
        self.inner.set_inline_style(style);
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }
}

// ---------------------------------------------------------------------------
// MouseScreen — intercepts mouse-move events and posts MouseMoved messages.
//
// Mirroring Python's `App.on_mouse_move`, which fires regardless of which
// widget is under the cursor, we hook `on_event_capture` (capture phase,
// root-to-leaf) so every MouseMove passes through here before children see it.
// ---------------------------------------------------------------------------

struct MouseScreen {
    log: RichLog,
    ball: Ball,
}

impl MouseScreen {
    fn new() -> Self {
        Self {
            log: RichLog::new(),
            ball: Ball::new(),
        }
    }
}

impl Widget for MouseScreen {
    fn style_type(&self) -> &'static str {
        "MouseScreen"
    }

    fn render(
        &self,
        _console: &rich_rs::Console,
        _options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        // Passthrough: no own rendering surface.
        rich_rs::Segments::default()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        vec![Box::new(std::mem::replace(
            &mut self.log,
            RichLog::new(),
        )) as Box<dyn Widget>,
        Box::new(std::mem::replace(
            &mut self.ball,
            Ball::new(),
        )) as Box<dyn Widget>]
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::MouseMove(m) = event {
            ctx.post_message(MouseMoved {
                screen_x: m.screen_x,
                screen_y: m.screen_y,
            });
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.log.on_event(event, ctx);
        self.ball.on_event(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.log.on_message(message, ctx);
        self.ball.on_message(message, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.log.on_mouse_move(x, y) || self.ball.on_mouse_move(x, y)
    }

    fn on_mouse_scroll(&mut self, dx: i32, dy: i32, ctx: &mut EventCtx) {
        self.log.on_mouse_scroll(dx, dy, ctx);
        self.ball.on_mouse_scroll(dx, dy, ctx);
    }

    fn set_inline_style(&mut self, style: Style) {
        self.log.set_inline_style(style.clone());
        self.ball.set_inline_style(style);
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        // No own seed needed; children provide their own via take_composed_children.
        NodeSeed::default()
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct MouseApp;

impl TextualApp for MouseApp {
    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(MouseScreen::new())
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Some(m) = message.downcast_ref::<MouseMoved>() {
            let screen_x = m.screen_x;
            let screen_y = m.screen_y;

            // Write event info to the RichLog.
            let line = format!(
                "MouseMove(screen_x={screen_x}, screen_y={screen_y})"
            );
            let _ = app.with_query_one_mut_as::<RichLog, _>("RichLog", |log| {
                log.write(line);
            });

            // Move the Ball: offset = screen_offset - (8, 2).
            // Python: Ball.offset = event.screen_offset - (8, 2)
            // The Ball has `border: tall` (1 row top, 1 bottom = 2 total) and
            // `padding: 0 4` (4 cols each side = 8 total), so subtracting those
            // values centres the ball on the cursor.
            let ox = (screen_x as i16).saturating_sub(8);
            let oy = (screen_y as i16).saturating_sub(2);
            let _ = app.query_mut("Ball").map(|q| {
                q.set_styles(|s| {
                    s.style.offset = Some(Offset {
                        x: OffsetValue::Cells(ox),
                        y: OffsetValue::Cells(oy),
                    });
                })
                .refresh()
            });

            ctx.request_repaint();
            ctx.set_handled();
        }
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(MouseApp::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_app_composes_without_panic() {
        let mut app = MouseApp;
        let _root = app.compose();
    }
}
