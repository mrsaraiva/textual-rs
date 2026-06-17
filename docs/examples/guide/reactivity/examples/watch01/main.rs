/// Port of Python Textual `docs/examples/guide/reactivity/watch01.py`.
///
/// Demonstrates reactive watchers: when `color` changes, `watch_color` is
/// called with the old and new value, updating background colors on two
/// `Static` panels.
///
/// Python uses `reactive(Color.parse("transparent"))` and `watch_color(old, new)`
/// to set `self.query_one("#old").styles.background` and `#new`. In Rust there
/// is no `reactive` field + automatic watcher dispatch on App-level structs, so
/// we track `current_color` manually and update both panels on each `InputSubmitted`.
///
/// Framework gap: Python `reactive` fields with automatic `watch_*` callbacks on
/// `App` subclasses are not yet expressible in Rust textual-rs. The watcher is
/// implemented manually inside `on_message_with_app`.
use textual::prelude::*;
use textual::style::parse_color_like;

const CSS: &str = r#"
Input {
    dock: top;
    margin-top: 1;
}

#colors {
    grid-size: 2 1;
    grid-gutter: 2 4;
    grid-columns: 1fr;
    margin: 0 1;
}

#old {
    height: 100%;
    border: wide $secondary;
}

#new {
    height: 100%;
    border: wide $secondary;
}
"#;

struct WatchApp {
    /// Tracks the current color (mirrors Python's `reactive` field `color`).
    current_color: Option<Color>,
}

impl WatchApp {
    fn new() -> Self {
        Self {
            current_color: None,
        }
    }
}

impl TextualApp for WatchApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Input::new().with_placeholder("Enter a color"))
            .with_child(
                Grid::new(1, 2)
                    .id("colors")
                    .with_child(Static::new("").id("old"))
                    .with_child(Static::new("").id("new")),
            )
    }

    /// Handles `InputSubmitted`: parse the entered color, then invoke the
    /// watcher logic (mirrors Python's `watch_color(old_color, new_color)`).
    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Some(m) = message.downcast_ref::<InputSubmitted>() {
            let value = m.value.trim().to_string();
            if let Some(new_color) = parse_color_like(&value) {
                let old_color = self.current_color;

                // watch_color(old_color, new_color) — set backgrounds
                if let Some(old) = old_color {
                    let style = Style::new().bg(old);
                    let _ = app.with_query_one_mut_as::<Static, _>("#old", |s| {
                        s.set_inline_style(style);
                    });
                }
                let new_style = Style::new().bg(new_color);
                let _ = app.with_query_one_mut_as::<Static, _>("#new", |s| {
                    s.set_inline_style(new_style);
                });

                self.current_color = Some(new_color);

                // Clear the input
                let _ = app.with_query_one_mut_as::<Input, _>("Input", |input| {
                    input.set_text(String::new());
                });
                ctx.request_repaint();
            }
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(WatchApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_app_composes_without_panic() {
        let mut app = WatchApp::new();
        let _root = app.compose();
    }

    #[test]
    fn initial_color_is_none() {
        let app = WatchApp::new();
        assert!(app.current_color.is_none());
    }
}
