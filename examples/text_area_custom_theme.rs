use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_custom_theme.py`.
const TEXT: &str = r#"# says hello
def hello(name):
    print("hello" + name)

# says goodbye
def goodbye(name):
    print("goodbye" + name)
"#;

#[tokio::main]
async fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }

    let mut text_area = TextArea::new(TEXT)
        .with_language("python")
        .with_cursor_blink(false);

    let white = Color::parse("white").unwrap_or(Color::rgb(255, 255, 255));
    let blue = Color::parse("blue").unwrap_or(Color::rgb(0, 0, 255));
    let yellow = Color::parse("yellow").unwrap_or(Color::rgb(255, 255, 0));
    let red = Color::parse("red").unwrap_or(Color::rgb(255, 0, 0));
    let magenta = Color::parse("magenta").unwrap_or(Color::rgb(255, 0, 255));

    let mut theme = TextAreaTheme::new("my_cool_theme");
    theme.cursor_style = Style::default().fg(white).bg(blue);
    theme.cursor_line_style = Style::default().bg(yellow);
    theme.syntax_styles
        .insert("string".to_string(), Style::default().fg(red));
    theme.syntax_styles
        .insert("comment".to_string(), Style::default().fg(magenta));

    text_area.register_theme(theme);
    text_area.set_theme("my_cool_theme");

    let mut root = AppRoot::new().with_child(text_area);
    let mut app = App::new()?;
    app.run_widget_tree(&mut root).await
}
