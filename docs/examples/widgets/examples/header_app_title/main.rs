/// Port of Python Textual `docs/examples/widgets/header_app_title.py`.
///
/// Demonstrates the `Header` widget with dynamic title and sub-title:
/// - Composes a single `Header` widget.
/// - On mount, sets `app.title` to "Header Application" and
///   `app.sub_title` to "With title and sub-title".
use textual::prelude::*;

struct HeaderApp;

impl TextualApp for HeaderApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Header::new())
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut textual::event::WidgetCtx) {
        app.set_title("Header Application");
        app.set_sub_title("With title and sub-title");
        ctx.request_repaint();
    }
}

fn main() -> textual::Result<()> {
    run_sync(HeaderApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_app_composes_without_panic() {
        let mut app = HeaderApp;
        let root = app.compose();
        assert!(!root.children().is_empty());
    }
}
