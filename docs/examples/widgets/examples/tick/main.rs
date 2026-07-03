use textual::prelude::*;

/// Minimal tick + input latency harness.
///
/// Use this to validate event-loop responsiveness while the app is receiving
/// frequent tick updates and keyboard input at the same time.
#[derive(Clone, Default)]
struct TickApp {
    tick_count: u64,
}

impl TextualApp for TickApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Container::new()
                .with_child(Label::new("Ticks: 0").with_shrink(false).wrap(false))
                .with_child(
                    Input::new().with_placeholder("Type here while ticks keep updating..."),
                ),
        )
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut textual::event::WidgetCtx) {
        self.tick_count = self.tick_count.saturating_add(1);
        let text = format!("Ticks: {}", self.tick_count);
        let _ = app.with_query_one_mut_as::<Label, _>("Label", |label| {
            label.set_text(text);
        });
        ctx.request_repaint();
    }
}

fn main() -> Result<()> {
    run_sync(TickApp::default())
}
