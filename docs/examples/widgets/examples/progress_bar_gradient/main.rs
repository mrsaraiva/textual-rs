/// Port of Python Textual `docs/examples/widgets/progress_bar_gradient.py`.
///
/// Demonstrates a `ProgressBar` with a 12-stop rainbow gradient applied:
/// - Progress bar centered on screen via Center > Middle layout.
/// - A multi-stop color gradient sweeps across the filled portion of the bar.
/// - On mount, progress is set to 70%.
///
/// Python source:
/// ```python
/// gradient = Gradient.from_colors(
///     "#881177", "#aa3355", "#cc6666", "#ee9944",
///     "#eedd00", "#99dd55", "#44dd88", "#22ccbb",
///     "#00bbcc", "#0099cc", "#3366bb", "#663399",
/// )
/// ProgressBar(total=100, gradient=gradient)
/// ```
use textual::prelude::*;
use textual::renderables::LinearGradient;

/// Build the 12-stop rainbow `LinearGradient` matching Python's
/// `Gradient.from_colors(...)` call in the original demo.
fn rainbow_gradient() -> LinearGradient {
    let colors: &[Color] = &[
        Color::rgb(0x88, 0x11, 0x77),
        Color::rgb(0xaa, 0x33, 0x55),
        Color::rgb(0xcc, 0x66, 0x66),
        Color::rgb(0xee, 0x99, 0x44),
        Color::rgb(0xee, 0xdd, 0x00),
        Color::rgb(0x99, 0xdd, 0x55),
        Color::rgb(0x44, 0xdd, 0x88),
        Color::rgb(0x22, 0xcc, 0xbb),
        Color::rgb(0x00, 0xbb, 0xcc),
        Color::rgb(0x00, 0x99, 0xcc),
        Color::rgb(0x33, 0x66, 0xbb),
        Color::rgb(0x66, 0x33, 0x99),
    ];
    let n = colors.len() - 1;
    let stops: Vec<(f32, Color)> = colors
        .iter()
        .enumerate()
        .map(|(i, &c)| (i as f32 / n as f32, c))
        .collect();
    LinearGradient::new(0.0, stops)
}

struct ProgressApp;

impl TextualApp for ProgressApp {
    fn compose(&mut self) -> AppRoot {
        let bar = ProgressBar::new(Some(100.0)).with_gradient(rainbow_gradient());

        AppRoot::new().with_child(
            Center::new().with_child(Middle::new().with_child(bar)),
        )
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut textual::event::WidgetCtx) {
        if let Ok(handle) = app.query_one_typed::<ProgressBar>("ProgressBar") {
            let _ = handle.update(app, |bar, rctx| {
                bar.update(None, Some(70.0), None, rctx);
            });
        }
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

    #[test]
    fn rainbow_gradient_has_twelve_stops() {
        // Verify the gradient is built from 12 colors as in the Python source.
        // LinearGradient does not expose stop count directly, but sampling at
        // 0.0 and 1.0 should return the first and last colors without panic.
        let g = rainbow_gradient();
        let c_start = g.get_color(0.0);
        let c_end = g.get_color(1.0);
        // First color: #881177
        assert_eq!(c_start.r, 0x88);
        assert_eq!(c_start.g, 0x11);
        assert_eq!(c_start.b, 0x77);
        // Last color: #663399
        assert_eq!(c_end.r, 0x66);
        assert_eq!(c_end.g, 0x33);
        assert_eq!(c_end.b, 0x99);
    }

    /// LIVENESS (startup behaviour): this demo has no user interaction — its one
    /// behaviour is the `on_mount` setting progress to 70%. Under the headless
    /// harness we assert the mount hook actually ran by reading the bar's
    /// observable progress (70.0). A dead mount hook leaves it at 0.
    #[test]
    fn liveness_on_mount_sets_progress_to_70() {
        ProgressApp
            .run_test(|pilot| {
                let app = pilot.app();
                let progress = app
                    .query_one_typed::<ProgressBar>("ProgressBar")
                    .ok()
                    .and_then(|h| h.read(app, |b| b.progress()).ok())
                    .unwrap_or(-1.0);
                assert_eq!(progress, 70.0, "on_mount must set progress to 70");
                Ok(())
            })
            .expect("run_test");
    }
}
