/// Port of Python Textual `docs/examples/widgets/sparkline_colors.py`.
///
/// Demonstrates `Sparkline` with different color configurations via CSS
/// component classes (`sparkline--max-color` and `sparkline--min-color`).
/// Ten sparklines are shown, each with a different color combination.
use textual::prelude::*;

const CSS: &str = r#"
Sparkline {
    width: 100%;
    margin: 1;
}

#fst > .sparkline--max-color {
    color: $success;
}
#fst > .sparkline--min-color {
    color: $warning;
}

#snd > .sparkline--max-color {
    color: $warning;
}
#snd > .sparkline--min-color {
    color: $success;
}

#trd > .sparkline--max-color {
    color: $error;
}
#trd > .sparkline--min-color {
    color: $warning;
}

#frt > .sparkline--max-color {
    color: $warning;
}
#frt > .sparkline--min-color {
    color: $error;
}

#fft > .sparkline--max-color {
    color: $accent;
}
#fft > .sparkline--min-color {
    color: $accent 30%;
}

#sxt > .sparkline--max-color {
    color: $primary 30%;
}
#sxt > .sparkline--min-color {
    color: $primary;
}

#svt > .sparkline--max-color {
    color: $error;
}
#svt > .sparkline--min-color {
    color: $error 30%;
}

#egt > .sparkline--max-color {
    color: $error 30%;
}
#egt > .sparkline--min-color {
    color: $error;
}

#nnt > .sparkline--max-color {
    color: $success;
}
#nnt > .sparkline--min-color {
    color: $success 30%;
}

#tnt > .sparkline--max-color {
    color: $success 30%;
}
#tnt > .sparkline--min-color {
    color: $success;
}
"#;

struct SparklineColorsApp;

impl TextualApp for SparklineColorsApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let nums: Vec<f64> = (0..360 * 6)
            .step_by(20)
            .map(|x| (x as f64 / 3.14_f64).sin().abs())
            .collect();

        AppRoot::new().with_compose(vec![
            ChildDecl::from(Sparkline::new(nums.clone()).summary_function(summary_max))
                .with_id("fst"),
            ChildDecl::from(Sparkline::new(nums.clone()).summary_function(summary_max))
                .with_id("snd"),
            ChildDecl::from(Sparkline::new(nums.clone()).summary_function(summary_max))
                .with_id("trd"),
            ChildDecl::from(Sparkline::new(nums.clone()).summary_function(summary_max))
                .with_id("frt"),
            ChildDecl::from(Sparkline::new(nums.clone()).summary_function(summary_max))
                .with_id("fft"),
            ChildDecl::from(Sparkline::new(nums.clone()).summary_function(summary_max))
                .with_id("sxt"),
            ChildDecl::from(Sparkline::new(nums.clone()).summary_function(summary_max))
                .with_id("svt"),
            ChildDecl::from(Sparkline::new(nums.clone()).summary_function(summary_max))
                .with_id("egt"),
            ChildDecl::from(Sparkline::new(nums.clone()).summary_function(summary_max))
                .with_id("nnt"),
            ChildDecl::from(Sparkline::new(nums.clone()).summary_function(summary_max))
                .with_id("tnt"),
        ])
    }
}

fn main() -> textual::Result<()> {
    run_sync(SparklineColorsApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_colors_app_composes_without_panic() {
        let mut app = SparklineColorsApp;
        let _root = app.compose();
    }

    #[test]
    fn compose_produces_ten_sparklines() {
        let mut app = SparklineColorsApp;
        let root = app.compose();
        assert_eq!(root.children().len(), 10);
    }
}
