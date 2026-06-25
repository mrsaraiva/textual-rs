/// Port of Python Textual `docs/examples/widgets/link.py`.
///
/// Demonstrates the `Link` widget centered on screen.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}
"#;

struct LinkApp;

impl TextualApp for LinkApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Link::new("Go to textualize.io")
                .with_url("https://textualize.io")
                .with_tooltip("Click me"),
        )
    }
}

fn main() -> textual::Result<()> {
    run_sync(LinkApp)
}

#[cfg(test)]
mod liveness {
    use super::*;
    use textual::run_test;

    /// UNCLEAR (headless): the Link's representative interaction is clicking it,
    /// which opens an external URL (`https://textualize.io`) via the OS browser
    /// (the `open` crate). That side effect is not observable in a headless
    /// frame and must not actually launch a browser in CI, so we cannot assert a
    /// frame change for the real interaction.
    ///
    /// We keep a smoke probe that the app composes and renders the Link without
    /// panicking under the headless harness (the click action itself is left
    /// unexercised). Promote to a real liveness assertion only if the Link grows
    /// an observable in-frame state (e.g. a focus/hover style we can drive).
    #[test]
    fn link_renders_under_harness() {
        run_test(LinkApp, |pilot| {
            let fp = pilot.app().frame_fingerprint();
            assert_ne!(fp, 0, "the Link app must render a non-empty frame");
            Ok(())
        })
        .unwrap();
    }
}
