use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/input_types.py`.
#[tokio::main]
async fn main() -> Result<()> {
    let form = Container::new()
        .with_child(
            Input::new()
                .with_placeholder("An integer")
                .with_type(InputType::Integer),
        )
        .with_child(
            Input::new()
                .with_placeholder("A number")
                .with_type(InputType::Number),
        );

    let mut root = AppRoot::new().with_child(form);
    let mut app = App::new()?;
    app.run_widget_tree(&mut root).await
}
