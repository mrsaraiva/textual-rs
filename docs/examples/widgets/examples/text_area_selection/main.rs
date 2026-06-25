use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_selection.py`.
const TEXT: &str = r#"def hello(name):
    print("hello" + name)

def goodbye(name):
    print("goodbye" + name)
"#;

struct TextAreaSelectionApp;

impl TextualApp for TextAreaSelectionApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            TextArea::code_editor(TEXT)
                .with_language("python")
                .with_selection(TextAreaSelection {
                    start: TextAreaCursor { row: 0, col: 0 },
                    end: TextAreaCursor { row: 2, col: 0 },
                }),
        )
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(TextAreaSelectionApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS: the editor mounts with a multi-line selection. Focusing and
    /// pressing right collapses/moves the selection cursor, changing the
    /// rendered highlight — the frame must change. A dead TextArea (keys not
    /// routed) leaves the frame identical.
    #[test]
    fn liveness_move_collapses_selection() {
        TextAreaSelectionApp
            .run_test(|pilot| {
                pilot.press(&["tab"])?; // focus the editor
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["right"])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(
                    before, after,
                    "moving the cursor must change the selection highlight (frame changes)"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
