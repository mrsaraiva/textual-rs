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

/// Modal screen with a dialog to quit. Mirrors Python `modal02.py`'s
/// `QuitScreen(ModalScreen)`: same handler as modal01 but the screen is modal
/// (default `is_modal()`), so the screen below is dimmed rather than replaced.
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
    use textual::event::EventCtx;

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
    fn modal02_screen_is_modal_by_default() {
        let screen = QuitScreen;
        assert!(screen.is_modal());
    }

    #[test]
    fn modal02_registers_quit_mode_and_pushes_screen() {
        let mut definition = ModalApp;
        let mut app = App::new().expect("app should initialize");
        definition
            .configure(&mut app)
            .expect("modal02 configure should succeed");

        assert_eq!(app.screen_count(), 0);
        assert!(app.action_push_screen("quit"));
        assert_eq!(app.screen_count(), 1);
    }

    #[test]
    fn modal02_cancel_button_dismisses_screen_via_handler() {
        let mut screen = QuitScreen;
        let (staged, stop) = press(&mut screen, "cancel");
        assert!(matches!(staged, Some(ScreenResult::Dismissed)));
        assert!(!stop);
    }

    #[test]
    fn modal02_quit_button_requests_exit() {
        let mut screen = QuitScreen;
        let (staged, stop) = press(&mut screen, "quit");
        assert!(stop, "quit should request app stop");
        assert!(staged.is_none());
    }

    /// LIVENESS probe (Pilot, headless): pressing `q` pushes the modal
    /// QuitScreen (dimming the base screen rather than replacing it) and changes
    /// the frame; clicking `#cancel` dismisses it and changes the frame back.
    #[test]
    fn modal02_push_and_dismiss_is_live() {
        run_test(ModalApp, |pilot| {
            assert_eq!(pilot.app().screen_count(), 0);
            let before = pilot.app().frame_fingerprint();

            pilot.press(&["q"])?;
            assert_eq!(pilot.app().screen_count(), 1, "q must push the modal QuitScreen");
            let pushed = pilot.app().frame_fingerprint();
            assert_ne!(before, pushed, "pushing the modal must change the frame");

            pilot.click("#cancel")?;
            assert_eq!(pilot.app().screen_count(), 0, "cancel must dismiss the modal");
            let dismissed = pilot.app().frame_fingerprint();
            assert_ne!(pushed, dismissed, "dismissing the modal must change the frame");
            Ok(())
        })
        .expect("modal02 push/dismiss harness should run");
    }
}
