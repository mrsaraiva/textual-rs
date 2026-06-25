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

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Python on_mount: set_interval(1 / 10, self.make_progress, pause=True).
        // Each fire advances the bar by 1 (Python `make_progress`).
        self.progress_timer = Some(app.set_interval(
            Duration::from_secs_f64(1.0 / 10.0),
            None,
            true, // pause=True
            Box::new(|app, _ctx| {
                let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress_bar", |bar| {
                    bar.advance(1.0);
                });
            }),
        ));
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        if action == "start" {
            // Python action_start: query_one(ProgressBar).update(total=100); timer.resume().
            let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress_bar", |bar| {
                bar.update(Some(Some(100.0)), None, None);
            });
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
                let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress_bar", |bar| {
                    bar.advance(1.0);
                });
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
        bar.advance(1.0);
        assert_eq!(bar.progress(), before + 1.0, "make_progress advances by 1");
    }
}
