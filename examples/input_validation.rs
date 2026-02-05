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
        .with_validators(validators);

    let root_widget = ValidationDemo::new(failures.clone()).with_child(
        Container::new()
            .with_child(Label::new(
                "Enter an even number between 1 and 100 that is also a palindrome.",
            ))
            .with_child(input)
            .with_child(Pretty::new(failures_for_widget)),
    );

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

struct ValidationDemo {
    id: WidgetId,
    failures: Arc<Mutex<Vec<String>>>,
    child: Box<dyn Widget>,
}

impl ValidationDemo {
    fn new(failures: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            id: WidgetId::new(),
            failures,
            child: Box::new(Spacer::new(1)),
        }
    }

    fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.child = Box::new(child);
        self
    }
}

impl Widget for ValidationDemo {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> rich_rs::Segments {
        self.child.render_styled(console, options)
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.child.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        match &message.message {
            Message::InputChanged { value, validation } => {
                let next = if value.trim().is_empty() {
                    Vec::new()
                } else if !validation.is_valid {
                    validation.failure_descriptions.clone()
                } else {
                    Vec::new()
                };
                *self.failures.lock().unwrap() = next;
                ctx.request_repaint();
            }
            _ => {}
        }
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
    }
}
