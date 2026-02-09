use std::sync::{Arc, Mutex};

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
    failures: Arc<Mutex<Vec<String>>>,
    pretty_str: Arc<Mutex<String>>,
}

impl InputValidationApp {
    fn new() -> Self {
        Self {
            failures: Arc::new(Mutex::new(Vec::new())),
            pretty_str: Arc::new(Mutex::new(format!("{:?}", Vec::<String>::new()))),
        }
    }
}

impl TextualApp for InputValidationApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Container::new()
                .with_child(Label::new(
                    "Enter an even number between 1 and 100 that is also a palindrome.",
                ))
                .with_child(
                    Input::new()
                        .with_placeholder("Enter a number...")
                        .with_validators(vec![
                            Arc::new(Number::new().minimum(1.0).maximum(100.0)) as ValidatorRef,
                            Arc::new(Function::new(is_even, "Value is not even.")) as ValidatorRef,
                            Arc::new(Palindrome) as ValidatorRef,
                        ]),
                )
                .with_child(Pretty::shared(self.pretty_str.clone())),
        )
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Message::InputChanged { value, validation } = &message.message {
            let next = if value.trim().is_empty() || validation.is_valid {
                Vec::new()
            } else {
                validation.failure_descriptions.clone()
            };
            *self.failures.lock().unwrap_or_else(|e| e.into_inner()) = next.clone();
            *self.pretty_str.lock().unwrap_or_else(|e| e.into_inner()) = format!("{:?}", next);
            ctx.request_repaint();
        }
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
        let value = value.trim();
        if value.is_empty() {
            return ValidationResult::failure("Value is required.");
        }
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
