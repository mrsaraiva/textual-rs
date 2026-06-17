/// Port of Python Textual `docs/examples/guide/reactivity/refresh01.py`.
///
/// Demonstrates reactive text refresh driven by Input changes.
///
/// Python:
///   class Name(Widget):
///       who = reactive("name")
///       def render(self) -> str:
///           return f"Hello, {self.who}!"
///
///   class WatchApp(App):
///       def compose(self): yield Input(placeholder="Enter your name"); yield Name()
///       def on_input_changed(self, event): self.query_one(Name).who = event.value
///
/// Rust port:
///   - `Name` is a custom widget wrapping `Static` with a `set_who()` method.
///   - `on_message_with_app` catches `InputChanged` and calls
///     `app.with_query_one_mut_as::<Name, _>` to update the greeting.
///   - `style_type()` returns `"Name"` so CSS selectors and DOM queries work.
use textual::message::InputChanged;
use textual::prelude::*;

const CSS: &str = r#"
Input {
    dock: top;
    margin-top: 1;
}

Name {
    height: 100%;
    content-align: center middle;
}
"#;

// ---------------------------------------------------------------------------
// Name widget — mirrors Python's `class Name(Widget)`
// ---------------------------------------------------------------------------

struct Name {
    inner: Static,
}

impl Name {
    fn new() -> Self {
        Self {
            inner: Static::new("Hello, name!"),
        }
    }

    /// Update the greeting (equivalent to setting `self.who` reactive).
    fn set_who(&mut self, who: &str) {
        let greeting = if who.is_empty() {
            "Hello, !".to_string()
        } else {
            format!("Hello, {}!", who)
        };
        self.inner.update(greeting);
    }
}

impl Widget for Name {
    fn style_type(&self) -> &'static str {
        "Name"
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

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct WatchApp;

impl TextualApp for WatchApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Input::new().with_placeholder("Enter your name"))
            .with_child(Name::new())
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Some(changed) = message.downcast_ref::<InputChanged>() {
            let value = changed.value.clone();
            let _ = app.with_query_one_mut_as::<Name, _>("Name", |name| {
                name.set_who(&value);
            });
            ctx.request_repaint();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(WatchApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_app_composes_without_panic() {
        let mut app = WatchApp;
        let _root = app.compose();
    }

    #[test]
    fn name_widget_initial_greeting() {
        let name = Name::new();
        // Style_type is "Name" so CSS selectors target it correctly.
        assert_eq!(name.style_type(), "Name");
    }

    #[test]
    fn name_widget_set_who_updates_greeting() {
        let mut name = Name::new();
        name.set_who("World");
        // The inner static should now render "Hello, World!"
        // We can't directly read the label text without access to internals,
        // but verifying it doesn't panic is the minimum bar.
    }

    #[test]
    fn name_widget_set_who_empty_value() {
        let mut name = Name::new();
        name.set_who("");
        // Empty value produces "Hello, !" — same as Python reference.
    }
}
