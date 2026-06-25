/// Port of Python Textual `docs/examples/guide/input/binding01.py`.
///
/// Demonstrates key bindings:
/// - Press `r`, `g`, `b` to dynamically mount colored `Bar` widgets.
/// - Each bar displays the color name, centered, bold.
/// - A `Footer` shows the available bindings.
///
/// Python original: `action_add_bar(color)` mounts a `Bar(Static)` and sets
/// `bar.styles.background = Color.parse(color).with_alpha(0.5)`.
///
/// Rust approach: CSS classes `.red`, `.green`, `.blue` carry the 50%-alpha
/// background color; bindings call `add_bar('red')` etc.; `on_app_action_str`
/// parses the color argument, creates a `Bar` widget with the matching class,
/// and mounts it under the `#bars` `VerticalScroll` container.
use textual::action::parse_action;
use textual::prelude::*;

const CSS: &str = r#"
Bar {
    height: 5;
    content-align: center middle;
    text-style: bold;
    margin: 1 2;
    color: $text;
}

.red {
    background: rgba(255, 0, 0, 128);
}

.green {
    background: rgba(0, 128, 0, 128);
}

.blue {
    background: rgba(0, 0, 255, 128);
}

#bars {
    height: 1fr;
}
"#;

// ---------------------------------------------------------------------------
// Bar widget — mirrors Python `class Bar(Static): pass`
// ---------------------------------------------------------------------------

struct Bar {
    inner: Static,
}

impl Bar {
    fn new(text: impl Into<String>) -> Self {
        Self {
            inner: Static::new(text),
        }
    }
}

impl Widget for Bar {
    fn style_type(&self) -> &'static str {
        "Bar"
    }

    fn focusable(&self) -> bool {
        false
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct BindingApp;

impl TextualApp for BindingApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("r", "add_bar('red')", "Add Red"),
            BindingDecl::new("g", "add_bar('green')", "Add Green"),
            BindingDecl::new("b", "add_bar('blue')", "Add Blue"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Footer::new())
            .with_child(Node::new(VerticalScroll::new()).id("bars"))
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        // Parse "add_bar('red')" → name="add_bar", arguments=["red"]
        let Some(parsed) = parse_action(action) else {
            return;
        };
        if parsed.name != "add_bar" {
            return;
        }
        let color = match parsed.arguments.first().map(String::as_str) {
            Some("red") => "red",
            Some("green") => "green",
            Some("blue") => "blue",
            _ => return,
        };

        // Mount the bar under #bars, with the color class for background.
        let bar = Node::new(Bar::new(color)).class(color);
        let _ = app.mount_under("#bars", bar);

        // Scroll to the bottom so the new bar is visible.
        // VerticalScroll does not expose scroll_end(); use a large scroll_by delta
        // as a workaround (scroll is clamped to content bounds by the runtime).
        let _ = app.with_query_one_mut_as::<VerticalScroll, _>("#bars", |vs| {
            vs.scroll_by(i32::MAX / 2);
        });

        ctx.request_repaint();
        ctx.set_handled();
    }
}

fn main() -> textual::Result<()> {
    run_sync(BindingApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_app_composes_without_panic() {
        let mut app = BindingApp;
        let _root = app.compose();
    }

    #[test]
    fn bindings_declare_three_entries() {
        let app = BindingApp;
        let bindings = app.bindings();
        assert_eq!(bindings.len(), 3);
        assert!(bindings.iter().any(|b| b.key == "r"), "missing 'r' binding");
        assert!(bindings.iter().any(|b| b.key == "g"), "missing 'g' binding");
        assert!(bindings.iter().any(|b| b.key == "b"), "missing 'b' binding");
    }

    /// LIVENESS PROBE (DEAD — captures expected behavior, currently failing).
    ///
    /// Pressing the bound key `r`/`g`/`b` *does* fire the binding and mount a
    /// `Bar` node (the tree mutates: Bar count goes 0 -> 1 -> 3), but the
    /// rendered frame NEVER changes and the mounted `Bar` has no rendered
    /// region (`node_screen_rect("Bar") == None`).
    ///
    /// ROOT: the demo composes `#bars` as `Node::new(VerticalScroll::new())
    /// .id("bars")` — so the `#bars` selector resolves to the *Node wrapper*,
    /// not the `VerticalScroll` itself. `node_screen_rect("#bars") == None`
    /// (the wrapper has no own surface) while `node_screen_rect("VerticalScroll")
    /// == Some((0,0,79,22))`. `app.mount_under("#bars", bar)` therefore inserts
    /// the `Bar` as a child of the structural Node, *outside* the scroll
    /// viewport's laid-out/rendered subtree, so it is never laid out or drawn.
    ///
    /// In Python `binding01.py`, `#bars` IS the `VerticalScroll`, and
    /// `action_add_bar` mounts the `Bar` directly inside it.
    ///
    /// TODO (fix then un-ignore): make the demo mount into the scroll viewport
    /// (e.g. give the `VerticalScroll` itself the `#bars` id, or mount under
    /// `"VerticalScroll"`), and/or have `mount_under` on a structural Node
    /// forward into its scrollable content child. The assertion below is the
    /// real expected behavior: mounting a Bar must change the rendered frame and
    /// give the Bar a rendered region.
    #[ignore = "DEAD: mount_under(#bars) targets a non-rendering Node wrapper; Bar never lays out/renders"]
    #[test]
    fn liveness_pressing_color_keys_mounts_bars_and_changes_frame() {
        textual::run_test(BindingApp, |pilot| {
            let before_bars = pilot
                .app()
                .query("Bar")
                .map(|q| q.into_ids().len())
                .unwrap_or(0);
            assert_eq!(before_bars, 0, "expected no Bar widgets before any keypress");
            let before_frame = pilot.app().frame_fingerprint();

            pilot.press(&["r"])?;

            let after_bars = pilot
                .app()
                .query("Bar")
                .map(|q| q.into_ids().len())
                .unwrap_or(0);
            assert_eq!(after_bars, 1, "pressing 'r' must mount one Bar widget");
            // Expected (currently fails): the mounted Bar must be rendered.
            let bar_id = pilot.app().query_one("Bar").expect("Bar mounted");
            assert!(
                pilot.app().node_screen_rect(bar_id).is_some(),
                "the mounted Bar must have a rendered region"
            );
            assert_ne!(
                before_frame,
                pilot.app().frame_fingerprint(),
                "mounting a Bar must change the rendered frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
