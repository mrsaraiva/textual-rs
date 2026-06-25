/// Port of Python Textual `docs/examples/widgets/radio_button.py`.
///
/// Demonstrates `RadioButton` widgets inside a `RadioSet`:
/// - Nine radio buttons with various sci-fi movie/show labels.
/// - "Serenity" is pre-selected (value=True in Python).
/// - The RadioSet is centered on screen (width: 50%).
///
/// Python: `on_mount` calls `self.query_one(RadioSet).focus()`.
/// Rust: The RadioSet is the first focusable widget, so it receives focus
/// automatically on start.
///
/// Note: Python uses `Text.from_markup(...)` with emoji shortcodes for
/// "Total Recall". In Rust we use the plain Unicode equivalent.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

RadioSet {
    width: 50%;
}
"#;

struct RadioChoicesApp;

impl TextualApp for RadioChoicesApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let radio_set = RadioSet::new()
            .with_button(RadioButton::new("Battlestar Galactica"))
            .with_button(RadioButton::new("Dune 1984"))
            .with_button(RadioButton::new("Dune 2021"))
            .with_button(RadioButton::new("Serenity").with_value(true))
            .with_button(RadioButton::new("Star Trek: The Motion Picture"))
            .with_button(RadioButton::new("Star Wars: A New Hope"))
            .with_button(RadioButton::new("The Last Starfighter"))
            .with_button(RadioButton::new(
                "Total Recall 👉 🔴",
            ))
            .with_button(RadioButton::new("Wing Commander"));

        AppRoot::new().with_child(radio_set)
    }
}

fn main() -> textual::Result<()> {
    run_sync(RadioChoicesApp)
}

// ---------------------------------------------------------------------------
// Regression tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_choices_app_composes_without_panic() {
        let mut app = RadioChoicesApp;
        let _root = app.compose();
    }

    #[test]
    fn radio_set_has_nine_buttons() {
        let radio_set = RadioSet::new()
            .with_button(RadioButton::new("Battlestar Galactica"))
            .with_button(RadioButton::new("Dune 1984"))
            .with_button(RadioButton::new("Dune 2021"))
            .with_button(RadioButton::new("Serenity").with_value(true))
            .with_button(RadioButton::new("Star Trek: The Motion Picture"))
            .with_button(RadioButton::new("Star Wars: A New Hope"))
            .with_button(RadioButton::new("The Last Starfighter"))
            .with_button(RadioButton::new("Total Recall 👉 🔴"))
            .with_button(RadioButton::new("Wing Commander"));
        assert_eq!(radio_set.len(), 9);
    }

    #[test]
    fn serenity_is_pre_selected() {
        let radio_set = RadioSet::new()
            .with_button(RadioButton::new("Battlestar Galactica"))
            .with_button(RadioButton::new("Dune 1984"))
            .with_button(RadioButton::new("Dune 2021"))
            .with_button(RadioButton::new("Serenity").with_value(true))
            .with_button(RadioButton::new("Star Trek: The Motion Picture"))
            .with_button(RadioButton::new("Star Wars: A New Hope"))
            .with_button(RadioButton::new("The Last Starfighter"))
            .with_button(RadioButton::new("Total Recall 👉 🔴"))
            .with_button(RadioButton::new("Wing Commander"));
        // "Serenity" is at index 3 and was set with value=true.
        assert_eq!(radio_set.pressed_index(), Some(3));
    }

    /// LIVENESS: tab to focus the RadioSet ("Serenity", index 3, pressed at
    /// mount), navigate down and select (enter). We assert on the observable
    /// widget state (`pressed_index` moves off 3) — the true thing the
    /// interaction mutates. A dead RadioSet (keys not routed / not focusable)
    /// leaves the pressed index put.
    #[test]
    fn liveness_navigate_and_select() {
        RadioChoicesApp
            .run_test(|pilot| {
                let pressed = |pilot: &Pilot| -> Option<usize> {
                    let app = pilot.app();
                    app.query_one_typed::<RadioSet>("RadioSet")
                        .ok()
                        .and_then(|h| h.read(app, |r| r.pressed_index()).ok())
                        .flatten()
                };
                pilot.press(&["tab"])?; // focus the radio set
                assert_eq!(pressed(pilot), Some(3), "Serenity pressed at mount");
                pilot.press(&["down", "enter"])?;
                assert_ne!(
                    pressed(pilot),
                    Some(3),
                    "selecting another button must move the pressed index"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
