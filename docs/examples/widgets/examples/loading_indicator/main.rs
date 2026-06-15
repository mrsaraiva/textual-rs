/// Port of Python Textual `docs/examples/widgets/loading_indicator.py`.
///
/// Demonstrates the `LoadingIndicator` widget:
/// - A single LoadingIndicator filling the entire screen
use textual::prelude::*;

struct LoadingApp;

impl TextualApp for LoadingApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(LoadingIndicator::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(LoadingApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_app_composes_without_panic() {
        let mut app = LoadingApp;
        let _root = app.compose();
    }

    #[test]
    fn compose_produces_loading_indicator() {
        let mut app = LoadingApp;
        let root = app.compose();
        assert!(!root.children().is_empty());
    }
}
