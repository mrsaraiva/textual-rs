/// Port of Python Textual `docs/examples/widgets/progress_bar_styled.py`.
///
/// Demonstrates `ProgressBar` with custom CSS styling applied via
/// `progress_bar_styled.tcss`. Layout: Center > Middle > ProgressBar, Footer at bottom.
///
/// Pressing `s` sets total=100 and starts advancing the bar by 1 step per tick
/// (mirroring Python's `set_interval(1/10, make_progress, pause=True)`/`resume()`).
///
/// NOTE: The initial screen shows an indeterminate (animated) progress bar.
/// The animation position is time-based and will differ between Python and Rust
/// runs, making exact parity verification non-deterministic for the animated bar.
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

struct StyledProgressBar {
    /// Whether the progress timer is running.
    started: bool,
    /// Current progress value (only meaningful when started).
    progress: f64,
}

impl StyledProgressBar {
    fn new() -> Self {
        Self {
            started: false,
            progress: 0.0,
        }
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
                bar.update(Some(Some(100.0)), Some(0.0), None);
            });
            self.progress = 0.0;
            self.started = true;
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
    fn initial_state_not_started() {
        let app = StyledProgressBar::new();
        assert!(!app.started);
        assert_eq!(app.progress, 0.0);
    }
}
