use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/input.py`.
#[tokio::main]
async fn main() -> Result<()> {
    let form = Container::new()
        .with_child(Input::new().with_placeholder("First Name"))
        .with_child(Input::new().with_placeholder("Last Name"));

    let mut root = AppRoot::new().with_child(form);
    let mut app = App::new()?;
    app.run_widget_tree(&mut root).await
}
