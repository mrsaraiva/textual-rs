/// Port of Python Textual `docs/examples/widgets/log.py`.
///
/// Demonstrates the `Log` widget:
/// - A single Log fills the screen.
/// - On ready: writes "Hello, World!" then writes the Litany Against Fear
///   passage 10 times via `write_line`.
use textual::prelude::*;

const TEXT: &str = "I must not fear.\n\
Fear is the mind-killer.\n\
Fear is the little-death that brings total obliteration.\n\
I will face my fear.\n\
I will permit it to pass over me and through me.\n\
And when it has gone past, I will turn the inner eye to see its path.\n\
Where the fear has gone there will be nothing. Only I will remain.";

const CSS: &str = r#"
Log {
    width: 1fr;
    height: 1fr;
}
"#;

struct LogApp;

impl TextualApp for LogApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Log::new())
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut textual::event::WidgetCtx) {
        let _ = app.with_query_one_mut_as::<Log, _>("Log", |log| {
            log.write_line("Hello, World!");
            for _ in 0..10 {
                log.write_line(TEXT);
            }
        });
        ctx.request_repaint();
    }
}

fn main() -> textual::Result<()> {
    run_sync(LogApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_app_composes_without_panic() {
        let mut app = LogApp;
        let _root = app.compose();
    }

    #[test]
    fn text_constant_has_expected_lines() {
        let lines: Vec<&str> = TEXT.lines().collect();
        assert_eq!(lines.len(), 7);
        assert_eq!(lines[0], "I must not fear.");
        assert_eq!(
            lines[6],
            "Where the fear has gone there will be nothing. Only I will remain."
        );
    }
}
