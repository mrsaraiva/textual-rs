/// Port of Python Textual `docs/examples/guide/actions/actions04.py`.
///
/// Demonstrates app-level action dispatch via key bindings **and** `@click`
/// action-links.
///
/// Python features ported:
/// - A `Static` widget showing a bold heading and clickable colour labels.
/// - `[@click=set_background('red')]` markup links: clicking a colour fires the
///   `set_background` action, resolved widget → screen → app (no explicit
///   namespace) and handled by `on_app_action_str`.
/// - App-level BINDINGS (r/g/b) → `set_background('red'/'green'/'blue')` change
///   the screen background colour via the same handler.
use textual::prelude::*;

/// Mirrors the Python TEXT variable, including the `[@click=...]` action-link
/// wrappers (now routed by the runtime's `@click` hit-test → dispatch).
const TEXT: &str = "
[b]Set your background[/b]
[@click=set_background('red')]Red[/]
[@click=set_background('green')]Green[/]
[@click=set_background('blue')]Blue[/]
";

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

    fn screen_bg(app: &App) -> Option<textual::style::Color> {
        let node = app.query_one("Screen").ok()?;
        app.node_explicit_bg(node)
    }

    /// LIVENESS PROBE: the r/g/b BINDINGS must dispatch `set_background('...')`,
    /// resolved to the app and handled by `on_app_action_str`, tinting the
    /// screen. Guards the binding -> action -> set_bg path. (The demo's `@click`
    /// links share the same handler, but the headless click path can't route
    /// `@click` — see actions03 — so this probe exercises the key-binding route.)
    #[test]
    fn liveness_color_bindings_set_screen_background() {
        textual::run_test(ActionsApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["r"])?;
            assert_eq!(
                screen_bg(pilot.app()),
                textual::style::parse_color_like("red"),
                "binding 'r' must set screen background red"
            );
            assert_ne!(before, pilot.app().frame_fingerprint(), "frame must change");
            // The screen surface visibly turns red (content is present, so the
            // Screen's runtime-set inline bg composites into the frame).
            let red = textual::style::parse_color_like("red");
            let red_cells = (0..24)
                .flat_map(|y| (0..80).map(move |x| (x, y)))
                .filter(|(x, y)| pilot.app().frame_cell_bg(*x, *y) == red)
                .count();
            assert!(red_cells > 0, "the rendered screen surface must turn red");

            pilot.press(&["g"])?;
            assert_eq!(
                screen_bg(pilot.app()),
                textual::style::parse_color_like("green"),
                "binding 'g' must set screen background green"
            );
            Ok(())
        })
        .unwrap();
    }
}
