/// Port of Python Textual `docs/examples/guide/actions/actions03.py`.
///
/// Demonstrates `action_set_background`: clicking a color label changes the
/// screen background.
///
/// Python uses `[@click=app.set_background('red')]` markup inside a single
/// `Static` widget. Custom `app.*` actions dispatched from `@click` markup
/// have no `TextualApp` hook in the current framework (framework gap: the
/// `ActionDispatchRequested` path for unknown `app.*` names does not fall back
/// to `on_app_action_str` or `on_action_with_app`).
///
/// This port preserves the interactive behavior (click → background changes)
/// using `Button` widgets and `on_message_with_app` / `ButtonPressed`.
use textual::prelude::*;

struct ActionsApp;

impl ActionsApp {
    fn set_background(&self, color: &str, app: &mut App, ctx: &mut EventCtx) {
        if let Some(c) = textual::style::parse_color_like(color) {
            if let Ok(q) = app.query_mut("Screen") {
                q.set_styles(|styles| styles.set_bg(c));
            }
            ctx.request_repaint();
        }
    }
}

impl TextualApp for ActionsApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new("[b]Set your background[/b]"))
            .with_child(Button::new("Red").id("red"))
            .with_child(Button::new("Green").id("green"))
            .with_child(Button::new("Blue").id("blue"))
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(m) = message.downcast_ref::<ButtonPressed>() {
            let color = match m.button_id.as_deref() {
                Some("red") => "red",
                Some("green") => "green",
                Some("blue") => "blue",
                _ => return,
            };
            self.set_background(color, app, ctx);
            ctx.set_handled();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(ActionsApp)
}
