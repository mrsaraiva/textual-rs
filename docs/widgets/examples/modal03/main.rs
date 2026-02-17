use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use rich_rs::Segments;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use textual::compose;
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.";

struct QuitDialogRoot {
    decision: Arc<Mutex<Option<bool>>>,
}

impl QuitDialogRoot {
    fn new(decision: Arc<Mutex<Option<bool>>>) -> Self {
        Self { decision }
    }
}

impl Widget for QuitDialogRoot {
    fn style_type(&self) -> &'static str {
        "QuitScreen"
    }

    fn compose(&self) -> ComposeResult {
        compose![Node::new(
            Grid::new(2, 2)
                .with_cell(
                    0,
                    0,
                    Node::new(Label::new("Are you sure you want to quit?")).id("question"),
                )
                .with_cell(1, 0, Node::new(Button::error("Quit")).id("quit"))
                .with_cell(1, 1, Node::new(Button::primary("Cancel")).id("cancel")),
        )
        .id("dialog")]
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Message::ButtonPressed(ButtonPressed { description }) = &message.message {
            let confirmed = description.contains("variant='error'");
            if let Ok(mut decision) = self.decision.lock() {
                *decision = Some(confirmed);
            }
            ctx.post_message(Message::AppPopScreen(AppPopScreen));
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn render(&self, _console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();
        for row in 0..height {
            out.push(rich_rs::Segment::new(" ".repeat(width)));
            if row + 1 < height {
                out.push(rich_rs::Segment::line());
            }
        }
        out
    }
}

struct QuitScreen {
    decision: Arc<Mutex<Option<bool>>>,
}

impl QuitScreen {
    fn new(decision: Arc<Mutex<Option<bool>>>) -> Self {
        Self { decision }
    }
}

impl Screen for QuitScreen {
    fn name(&self) -> &str {
        "QuitScreen"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(QuitDialogRoot::new(self.decision.clone()))
    }

    fn css(&self) -> Option<&str> {
        Some(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/shared/modal01.tcss"
        ))
    }
}

#[derive(Default)]
struct Modal03App {
    callback_exit_requested: Arc<AtomicBool>,
}

impl TextualApp for Modal03App {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("q", "request_quit", "Quit")]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Header::new().title("ModalApp"))
            .with_child(Label::new(TEXT.repeat(8)))
            .with_child(Footer::new())
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        if key.code != KeyCode::Char('q')
            || key.modifiers != KeyModifiers::NONE
            || !matches!(key.kind, KeyEventKind::Press)
        {
            return;
        }

        if app.screen_count() > 0 {
            ctx.set_handled();
            return;
        }

        let decision = Arc::new(Mutex::new(None));
        let callback_decision = decision.clone();
        let callback_exit_requested = self.callback_exit_requested.clone();

        app.push_screen_with_callback(
            Box::new(QuitScreen::new(decision)),
            Box::new(move |_result| {
                let should_exit = callback_decision.lock().map(|d| *d).ok().flatten();
                if should_exit.unwrap_or(false) {
                    callback_exit_requested.store(true, Ordering::SeqCst);
                }
            }),
        );
        ctx.request_repaint();
        ctx.set_handled();
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        if self.callback_exit_requested.swap(false, Ordering::SeqCst) {
            app.stop();
            ctx.request_repaint();
            ctx.set_handled();
        }
    }
}

fn main() -> Result<()> {
    run_sync(Modal03App::default())
}
