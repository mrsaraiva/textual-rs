/// Port of Python Textual `docs/examples/widgets/sparkline_basic.py`.
///
/// Demonstrates the basic usage of `Sparkline` with a fixed data set and
/// `summary_function=max`. The screen is centered/middle aligned and the
/// sparkline has a width of 3 columns and a 2-cell margin.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

Sparkline {
    width: 3;
    margin: 2;
}
"#;

const DATA: &[f64] = &[1.0, 2.0, 2.0, 1.0, 1.0, 4.0, 3.0, 1.0, 1.0, 8.0, 8.0, 2.0];

struct SparklineBasicApp;

impl TextualApp for SparklineBasicApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let sparkline = Sparkline::new(DATA.to_vec()).summary_function(summary_max);

        AppRoot::new().with_child(sparkline)
    }
}

fn main() -> textual::Result<()> {
    run_sync(SparklineBasicApp)
}

// ---------------------------------------------------------------------------
// Regression tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_basic_app_composes_without_panic() {
        let mut app = SparklineBasicApp;
        let _root = app.compose();
    }

    #[test]
    fn data_matches_python_source() {
        assert_eq!(
            DATA,
            &[1.0, 2.0, 2.0, 1.0, 1.0, 4.0, 3.0, 1.0, 1.0, 8.0, 8.0, 2.0]
        );
    }
}
