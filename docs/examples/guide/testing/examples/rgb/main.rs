/// Port of Python Textual `docs/examples/guide/testing/rgb.py`.
///
/// Demonstrates an RGB colour-switching app used as a testing example.
///
/// Python features ported:
/// - Three buttons (Red, Green, Blue) laid out in a `Horizontal` container,
///   centred on the screen.
/// - A `Footer` showing the declared key bindings.
/// - App-level BINDINGS (r/g/b) → `switch_color('<color>')` change the
///   screen background colour.
/// - `Button.Pressed` handler forwards to the same action using the button id
///   as the colour name.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}
Horizontal {
    width: auto;
    height: auto;
}
"#;

struct RGBApp;

impl TextualApp for RGBApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("r", "switch_color('red')", "Go Red"),
            BindingDecl::new("g", "switch_color('green')", "Go Green"),
            BindingDecl::new("b", "switch_color('blue')", "Go Blue"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Horizontal::new()
                    .with_child(Button::new("Red").id("red"))
                    .with_child(Button::new("Green").id("green"))
                    .with_child(Button::new("Blue").id("blue")),
            )
            .with_child(Footer::new())
    }

    /// Handle button presses: use the button id as the colour name.
    ///
    /// Python: `@on(Button.Pressed) def pressed_button(self, event): self.action_switch_color(event.button.id)`
    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            if let Some(color_name) = bp.button_id.as_deref() {
                switch_screen_color(app, color_name, ctx);
            }
            ctx.set_handled();
        }
    }

    /// Handle the `switch_color('<color>')` action dispatched by key bindings.
    ///
    /// Python: `def action_switch_color(self, color: str): self.screen.styles.background = color`
    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut textual::event::WidgetCtx) {
        if let Some(parsed) = parse_action(action) {
            if parsed.name == "switch_color" {
                if let Some(color_name) = parsed.arguments.first() {
                    switch_screen_color(app, color_name, ctx);
                }
            }
        }
    }
}

fn switch_screen_color(app: &mut App, color_name: &str, ctx: &mut textual::event::WidgetCtx) {
    if let Some(color) = textual::style::parse_color_like(color_name) {
        let _ = app.query_mut("Screen").map(|q| {
            q.set_styles(|styles| styles.set_bg(color));
        });
        ctx.set_handled();
        ctx.request_repaint();
    }
}

fn main() -> textual::Result<()> {
    run_sync(RGBApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_composes_without_panic() {
        let mut app = RGBApp;
        let _root = app.compose();
    }

    #[test]
    fn bindings_declared() {
        let app = RGBApp;
        let bindings = app.bindings();
        assert_eq!(bindings.len(), 3);
        assert_eq!(bindings[0].key, "r");
        assert_eq!(bindings[1].key, "g");
        assert_eq!(bindings[2].key, "b");
    }

    #[test]
    fn binding_actions_contain_color_args() {
        let app = RGBApp;
        let bindings = app.bindings();
        assert!(bindings[0].action.contains("red"));
        assert!(bindings[1].action.contains("green"));
        assert!(bindings[2].action.contains("blue"));
    }
}
