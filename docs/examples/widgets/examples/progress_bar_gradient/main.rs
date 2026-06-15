/// Port of Python Textual `docs/examples/widgets/progress_bar_gradient.py`.
///
/// Demonstrates a `ProgressBar` with a rainbow gradient applied:
/// - Progress bar centered on screen via Center > Middle layout.
/// - A color gradient sweeps across the filled portion of the bar.
/// - On mount, progress is set to 70%.
///
/// Note: Python's `Gradient.from_colors` supports an arbitrary number of color
/// stops; Rust's `ProgressBar::with_gradient` currently supports only a
/// two-stop (start → end) linear gradient. The port uses the first and last
/// colors of the Python rainbow as the gradient endpoints.
use textual::prelude::*;

struct ProgressApp;

impl TextualApp for ProgressApp {
    fn compose(&mut self) -> AppRoot {
        let start = Color::rgb(0x88, 0x11, 0x77);
        let end = Color::rgb(0x66, 0x33, 0x99);
        let bar = ProgressBar::new(Some(100.0)).with_gradient(start, end);

        AppRoot::new().with_child(
            Center::new().with_child(Middle::new().with_child(bar)),
        )
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        let _ = app.with_query_one_mut_as::<ProgressBar, _>("ProgressBar", |bar| {
            bar.update(None, Some(70.0), None);
        });
        ctx.request_repaint();
    }
}

fn main() -> textual::Result<()> {
    run_sync(ProgressApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_composes_without_panic() {
        let mut app = ProgressApp;
        let _root = app.compose();
    }
}
