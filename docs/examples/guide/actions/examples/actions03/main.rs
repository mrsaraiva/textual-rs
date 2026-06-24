/// Port of Python Textual `docs/examples/guide/actions/actions03.py`.
///
/// Demonstrates `@click` action-link routing: clicking a colour name in a
/// single `Static` changes the screen background.  The colour names are wrapped
/// in `[@click=app.set_background('red')]…[/]` markup; clicking a span fires the
/// named action, which is resolved against the app namespace and handled by the
/// app's custom `set_background` action.
///
/// Python:
/// ```python
/// TEXT = """
/// [b]Set your background[/b]
/// [@click=app.set_background('red')]Red[/]
/// [@click=app.set_background('green')]Green[/]
/// [@click=app.set_background('blue')]Blue[/]
/// """
/// class ActionsApp(App):
///     def compose(self) -> ComposeResult:
///         yield Static(TEXT)
///     def action_set_background(self, color: str) -> None:
///         self.screen.styles.background = color
/// ```
use textual::prelude::*;

const TEXT: &str = "\
[b]Set your background[/b]
[@click=app.set_background('red')]Red[/]
[@click=app.set_background('green')]Green[/]
[@click=app.set_background('blue')]Blue[/]";

struct ActionsApp;

impl TextualApp for ActionsApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(TEXT))
    }

    /// Handle the custom `app.set_background` action fired by the `@click`
    /// links.  Mirrors Python `action_set_background(self, color)`.
    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        if let Some(parsed) = parse_action(action) {
            if parsed.name == "set_background" {
                if let Some(color_name) = parsed.arguments.first() {
                    if let Some(color) = textual::style::parse_color_like(color_name) {
                        let _ = app.query_mut("Screen").map(|q| {
                            q.set_styles(|styles| styles.set_bg(color));
                        });
                        ctx.set_handled();
                        ctx.request_repaint();
                    }
                }
            }
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(ActionsApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_composes_without_panic() {
        let mut app = ActionsApp;
        let _root = app.compose();
    }

    #[test]
    fn text_carries_click_actions() {
        // The colour names must be wrapped in @click action-link markup so the
        // runtime can route clicks to `app.set_background`.
        assert!(TEXT.contains("[@click=app.set_background('red')]"));
        assert!(TEXT.contains("[@click=app.set_background('green')]"));
        assert!(TEXT.contains("[@click=app.set_background('blue')]"));
    }
}
