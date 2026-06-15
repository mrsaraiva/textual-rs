/// Port of Python Textual `docs/examples/widgets/progress_bar_isolated.py`.
///
/// Demonstrates `ProgressBar` in indeterminate and determinate modes:
/// - Initially renders an indeterminate (animated) progress bar centered on screen.
/// - Pressing `s` sets total=100 and starts advancing the bar by 1 step per tick.
///
/// Layout: Center > Middle > ProgressBar, Footer at bottom.
use textual::prelude::*;

struct IndeterminateProgressBar {
    /// Whether the progress timer is running.
    started: bool,
    /// Accumulated progress steps (only meaningful when started).
    progress: f64,
}

impl IndeterminateProgressBar {
    fn new() -> Self {
        Self {
            started: false,
            progress: 0.0,
        }
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

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        if self.started {
            self.progress += 1.0;
            let p = self.progress;
            let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress_bar", |bar| {
                bar.advance(p - bar.progress());
            });
            ctx.request_repaint();
        }
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        if action == "start" {
            let _ = app.with_query_one_mut_as::<ProgressBar, _>("#progress_bar", |bar| {
                bar.update(Some(Some(100.0)), None, None);
            });
            self.started = true;
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
    fn initial_state_not_started() {
        let app = IndeterminateProgressBar::new();
        assert!(!app.started);
        assert_eq!(app.progress, 0.0);
    }
}
