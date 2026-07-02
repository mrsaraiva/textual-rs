use rich_rs::Segments;
use textual::compose;
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.";

struct QuitDialogRoot;

impl Widget for QuitDialogRoot {
    fn style_type(&self) -> &'static str {
        "QuitScreen"
    }

    fn compose(&mut self) -> ComposeResult {
        compose![
            Grid::new(2, 2)
                .id("dialog")
                .with_child(Label::new("Are you sure you want to quit?").with_id("question"))
                .with_child(Button::error("Quit").id("quit"))
                .with_child(Button::primary("Cancel").id("cancel"))
        ]
    }

    fn render(&self, _console: &rich_rs::Console, _options: &rich_rs::ConsoleOptions) -> Segments {
        Segments::new()
    }
}

/// Screen with a dialog to quit. Mirrors Python `modal01.py`'s `QuitScreen`:
/// the screen owns its own `on_button_pressed` handler (it is a DOM node, not a
/// compose-only container).
struct QuitScreen;

impl Screen for QuitScreen {
    fn name(&self) -> &str {
        "QuitScreen"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(QuitDialogRoot)
    }

    fn css(&self) -> Option<&str> {
        Some(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/shared/modal01.tcss"
        ))
    }

    fn is_modal(&self) -> bool {
        false
    }

    fn on_button_pressed(
        &mut self,
        pressed: &ButtonPressed,
        _control: NodeId,
        ctx: &mut ScreenMessageCtx,
    ) {
        // Python: if event.button.id == "quit": self.app.exit() else self.app.pop_screen()
        if pressed.button_id.as_deref() == Some("quit") {
            ctx.exit();
        } else {
            ctx.dismiss_none();
        }
    }
}

struct ModalApp;

impl TextualApp for ModalApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("q", "app.push_screen('quit')", "Quit")]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Header::new())
            .with_child(Label::new(TEXT.repeat(8)))
            .with_child(Footer::new())
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.add_mode("quit", || Box::new(QuitScreen));
        Ok(())
    }
}

fn main() -> Result<()> {
    run_sync(ModalApp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn press(screen: &mut QuitScreen, button_id: &str) -> (Option<ScreenResult>, bool) {
        let slot: Mutex<Option<ScreenResult>> = Mutex::new(None);
        let mut ctx = EventCtx::default();
        let mut screen_ctx = ScreenMessageCtx::for_test(&mut ctx, &slot);
        screen.on_button_pressed(
            &ButtonPressed {
                description: button_id.into(),
                button_id: Some(button_id.into()),
            },
            NodeId::default(),
            &mut screen_ctx,
        );
        let staged = slot.lock().unwrap().take();
        (staged, ctx.stop_requested())
    }

    #[test]
    fn modal01_screen_is_non_modal() {
        let screen = QuitScreen;
        assert!(!screen.is_modal());
    }

    #[test]
    fn modal01_registers_quit_mode_and_pushes_screen() {
        let mut definition = ModalApp;
        let mut app = App::new().expect("app should initialize");
        definition
            .configure(&mut app)
            .expect("modal01 configure should succeed");

        assert_eq!(app.screen_count(), 0);
        assert!(app.action_push_screen("quit"));
        assert_eq!(app.screen_count(), 1);
    }

    #[test]
    fn modal01_cancel_button_dismisses_screen_via_handler() {
        let mut screen = QuitScreen;
        let (staged, stop) = press(&mut screen, "cancel");
        assert!(matches!(staged, Some(ScreenResult::Dismissed)));
        assert!(!stop);
    }

    #[test]
    fn modal01_quit_button_requests_exit() {
        let mut screen = QuitScreen;
        let (staged, stop) = press(&mut screen, "quit");
        assert!(stop, "quit should request app stop");
        assert!(staged.is_none(), "quit exits rather than dismissing");
    }

    /// LIVENESS probe (Pilot, headless): pressing the bound `q` key pushes the
    /// QuitScreen and changes the rendered frame; clicking `#cancel` inside the
    /// pushed screen dismisses it and changes the frame back. Guards the full
    /// push-screen / dismiss interaction loop end-to-end through the real
    /// runtime, not just the unit-level handler.
    #[test]
    fn modal01_push_and_dismiss_is_live() {
        run_test(ModalApp, |pilot| {
            assert_eq!(pilot.app().screen_count(), 0, "no modal at startup");
            let before = pilot.app().frame_fingerprint();

            // Press `q` (bound to app.push_screen('quit')).
            pilot.press(&["q"])?;
            assert_eq!(pilot.app().screen_count(), 1, "q must push the QuitScreen");
            let pushed = pilot.app().frame_fingerprint();
            assert_ne!(before, pushed, "pushing the modal must change the frame");

            // Click Cancel inside the modal -> dismiss back to the base screen.
            pilot.click("#cancel")?;
            assert_eq!(pilot.app().screen_count(), 0, "cancel must dismiss the screen");
            let dismissed = pilot.app().frame_fingerprint();
            assert_ne!(pushed, dismissed, "dismissing the modal must change the frame");
            Ok(())
        })
        .expect("modal01 push/dismiss harness should run");
    }
}
