/// Port of Python Textual `docs/examples/widgets/header.py`.
///
/// Demonstrates the `Header` widget: a simple app that renders only a Header.
use textual::prelude::*;

struct HeaderApp;

impl TextualApp for HeaderApp {
    fn title(&self) -> &'static str {
        "HeaderApp"
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Header::new())
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
        let _root = app.compose();
    }
}
