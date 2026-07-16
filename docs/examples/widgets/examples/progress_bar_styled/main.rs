/// Port of Python Textual `docs/examples/widgets/progress_bar_styled.py`.
///
/// Demonstrates `ProgressBar` with custom CSS styling applied via
/// `progress_bar_styled.tcss`. Layout: Center > Middle > ProgressBar, Footer at bottom.
///
/// Pressing `s` sets total=100 and starts advancing the bar by 1 step per timer
/// tick, mirroring Python's `set_interval(1/10, make_progress, pause=True)` +
/// `resume()`.
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
/// `resume()`s that timer.
///
/// NOTE: The initial screen shows an indeterminate (animated) progress bar.
/// The animation position is time-based and will differ between Python and Rust
/// runs, making exact parity verification non-deterministic for the animated bar.
use std::time::Duration;
use textual::prelude::*;

const CSS: &str = r#"
Bar > .bar--indeterminate {
    color: $primary;
    background: $secondary;
}

Bar > .bar--bar {
    color: $primary;
    background: $primary 30%;
}

Bar > .bar--complete {
    color: $error;
}

PercentageStatus {
    text-style: reverse;
    color: $secondary;
}

ETAStatus {
    text-style: underline;
}
"#;

#[derive(Default)]
struct StyledProgressBar {
    /// Paused progress timer, resumed by the `start` action (Python `progress_timer`).
    progress_timer: Option<TimerHandle>,
}

impl StyledProgressBar {
    fn new() -> Self {
        Self::default()
    }
}

impl TextualApp for StyledProgressBar {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("s", "start", "Start")]
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let bar = ChildDecl::from(ProgressBar::new(None)).with_id("progress_bar");
        let middle = Middle::new().with_compose(vec![bar]);
        let center = Center::new().with_child(middle);
        AppRoot::new()
            .with_child(center)
            .with_child(Footer::new())
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut textual::event::WidgetCtx) {
        // Python on_mount: set_interval(1 / 10, self.make_progress, pause=True).
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
                    bar.update(Some(Some(100.0)), Some(0.0), None, rctx);
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
    run_sync(StyledProgressBar::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_composes_without_panic() {
        let mut app = StyledProgressBar::new();
        let _root = app.compose();
    }

    #[test]
    fn initial_state_has_no_timer() {
        let app = StyledProgressBar::new();
        assert!(app.progress_timer.is_none());
    }

    /// Deterministic timer wiring (public API only): registering a paused interval
    /// yields a live-but-paused handle; `resume()` activates it, and the
    /// bar-advance callback advances a ProgressBar by exactly 1.
    #[test]
    fn paused_interval_resume_and_advance_wiring() {
        let mut app = App::new().expect("app");

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
        assert!(app.timer_is_active(handle), "interval is registered at mount");

        app.resume_timer(handle);
        assert!(app.timer_is_active(handle));
        app.stop_timer(handle);
        assert!(!app.timer_is_active(handle), "stopped timer is removed");

        let mut bar = ProgressBar::new(Some(100.0));
        let before = bar.progress();
        let mut rctx = textual::reactive::ReactiveCtx::new(textual::node_id::NodeId::default());
        bar.advance(1.0, &mut rctx);
        assert_eq!(bar.progress(), before + 1.0, "make_progress advances by 1");
    }

    /// LIVENESS (end-to-end): paused at mount; pressing `s` resumes the
    /// `set_interval`, and advancing the deterministic clock fires
    /// `make_progress`, advancing the bar's observable progress past 0. A dead
    /// wiring (action unrouted / timer never resumed) leaves it at 0.
    #[test]
    fn liveness_start_then_clock_advances_bar() {
        StyledProgressBar::new()
            .run_test(|pilot| {
                let read = |pilot: &Pilot| -> f64 {
                    let app = pilot.app();
                    app.query_one_typed::<ProgressBar>("#progress_bar")
                        .ok()
                        .and_then(|h| h.read(app, |b| b.progress()).ok())
                        .unwrap_or(-1.0)
                };
                pilot.advance_clock(Duration::from_secs(1))?;
                assert_eq!(read(pilot), 0.0, "paused at mount; clock advance inert");
                pilot.press(&["s"])?;
                pilot.advance_clock(Duration::from_secs(1))?;
                assert!(read(pilot) > 0.0, "after `s`, clock must advance the bar");
                Ok(())
            })
            .expect("run_test");
    }
}

#[cfg(test)]
use textual::runtime::Pilot;
