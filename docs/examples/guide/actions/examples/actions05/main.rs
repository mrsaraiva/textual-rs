/// Port of Python Textual `docs/examples/guide/actions/actions05.py`.
///
/// Demonstrates the action **namespace chain** (widget → screen → app):
/// - Each `ColorSwitcher` (a `Static` subclass) declares its own
///   `set_background` action.  Its `[@click=set_background('cyan')]` links are
///   *unnamespaced*, so they resolve to the nearest handler on the bubble path
///   — the clicked `ColorSwitcher` itself — and tint only that panel.
/// - The app-level key bindings `r`/`g`/`b` use the same `set_background` action
///   name, but with no focused widget declaring it they resolve up to the app,
///   whose `set_background` tints the whole screen.
///
/// Python:
/// ```python
/// class ColorSwitcher(Static):
///     def action_set_background(self, color: str) -> None:
///         self.styles.background = color
/// class ActionsApp(App):
///     BINDINGS = [("r", "set_background('red')", "Red"), ...]
///     def action_set_background(self, color: str) -> None:
///         self.screen.styles.background = color
/// ```
use textual::prelude::*;
use textual::style::Color;

const TEXT: &str = "
[b]Set your background[/b]
[@click=set_background('cyan')]Cyan[/]
[@click=set_background('magenta')]Magenta[/]
[@click=set_background('yellow')]Yellow[/]
";

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

/// `ColorSwitcher` — a `Static` subclass that owns a `set_background` action
/// changing only *its own* background.  Mirrors Python's
/// `class ColorSwitcher(Static)`.
struct ColorSwitcher {
    inner: Static,
    bg: Option<Color>,
}

impl ColorSwitcher {
    fn new() -> Self {
        Self {
            inner: Static::new(TEXT),
            bg: None,
        }
    }
}

const COLOR_SWITCHER_ACTIONS: &[ActionDecl] = &[ActionDecl {
    name: "set_background",
    namespace: "",
    description: "Set this panel's background",
    default_binding: None,
}];

impl Widget for ColorSwitcher {
    fn style_type(&self) -> &'static str {
        "ColorSwitcher"
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    /// Inject this panel's chosen background as inline style (highest CSS
    /// specificity), mirroring Python `self.styles.background = color`.
    fn style(&self) -> Option<Style> {
        let mut style = self.inner.style().unwrap_or_default();
        if let Some(bg) = self.bg {
            style.bg = Some(bg);
        }
        Some(style)
    }

    fn action_registry(&self) -> &[ActionDecl] {
        COLOR_SWITCHER_ACTIONS
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut textual::event::WidgetCtx) -> bool {
        if action.name == "set_background" {
            if let Some(color_name) = action.arguments.first().and_then(|a| a.as_str()) {
                if let Some(color) = textual::style::parse_color_like(color_name) {
                    self.bg = Some(color);
                    ctx.request_style_invalidation();
                    ctx.request_repaint();
                    ctx.set_handled();
                    return true;
                }
            }
        }
        false
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }
}

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
            .with_child(ColorSwitcher::new())
            .with_child(ColorSwitcher::new())
    }

    /// App-level `set_background` (key bindings): tints the whole screen.
    ///
    /// Python: `def action_set_background(self, color): self.screen.styles.background = color`
    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut textual::event::WidgetCtx) {
        if let Ok(parsed) = parse_action(action) {
            if parsed.name == "set_background" {
                if let Some(color_name) = parsed.arguments.first().and_then(|a| a.as_str()) {
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
        assert_eq!(app.bindings().len(), 3);
    }

    #[test]
    fn text_carries_widget_scoped_click_actions() {
        // Unnamespaced @click actions resolve to the ColorSwitcher widget.
        assert!(TEXT.contains("[@click=set_background('cyan')]"));
        assert!(TEXT.contains("[@click=set_background('magenta')]"));
        assert!(TEXT.contains("[@click=set_background('yellow')]"));
    }

    #[test]
    fn color_switcher_set_background_action_sets_bg() {
        let mut cs = ColorSwitcher::new();
        assert!(cs.bg.is_none());
        let action = parse_action("set_background('cyan')").unwrap();
        let mut ctx = EventCtx::default();
        assert!(cs.execute_action(&action, &mut ctx));
        assert!(cs.bg.is_some());
    }

    fn screen_bg(app: &App) -> Option<Color> {
        let node = app.query_one("Screen").ok()?;
        app.node_explicit_bg(node)
    }

    /// LIVENESS PROBE: with no focused widget owning `set_background`, the
    /// app-level r/g/b BINDINGS resolve up the namespace chain to the app and
    /// tint the whole screen. Guards the binding -> namespace-resolution -> app
    /// `on_app_action_str` -> Screen set_bg path. (The per-panel `@click`
    /// route — `ColorSwitcher::execute_action` — is covered by the unit test
    /// above; the headless click path can't route `@click` end-to-end.)
    #[test]
    fn liveness_app_binding_tints_whole_screen() {
        textual::run_test(ActionsApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["r"])?;
            assert_eq!(
                screen_bg(pilot.app()),
                textual::style::parse_color_like("red"),
                "app binding 'r' must tint the whole screen red"
            );
            assert_ne!(before, pilot.app().frame_fingerprint(), "frame must change");
            Ok(())
        })
        .unwrap();
    }
}
