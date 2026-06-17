/// Port of Python Textual `docs/examples/guide/reactivity/set_reactive01.py`.
///
/// Demonstrates setting reactive attributes by cycling greetings with the space bar.
///
/// Python uses a custom `Greeter(Horizontal)` widget with reactive fields `greeting`
/// and `who`; watchers update child Labels when the reactive field changes. In Rust
/// there is no equivalent "subclass a container with reactive fields" mechanism, so
/// the app holds state directly and updates the Label via `with_query_one_mut_as`.
/// This is a known framework gap: custom reactive widget composition.
///
/// Framework gap: Python `reactive`/`var` fields on custom widget subclasses are not
/// yet expressible in Rust textual-rs. The greeting label is updated directly in the
/// action handler instead of via a watcher on a Greeter sub-widget.
use textual::prelude::*;

const GREETINGS: &[&str] = &[
    "Bonjour",
    "Hola",
    "こんにちは",
    "你好",
    "안녕하세요",
    "Hello",
];

const CSS: &str = r##"
Screen {
    align: center middle;
}

#greeter {
    width: auto;
    height: 1;
}

#greeter Label {
    margin: 0 1;
}
"##;

struct NameApp {
    greeting_no: usize,
}

impl NameApp {
    fn new() -> Self {
        Self { greeting_no: 0 }
    }
}

impl TextualApp for NameApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("space", "greeting", "Next greeting")]
    }

    fn compose(&mut self) -> AppRoot {
        // Horizontal has no `.with_id()` builder; use `ChildDecl::with_id` instead
        // so the CSS id selector `#greeter` resolves correctly.
        // Label has `.with_id()` natively.
        let greeter = ChildDecl::from(Horizontal::new().with_compose(vec![
            ChildDecl::from(Label::new("Hello").with_id("greeting")),
            ChildDecl::from(Label::new("Textual").with_id("name")),
        ]))
        .with_id("greeter");
        AppRoot::new().with_compose(vec![greeter])
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        if action != "greeting" {
            return;
        }
        self.greeting_no = (self.greeting_no + 1) % GREETINGS.len();
        let new_greeting = GREETINGS[self.greeting_no].to_string();
        let _ = app.with_query_one_mut_as::<Label, _>("#greeting", |label| {
            label.set_text(new_greeting);
        });
        ctx.request_repaint();
        ctx.set_handled();
    }
}

fn main() -> textual::Result<()> {
    run_sync(NameApp::new())
}
