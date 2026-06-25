use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_custom_theme.py`.
const TEXT: &str = r#"# says hello
def hello(name):
    print("hello" + name)

# says goodbye
def goodbye(name):
    print("goodbye" + name)
"#;

struct TextAreaCustomThemeApp;

impl TextualApp for TextAreaCustomThemeApp {
    fn compose(&mut self) -> AppRoot {
        let mut text_area = TextArea::new(TEXT)
            .with_language("python")
            .with_cursor_blink(false);

        let mut theme = TextAreaTheme::new("my_cool_theme");
        theme.cursor_style = Style::default()
            .fg(Color::parse("white").unwrap_or(Color::rgb(255, 255, 255)))
            .bg(Color::parse("blue").unwrap_or(Color::rgb(0, 0, 255)));
        theme.cursor_line_style =
            Style::default().bg(Color::parse("yellow").unwrap_or(Color::rgb(255, 255, 0)));
        theme.syntax_styles.insert(
            "string".to_string(),
            Style::default().fg(Color::parse("red").unwrap_or(Color::rgb(255, 0, 0))),
        );
        theme.syntax_styles.insert(
            "comment".to_string(),
            Style::default().fg(Color::parse("magenta").unwrap_or(Color::rgb(255, 0, 255))),
        );

        text_area.register_theme(theme);
        let text_area = text_area.with_theme("my_cool_theme");
        AppRoot::new().with_child(text_area)
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(TextAreaCustomThemeApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS: focus the editor and type a character; the edit mutates the
    /// document and changes the rendered frame. A dead TextArea (keys not routed
    /// to editing) leaves both identical.
    #[test]
    fn liveness_type_inserts_text() {
        TextAreaCustomThemeApp
            .run_test(|pilot| {
                pilot.press(&["tab"])?;
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["X"])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(before, after, "typing must change the rendered frame");
                Ok(())
            })
            .expect("run_test");
    }
}
