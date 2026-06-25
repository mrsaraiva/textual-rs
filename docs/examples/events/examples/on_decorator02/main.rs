/// Port of Python Textual `docs/examples/events/on_decorator02.py`.
///
/// Demonstrates the `@on` decorator pattern (selector-based per-button handlers).
/// Python uses three separate handlers, one per button:
///   @on(Button.Pressed, "#bell")   def play_bell(...)
///   @on(Button.Pressed, ".toggle.dark") def toggle_dark(...)
///   @on(Button.Pressed, "#quit")   def quit(...)
///
/// Rust mirrors this with [`MessageRouter`] — the declarative `@on(Message,
/// selector)` analogue. Each handler registers against `ButtonPressed` with a CSS
/// selector that is matched against the message's control (`Button.id`), exactly
/// as Python matches `event.button`. The `.toggle.dark` button is given
/// `id="toggle-dark"` here since the Rust `ButtonPressed` carries an id rather
/// than class names.
///
/// CSS is ported from `on_decorator.tcss`.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
    layout: horizontal;
}

Button {
    margin: 2 4;
}
"#;

struct OnDecoratorApp {
    /// Declarative `@on(Button.Pressed, selector)` routing table.
    router: MessageRouter<OnDecoratorApp>,
}

impl OnDecoratorApp {
    fn new() -> Self {
        let mut router: MessageRouter<OnDecoratorApp> = MessageRouter::new();
        // @on(Button.Pressed, "#bell") -> play_bell
        router.on::<ButtonPressed, _>("#bell", |_app, _msg, ctx| {
            // play_bell: bell() is a no-op in Rust.
            ctx.set_handled();
        });
        // @on(Button.Pressed, "#toggle-dark") -> toggle_dark
        // (Python: ".toggle.dark"; ButtonPressed carries an id in Rust.)
        router.on::<ButtonPressed, _>("#toggle-dark", |_app, _msg, ctx| {
            ctx.run_action("app.toggle_dark");
            ctx.request_repaint();
            ctx.set_handled();
        });
        // @on(Button.Pressed, "#quit") -> quit
        router.on::<ButtonPressed, _>("#quit", |_app, _msg, ctx| {
            ctx.request_stop();
            ctx.set_handled();
        });
        Self { router }
    }
}

impl TextualApp for OnDecoratorApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Button::new("Bell").id("bell"))
            .with_child(Button::new("Toggle dark").id("toggle-dark"))
            .with_child(Button::new("Quit").id("quit"))
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        // Route through the declarative table, exactly like Python's `@on`
        // dispatch in `_get_dispatch_methods`.
        let mut router = std::mem::take(&mut self.router);
        router.dispatch(self, message, ctx);
        self.router = router;
    }
}

fn main() -> textual::Result<()> {
    run_sync(OnDecoratorApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use textual::node_id::node_id_from_ffi;

    fn press(id: &str) -> MessageEvent {
        MessageEvent::new(
            node_id_from_ffi(1),
            ButtonPressed {
                description: id.into(),
                button_id: Some(id.into()),
            },
        )
    }

    #[test]
    fn on_decorator02_app_composes_without_panic() {
        let mut app = OnDecoratorApp::new();
        let _root = app.compose();
    }

    #[test]
    fn bell_button_routes_to_play_bell_only() {
        let mut app = OnDecoratorApp::new();
        let mut ctx = EventCtx::default();
        ctx.set_node_id(node_id_from_ffi(1));
        app.on_message(&press("bell"), &mut ctx);
        // The bell handler marks the event handled; quit handler must not fire.
        assert!(ctx.handled());
        assert!(!ctx.stop_requested());
    }

    #[test]
    fn quit_button_routes_to_quit_handler() {
        let mut app = OnDecoratorApp::new();
        let mut ctx = EventCtx::default();
        ctx.set_node_id(node_id_from_ffi(1));
        app.on_message(&press("quit"), &mut ctx);
        assert!(ctx.stop_requested());
    }

    #[test]
    fn unmatched_button_routes_nowhere() {
        let mut app = OnDecoratorApp::new();
        let mut ctx = EventCtx::default();
        ctx.set_node_id(node_id_from_ffi(1));
        app.on_message(&press("other"), &mut ctx);
        assert!(!ctx.handled());
        assert!(!ctx.stop_requested());
    }

    /// LIVENESS probe (Pilot, headless): the declarative `MessageRouter` (the
    /// `@on(Button.Pressed, selector)` analogue) routes per-button. Clicking
    /// "Quit" routes through the router to the quit handler and requests app
    /// stop — the robust, observable proof the click → message → router path
    /// works. (The Toggle-dark branch is covered, `#[ignore]`d, below.)
    #[test]
    fn on_decorator02_router_quit_is_live() {
        run_test(OnDecoratorApp::new(), |pilot| {
            assert!(!pilot.app().headless_stop_requested(), "no stop before Quit");
            pilot.click("#quit")?;
            assert!(
                pilot.app().headless_stop_requested(),
                "routing Quit must request app exit"
            );
            Ok(())
        })
        .expect("on_decorator02 router quit harness should run");
    }

    /// LIVENESS probe (Pilot, headless): clicking "Toggle dark" routes to the
    /// toggle handler (`app.toggle_dark`), which should recolor the UI.
    ///
    /// Now LIVE via the public dark-mode accessor (same situation as
    /// on_decorator01: default-styled buttons on a blank screen produce no
    /// per-cell color change, so a frame-fingerprint probe was inconclusive).
    /// `App::is_dark()` exposes the flag, so the routed toggle's state flip is
    /// directly assertable.
    #[test]
    fn on_decorator02_router_toggle_dark_is_live() {
        run_test(OnDecoratorApp::new(), |pilot| {
            let before = pilot.app().is_dark();
            pilot.click("#toggle-dark")?;
            assert_ne!(
                before,
                pilot.app().is_dark(),
                "routing Toggle dark must flip the app's dark-mode state"
            );
            Ok(())
        })
        .expect("on_decorator02 router toggle-dark harness should run");
    }
}
