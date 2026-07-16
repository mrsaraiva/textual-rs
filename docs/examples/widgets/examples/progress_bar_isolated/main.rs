/// Port of Python Textual `docs/examples/widgets/progress_bar_isolated.py`.
///
/// Demonstrates `ProgressBar` in indeterminate and determinate modes:
/// - Initially renders an indeterminate (animated) progress bar centered on screen.
/// - Pressing `s` sets total=100 and starts advancing the bar by 1 step per timer
///   tick.
///
/// Python:
///   def on_mount(self):
///       self.progress_timer = self.set_interval(1 / 10, self.make_progress, pause=True)
///   def make_progress(self): self.query_one(ProgressBar).advance(1)
///   def action_start(self):
///       self.query_one(ProgressBar).update(total=100)
///       self.progress_timer.resume()
///
/// Rust faithful mapping: register a PAUSED `set_interval(1/10)` at mount whose
/// callback advances the bar by 1; the `start` action sets total=100 and
/// `resume()`s that timer. No per-frame `on_tick` push.
///
/// Layout: Center > Middle > ProgressBar, Footer at bottom.
use std::time::Duration;
use textual::prelude::*;

#[derive(Default)]
struct IndeterminateProgressBar {
    /// Paused progress timer, resumed by the `start` action (Python `progress_timer`).
    progress_timer: Option<TimerHandle>,
}

impl IndeterminateProgressBar {
    fn new() -> Self {
        Self::default()
    }
}

impl TextualApp for IndeterminateProgressBar {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("s", "start", "Start")]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Center::new().with_child(
                    Middle::new().with_compose(vec![
                        ChildDecl::from(ProgressBar::new(None)).with_id("progress_bar"),
                    ]),
                ),
            )
            .with_child(Footer::new())
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut textual::event::WidgetCtx) {
        // Python on_mount: set_interval(1 / 10, self.make_progress, pause=True).
        // Each fire advances the bar by 1 (Python `make_progress`).
        self.progress_timer = Some(app.set_interval(
            Duration::from_secs_f64(1.0 / 10.0),
            None,
            true, // pause=True
            Box::new(|app, _ctx| {
                if let Ok(handle) = app.query_one_typed::<ProgressBar>("#progress_bar") {
                    let _ = handle.update(app, |bar, rctx| bar.advance(1.0, rctx));
                }
            }),
        ));
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut textual::event::WidgetCtx) {
        if action == "start" {
            // Python action_start: query_one(ProgressBar).update(total=100); timer.resume().
            if let Ok(handle) = app.query_one_typed::<ProgressBar>("#progress_bar") {
                let _ = handle.update(app, |bar, rctx| {
                    bar.update(Some(Some(100.0)), None, None, rctx);
                });
            }
            if let Some(handle) = self.progress_timer {
                app.resume_timer(handle);
            }
            ctx.request_repaint();
            ctx.set_handled();
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
    fn initial_state_has_no_timer() {
        let app = IndeterminateProgressBar::new();
        assert!(app.progress_timer.is_none());
    }

    /// Deterministic timer wiring (public API only): registering a paused interval
    /// yields a live-but-paused handle; the `start` action's `resume()` activates
    /// it, and the bar-advance callback advances a ProgressBar by exactly 1.
    #[test]
    fn paused_interval_resume_and_advance_wiring() {
        let mut app = App::new().expect("app");

        // Mount-time registration: a PAUSED 1/10s interval (pause=True).
        let handle = app.set_interval(
            Duration::from_secs_f64(1.0 / 10.0),
            None,
            true,
            Box::new(|app, _ctx| {
                if let Ok(handle) = app.query_one_typed::<ProgressBar>("#progress_bar") {
                    let _ = handle.update(app, |bar, rctx| bar.advance(1.0, rctx));
                }
            }),
        );
        // The timer exists (registered) even while paused — `start` will resume it.
        assert!(app.timer_is_active(handle), "interval is registered at mount");

        // Resume (Python action_start) then stop — public state machine works.
        app.resume_timer(handle);
        assert!(app.timer_is_active(handle));
        app.stop_timer(handle);
        assert!(!app.timer_is_active(handle), "stopped timer is removed");

        // The callback body advances a ProgressBar by exactly 1 per fire.
        let mut bar = ProgressBar::new(Some(100.0));
        let before = bar.progress();
        let mut rctx = textual::reactive::ReactiveCtx::new(textual::node_id::NodeId::default());
        bar.advance(1.0, &mut rctx);
        assert_eq!(bar.progress(), before + 1.0, "make_progress advances by 1");
    }

    /// LIVENESS (end-to-end through the headless harness): the bar is paused at
    /// mount, so advancing the clock alone leaves it at 0; pressing `s` resumes
    /// the interval (and sets total=100), and advancing the deterministic clock
    /// then fires `make_progress` ticks that advance the bar. A dead wiring
    /// (action not routed, or timer never resumed) would leave it at 0.
    ///
    /// We assert on the observable widget state (`ProgressBar::progress`) — the
    /// true thing the demo mutates. (The 80×24 indeterminate bar at 10% happens
    /// to hash to the same frame fingerprint as the empty bar, so the rendered
    /// fingerprint is too coarse to be the liveness signal here; the state
    /// transition 0 → 10 is the honest proof the demo functions.)
    #[test]
    fn liveness_start_then_clock_advances_bar() {
        IndeterminateProgressBar::new()
            .run_test(|pilot| {
                let read_progress = |pilot: &Pilot| -> (f64, Option<f64>) {
                    let app = pilot.app();
                    app.query_one_typed::<ProgressBar>("#progress_bar")
                        .ok()
                        .and_then(|h| h.read(app, |b| (b.progress(), b.total())).ok())
                        .unwrap_or((-1.0, None))
                };
                // Paused at mount: clock advance before `s` must NOT advance it.
                pilot.advance_clock(Duration::from_secs(1))?;
                assert_eq!(
                    read_progress(pilot).0,
                    0.0,
                    "timer is paused at mount; clock advance must be inert"
                );
                // Resume via the `s` action, then advance: bar must fill.
                pilot.press(&["s"])?;
                pilot.advance_clock(Duration::from_secs(1))?;
                let (progress, total) = read_progress(pilot);
                assert!(
                    progress > 0.0,
                    "after `s`, advancing the clock must advance the bar (got {progress})"
                );
                assert_eq!(total, Some(100.0), "`s` sets total=100");
                Ok(())
            })
            .expect("run_test");
    }
}

#[cfg(test)]
use textual::runtime::Pilot;
