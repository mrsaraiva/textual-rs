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
}
