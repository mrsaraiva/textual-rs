use textual::compose;
use textual::prelude::*;

#[derive(Default)]
struct ModalApp {
    initialized: bool,
    overlay_open: bool,
}

impl ModalApp {
    fn set_overlay_visible(&self, app: &mut App, visible: bool, ctx: &mut EventCtx) {
        let Ok(overlay) = app.query_one("Overlay") else {
            return;
        };
        if visible {
            ctx.show_overlay(overlay);
        } else {
            ctx.hide_overlay(overlay);
        }
        ctx.request_repaint();
        ctx.set_handled();
    }
}

impl TextualApp for ModalApp {
    fn compose(&mut self) -> AppRoot {
        let base = Node::new(Vertical::new().with_compose(compose![
            Static::new("Modal Overlay Debug Harness").class("title"),
            Static::new("Click 'Open modal' to show the modal layer."),
            Static::new("Click 'Close modal' (inside modal) or press Escape to dismiss."),
            Button::primary("Open modal"),
            Static::new("Background content should remain visible behind the modal.").class("hint"),
        ]))
        .class("base");

        let modal = Node::new(
            Container::new().with_child(
                Node::new(Vertical::new().with_compose(compose![
                    Static::new("Modal Title").class("modal-title"),
                    Static::new("This is a standalone overlay verification example."),
                    Static::new("If overlay composition is correct, base UI remains underneath."),
                    Button::error("Close modal"),
                ]))
                .class("modal-card"),
            ),
        )
        .class("modal-layer");

        AppRoot::new().with_compose(compose![
            Overlay::new(base, modal).visible(false),
            Footer::new(),
        ])
    }

    fn css_path(&self) -> Option<&'static str> {
        Some(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/modal.tcss"))
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        // Ensure initial state is hidden in tree mode.
        self.set_overlay_visible(app, false, ctx);
        self.overlay_open = false;
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Message::ButtonPressed(ButtonPressed { description }) = &message.message {
            if description.contains("variant='primary'") {
                self.overlay_open = true;
                self.set_overlay_visible(app, true, ctx);
            } else if description.contains("variant='error'") {
                self.overlay_open = false;
                self.set_overlay_visible(app, false, ctx);
            }
        }
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(ModalApp::default())
}
