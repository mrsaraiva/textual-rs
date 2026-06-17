/// Port of Python Textual `docs/examples/guide/actions/actions04.py`.
///
/// Demonstrates app-level action dispatch via key bindings.
///
/// Python features ported:
/// - A `Static` widget showing a bold heading and colour labels.
/// - App-level BINDINGS (r/g/b) → `set_background('red'/'green'/'blue')` change
///   the screen background colour via `on_app_action_str`.
///
/// Framework gaps (not yet available in textual-rs):
/// - `[@click=app.set_background('red')]` Rich markup inline action links:
///   Python Textual renders the colour names as clickable links that fire the
///   `set_background` action when clicked.  The Rust Label/Static pipeline
///   parses Rich markup styles but does not yet attach `@click` metadata to
///   arbitrary markup spans or route those clicks as app-level actions.  The
///   colour labels are therefore displayed as plain text (no link decoration)
///   and click interactions are absent.  The keyboard bindings (r/g/b) are
///   fully functional.
use textual::prelude::*;

/// Mirrors the Python TEXT variable, with the `[@click=...]` wrappers omitted
/// because inline action links are not yet supported by the Rust Label/Static
/// rendering pipeline.
const TEXT: &str = "\
[b]Set your background[/b]
Red
Green
Blue";

struct ActionsApp;

impl TextualApp for ActionsApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("r", "set_background('red')", "Red"),
            BindingDecl::new("g", "set_background('green')", "Green"),
            BindingDecl::new("b", "set_background('blue')", "Blue"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(TEXT))
    }

    /// Handle the custom `set_background` action dispatched by the key bindings.
    ///
    /// Python: `def action_set_background(self, color: str) -> None: self.screen.styles.background = color`
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
    fn bindings_declared() {
        let app = ActionsApp;
        let bindings = app.bindings();
        assert_eq!(bindings.len(), 3);
        assert_eq!(bindings[0].key, "r");
        assert_eq!(bindings[1].key, "g");
        assert_eq!(bindings[2].key, "b");
    }

    #[test]
    fn binding_actions_contain_color_args() {
        let app = ActionsApp;
        let bindings = app.bindings();
        assert!(bindings[0].action.contains("red"));
        assert!(bindings[1].action.contains("green"));
        assert!(bindings[2].action.contains("blue"));
    }
}
