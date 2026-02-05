use std::sync::{Arc, Mutex};

use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/input_validation.py`.
#[tokio::main]
async fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }

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

    let failures: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let failures_for_widget = failures.clone();

    let validators: Vec<ValidatorRef> = vec![
        Arc::new(Number::new().minimum(1.0).maximum(100.0)) as ValidatorRef,
        Arc::new(Function::new(is_even, "Value is not even.")) as ValidatorRef,
        Arc::new(Palindrome) as ValidatorRef,
    ];

    let input = Input::new()
        .with_placeholder("Enter a number...")
        .with_validators(validators)
        .on_change({
            let failures = failures.clone();
            move |input| {
                let next = if input.text().trim().is_empty() {
                    Vec::new()
                } else if !input.validation_result().is_valid {
                    input.validation_result().failure_descriptions.clone()
                } else {
                    Vec::new()
                };
                *failures.lock().unwrap() = next;
            }
        });

    let root_widget = Container::new()
        .with_child(Label::new(
            "Enter an even number between 1 and 100 that is also a palindrome.",
        ))
        .with_child(input)
        .with_child(Pretty::new(failures_for_widget));

    let mut root = AppRoot::new().with_child(root_widget);
    let mut app = App::new()?;
    app.load_stylesheet(CSS);
    app.run_widget_tree(&mut root).await
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
