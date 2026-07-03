use std::sync::{Arc, Mutex};

use textual::compose;
use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/input_validation.py`.
const CSS: &str = r#"
Input.-valid {
    border: tall $success 60%;
}
Input.-valid:focus {
    border: tall $success;
}
Input {
    margin: 1 1;
}
Label {
    margin: 1 2;
}
Pretty {
    margin: 1 2;
}
"#;

struct InputValidationApp {
    pretty_str: Arc<Mutex<String>>,
}

impl InputValidationApp {
    fn new() -> Self {
        Self {
            pretty_str: Arc::new(Mutex::new(format!("{:?}", Vec::<String>::new()))),
        }
    }
}

impl TextualApp for InputValidationApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Container::new().with_compose(compose![
                Label::new(
                    "Enter an even number between 1 and 100 that is also a palindrome.",
                ),
                Input::new()
                    .with_placeholder("Enter a number...")
                    .with_validators(vec![
                        Arc::new(Number::new().minimum(1.0).maximum(100.0)) as ValidatorRef,
                        Arc::new(Function::new(is_even, "Value is not even.")) as ValidatorRef,
                        Arc::new(Palindrome) as ValidatorRef,
                    ]),
                Pretty::shared(self.pretty_str.clone()),
            ]))
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn on_input_changed(
        &mut self,
        _value: &str,
        validation: &ValidationResult,
        ctx: &mut textual::event::WidgetCtx,
    ) {
        let next = if validation.is_valid {
            Vec::new()
        } else {
            validation.failure_descriptions.clone()
        };
        *self.pretty_str.lock().unwrap_or_else(|e| e.into_inner()) = format!("{:?}", next);
        ctx.request_repaint();
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(InputValidationApp::new())
}

fn is_even(value: &str) -> bool {
    value.trim().parse::<i64>().is_ok_and(|n| n % 2 == 0)
}

struct Palindrome;

impl Validator for Palindrome {
    fn validate(&self, value: &str) -> ValidationResult {
        if is_palindrome(value) {
            ValidationResult::success()
        } else {
            ValidationResult::failure("That's not a palindrome :/")
        }
    }
}

fn is_palindrome(value: &str) -> bool {
    value.chars().eq(value.chars().rev())
}

#[cfg(test)]
mod liveness {
    use super::*;
    use textual::run_test;

    /// LIVENESS: typing an invalid value ("7") into the focused Input fires
    /// `on_input_changed`, which writes the validation failure descriptions into
    /// the shared Pretty string. We observe the shared `Arc<Mutex<String>>`
    /// directly (cloned before the app moves it): it must transition from the
    /// empty-list initial state to a non-empty failure list, AND the rendered
    /// frame must change. Proves the input -> validation -> Pretty path is wired.
    #[test]
    fn invalid_input_publishes_failures_and_changes_frame() {
        let app = InputValidationApp::new();
        let pretty = app.pretty_str.clone();
        let initial = pretty.lock().unwrap().clone();

        run_test(app, |pilot| {
            let before = pilot.app().frame_fingerprint();
            // "7" is odd and not in an even/palindrome class -> validators fail.
            pilot.press(&["7"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "typing an invalid number must re-render (echo + Pretty update)"
            );
            Ok(())
        })
        .unwrap();

        let final_str = pretty.lock().unwrap().clone();
        assert_ne!(
            initial, final_str,
            "validation failures must be published into the shared Pretty string"
        );
        assert!(
            final_str.contains("not") || final_str.len() > initial.len(),
            "Pretty string should carry validation failure descriptions, got: {final_str}"
        );
    }
}
