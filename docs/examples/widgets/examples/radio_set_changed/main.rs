/// Port of Python Textual `docs/examples/widgets/radio_set_changed.py`.
///
/// Demonstrates `RadioSet` with a `Changed` event handler that updates two
/// labels showing the pressed button label and its index.
///
/// Layout:
/// - `VerticalScroll` (centered)
///   - `Horizontal` containing a `RadioSet` (id="focus_me") with 9 buttons
///   - `Horizontal` containing a `Label` (id="pressed") for the button label
///   - `Horizontal` containing a `Label` (id="index") for the button index
///
/// On mount, the `RadioSet` is focused. When a button is selected, both
/// labels update via `on_radio_set_changed` (mapped to `on_message_with_app`).
use textual::prelude::*;

const CSS: &str = r#"
VerticalScroll {
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

struct RadioSetChangedApp;

impl TextualApp for RadioSetChangedApp {
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
                "Total Recall \u{1F449} \u{1F534}",
            ))
            .with_button(RadioButton::new("Wing Commander"));

        let radio_set_with_id = ChildDecl::from(radio_set).with_id("focus_me");

        let horizontal_set = Horizontal::new().with_compose(vec![radio_set_with_id]);

        let label_pressed = Label::new("").with_id("pressed");
        let label_index = Label::new("").with_id("index");

        let horizontal_pressed = Horizontal::new().with_child(label_pressed);
        let horizontal_index = Horizontal::new().with_child(label_index);

        let vs = VerticalScroll::new().with_compose(vec![
            ChildDecl::from(horizontal_set),
            ChildDecl::from(horizontal_pressed),
            ChildDecl::from(horizontal_index),
        ]);

        AppRoot::new().with_child(vs)
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut textual::event::WidgetCtx) {
        let _ = app.query_mut("#focus_me").map(|q| q.focus());
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut textual::event::WidgetCtx,
    ) {
        if let Some(ev) = message.downcast_ref::<RadioSetChanged>() {
            let index = ev.index;

            // Get the pressed button label from the RadioSet.
            let label_text = app
                .with_query_one_mut_as::<RadioSet, _>("#focus_me", |rs| {
                    rs.button(index)
                        .map(|b| b.label().to_string())
                        .unwrap_or_default()
                })
                .unwrap_or_default();

            let pressed_text = format!("Pressed button label: {}", label_text);
            let index_text = format!("Pressed button index: {}", index);

            let _ = app.with_query_one_mut_as::<Label, _>("#pressed", |label| {
                label.set_text(pressed_text);
            });
            let _ = app.with_query_one_mut_as::<Label, _>("#index", |label| {
                label.set_text(index_text);
            });

            ctx.request_repaint();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(RadioSetChangedApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_set_changed_app_composes_without_panic() {
        let mut app = RadioSetChangedApp;
        let _root = app.compose();
    }

    #[test]
    fn compose_produces_vertical_scroll() {
        let mut app = RadioSetChangedApp;
        let root = app.compose();
        assert!(!root.children().is_empty());
    }

    #[test]
    fn radio_set_has_serenity_preselected() {
        let set = RadioSet::new()
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

        assert_eq!(set.pressed_index(), Some(3), "Serenity should be at index 3");
        assert_eq!(set.len(), 9);
    }

    /// LIVENESS: the RadioSet is focused at mount. Navigating (down) and
    /// selecting (enter) emits `RadioSet.Changed`, whose handler fills the
    /// `#pressed` / `#index` labels (empty at start) — so the frame must change.
    /// A dead `on_radio_set_changed` wiring leaves the labels empty / frame
    /// identical.
    #[test]
    fn liveness_select_updates_labels() {
        RadioSetChangedApp
            .run_test(|pilot| {
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["down", "enter"])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(
                    before, after,
                    "selecting a radio button must update the labels (frame changes)"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
