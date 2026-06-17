/// Port of Python Textual `docs/examples/guide/actions/actions05.py`.
///
/// Demonstrates widget-scoped action dispatch via `[@click=...]` Rich markup
/// and app-level bindings for background colour changes.
///
/// Python features ported:
/// - App-level BINDINGS (r/g/b) → `set_background('red'/'green'/'blue')` change
///   the *screen* background colour via `on_app_action_str`.
/// - Two `ColorSwitcher` (Static-based) panels stacked in a 1-fr grid layout,
///   each showing the same action-link text.
///
/// Framework gaps (not yet available in textual-rs):
/// - `[@click=action('arg')]` Rich markup inline action dispatch: Python Textual
///   renders these as clickable links that fire `action_set_background(color)`
///   on the widget that owns the text.  The Rust Label/Static pipeline parses
///   Rich markup styles but does not yet attach `@click` metadata to arbitrary
///   markup spans or route those clicks as widget-level actions.  The text is
///   shown verbatim (without the `[@click=...]` tags) so the visual resembles
///   the Python output; click interactions are absent.
/// - Per-widget `action_set_background`: in Python `ColorSwitcher` is a `Static`
///   subclass with its own action method that targets only that widget's
///   background.  Rust has no equivalent widget-action dispatch mechanism yet.
use textual::prelude::*;

/// Text content equivalent to the Python TEXT variable, minus the
/// `[@click=…]` markup wrappers that are not yet supported.
const TEXT: &str = "\
[b]Set your background[/b]
Cyan
Magenta
Yellow";

const CSS: &str = r#"
Screen {
    layout: grid;
    grid-size: 1;
    grid-gutter: 2 4;
    grid-rows: 1fr;
}

ColorSwitcher {
    height: 100%;
    margin: 2 4;
}
"#;

struct ActionsApp;

impl TextualApp for ActionsApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("r", "set_background('red')", "Red"),
            BindingDecl::new("g", "set_background('green')", "Green"),
            BindingDecl::new("b", "set_background('blue')", "Blue"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new(TEXT).class("ColorSwitcher"))
            .with_child(Static::new(TEXT).class("ColorSwitcher"))
            .with_child(Footer::new())
    }

    /// Handle the custom `set_background` action from the declarative bindings.
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
    }
}
