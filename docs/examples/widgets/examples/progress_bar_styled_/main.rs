/// Port of Python Textual `docs/examples/widgets/progress_bar_styled_.py`.
///
/// Demonstrates a styled `ProgressBar`:
/// - Starts in indeterminate state (no total set).
/// - Press 's' to set total=100 and begin advancing progress at 10 steps/sec.
/// - Press 'u' to jump to 100% complete.
/// - Custom CSS overrides `Bar`, `PercentageStatus`, and `ETAStatus` component styles.
///
/// NOTE: This example is non-deterministic — the indeterminate bar animates
/// (sliding highlight) and the ETA countdown changes continuously.
/// Plain-text snapshot comparison cannot verify parity for live animation.
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
    /// Whether the progress timer is running.
    running: bool,
    /// Tick counter to throttle progress advances to ~10 per second.
    tick_count: u64,
}

impl TextualApp for StyledProgressBar {
    fn title(&self) -> &'static str {
        "StyledProgressBar"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("s", "start", "Start")]
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Center::new().with_child(
                    Middle::new().with_child(ProgressBar::new(None)),
                ),
            )
            .with_child(Footer::new())
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        if action == "start" {
            // Set total=100 and start advancing progress.
            let _ = app.with_query_one_mut_as::<ProgressBar, _>("ProgressBar", |bar| {
                bar.update(Some(Some(100.0)), Some(0.0), None);
            });
            self.running = true;
            self.tick_count = 0;
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        match key.key.as_str() {
            "u" => {
                // Jump to 100% complete.
                let _ = app.with_query_one_mut_as::<ProgressBar, _>("ProgressBar", |bar| {
                    bar.update(Some(Some(100.0)), Some(100.0), None);
                });
                ctx.request_repaint();
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn on_tick_with_app(&mut self, app: &mut App, tick: u64, ctx: &mut EventCtx) {
        let _ = tick;
        if !self.running {
            return;
        }
        self.tick_count += 1;
        // Advance at ~10 steps per second. The runtime tick is typically ~30 fps,
        // so advance every 3 ticks ≈ 10 Hz.
        if self.tick_count % 3 == 0 {
            let mut stop = false;
            let _ = app.with_query_one_mut_as::<ProgressBar, _>("ProgressBar", |bar| {
                bar.advance(1.0);
                if bar.percentage().map(|p| p >= 1.0).unwrap_or(false) {
                    stop = true;
                }
            });
            if stop {
                self.running = false;
            }
            ctx.request_repaint();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(StyledProgressBar::default())
}
