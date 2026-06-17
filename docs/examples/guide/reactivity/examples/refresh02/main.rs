/// Port of Python Textual `docs/examples/guide/reactivity/refresh02.py`.
///
/// Demonstrates reactive layout invalidation:
/// - A custom `Name` widget renders "Hello, {who}!" where `who` defaults to "name".
/// - `Input` at the top lets the user type a name.
/// - On every keystroke `on_input_changed` updates the `Name` widget's `who` field,
///   which in Python triggers a full layout re-computation via `reactive(layout=True)`.
///
/// Python's `reactive("name", layout=True)` has no direct Rust-side equivalent
/// as a struct-field decorator, so the field is stored as a plain `String` and
/// the `on_message_with_app` hook intercepts `InputChanged` to update the field
/// and call `ctx.request_layout_invalidation()` — the faithful equivalent of
/// `layout=True`: it merges `InvalidationFlags::layout()` so the auto-sized
/// bordered `Name` box resizes when `who` changes length.
///
/// ERGONOMICS GAP (not a layout-invalidation gap): Python's `on_input_changed`
/// handler can mutate sibling widgets directly because the App owns the DOM. In
/// textual-rs the typed `on_input_changed` hook does not receive `&mut App`, so
/// cross-widget mutation must go through `on_message_with_app` +
/// `app.with_query_one_mut_as`. This is a convenience-API gap only; the
/// underlying layout invalidation is fully available via `EventCtx`.
use textual::prelude::*;

const CSS: &str = r#"
Input {
    dock: top;
    margin-top: 1;
}

Name {
    width: auto;
    height: auto;
    border: heavy $secondary;
}
"#;

// ---------------------------------------------------------------------------
// Name widget
// ---------------------------------------------------------------------------

struct NameWidget {
    who: String,
}

impl NameWidget {
    fn new() -> Self {
        Self {
            who: "name".to_string(),
        }
    }

    fn set_who(&mut self, who: impl Into<String>) {
        self.who = who.into();
    }
}

impl Widget for NameWidget {
    fn style_type(&self) -> &'static str {
        "Name"
    }

    fn focusable(&self) -> bool {
        false
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        Static::new(format!("Hello, {}!", self.who)).render(console, options)
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
            .with_child(NameWidget::new())
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Some(m) = message.downcast_ref::<InputChanged>() {
            let new_who = m.value.clone();
            let _ = app.with_query_one_mut_as::<NameWidget, _>("Name", |name| {
                name.set_who(new_who);
            });
            ctx.request_layout_invalidation();
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
    fn name_widget_defaults_to_name() {
        let w = NameWidget::new();
        assert_eq!(w.who, "name");
    }

    #[test]
    fn name_widget_set_who() {
        let mut w = NameWidget::new();
        w.set_who("Alice");
        assert_eq!(w.who, "Alice");
    }
}
