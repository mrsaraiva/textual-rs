/// Port of Python Textual `docs/examples/widgets/progress_bar_isolated_.py`.
///
/// Demonstrates `ProgressBar` in both indeterminate and determinate modes:
/// - Initially renders an indeterminate (animated) progress bar centered on screen.
/// - Pressing `s` sets total=100 and starts advancing the bar by 1 step per tick.
/// - Keys `f`, `t`, `u` are test helpers for freezing to known states.
///
/// Layout: Center > Middle > ProgressBar, Footer at bottom.
use textual::prelude::*;

struct IndeterminateProgressBar {
    /// Whether the progress timer is running.
    started: bool,
}

impl IndeterminateProgressBar {
    fn new() -> Self {
        Self { started: false }
    }
}

impl TextualApp for IndeterminateProgressBar {
    fn title(&self) -> &'static str {
        "IndeterminateProgressBar"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("s", "start", "Start")]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                CenterMiddle::new().with_compose(vec![
                    ChildDecl::from(ProgressBar::new(None)).with_id("progress_bar"),
                ]),
            )
            .with_child(Footer::new())
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        if self.started {
            let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress_bar", |bar| {
                bar.advance(1.0);
            });
            ctx.request_repaint();
        }
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        if action == "start" {
            let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress_bar", |bar| {
                bar.update(Some(Some(100.0)), Some(0.0), None);
            });
            self.started = true;
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        match key.key.as_str() {
            "f" => {
                // Freeze time for indeterminate progress bar (Python: clock.set_time(5))
                // In Rust there is no MockClock; just request a repaint.
                ctx.request_repaint();
                ctx.set_handled();
            }
            "t" => {
                // Freeze to show a known ETA (Python: clock.set_time(0), update, clock.set_time(3.9), update(progress=39))
                let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress_bar", |bar| {
                    bar.update(Some(Some(100.0)), Some(39.0), None);
                });
                self.started = false;
                ctx.request_repaint();
                ctx.set_handled();
            }
            "u" => {
                // Show completed state (Python: update(total=100, progress=100))
                let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress_bar", |bar| {
                    bar.update(Some(Some(100.0)), Some(100.0), None);
                });
                self.started = false;
                ctx.request_repaint();
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(IndeterminateProgressBar::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_composes_without_panic() {
        let mut app = IndeterminateProgressBar::new();
        let _root = app.compose();
    }

    #[test]
    fn initial_state_not_started() {
        let app = IndeterminateProgressBar::new();
        assert!(!app.started);
    }
}
