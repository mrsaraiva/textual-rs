/// Port of Python Textual `docs/examples/widgets/radio_set.py`.
///
/// Demonstrates `RadioSet` with two approaches:
/// 1. A `RadioSet` built up from individual `RadioButton` widgets, with "Serenity"
///    pre-selected and the set focused on mount.
/// 2. A `RadioSet` built from a collection of string labels (via `from_labels`).
///
/// Both sets are displayed side-by-side in a `Horizontal` container.
/// The first set is auto-focused on mount, mirroring Python `on_mount`'s
/// `self.query_one("#focus_me").focus()`.
///
/// Note: Python's `Text.from_markup()` for the "Total Recall" button uses Rich
/// emoji shortcodes (`:backhand_index_pointing_right: :red_circle:`). Rust
/// renders this as plain text without emoji substitution.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

Horizontal {
    align: center middle;
    height: auto;
}

RadioSet {
    width: 45%;
}
"#;

struct RadioChoicesApp;

impl TextualApp for RadioChoicesApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        // First RadioSet: built from individual RadioButtons.
        // "Serenity" is pre-selected (value=true).
        let radio_set_1 = RadioSet::new()
            .with_button(RadioButton::new("Battlestar Galactica"))
            .with_button(RadioButton::new("Dune 1984"))
            .with_button(RadioButton::new("Dune 2021"))
            .with_button(RadioButton::new("Serenity").with_value(true))
            .with_button(RadioButton::new("Star Trek: The Motion Picture"))
            .with_button(RadioButton::new("Star Wars: A New Hope"))
            .with_button(RadioButton::new("The Last Starfighter"))
            .with_button(RadioButton::new(
                "Total Recall \u{1F449} \u{1F534}",
            ))
            .with_button(RadioButton::new("Wing Commander"));

        // Second RadioSet: built from a collection of string labels.
        let radio_set_2 = RadioSet::from_labels(&[
            "Amanda",
            "Connor MacLeod",
            "Duncan MacLeod",
            "Heather MacLeod",
            "Joe Dawson",
            "Kurgan, The",
            "Methos",
            "Rachel Ellenstein",
            "Ramírez",
        ]);

        let horizontal = Horizontal::new()
            .with_child(radio_set_1)
            .with_child(radio_set_2);

        AppRoot::new().with_child(horizontal)
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Mirror Python `on_mount`: focus the first RadioSet (id="focus_me").
        // Since we can't set an id on RadioSet without modifying the framework,
        // we focus the first RadioSet by type selector.
        let _ = app.query_mut("RadioSet").map(|q| q.focus());
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
    fn radio_set_1_has_serenity_preselected() {
        let set = RadioSet::new()
            .with_button(RadioButton::new("Battlestar Galactica"))
            .with_button(RadioButton::new("Dune 1984"))
            .with_button(RadioButton::new("Dune 2021"))
            .with_button(RadioButton::new("Serenity").with_value(true))
            .with_button(RadioButton::new("Star Trek: The Motion Picture"))
            .with_button(RadioButton::new("Star Wars: A New Hope"))
            .with_button(RadioButton::new("The Last Starfighter"))
            .with_button(RadioButton::new("Total Recall \u{1F449} \u{1F534}"))
            .with_button(RadioButton::new("Wing Commander"));

        assert_eq!(set.pressed_index(), Some(3), "Serenity should be index 3");
        assert_eq!(set.len(), 9);
    }

    #[test]
    fn radio_set_2_from_labels_has_correct_count() {
        let set = RadioSet::from_labels(&[
            "Amanda",
            "Connor MacLeod",
            "Duncan MacLeod",
            "Heather MacLeod",
            "Joe Dawson",
            "Kurgan, The",
            "Methos",
            "Rachel Ellenstein",
            "Ramírez",
        ]);
        assert_eq!(set.len(), 9);
        assert_eq!(set.children()[0].label(), "Amanda");
        assert_eq!(set.children()[8].label(), "Ramírez");
    }
}
