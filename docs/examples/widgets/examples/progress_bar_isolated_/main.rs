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

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut textual::event::WidgetCtx) {
        if self.started {
            let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress_bar", |bar| {
                bar.advance(1.0);
            });
            ctx.request_repaint();
        }
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut textual::event::WidgetCtx) {
        if action == "start" {
            let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress_bar", |bar| {
                bar.update(Some(Some(100.0)), Some(0.0), None);
            });
            self.started = true;
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut textual::event::WidgetCtx) {
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

    /// LIVENESS: pressing `s` routes the `start` action, which flips the bar to
    /// determinate (`total=100`, progress reset to 0) and arms `started`. That
    /// state transition is the demo's observable response to the binding.
    ///
    /// NOTE on the advance loop: this port drives `make_progress` from
    /// `on_tick_with_app` (the per-frame app tick), NOT a `set_interval` timer.
    /// The headless Pilot pumps timers + messages but does not synthesise the
    /// wall-clock app tick, so `advance_clock` does not advance the bar here —
    /// see the sibling `progress_bar_isolated` port (set_interval based), whose
    /// probe DOES advance under `advance_clock`. The bar-fill loop of THIS port
    /// is therefore only exercised live, not headless; we assert the `s` action
    /// liveness (the routable, headless-observable part).
    #[test]
    fn liveness_start_action_makes_bar_determinate() {
        IndeterminateProgressBar::new()
            .run_test(|pilot| {
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["s"])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(
                    before, after,
                    "pressing `s` (start) must change the rendered frame"
                );
                let app = pilot.app();
                let total = app
                    .query_one_typed::<ProgressBar>("#progress_bar")
                    .ok()
                    .and_then(|h| h.read(app, |b| b.total()).ok())
                    .flatten();
                assert_eq!(total, Some(100.0), "`s` sets total=100");
                Ok(())
            })
            .expect("run_test");
    }
}
