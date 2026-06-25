/// Port of Python Textual `docs/examples/events/on_decorator01.py`.
///
/// Demonstrates handling button presses in a single `on_button_pressed` handler
/// that branches on `button.id`. Three buttons:
/// - "Bell"        → rings the bell (no-op in Rust)
/// - "Toggle dark" → toggles dark/light theme
/// - "Quit"        → exits the app
///
/// CSS is ported from `on_decorator.tcss`.
///
/// Python uses `classes="toggle dark"` to identify the toggle button, but the
/// Rust `ButtonPressed` message carries only `button_id`, so we assign
/// `id="toggle-dark"` to that button instead.
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

struct OnDecoratorApp;

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

    /// Single handler that branches on `button.id` — the non-`@on` form, exactly
    /// like Python's `on_button_pressed`. (The selector-routed `@on` variant is
    /// `on_decorator02`.)
    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            match bp.button_id.as_deref() {
                Some("bell") => {
                    // bell() — no-op in Rust
                    ctx.set_handled();
                }
                Some("toggle-dark") => {
                    // self.theme = ... — route through the action subsystem.
                    ctx.run_action("app.toggle_dark");
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                Some("quit") => {
                    ctx.request_stop();
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(OnDecoratorApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_decorator_app_composes_without_panic() {
        let mut app = OnDecoratorApp;
        let _root = app.compose();
    }

    fn press(id: &str) -> MessageEvent {
        MessageEvent::new(
            textual::node_id::node_id_from_ffi(1),
            ButtonPressed {
                description: id.into(),
                button_id: Some(id.into()),
            },
        )
    }

    #[test]
    fn quit_button_requests_stop() {
        let mut app = OnDecoratorApp;
        let mut ctx = EventCtx::default();
        app.on_message(&press("quit"), &mut ctx);
        assert!(ctx.stop_requested());
    }

    #[test]
    fn bell_button_handled_without_stop() {
        let mut app = OnDecoratorApp;
        let mut ctx = EventCtx::default();
        app.on_message(&press("bell"), &mut ctx);
        assert!(ctx.handled());
        assert!(!ctx.stop_requested());
    }

    /// LIVENESS probe (Pilot, headless): the single `on_message` handler branches
    /// on `button.id`. Clicking "Quit" requests app stop — the robust, observable
    /// liveness signal that the click → message → handler path works.
    /// (See `on_decorator01_toggle_dark_recolors_is_live` for the Toggle-dark
    /// branch, which is `#[ignore]`d because the toggle is not observable here.)
    #[test]
    fn on_decorator01_quit_button_is_live() {
        run_test(OnDecoratorApp, |pilot| {
            assert!(!pilot.app().headless_stop_requested(), "no stop before Quit");
            pilot.click("#quit")?;
            assert!(
                pilot.app().headless_stop_requested(),
                "clicking Quit must request app exit"
            );
            Ok(())
        })
        .expect("on_decorator01 quit harness should run");
    }

    /// LIVENESS probe (Pilot, headless): clicking "Toggle dark" runs
    /// `app.toggle_dark`, which should recolor the UI so the rendered frame
    /// changes.
    ///
    /// Now LIVE via the public dark-mode accessor. With this demo's
    /// default-styled buttons on an otherwise blank screen, `app.toggle_dark`
    /// produces no per-cell color change in the rendered `FrameBuffer`, so a
    /// frame-fingerprint probe was inconclusive. `App::is_dark()` exposes the
    /// dark-mode flag, so the toggle's state flip is directly assertable.
    #[test]
    fn on_decorator01_toggle_dark_recolors_is_live() {
        run_test(OnDecoratorApp, |pilot| {
            let before = pilot.app().is_dark();
            pilot.click("#toggle-dark")?;
            assert_ne!(
                before,
                pilot.app().is_dark(),
                "clicking Toggle dark must flip the app's dark-mode state"
            );
            Ok(())
        })
        .expect("on_decorator01 toggle-dark harness should run");
    }
}
