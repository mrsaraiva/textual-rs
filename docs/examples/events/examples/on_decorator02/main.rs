/// Port of Python Textual `docs/examples/events/on_decorator02.py`.
///
/// Demonstrates the `@on` decorator pattern (selector-based per-button handlers).
/// Python uses `@on(Button.Pressed, "#bell")`, `@on(Button.Pressed, ".toggle.dark")`,
/// and `@on(Button.Pressed, "#quit")` — three separate handler methods, one per button.
///
/// In Rust we implement the same branching logic in `on_message_with_app`,
/// matching each button id individually (same semantics, different syntax).
/// The button with `classes="toggle dark"` in Python is given `id="toggle-dark"` in
/// Rust since `ButtonPressed` carries `button_id` but not class names.
///
/// CSS is ported from `on_decorator.tcss`.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
    layout: horizontal;
}

Button {
    margin: 2 4;
}
"#;

struct OnDecoratorApp;

impl TextualApp for OnDecoratorApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Button::new("Bell").id("bell"))
            .with_child(Button::new("Toggle dark").id("toggle-dark"))
            .with_child(Button::new("Quit").id("quit"))
    }

    /// `play_bell` — Python's `@on(Button.Pressed, "#bell")`
    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            match bp.button_id.as_deref() {
                Some("bell") => {
                    // play_bell: bell() is a no-op in Rust
                    ctx.set_handled();
                }
                Some("toggle-dark") => {
                    // toggle_dark
                    app.action_toggle_dark();
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                Some("quit") => {
                    // quit
                    ctx.request_stop();
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(OnDecoratorApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_decorator02_app_composes_without_panic() {
        let mut app = OnDecoratorApp;
        let _root = app.compose();
    }
}
